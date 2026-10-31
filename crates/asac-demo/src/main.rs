//! Live end-to-end demo of the ASAC pipeline.
//!
//! Crawls a seed URL over Tor (via `asac-fetch`), indexes what it finds behind the safety filter, then answers
//! one query and prints the rendered HTML results page. This exercises the real Fetcher and the search glue
//! against the live Tor network; it needs network access and a first-run circuit bootstrap, so it is a manual
//! binary, not a CI test.
//!
//! Usage: `cargo run -p asac-demo -- <seed-http-url> <query>`

use asac_crawler::{crawl, CrawlConfig};
use asac_fetch::TorFetcher;
use asac_index::{AsacIndex, ContentFilter, HashMatchBackend};
use asac_site::search_page;

/// No partner hash-match backend wired in this demo; the keyword/term filters still apply.
struct NoKnownIllegal;
impl HashMatchBackend for NoKnownIllegal {
    fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
        false
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <seed-http-url> <query>", args.first().map(String::as_str).unwrap_or("asac-demo"));
        std::process::exit(2);
    }
    let seed = &args[1];
    let query = &args[2];

    eprintln!("bootstrapping Tor (first run builds a circuit; this is slow)...");
    let fetcher = TorFetcher::bootstrap("asac-crawler/0.0 (+https://github.com/Tarekswd/asac)")?;

    eprintln!("crawling {seed} over Tor...");
    let cfg = CrawlConfig { max_pages: 20, ..CrawlConfig::default() };
    let docs = crawl(&fetcher, &[seed.as_str()], &cfg);
    eprintln!("crawled {} page(s)", docs.len());

    let mut index = AsacIndex::new()?;
    let filter = ContentFilter::new(NoKnownIllegal);
    for d in &docs {
        index.add(&filter, &d.title, &d.url, &d.text, &d.links.join("\n"))?;
    }
    index.commit()?;

    // Link-graph authority over what we crawled, for ranking v2.
    let pages: Vec<(String, Vec<String>)> = docs.iter().map(|d| (d.url.clone(), d.links.clone())).collect();
    let authority = asac_rank::Authority::from_pages(&pages);

    eprintln!("answering query {query:?}...");
    print!("{}", search_page(&index.reader(), query, 10, &authority)?);
    Ok(())
}
