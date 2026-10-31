//! Potato benchmark harness (Phase 0).
//!
//! Measures client-observable performance of an HTTP fetch made through a **SOCKS5 proxy**: the time to open
//! the proxied connection (which includes the proxy's circuit setup), time-to-first-byte, total fetch time,
//! and download throughput. Point `--proxy` at vanilla Arti's SOCKS port for the baseline, at a chutney
//! client's SOCKS port, or at Potato's proxy later, and compare.
//!
//! Circuit-build time proper is an Arti-internal metric (Arti timeout stats), not measured here; see
//! PLAN.md section 6.
//!
//! Usage: `potato-bench [--proxy host:port] [--host H] [--port P] [--path /] [--reps N] [--timeout S]`
//! Output: one JSON object per rep plus a summary object, on stdout.

use std::time::Instant;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

struct Args {
    proxy: String,
    host: String,
    port: u16,
    path: String,
    reps: usize,
    timeout_s: u64,
}

fn parse_args() -> Args {
    let mut a = Args {
        proxy: "127.0.0.1:9050".into(),
        host: "example.com".into(),
        port: 80,
        path: "/".into(),
        reps: 3,
        timeout_s: 120,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--proxy" => {
                if let Some(v) = it.next() {
                    a.proxy = v;
                }
            }
            "--host" => {
                if let Some(v) = it.next() {
                    a.host = v;
                }
            }
            "--port" => {
                if let Some(v) = it.next() {
                    if let Ok(p) = v.parse() {
                        a.port = p;
                    }
                }
            }
            "--path" => {
                if let Some(v) = it.next() {
                    a.path = v;
                }
            }
            "--reps" => {
                if let Some(v) = it.next() {
                    if let Ok(n) = v.parse() {
                        a.reps = n;
                    }
                }
            }
            "--timeout" => {
                if let Some(v) = it.next() {
                    if let Ok(s) = v.parse() {
                        a.timeout_s = s;
                    }
                }
            }
            other => eprintln!("ignoring unknown arg: {other}"),
        }
    }
    a
}

/// Build a SOCKS5 CONNECT request for a domain-name target (RFC 1928, ATYP=0x03).
fn socks5_connect_request(host: &str, port: u16) -> Result<Vec<u8>> {
    let hb = host.as_bytes();
    if hb.is_empty() || hb.len() > 255 {
        bail!("hostname length {} out of range for SOCKS5", hb.len());
    }
    let mut r = Vec::with_capacity(7 + hb.len());
    r.extend_from_slice(&[0x05, 0x01, 0x00, 0x03]); // VER, CONNECT, RSV, ATYP=domain
    r.push(hb.len() as u8);
    r.extend_from_slice(hb);
    r.extend_from_slice(&port.to_be_bytes());
    Ok(r)
}

/// Open a TCP stream to `host:port` through the SOCKS5 proxy at `proxy` (no authentication).
async fn socks5_open(proxy: &str, host: &str, port: u16) -> Result<TcpStream> {
    let mut s = TcpStream::connect(proxy)
        .await
        .with_context(|| format!("connecting to SOCKS proxy {proxy}"))?;

    // Greeting: offer "no authentication".
    s.write_all(&[0x05, 0x01, 0x00]).await?;
    let mut greeting = [0u8; 2];
    s.read_exact(&mut greeting).await?;
    if greeting != [0x05, 0x00] {
        bail!("SOCKS5 proxy did not accept no-auth: {greeting:?}");
    }

    // CONNECT request.
    s.write_all(&socks5_connect_request(host, port)?).await?;

    // Reply header: VER, REP, RSV, ATYP.
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[1] != 0x00 {
        bail!("SOCKS5 CONNECT failed, reply code = {}", head[1]);
    }
    // Consume the bound address + port per ATYP.
    match head[3] {
        0x01 => {
            let mut b = [0u8; 4];
            s.read_exact(&mut b).await?;
        }
        0x04 => {
            let mut b = [0u8; 16];
            s.read_exact(&mut b).await?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            s.read_exact(&mut len).await?;
            let mut b = vec![0u8; len[0] as usize];
            s.read_exact(&mut b).await?;
        }
        atyp => bail!("SOCKS5 reply had unknown ATYP {atyp}"),
    }
    let mut bound_port = [0u8; 2];
    s.read_exact(&mut bound_port).await?;
    Ok(s)
}

struct Metrics {
    connect_ms: f64,
    ttfb_ms: f64,
    total_ms: f64,
    bytes: usize,
    throughput_kib_s: f64,
}

async fn one_fetch(a: &Args) -> Result<Metrics> {
    let t_connect = Instant::now();
    let mut s = socks5_open(&a.proxy, &a.host, a.port).await?;
    let connect_ms = t_connect.elapsed().as_secs_f64() * 1000.0;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        a.path, a.host
    );
    let t_req = Instant::now();
    s.write_all(req.as_bytes()).await?;
    s.flush().await?;

    let mut first = [0u8; 1];
    if s.read(&mut first).await? == 0 {
        bail!("connection closed before any response byte");
    }
    let ttfb_ms = t_req.elapsed().as_secs_f64() * 1000.0;

    let mut bytes = 1usize;
    let mut tmp = [0u8; 8192];
    loop {
        let m = s.read(&mut tmp).await?;
        if m == 0 {
            break;
        }
        bytes += m;
    }
    let total_ms = t_req.elapsed().as_secs_f64() * 1000.0;

    let download_s = (total_ms - ttfb_ms) / 1000.0;
    // Only report throughput when the download phase lasted long enough to measure. Tiny responses (e.g. a
    // short HTML page) arrive in ~one read, making download_s ~= 0 and the ratio meaningless; use a large
    // target for a real throughput number (see README).
    let throughput_kib_s = if download_s >= 0.01 {
        (bytes as f64 / 1024.0) / download_s
    } else {
        0.0
    };
    Ok(Metrics {
        connect_ms,
        ttfb_ms,
        total_ms,
        bytes,
        throughput_kib_s,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let a = parse_args();
    eprintln!(
        "bench: proxy={} target={}:{}{} reps={} timeout={}s",
        a.proxy, a.host, a.port, a.path, a.reps, a.timeout_s
    );

    let mut ttfbs = Vec::new();
    for i in 0..a.reps {
        match timeout(Duration::from_secs(a.timeout_s), one_fetch(&a)).await {
            Ok(Ok(m)) => {
                ttfbs.push(m.ttfb_ms);
                println!(
                    "{{\"rep\":{},\"connect_ms\":{:.1},\"ttfb_ms\":{:.1},\"total_ms\":{:.1},\"bytes\":{},\"throughput_kib_s\":{:.1}}}",
                    i, m.connect_ms, m.ttfb_ms, m.total_ms, m.bytes, m.throughput_kib_s
                );
            }
            Ok(Err(e)) => eprintln!("rep {i} error: {e:#}"),
            Err(_) => eprintln!("rep {i} timed out after {}s", a.timeout_s),
        }
    }

    if ttfbs.is_empty() {
        bail!("no successful reps");
    }
    ttfbs.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let median = ttfbs[ttfbs.len() / 2];
    println!(
        "{{\"summary\":true,\"reps_ok\":{},\"median_ttfb_ms\":{:.1}}}",
        ttfbs.len(),
        median
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socks5_request_for_domain() {
        let r = socks5_connect_request("example.com", 80).unwrap();
        assert_eq!(&r[..5], &[0x05, 0x01, 0x00, 0x03, 11]); // VER,CONNECT,RSV,domain,len=11
        assert_eq!(&r[5..16], b"example.com");
        assert_eq!(&r[16..], &[0x00, 0x50]); // port 80
    }

    #[test]
    fn rejects_empty_and_overlong_host() {
        assert!(socks5_connect_request("", 80).is_err());
        assert!(socks5_connect_request(&"a".repeat(256), 80).is_err());
    }
}
