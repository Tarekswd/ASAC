//! Polite, robots-respecting crawl core for ASAC (PLAN.md 4.3 #1).
//!
//! Fetching is the one part that touches the network, and it differs by target: onion services use the Rust
//! `arti-client` path, clear-web uses Scrapling's plain HTTP fetcher over Tor (JS/stealth/anti-bot all off).
//! Both reduce to [`Page`], so this crate puts fetching behind the [`Fetcher`] trait and keeps the crawl logic
//! — robots.txt, scope, link extraction, the frontier — pure and unit-testable offline. The real Tor/Scrapling
//! backends are the I/O layer added when wiring to a live network; they can't be exercised without one.
//!
//! The crawler fetches HTML only, never runs JavaScript, and never downloads media (non-HTML responses are
//! dropped). Output is exactly what [`asac_index`](../asac_index/index.html) stores: title, URL, text, links.

use std::collections::{HashMap, HashSet, VecDeque};

use scraper::{Html, Selector};
use texting_robots::{get_robots_url, Robot};
use url::Url;

/// A fetched response. Onion (`arti-client`) and clear-web (Scrapling) backends both produce this.
pub struct Page {
    pub status: u16,
    pub content_type: String,
    pub body: String,
}

/// Why a fetch could not produce a [`Page`]. Real backends map Tor/HTTP failures onto these.
#[derive(Debug)]
pub enum FetchError {
    /// Transport failure (circuit failed, DNS, timeout, connection reset, ...).
    Network(String),
}

/// The network seam. Implemented by the onion (`arti-client`) and clear-web (Scrapling-over-Tor) backends;
/// the crawl logic only ever sees this trait, so it stays testable with an in-memory fake.
pub trait Fetcher {
    fn fetch(&self, url: &str) -> Result<Page, FetchError>;
}

/// Content extracted from one HTML page: the fields ASAC indexes.
pub struct Extracted {
    pub title: String,
    pub text: String,
    /// Absolute `http(s)` outgoing links (relative hrefs resolved against the page URL, fragments stripped).
    pub links: Vec<String>,
}

const NON_TEXT_TAGS: [&str; 4] = ["script", "style", "noscript", "template"];

/// Parse HTML into title, visible text, and absolute outgoing links. Text inside `<script>`/`<style>` (and
/// other non-rendered tags) is skipped so indexed text is human-visible content, never JavaScript source.
pub fn extract(base: &Url, html: &str) -> Extracted {
    let doc = Html::parse_document(html);

    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| doc.select(&sel).next())
        .map(|e| e.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    // Walk the body's text nodes, skipping any whose ancestry includes a non-text tag.
    let mut text = String::new();
    let body = Selector::parse("body").ok().and_then(|sel| doc.select(&sel).next());
    let root = body.map(|b| *b).unwrap_or_else(|| doc.tree.root());
    for node in root.descendants() {
        let Some(t) = node.value().as_text() else { continue };
        let mut skip = false;
        let mut cur = node.parent();
        while let Some(p) = cur {
            if let Some(el) = p.value().as_element() {
                if NON_TEXT_TAGS.contains(&el.name()) {
                    skip = true;
                    break;
                }
            }
            cur = p.parent();
        }
        if skip {
            continue;
        }
        let chunk = t.trim();
        if !chunk.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(chunk);
        }
    }

    let mut links = Vec::new();
    if let Ok(sel) = Selector::parse("a[href]") {
        for el in doc.select(&sel) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(mut u) = base.join(href) {
                    if matches!(u.scheme(), "http" | "https") {
                        u.set_fragment(None);
                        links.push(u.to_string());
                    }
                }
            }
        }
    }

    Extracted { title, text, links }
}

/// How a crawl is bounded. Defaults: polite, same-host, 1000 pages.
pub struct CrawlConfig {
    pub user_agent: String,
    pub max_pages: usize,
    pub same_host_only: bool,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self { user_agent: "asac-crawler".to_string(), max_pages: 1000, same_host_only: true }
    }
}

/// One crawled page, ready to hand to `asac_index::AsacIndex::add` (title, url, text, links).
pub struct CrawledDoc {
    pub url: String,
    pub title: String,
    pub text: String,
    pub links: Vec<String>,
}

/// Breadth-first crawl from `seeds`, fetching through `fetcher`. Respects each origin's `robots.txt`, stays in
/// scope (same host unless disabled), fetches HTML only, de-duplicates URLs, and stops at `max_pages`.
///
/// shortcut: robots crawl-delay is surfaced but wall-clock rate limiting and per-onion circuit isolation are
/// the live fetcher backend's job (not unit-testable here); an origin whose `robots.txt` can't be fetched is
/// treated as "allow all" rather than failing closed on 5xx. Upgrade both when wiring the arti-client backend.
pub fn crawl<F: Fetcher>(fetcher: &F, seeds: &[&str], cfg: &CrawlConfig) -> Vec<CrawledDoc> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<Url> = seeds.iter().filter_map(|s| Url::parse(s).ok()).collect();
    let mut robots: HashMap<String, Option<Robot>> = HashMap::new();
    let mut out = Vec::new();

    while let Some(url) = queue.pop_front() {
        if out.len() >= cfg.max_pages {
            break;
        }
        if !visited.insert(url.as_str().to_string()) {
            continue;
        }
        if !robots_allows(fetcher, &mut robots, &cfg.user_agent, &url) {
            continue;
        }
        let page = match fetcher.fetch(url.as_str()) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("fetch failed for {url}: {e:?}"); // surface transport failures instead of dropping silently
                continue;
            }
        };
        if page.status / 100 != 2 || !page.content_type.to_ascii_lowercase().contains("html") {
            continue; // HTML only; never follow or index non-HTML (media, etc.)
        }
        let ex = extract(&url, &page.body);
        for l in &ex.links {
            if let Ok(u) = Url::parse(l) {
                if cfg.same_host_only && u.host_str() != url.host_str() {
                    continue;
                }
                queue.push_back(u);
            }
        }
        out.push(CrawledDoc { url: url.to_string(), title: ex.title, text: ex.text, links: ex.links });
    }
    out
}

/// Check (and lazily fetch + cache) an origin's `robots.txt`. An origin with no fetchable `robots.txt` allows
/// everything (see the crawl `shortcut` note).
fn robots_allows<F: Fetcher>(
    fetcher: &F,
    cache: &mut HashMap<String, Option<Robot>>,
    user_agent: &str,
    url: &Url,
) -> bool {
    let key = url.origin().ascii_serialization();
    if !cache.contains_key(&key) {
        let robot = get_robots_url(url.as_str())
            .ok()
            .and_then(|ru| fetcher.fetch(&ru).ok())
            .filter(|p| p.status / 100 == 2)
            .and_then(|p| Robot::new(user_agent, p.body.as_bytes()).ok());
        cache.insert(key.clone(), robot);
    }
    match &cache[&key] {
        Some(r) => r.allowed(url.as_str()),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory fetcher: serves a fixed URL->page map; anything else is a network error.
    struct FakeFetcher {
        pages: HashMap<String, Page>,
    }
    impl FakeFetcher {
        fn new() -> Self {
            Self { pages: HashMap::new() }
        }
        fn add(&mut self, url: &str, content_type: &str, body: &str) {
            self.pages.insert(
                url.to_string(),
                Page { status: 200, content_type: content_type.to_string(), body: body.to_string() },
            );
        }
    }
    impl Fetcher for FakeFetcher {
        fn fetch(&self, url: &str) -> Result<Page, FetchError> {
            match self.pages.get(url) {
                Some(p) => Ok(Page {
                    status: p.status,
                    content_type: p.content_type.clone(),
                    body: p.body.clone(),
                }),
                None => Err(FetchError::Network(format!("no route to {url}"))),
            }
        }
    }

    #[test]
    fn extract_pulls_title_text_and_absolute_links_skipping_scripts() {
        let base = Url::parse("http://site.example/dir/page.html").unwrap();
        let html = r#"<html><head><title> Hello World </title>
            <style>.x{color:red}</style></head>
            <body><h1>Greetings</h1><p>Visible text here.</p>
            <script>var secret = "do not index";</script>
            <a href="next.html">next</a><a href="http://other.example/x">ext</a>
            <a href="mailto:a@b.c">mail</a></body></html>"#;
        let ex = extract(&base, html);
        assert_eq!(ex.title, "Hello World");
        assert!(ex.text.contains("Greetings") && ex.text.contains("Visible text here."));
        assert!(!ex.text.contains("secret"), "script text must not be indexed: {}", ex.text);
        assert!(!ex.text.contains("color:red"), "style text must not be indexed");
        assert!(ex.links.contains(&"http://site.example/dir/next.html".to_string()));
        assert!(ex.links.contains(&"http://other.example/x".to_string()));
        assert!(!ex.links.iter().any(|l| l.starts_with("mailto")), "non-http links dropped");
    }

    #[test]
    fn crawl_respects_robots_scope_and_cap() {
        let mut f = FakeFetcher::new();
        f.add("http://site.example/robots.txt", "text/plain", "User-Agent: *\nDisallow: /private");
        f.add("http://site.example/", "text/html",
            r#"<a href="/a.html">a</a><a href="/private/secret.html">p</a><a href="http://evil.example/">e</a>"#);
        f.add("http://site.example/a.html", "text/html", "<title>Page A</title><p>alpha</p>");
        f.add("http://site.example/private/secret.html", "text/html", "<p>should be skipped</p>");
        f.add("http://evil.example/", "text/html", "<p>off host</p>");

        let docs = crawl(&f, &["http://site.example/"], &CrawlConfig::default());
        let urls: Vec<&str> = docs.iter().map(|d| d.url.as_str()).collect();

        assert!(urls.contains(&"http://site.example/"));
        assert!(urls.contains(&"http://site.example/a.html"));
        assert!(!urls.iter().any(|u| u.contains("/private/")), "robots Disallow must be honored");
        assert!(!urls.iter().any(|u| u.contains("evil.example")), "same_host_only must keep us on host");
    }

    #[test]
    fn crawl_output_feeds_the_index() {
        use asac_index::{AsacIndex, ContentFilter, HashMatchBackend};

        struct NoKnownIllegal;
        impl HashMatchBackend for NoKnownIllegal {
            fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
                false
            }
        }

        let mut f = FakeFetcher::new();
        f.add("http://site.example/", "text/html",
            "<title>Rust Crawler</title><p>systems programming memory safety</p>");

        let docs = crawl(&f, &["http://site.example/"], &CrawlConfig::default());
        assert!(!docs.is_empty());

        let mut idx = AsacIndex::new().unwrap();
        let filter = ContentFilter::new(NoKnownIllegal);
        for d in &docs {
            idx.add(&filter, &d.title, &d.url, &d.text, &d.links.join("\n")).unwrap();
        }
        idx.commit().unwrap();

        let hits = idx.search("memory safety", 5).unwrap();
        assert!(hits[0].json.contains("Rust Crawler"), "crawled page must be searchable");
    }
}
