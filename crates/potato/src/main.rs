//! Potato Phase 0 smoke binary.
//!
//! Goal of this phase: prove the workspace depends on Arti and can reach the Tor
//! network through `arti-client`. The SOCKS proxy itself is Arti's own `arti proxy`
//! for now (arti-client does not yet expose an embeddable SOCKS API,
//! <https://docs.rs/arti-client>); Phase 1 wraps that proxy with Potato's policy layer.

use anyhow::Result;
use arti_client::{TorClient, TorClientConfig};
use futures::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Default config: real Tor network, state cached in the OS default directories.
    let config = TorClientConfig::default();

    eprintln!("bootstrapping Tor via arti-client (first run takes a while)...");
    let tor = TorClient::create_bootstrapped(config).await?;

    // Smoke test: fetch example.com over Tor and print its HTTP status line.
    let mut stream = tor.connect(("example.com", 80)).await?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n")
        .await?;
    stream.flush().await?; // Arti buffers; flush is required.

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let text = String::from_utf8_lossy(&buf);
    let status = text.lines().next().unwrap_or("<no response>");
    println!("reached example.com over Tor: {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use arti_client::TorClientConfig;

    /// Offline self-check: the default client config builds without touching the
    /// network. The full bootstrap in `main` needs real network and is run manually
    /// (`cargo run -p potato`), so it is deliberately not a CI test.
    #[test]
    fn default_config_builds() {
        let _config = TorClientConfig::default();
    }
}
