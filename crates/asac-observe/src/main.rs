//! F5 observer (sim/shadow): the global network observer. It records the arrival time and source of every
//! packet it receives into a CSV, so the analysis can test whether the searcher is distinguishable from the
//! cover-only clients (target: no better than chance, 1/N). It measures *timing*, not delivery, so UDP loss is
//! harmless.
//!
//! Usage: `asac-observe [--listen <addr>] [--out <file>]`  (defaults: 0.0.0.0:9999, stdout). Runs until killed
//! (Shadow stops it at `stop_time`).

use std::io::Write;
use std::net::UdpSocket;
use std::time::Instant;

/// One CSV row: milliseconds since the observer started, the sender's IP, and the packet length.
fn record_line(elapsed_ms: u128, src_ip: &str, len: usize) -> String {
    format!("{elapsed_ms},{src_ip},{len}\n")
}

fn main() -> std::io::Result<()> {
    let mut addr = "0.0.0.0:9999".to_string();
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            // `--listen` may be a bare flag (keep the default addr) or be followed by an address.
            "--listen" => {
                if let Some(v) = args.peek() {
                    if !v.starts_with("--") {
                        addr = args.next().unwrap();
                    }
                }
            }
            "--out" => out = args.next(),
            _ => {}
        }
    }

    let sock = UdpSocket::bind(&addr)?;
    let mut sink: Box<dyn Write> = match &out {
        Some(p) => Box::new(std::io::BufWriter::new(std::fs::File::create(p)?)),
        None => Box::new(std::io::stdout()),
    };
    eprintln!(
        "observer listening on {addr}, writing {}",
        out.as_deref().unwrap_or("stdout")
    );
    sink.write_all(b"elapsed_ms,src_ip,len\n")?;
    sink.flush()?;

    let start = Instant::now();
    let mut buf = [0u8; 4096];
    loop {
        let (n, src) = sock.recv_from(&mut buf)?;
        sink.write_all(record_line(start.elapsed().as_millis(), &src.ip().to_string(), n).as_bytes())?;
        sink.flush()?; // flush each row so a killed process still leaves a complete log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_line_is_csv() {
        assert_eq!(record_line(1234, "11.0.0.2", 512), "1234,11.0.0.2,512\n");
    }
}
