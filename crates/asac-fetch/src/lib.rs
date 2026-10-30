//! Live [`Fetcher`](asac_crawler::Fetcher) backend: fetches HTTP over Tor with `arti-client`.
//!
//! This is the real network layer behind the crawler's trait seam. It connects through a bootstrapped
//! `TorClient` (onion addresses and clear-web hosts alike), writes a minimal HTTP/1.1 `GET`, and parses the
//! response. `arti-client` is async and the [`Fetcher`] trait is sync, so each fetch is driven on an owned
//! Tokio runtime via `block_on` — fine because the crawler drives the fetcher from an ordinary (non-async)
//! thread. The HTTP request/response handling follows the raw-stream pattern proven in `crates/potato`.
//!
//! Follows up to [`MAX_REDIRECTS`] HTTP redirects (3xx `Location`), so an `http`->`https` redirect reaches the
//! real page instead of being dropped.
//!
//! shortcut: `http` and `https` (TLS over the Tor stream via rustls/ring, Mozilla roots from `webpki-roots`);
//! bodies are de-chunked, then gzip/deflate-decompressed, then read as UTF-8. Brotli/zstd `Content-Encoding`
//! is passed through undecoded — add it if a target needs it.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use arti_client::{TorClient, TorClientConfig};
use asac_crawler::{FetchError, Fetcher, Page};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{rustls, TlsConnector};
use tor_rtcompat::PreferredRuntime;
use url::Url;

/// A crawler [`Fetcher`] backed by a bootstrapped Tor client. Keeps its Tokio runtime alive for the client's
/// background tasks. (`create_bootstrapped` hands back an `Arc`-wrapped client.)
pub struct TorFetcher {
    rt: tokio::runtime::Runtime,
    client: Arc<TorClient<PreferredRuntime>>,
    tls: TlsConnector,
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
        Ok(Self { rt, client, tls: tls_connector(), user_agent: user_agent.to_string() })
    }

    async fn get(
        client: &TorClient<PreferredRuntime>,
        tls: &TlsConnector,
        ua: &str,
        url: &Url,
    ) -> Result<Page> {
        let mut current = url.clone();
        for _hop in 0..MAX_REDIRECTS {
            let raw = fetch_once(client, tls, ua, &current).await?;
            let page = parse_response(&raw);
            if (300..400).contains(&page.status) {
                if let Some(next) = redirect_location(&raw, &current) {
                    current = next;
                    continue;
                }
            }
            return Ok(page);
        }
        Err(anyhow!("too many redirects (>{MAX_REDIRECTS}) starting at {url}"))
    }
}

/// Maximum HTTP redirects to follow before giving up.
const MAX_REDIRECTS: usize = 5;

/// One request/response over Tor (plain or TLS), returning the raw response bytes.
async fn fetch_once(
    client: &TorClient<PreferredRuntime>,
    tls: &TlsConnector,
    ua: &str,
    url: &Url,
) -> Result<Vec<u8>> {
    let host = url.host_str().ok_or_else(|| anyhow!("url has no host"))?;
    let mut path = url.path().to_string();
    if let Some(q) = url.query() {
        path.push('?');
        path.push_str(q);
    }
    if path.is_empty() {
        path.push('/');
    }

    match url.scheme() {
        "http" => {
            let stream = client.connect((host, url.port().unwrap_or(80))).await.context("tor connect")?;
            http_exchange(stream, host, &path, ua).await
        }
        "https" => {
            let stream = client.connect((host, url.port().unwrap_or(443))).await.context("tor connect")?;
            let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
                .map_err(|e| anyhow!("bad TLS server name {host}: {e}"))?;
            let tls_stream = tls.connect(server_name, stream).await.context("TLS handshake")?;
            http_exchange(tls_stream, host, &path, ua).await
        }
        other => Err(anyhow!("unsupported scheme {other:?}")),
    }
}

/// The redirect target of a 3xx response: its `Location` resolved against the request URL, `http(s)` only.
fn redirect_location(raw: &[u8], base: &Url) -> Option<Url> {
    let text = String::from_utf8_lossy(raw);
    let head = text.split_once("\r\n\r\n").map_or(text.as_ref(), |(h, _)| h);
    for line in head.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("location") {
                let u = base.join(v.trim()).ok()?;
                return matches!(u.scheme(), "http" | "https").then_some(u);
            }
        }
    }
    None
}

impl Fetcher for TorFetcher {
    fn fetch(&self, url: &str) -> Result<Page, FetchError> {
        let parsed = Url::parse(url).map_err(|e| FetchError::Network(format!("bad url {url}: {e}")))?;
        self.rt
            .block_on(Self::get(&self.client, &self.tls, &self.user_agent, &parsed))
            .map_err(|e| FetchError::Network(e.to_string()))
    }
}

/// Write a minimal HTTP/1.1 `GET` and read the whole raw response. Generic over the stream so it serves both
/// the raw Tor `DataStream` (http) and a TLS stream over it (https).
async fn http_exchange<S>(mut stream: S, host: &str, path: &str, ua: &str) -> Result<Vec<u8>>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {ua}\r\n\
Accept: text/html\r\nAccept-Encoding: gzip, deflate\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.context("send request")?;
    stream.flush().await.context("flush request")?; // Arti buffers a partial cell until flush.
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.context("read response")?;
    Ok(buf)
}

/// A TLS client connector trusting the Mozilla root store, using the pure-Rust `ring` crypto provider.
fn tls_connector() -> TlsConnector {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
}

/// Find the first occurrence of `needle` in `hay`.
fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Parse a raw HTTP/1.1 response into a [`Page`]: status code, `Content-Type`, and body. The body is handled
/// as bytes (de-chunked, then gzip/deflate-decompressed) before a lossy UTF-8 conversion, so binary
/// compressed bodies survive. Pure, so it is unit-tested without a network.
fn parse_response(raw: &[u8]) -> Page {
    let (head_bytes, body_bytes) = match find_subslice(raw, b"\r\n\r\n") {
        Some(i) => (&raw[..i], &raw[i + 4..]),
        None => (raw, &[][..]),
    };
    let head = String::from_utf8_lossy(head_bytes); // headers are ASCII

    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok())
        .unwrap_or(0);

    let mut content_type = String::new();
    let mut chunked = false;
    let mut encoding = String::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-type" => content_type = v.trim().to_string(),
                "transfer-encoding" if v.to_ascii_lowercase().contains("chunked") => chunked = true,
                "content-encoding" => encoding = v.trim().to_ascii_lowercase(),
                _ => {}
            }
        }
    }

    let dechunked = if chunked { dechunk(body_bytes) } else { body_bytes.to_vec() };
    let decoded = decompress(&dechunked, &encoding);
    let body = String::from_utf8_lossy(&decoded).into_owned();
    Page { status, content_type, body }
}

/// Decode an HTTP/1.1 chunked transfer body (chunk = hex size line, CRLF, bytes, CRLF; terminated by a 0 size).
fn dechunk(mut s: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(nl) = find_subslice(s, b"\r\n") {
        let size_field = s[..nl].split(|&b| b == b';').next().unwrap_or(&[]); // drop any chunk extension
        let size = std::str::from_utf8(size_field)
            .ok()
            .and_then(|h| usize::from_str_radix(h.trim(), 16).ok())
            .unwrap_or(0);
        s = &s[nl + 2..];
        if size == 0 {
            break;
        }
        let take = size.min(s.len());
        out.extend_from_slice(&s[..take]);
        s = &s[take..];
        if take < size {
            break; // truncated
        }
        if s.starts_with(b"\r\n") {
            s = &s[2..];
        }
    }
    out
}

/// Decompress a body per its `Content-Encoding` (`gzip`/`deflate`); other encodings (incl. `br`/`zstd`) and
/// identity pass through unchanged. On any decode error the original bytes are returned rather than lost.
///
/// shortcut: handles gzip and deflate (the common two over Tor); brotli/zstd are passed through undecoded —
/// add them if a target needs them.
fn decompress(body: &[u8], encoding: &str) -> Vec<u8> {
    use std::io::Read;
    let read_all = |mut r: Box<dyn Read>| -> Option<Vec<u8>> {
        let mut out = Vec::new();
        r.read_to_end(&mut out).ok().map(|_| out)
    };
    match encoding {
        "gzip" | "x-gzip" => read_all(Box::new(flate2::read::GzDecoder::new(body))).unwrap_or_else(|| body.to_vec()),
        // HTTP "deflate" is usually zlib-wrapped; fall back to raw deflate.
        "deflate" => read_all(Box::new(flate2::read::ZlibDecoder::new(body)))
            .or_else(|| read_all(Box::new(flate2::read::DeflateDecoder::new(body))))
            .unwrap_or_else(|| body.to_vec()),
        _ => body.to_vec(),
    }
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

    #[test]
    fn decodes_gzip_body() {
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(b"<html>hello gzip</html>").unwrap();
        let gz = enc.finish().unwrap();

        let mut raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Encoding: gzip\r\n\r\n".to_vec();
        raw.extend_from_slice(&gz);

        let p = parse_response(&raw);
        assert_eq!(p.status, 200);
        assert_eq!(p.body, "<html>hello gzip</html>");
    }

    #[test]
    fn decodes_chunked_then_gzip() {
        // Realistic combo: chunked transfer encoding wrapping a gzip body.
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(b"chunked and gzipped").unwrap();
        let gz = enc.finish().unwrap();
        let mut chunk = format!("{:x}\r\n", gz.len()).into_bytes();
        chunk.extend_from_slice(&gz);
        chunk.extend_from_slice(b"\r\n0\r\n\r\n");

        let mut raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nTransfer-Encoding: chunked\r\nContent-Encoding: gzip\r\n\r\n".to_vec();
        raw.extend_from_slice(&chunk);

        assert_eq!(parse_response(&raw).body, "chunked and gzipped");
    }

    #[test]
    fn redirect_location_resolves_absolute_and_relative() {
        let base = Url::parse("http://example.com/old").unwrap();

        let raw_abs = b"HTTP/1.1 301 Moved\r\nLocation: https://example.com/new\r\n\r\n";
        assert_eq!(redirect_location(raw_abs, &base).unwrap().as_str(), "https://example.com/new");

        let raw_rel = b"HTTP/1.1 302 Found\r\nlocation: /elsewhere\r\n\r\n";
        assert_eq!(redirect_location(raw_rel, &base).unwrap().as_str(), "http://example.com/elsewhere");
    }

    #[test]
    fn redirect_location_rejects_non_http_schemes_and_missing_header() {
        let base = Url::parse("http://example.com/").unwrap();
        let ftp = b"HTTP/1.1 301 Moved\r\nLocation: ftp://example.com/x\r\n\r\n";
        assert!(redirect_location(ftp, &base).is_none(), "non-http(s) redirect target must be refused");
        let none = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\nbody";
        assert!(redirect_location(none, &base).is_none());
    }
}
