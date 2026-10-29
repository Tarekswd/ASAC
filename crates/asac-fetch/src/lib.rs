//! Live [`Fetcher`](asac_crawler::Fetcher) backend: fetches HTTP over Tor with `arti-client`.
//!
//! This is the real network layer behind the crawler's trait seam. It connects through a bootstrapped
//! `TorClient` (onion addresses and clear-web hosts alike), writes a minimal HTTP/1.1 `GET`, and parses the
//! response. `arti-client` is async and the [`Fetcher`] trait is sync, so each fetch is driven on an owned
//! Tokio runtime via `block_on` — fine because the crawler drives the fetcher from an ordinary (non-async)
//! thread. The HTTP request/response handling follows the raw-stream pattern proven in `crates/potato`.
//!
//! shortcut: `http` only (TLS/`https` would need a rustls layer over the Tor stream — add when a target needs
//! it); response bodies are decoded as UTF-8 and de-chunked only for `Transfer-Encoding: chunked`, with no
//! gzip/deflate handling. Onion services and many clear-web pages serve plain `http` over the Tor circuit, so
//! this is enough to crawl live; richer transport is the upgrade.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use arti_client::{TorClient, TorClientConfig};
use asac_crawler::{FetchError, Fetcher, Page};
use futures::io::{AsyncReadExt, AsyncWriteExt};
use tor_rtcompat::PreferredRuntime;
use url::Url;

/// A crawler [`Fetcher`] backed by a bootstrapped Tor client. Keeps its Tokio runtime alive for the client's
/// background tasks. (`create_bootstrapped` hands back an `Arc`-wrapped client.)
pub struct TorFetcher {
    rt: tokio::runtime::Runtime,
    client: Arc<TorClient<PreferredRuntime>>,
    user_agent: String,
}

impl TorFetcher {
    /// Bootstrap a Tor client on a fresh multi-thread Tokio runtime (first run is slow; it builds a circuit).
    pub fn bootstrap(user_agent: &str) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("build tokio runtime")?;
        let client = rt
            .block_on(TorClient::create_bootstrapped(TorClientConfig::default()))
            .context("bootstrap Tor client")?;
        Ok(Self { rt, client, user_agent: user_agent.to_string() })
    }

    async fn get(client: &TorClient<PreferredRuntime>, ua: &str, url: &Url) -> Result<Page> {
        if url.scheme() != "http" {
            return Err(anyhow!("only http is supported, got scheme {:?}", url.scheme()));
        }
        let host = url.host_str().ok_or_else(|| anyhow!("url has no host"))?;
        let port = url.port().unwrap_or(80);

        let mut path = url.path().to_string();
        if let Some(q) = url.query() {
            path.push('?');
            path.push_str(q);
        }
        if path.is_empty() {
            path.push('/');
        }

        let mut stream = client.connect((host, port)).await.context("tor connect")?;
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {ua}\r\n\
Accept: text/html\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).await.context("send request")?;
        stream.flush().await.context("flush request")?; // Arti buffers; flush is required.

        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.context("read response")?;
        Ok(parse_response(&buf))
    }
}

impl Fetcher for TorFetcher {
    fn fetch(&self, url: &str) -> Result<Page, FetchError> {
        let parsed = Url::parse(url).map_err(|e| FetchError::Network(format!("bad url {url}: {e}")))?;
        self.rt
            .block_on(Self::get(&self.client, &self.user_agent, &parsed))
            .map_err(|e| FetchError::Network(e.to_string()))
    }
}

/// Parse a raw HTTP/1.1 response into a [`Page`]: status code, `Content-Type`, and body (de-chunked if the
/// response used chunked transfer encoding). Pure, so it is unit-tested without a network.
fn parse_response(raw: &[u8]) -> Page {
    let text = String::from_utf8_lossy(raw);
    let (head, body_raw) = text.split_once("\r\n\r\n").unwrap_or((text.as_ref(), ""));

    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);

    let mut content_type = String::new();
    let mut chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-type" => content_type = v.trim().to_string(),
                "transfer-encoding" if v.to_ascii_lowercase().contains("chunked") => chunked = true,
                _ => {}
            }
        }
    }

    let body = if chunked { dechunk(body_raw) } else { body_raw.to_string() };
    Page { status, content_type, body }
}

/// Decode an HTTP/1.1 chunked body. Assumes UTF-8 (chunk sizes count bytes; `String::from_utf8_lossy` keeps
/// byte alignment for valid UTF-8, which HTML almost always is — see the module `shortcut` note).
fn dechunk(mut s: &str) -> String {
    let mut out = String::new();
    while let Some(nl) = s.find("\r\n") {
        let size = usize::from_str_radix(s[..nl].split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        s = &s[nl + 2..];
        if size == 0 {
            break;
        }
        if s.len() < size {
            out.push_str(s);
            break;
        }
        out.push_str(&s[..size]);
        s = &s[size..];
        s = s.strip_prefix("\r\n").unwrap_or(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html>hi</html>";
        let p = parse_response(raw);
        assert_eq!(p.status, 200);
        assert!(p.content_type.contains("text/html"));
        assert_eq!(p.body, "<html>hi</html>");
    }

    #[test]
    fn parses_chunked_response() {
        // "Wiki" + "pedia" in two chunks, terminated by a zero-length chunk.
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nTransfer-Encoding: chunked\r\n\r\n\
4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        let p = parse_response(raw);
        assert_eq!(p.status, 200);
        assert_eq!(p.body, "Wikipedia");
    }

    #[test]
    fn parses_non_200_status() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n\r\nnope";
        let p = parse_response(raw);
        assert_eq!(p.status, 404);
        assert_eq!(p.body, "nope");
    }
}
