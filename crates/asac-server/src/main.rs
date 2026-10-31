//! Serve ASAC as a v3 onion service (PLAN.md 4.3 #5).
//!
//! Two phases. First (synchronous) it crawls the seed URLs over Tor via `asac-fetch` and builds a
//! safety-gated index — `TorFetcher` owns its own runtime, so this finishes before any async work starts.
//! Then (async) it bootstraps a Tor client, launches an onion service, and answers each incoming stream with
//! the rendered HTML page from `asac-site`, served as plain HTTP over the onion circuit (the Tor layer is the
//! transport encryption; the page carries no JS, cookies, or third-party resources).
//!
//! Needs the live Tor network and a first-run bootstrap, so it is a manual binary, not a CI test. The request
//! parsing and response building are pure and unit-tested.
//!
//! shortcut: serves requests one at a time (no per-request task spawning) and serves on port 80 regardless of
//! the requested port; Equi-X PoW and graceful shutdown are not wired. Fine for a single-user prototype
//! (privacy over throughput); add concurrency + PoW config when load or abuse require it.
//!
//! Usage: `cargo run -p asac-server -- <nickname> <seed-http-url> [more seeds...]`

use std::process::exit;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use arti_client::{TorClient, TorClientConfig};
use asac_crawler::{crawl, CrawlConfig};
use asac_fetch::TorFetcher;
use asac_index::{AsacIndex, AsacReader, ContentFilter, HashMatchBackend};
use asac_rank::Authority;
use asac_site::handle;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use futures::StreamExt;
use safelog::DisplayRedacted;
use tor_cell::relaycell::msg::Connected;
use tor_hsservice::{config::OnionServiceConfigBuilder, handle_rend_requests, HsNickname};

/// No partner hash-match backend in this binary; the keyword/term filters still apply.
struct NoKnownIllegal;
impl HashMatchBackend for NoKnownIllegal {
    fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
        false
    }
}

const USER_AGENT: &str = "asac-crawler/0.0 (+https://github.com/Tarekswd/asac)";
const MAX_RESULTS: usize = 10;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        let prog = args.first().map(String::as_str).unwrap_or("asac-server");
        eprintln!("usage: {prog} <nickname> <seed-http-url> [more seeds...]");
        exit(2);
    }
    let nickname = args[1].clone();
    let seeds: Vec<&str> = args[2..].iter().map(String::as_str).collect();

    // Phase 1 (sync): crawl over Tor and build the index. TorFetcher owns its runtime; drop it before serving.
    eprintln!("bootstrapping Tor to crawl (first run is slow)...");
    let fetcher = TorFetcher::bootstrap(USER_AGENT)?;
    let cfg = CrawlConfig {
        max_pages: 50,
        ..CrawlConfig::default()
    };
    let docs = crawl(&fetcher, &seeds, &cfg);
    eprintln!("crawled {} page(s)", docs.len());
    drop(fetcher);

    // Persistent index per nickname: survives restarts, and add() upserts by URL so this re-crawl refreshes.
    let index_dir = std::path::PathBuf::from(format!("asac-index-{nickname}"));
    let mut index = AsacIndex::open_or_create(&index_dir)?;
    let filter = ContentFilter::new(NoKnownIllegal);
    for d in &docs {
        index.add(&filter, &d.title, &d.url, &d.text, &d.links.join("\n"))?;
    }
    index.commit()?;

    // Link-graph authority over the crawl, for ranking v2. Shared + swappable so a re-crawl can refresh it.
    let authority: SharedAuthority = Arc::new(RwLock::new(Arc::new(authority_from(&docs))));

    // A read handle for serving; cloned before the index (writer) moves to the re-crawl thread (if any).
    let reader = Arc::new(index.reader());

    // Optional scheduled re-crawl (ASAC_RECRAWL_SECS=N): refreshes the index in the background while serving;
    // tantivy readers see each commit and the authority table is recomputed. Without it, the index and
    // authority are the one-shot startup crawl.
    match recrawl_interval(std::env::var("ASAC_RECRAWL_SECS").ok().as_deref()) {
        Some(interval) => {
            eprintln!("scheduled re-crawl every {}s", interval.as_secs());
            spawn_recrawl(index, args[2..].to_vec(), interval, authority.clone());
        }
        None => drop(index),
    }

    // Phase 2 (async): serve as an onion service.
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(serve(reader, authority, &nickname))
}

/// A link-authority table shared across request tasks and swappable by the re-crawl thread.
type SharedAuthority = Arc<RwLock<Arc<Authority>>>;

/// Compute link-graph authority from crawled docs.
fn authority_from(docs: &[asac_crawler::CrawledDoc]) -> Authority {
    let pages: Vec<(String, Vec<String>)> = docs.iter().map(|d| (d.url.clone(), d.links.clone())).collect();
    Authority::from_pages(&pages)
}

/// Parse `ASAC_RECRAWL_SECS` into a re-crawl interval; `None` means crawl once (no re-crawl).
fn recrawl_interval(raw: Option<&str>) -> Option<Duration> {
    match raw.and_then(|s| s.trim().parse::<u64>().ok()) {
        Some(secs) if secs > 0 => Some(Duration::from_secs(secs)),
        _ => None,
    }
}

/// Re-crawl `seeds` every `interval` on a dedicated thread, upserting into `index` and recomputing `authority`;
/// serving readers (which share the underlying index) pick up each commit, and request tasks read the new
/// authority on their next request. Runs the sync `TorFetcher` off the async runtime.
fn spawn_recrawl(mut index: AsacIndex, seeds: Vec<String>, interval: Duration, authority: SharedAuthority) {
    thread::spawn(move || {
        let fetcher = match TorFetcher::bootstrap(USER_AGENT) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("re-crawl disabled: Tor bootstrap failed: {e}");
                return;
            }
        };
        let filter = ContentFilter::new(NoKnownIllegal);
        let cfg = CrawlConfig {
            max_pages: 50,
            ..CrawlConfig::default()
        };
        loop {
            thread::sleep(interval);
            let seed_refs: Vec<&str> = seeds.iter().map(String::as_str).collect();
            let docs = crawl(&fetcher, &seed_refs, &cfg);
            for d in &docs {
                if let Err(e) = index.add(&filter, &d.title, &d.url, &d.text, &d.links.join("\n")) {
                    eprintln!("re-crawl index error: {e}");
                }
            }
            if let Err(e) = index.commit() {
                eprintln!("re-crawl commit error: {e}");
            }
            // Refresh the shared authority from this crawl (brief write lock; readers clone the old Arc until).
            if let Ok(mut slot) = authority.write() {
                *slot = Arc::new(authority_from(&docs));
            }
            eprintln!("re-crawled {} page(s)", docs.len());
        }
    });
}

async fn serve(reader: Arc<AsacReader>, authority: SharedAuthority, nickname: &str) -> Result<()> {
    eprintln!("bootstrapping Tor to serve...");
    let client = TorClient::create_bootstrapped(TorClientConfig::default()).await?;

    let nick = HsNickname::new(nickname.to_string()).map_err(|e| anyhow!("invalid nickname: {e}"))?;
    let mut cfg_builder = OnionServiceConfigBuilder::default();
    cfg_builder.nickname(nick);
    #[cfg(feature = "pow")]
    cfg_builder.enable_pow(true); // require Equi-X proof-of-work under load (see the `pow` cargo feature)
    let svc_cfg = cfg_builder.build()?;
    let (service, requests) = client
        .launch_onion_service(svc_cfg)?
        .ok_or_else(|| anyhow!("onion service is disabled in the configuration"))?;

    match service.onion_address() {
        Some(addr) => eprintln!(
            "serving at http://{}/ (may take a few minutes to become reachable)",
            addr.display_unredacted()
        ),
        None => eprintln!("serving (onion address not yet available)"),
    }

    // handle_rend_requests accepts rendezvous requests and yields per-client stream requests. Serve each on
    // its own task so slow clients don't block others, and stop cleanly on Ctrl-C.
    let mut streams = Box::pin(handle_rend_requests(requests));
    loop {
        tokio::select! {
            maybe_req = streams.next() => {
                let Some(req) = maybe_req else { break };
                let reader = reader.clone();
                // Snapshot the current authority table for this request (a re-crawl may later swap it).
                let authority = authority.read().map(|a| a.clone()).unwrap_or_else(|_| Arc::new(Authority::empty()));
                tokio::spawn(async move {
                    let stream = match req.accept(Connected::new_empty()).await {
                        Ok(s) => s,
                        Err(e) => { eprintln!("accept failed: {e}"); return; }
                    };
                    if let Err(e) = serve_one(stream, &reader, &authority).await {
                        eprintln!("request failed: {e}");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => {
                eprintln!("shutting down (Ctrl-C)...");
                break;
            }
        }
    }
    drop(service); // stop advertising the onion service
    Ok(())
}

/// Read one request off the stream, render its results page, and write the HTTP response.
async fn serve_one<S>(mut stream: S, reader: &AsacReader, authority: &Authority) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let head = read_request_head(&mut stream).await?;
    let target = parse_request_target(&head).unwrap_or_else(|| "/".to_string());
    // Drop the underlying error detail: it could echo the query, and we never log queries (PLAN 4.3 #6).
    let html = handle(reader, &target, MAX_RESULTS, authority).map_err(|_| anyhow!("search failed"))?;
    stream.write_all(http_response(&html).as_bytes()).await?;
    stream.flush().await?; // Arti buffers a partial cell until flush.
                           // Graceful close (flush + end the stream) rather than an abrupt drop. An Arti onion-service stream may
                           // still END with a non-DONE reason on the client side regardless, so the fetcher tolerates that (asac-fetch).
    stream.close().await?;
    Ok(())
}

/// Read until the end of the HTTP request head (`\r\n\r\n`), EOF, or a sanity cap.
async fn read_request_head<S: AsyncRead + Unpin>(stream: &mut S) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > 8192 || buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    Ok(buf)
}

/// Extract the request target (path+query) from an HTTP request head: the 2nd token of the request line.
fn parse_request_target(head: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(head);
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    let _method = parts.next()?;
    parts.next().map(|t| t.to_string())
}

/// Wrap an HTML body in a minimal HTTP/1.1 `200` response with `Connection: close`.
fn http_response(html: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
Content-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_reads_the_request_line() {
        let head = b"GET /search?q=rust+lang HTTP/1.1\r\nHost: x.onion\r\n\r\n";
        assert_eq!(parse_request_target(head).as_deref(), Some("/search?q=rust+lang"));
    }

    #[test]
    fn parse_target_handles_garbage() {
        assert_eq!(parse_request_target(b""), None);
        assert_eq!(parse_request_target(b"GET"), None); // no target token
    }

    #[test]
    fn http_response_has_matching_content_length() {
        let body = "<html>hi</html>";
        let resp = http_response(body);
        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp.contains(&format!("Content-Length: {}\r\n", body.len())));
        assert!(resp.ends_with(body));
    }

    #[test]
    fn recrawl_interval_parsing() {
        assert_eq!(recrawl_interval(Some("3600")), Some(Duration::from_secs(3600)));
        assert_eq!(recrawl_interval(Some("  60 ")), Some(Duration::from_secs(60)));
        assert_eq!(recrawl_interval(Some("0")), None); // 0 disables
        assert_eq!(recrawl_interval(Some("nope")), None);
        assert_eq!(recrawl_interval(None), None);
    }

    #[test]
    fn nickname_and_config_build_offline() {
        // The one onion-service-config step reachable without a live network.
        let nick = HsNickname::new("asac".to_string()).expect("valid nickname");
        OnionServiceConfigBuilder::default()
            .nickname(nick)
            .build()
            .expect("config builds");
    }
}
