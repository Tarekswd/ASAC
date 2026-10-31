//! Search glue: the request handler that ties [`asac_index`] to [`asac_frontend`].
//!
//! This is the function a live front end (the arti onion service) calls per request: parse the query out of
//! the request target, run it against the BM25 index, map each hit to a renderable result, and return the
//! self-contained HTML page. It holds no state and does no I/O, so it is unit-tested offline.

use asac_frontend::{query_from_target, render_home, render_results, ResultItem};
use asac_index::AsacReader;
use asac_rank::{blend, freshness_factor, Authority};

/// Max characters of body text shown as a result snippet.
const SNIPPET_CHARS: usize = 200;

/// BM25 candidates to pull before re-ranking by authority (>= the page size shown).
const CANDIDATES: usize = 50;

/// How much a maximally-linked page's score is boosted by authority (see [`asac_rank::blend`]).
const AUTHORITY_WEIGHT: f32 = 0.5;

/// Freshness half-life: a page's freshness boost halves every ~30 days.
const FRESHNESS_HALF_LIFE_SECS: u64 = 30 * 24 * 3600;

/// Floor for the freshness multiplier: an ancient page keeps 60% of its score (demoted, not erased).
const FRESHNESS_FLOOR: f32 = 0.6;

/// Current time in Unix seconds (0 if before the epoch).
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Char-safe truncation with an ellipsis (never splits a UTF-8 code point).
fn snippet(body: &str) -> String {
    if body.chars().count() <= SNIPPET_CHARS {
        return body.to_string();
    }
    let mut out: String = body.chars().take(SNIPPET_CHARS).collect();
    out.push('…');
    out
}

/// Run `query` against the index, re-rank the top BM25 candidates by link authority, and render the results
/// page. Pass [`Authority::empty`] to rank by relevance alone. (Rendering escapes all fields — see
/// [`asac_frontend`].)
pub fn search_page(
    index: &AsacReader,
    query: &str,
    limit: usize,
    authority: &Authority,
) -> tantivy::Result<String> {
    let mut hits = index.search_docs(query, CANDIDATES.max(limit))?;
    let now = now_secs();
    // Score = BM25 x authority boost x freshness decay; highest first. (NaN-safe: equal keeps order.)
    let rank = |h: &asac_index::DocHit| {
        let age = now.saturating_sub(h.fetched_at);
        blend(h.score, authority.norm(&h.url), AUTHORITY_WEIGHT)
            * freshness_factor(age, FRESHNESS_HALF_LIFE_SECS, FRESHNESS_FLOOR)
    };
    hits.sort_by(|a, b| rank(b).partial_cmp(&rank(a)).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);

    let items: Vec<ResultItem> = hits
        .into_iter()
        .map(|h| ResultItem {
            title: if h.title.is_empty() {
                h.url.clone()
            } else {
                h.title
            },
            url: h.url,
            snippet: snippet(&h.body),
        })
        .collect();
    Ok(render_results(query, &items))
}

/// Handle a raw request target (e.g. `/search?q=rust`): the home page when there is no query, otherwise the
/// results page for it (re-ranked with `authority`).
pub fn handle(
    index: &AsacReader,
    target: &str,
    limit: usize,
    authority: &Authority,
) -> tantivy::Result<String> {
    match query_from_target(target) {
        Some(q) => search_page(index, &q, limit, authority),
        None => Ok(render_home()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asac_index::{AsacIndex, ContentFilter, HashMatchBackend};

    struct NoKnownIllegal;
    impl HashMatchBackend for NoKnownIllegal {
        fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
            false
        }
    }

    fn index() -> AsacIndex {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        idx.add(
            &f,
            "Rust Language",
            "http://site.example/rust",
            "systems programming memory safety",
            "",
        )
        .unwrap();
        idx.commit().unwrap();
        idx
    }

    #[test]
    fn query_target_renders_the_matching_result() {
        let idx = index();
        let html = handle(&idx.reader(), "/search?q=memory+safety", 10, &Authority::empty()).unwrap();
        assert!(html.contains("<a href=\"http://site.example/rust\">Rust Language</a>"));
        assert!(
            html.contains("systems programming"),
            "snippet should show body text"
        );
    }

    #[test]
    fn empty_target_shows_home_not_results() {
        let idx = index();
        let html = handle(&idx.reader(), "/", 10, &Authority::empty()).unwrap();
        assert!(html.contains("<form"));
        assert!(!html.contains("<ol>"), "home page has no results list");
    }

    #[test]
    fn no_match_renders_empty_results_page() {
        let idx = index();
        let html = search_page(&idx.reader(), "zzzznotacorpusword", 10, &Authority::empty()).unwrap();
        assert!(html.contains("No results"));
    }

    #[test]
    fn fresher_doc_wins_when_relevance_and_authority_tie() {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        let now = now_secs();
        let old = now.saturating_sub(365 * 24 * 3600); // a year old
                                                       // Same body (equal BM25), no links (equal authority); only recency differs.
        idx.add_at(&f, "Old", "http://old.example/", "widgets gadgets", "", old)
            .unwrap();
        idx.add_at(&f, "New", "http://new.example/", "widgets gadgets", "", now)
            .unwrap();
        idx.commit().unwrap();

        let html = search_page(&idx.reader(), "widgets gadgets", 10, &Authority::empty()).unwrap();
        let pos_new = html.find("http://new.example/").unwrap();
        let pos_old = html.find("http://old.example/").unwrap();
        assert!(
            pos_new < pos_old,
            "the fresher doc should rank first on a relevance/authority tie"
        );
    }

    #[test]
    fn authority_reorders_equally_relevant_results() {
        // Two docs with identical body text (equal BM25 for the query); one is linked by the other two.
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        idx.add(&f, "Popular", "http://a.example/", "widgets gadgets", "")
            .unwrap();
        idx.add(
            &f,
            "Obscure",
            "http://c.example/",
            "widgets gadgets",
            "http://a.example/",
        )
        .unwrap();
        idx.add(
            &f,
            "Linker",
            "http://b.example/",
            "unrelated",
            "http://a.example/",
        )
        .unwrap();
        idx.commit().unwrap();

        let pages = vec![
            ("http://a.example/".to_string(), vec![]),
            (
                "http://c.example/".to_string(),
                vec!["http://a.example/".to_string()],
            ),
            (
                "http://b.example/".to_string(),
                vec!["http://a.example/".to_string()],
            ),
        ];
        let authority = Authority::from_pages(&pages);
        let html = search_page(&idx.reader(), "widgets gadgets", 10, &authority).unwrap();
        let pos_a = html.find("http://a.example/").unwrap();
        let pos_c = html.find("http://c.example/").unwrap();
        assert!(
            pos_a < pos_c,
            "the linked-to page should rank above the equally-relevant obscure one"
        );
    }
}
