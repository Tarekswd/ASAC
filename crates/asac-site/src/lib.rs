//! Search glue: the request handler that ties [`asac_index`] to [`asac_frontend`].
//!
//! This is the function a live front end (the arti onion service) calls per request: parse the query out of
//! the request target, run it against the BM25 index, map each hit to a renderable result, and return the
//! self-contained HTML page. It holds no state and does no I/O, so it is unit-tested offline.

use asac_frontend::{query_from_target, render_home, render_results, ResultItem};
use asac_index::AsacReader;

/// Max characters of body text shown as a result snippet.
const SNIPPET_CHARS: usize = 200;

/// Char-safe truncation with an ellipsis (never splits a UTF-8 code point).
fn snippet(body: &str) -> String {
    if body.chars().count() <= SNIPPET_CHARS {
        return body.to_string();
    }
    let mut out: String = body.chars().take(SNIPPET_CHARS).collect();
    out.push('…');
    out
}

/// Run `query` against the index and render the results page. (Rendering escapes all fields — see
/// [`asac_frontend`].)
pub fn search_page(index: &AsacReader, query: &str, limit: usize) -> tantivy::Result<String> {
    let items: Vec<ResultItem> = index
        .search_docs(query, limit)?
        .into_iter()
        .map(|h| ResultItem {
            title: if h.title.is_empty() { h.url.clone() } else { h.title },
            url: h.url,
            snippet: snippet(&h.body),
        })
        .collect();
    Ok(render_results(query, &items))
}

/// Handle a raw request target (e.g. `/search?q=rust`): the home page when there is no query, otherwise the
/// results page for it.
pub fn handle(index: &AsacReader, target: &str, limit: usize) -> tantivy::Result<String> {
    match query_from_target(target) {
        Some(q) => search_page(index, &q, limit),
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
        idx.add(&f, "Rust Language", "http://site.example/rust", "systems programming memory safety", "")
            .unwrap();
        idx.commit().unwrap();
        idx
    }

    #[test]
    fn query_target_renders_the_matching_result() {
        let idx = index();
        let html = handle(&idx.reader(), "/search?q=memory+safety", 10).unwrap();
        assert!(html.contains("<a href=\"http://site.example/rust\">Rust Language</a>"));
        assert!(html.contains("systems programming"), "snippet should show body text");
    }

    #[test]
    fn empty_target_shows_home_not_results() {
        let idx = index();
        let html = handle(&idx.reader(), "/", 10).unwrap();
        assert!(html.contains("<form"));
        assert!(!html.contains("<ol>"), "home page has no results list");
    }

    #[test]
    fn no_match_renders_empty_results_page() {
        let idx = index();
        let html = search_page(&idx.reader(), "zzzznotacorpusword", 10).unwrap();
        assert!(html.contains("No results"));
    }
}
