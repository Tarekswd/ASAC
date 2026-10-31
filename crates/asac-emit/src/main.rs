//! F5 emitter (sim/shadow): one client host's outbound stream. It runs an [`asac_client::DecoyClient`] — the
//! Poisson cover-traffic scheduler (asac-mix F4) feeding vetted decoy queries (asac-decoy) — frames each tick
//! to a fixed size with [`asac_mix::frame`], and sends it to the observer. The host launched with `--query`
//! also mixes in one real search. Because the send schedule is independent of whether a real query is queued,
//! every host emits the same constant-rate stream and the observer cannot pick the searcher out by timing.
//!
//! Usage: `asac-emit --observer <host> [--port N] --rate R [--terms FILE] [--query TEXT] [--seed N]
//!         [--packet-size N] [--duration SECS]`. Runs until killed (Shadow stops it at `stop_time`) unless
//! `--duration` is given (handy for a local smoke run).

use std::net::UdpSocket;
use std::time::{Duration, Instant};

use asac_client::DecoyClient;
use asac_mix::frame;

const DEFAULT_PACKET_SIZE: usize = 512;
const DEFAULT_PORT: u16 = 9999;
// Fallback decoy terms so the emitter is usable without a --terms file (content only; timing is the point).
const FALLBACK_TERMS: &str = "search privacy anonymity network onion mixnet query index tor arti";

fn main() -> std::io::Result<()> {
    let mut rate = 2.0f64;
    let mut observer = "observer".to_string();
    let mut terms_file: Option<String> = None;
    let mut query: Option<String> = None;
    let mut seed = 1u64;
    let mut port = DEFAULT_PORT;
    let mut packet_size = DEFAULT_PACKET_SIZE;
    let mut duration: Option<f64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--rate" => rate = args.next().and_then(|v| v.parse().ok()).unwrap_or(rate),
            "--observer" => observer = args.next().unwrap_or(observer),
            "--port" => port = args.next().and_then(|v| v.parse().ok()).unwrap_or(port),
            "--terms" => terms_file = args.next(),
            "--query" => query = args.next(),
            "--seed" => seed = args.next().and_then(|v| v.parse().ok()).unwrap_or(seed),
            "--packet-size" => packet_size = args.next().and_then(|v| v.parse().ok()).unwrap_or(packet_size),
            "--duration" => duration = args.next().and_then(|v| v.parse().ok()),
            _ => {}
        }
    }

    let raw = terms_file
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| FALLBACK_TERMS.to_string());
    let terms: Vec<&str> = raw.split_whitespace().collect();
    let mut client = DecoyClient::new(&terms, rate, seed, 3).expect("terms list is non-empty");
    if let Some(q) = &query {
        client.enqueue_real(q);
    }

    let dest = format!("{observer}:{port}");
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(&dest)?;
    eprintln!(
        "emit -> {dest} rate={rate} seed={seed}{}",
        query
            .as_ref()
            .map(|q| format!(" query={q:?}"))
            .unwrap_or_default()
    );

    let start = Instant::now();
    loop {
        let (delay, q) = client.next_query();
        std::thread::sleep(Duration::from_secs_f64(delay));
        // Every packet is the same size (real and cover alike), so length leaks nothing; UDP loss is fine.
        if let Some(pkt) = frame(q.as_bytes(), packet_size) {
            let _ = sock.send(&pkt);
        }
        if let Some(d) = duration {
            if start.elapsed().as_secs_f64() >= d {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The glue invariant: whatever the client emits, it frames to exactly packet_size (so real and cover are
    // indistinguishable on the wire). Timing-independence itself is proven in asac-mix's tests.
    #[test]
    fn every_emitted_query_frames_to_packet_size() {
        let terms: Vec<&str> = FALLBACK_TERMS.split_whitespace().collect();
        let mut client = DecoyClient::new(&terms, 2.0, 7, 3).unwrap();
        client.enqueue_real("the real search");
        for _ in 0..50 {
            let (_delay, q) = client.next_query();
            let pkt = frame(q.as_bytes(), 512).expect("query fits in 512-byte packet");
            assert_eq!(pkt.len(), 512);
        }
    }
}
