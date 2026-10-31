//! ASAC front-end rendering core (PLAN.md 4.3 #5).
//!
//! Renders the results page as a single, self-contained HTML document: **no JavaScript, no third-party
//! resources, no cookies** (hard rules from PLAN 4.3 #5 and 2.3). The page is served as a v3 onion service
//! with Equi-X PoW — but that hosting (arti hidden service), the PoW (which comes from Arti/Tor, never
//! hand-rolled per 2.3), and the HTTP serving are the live I/O layer added when wiring to Tor. This crate is
//! the pure, testable part: turning an untrusted query + results into safe HTML, and reading the query out of
//! a request target.
//!
//! Everything dynamic is HTML-escaped, and result URLs are only turned into links when their scheme is
//! `http`/`https` — crawled data is untrusted, so a `javascript:` URL is rendered as inert text, never a link.

use url::Url;

/// One result row to render. Produced by the query service from the index; treated as untrusted here.
pub struct ResultItem {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// HTML-escape the five entities that matter in text and in single/double-quoted attribute values.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Read the search term `q` from a request target like `/search?q=hello+world`. Returns `None` when absent or
/// blank. Parsing reuses `url`'s query decoding (handles `+` and percent-encoding) by resolving the target
/// against a throwaway base.
pub fn query_from_target(target: &str) -> Option<String> {
    let base = Url::parse("http://local.invalid/").ok()?;
    let parsed = base.join(target).ok()?;
    for (k, v) in parsed.query_pairs() {
        if &*k == "q" {
            let q = v.into_owned();
            if !q.trim().is_empty() {
                return Some(q);
            }
        }
    }
    None
}

/// Link to `url` only if it is `http`/`https`; otherwise return the already-escaped label as inert text.
fn safe_anchor(url: &str, escaped_label: &str) -> String {
    match Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => {
            format!("<a href=\"{}\">{}</a>", escape(url), escaped_label)
        }
        _ => escaped_label.to_string(),
    }
}

fn page(query: &str, body: &str) -> String {
    // Inline style only; no external stylesheet, script, font, or image (no third-party resources).
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>ASAC</title>\n\
<style>body{{font-family:sans-serif;max-width:40rem;margin:2rem auto;padding:0 1rem}}\
.url{{color:#060;font-size:.85em;word-break:break-all}}li{{margin:1rem 0}}</style>\n\
</head>\n<body>\n\
<form action=\"/search\" method=\"get\">\n\
<input type=\"text\" name=\"q\" value=\"{q}\" aria-label=\"Search\">\n\
<button type=\"submit\">Search</button>\n\
</form>\n{body}\n</body>\n</html>\n",
        q = escape(query),
        body = body,
    )
}

/// The landing page: just the search form, no query run yet.
pub fn render_home() -> String {
    page("", "")
}

/// Render the results page for `query`. All dynamic values are escaped; non-http(s) result URLs are inert.
pub fn render_results(query: &str, results: &[ResultItem]) -> String {
    let body = if results.is_empty() {
        format!("<p>No results for &ldquo;{}&rdquo;.</p>", escape(query))
    } else {
        let mut list = String::from("<ol>\n");
        for r in results {
            let title = escape(&r.title);
            let title = if title.is_empty() { escape(&r.url) } else { title };
            list.push_str(&format!(
                "<li>{}<div class=\"url\">{}</div><p>{}</p></li>\n",
                safe_anchor(&r.url, &title),
                escape(&r.url),
                escape(&r.snippet),
            ));
        }
        list.push_str("</ol>\n");
        list
    };
    page(query, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_covers_all_five_entities() {
        assert_eq!(
            escape(r#"<a href="x" id='y'>&"#),
            "&lt;a href=&quot;x&quot; id=&#39;y&#39;&gt;&amp;"
        );
    }

    #[test]
    fn untrusted_query_and_results_cannot_inject_html_or_js() {
        let evil_q = r#"<script>alert(1)</script>" onmouseover="x"#;
        let results = [
            ResultItem {
                title: "<img src=x onerror=alert(1)>".to_string(),
                url: "javascript:alert(1)".to_string(),
                snippet: "safe".to_string(),
            },
            ResultItem {
                title: "Good Page".to_string(),
                url: "http://site.example/a".to_string(),
                snippet: "hello".to_string(),
            },
        ];
        let html = render_results(evil_q, &results);

        // No raw markup from untrusted input survived.
        assert!(!html.contains("<script>"), "query must be escaped");
        assert!(!html.contains("<img src=x"), "result title must be escaped");
        assert!(
            !html.contains(r#"onmouseover="x""#),
            "attribute breakout must be escaped"
        );
        // javascript: URL is never linked.
        assert!(
            !html.contains("href=\"javascript:"),
            "non-http URL must not become a link"
        );
        // The legitimate http result IS linked.
        assert!(html.contains("<a href=\"http://site.example/a\">Good Page</a>"));
        // No script tag at all (no-JS rule).
        assert!(
            !html.to_lowercase().contains("<script"),
            "page must contain no JavaScript"
        );
    }

    #[test]
    fn query_parsing_decodes_and_handles_missing() {
        assert_eq!(
            query_from_target("/search?q=hello+world").as_deref(),
            Some("hello world")
        );
        assert_eq!(query_from_target("/search?q=caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(query_from_target("/search?x=1").as_deref(), None);
        assert_eq!(query_from_target("/").as_deref(), None);
        assert_eq!(query_from_target("/search?q=%20%20").as_deref(), None); // blank -> None
    }

    #[test]
    fn empty_results_page_is_well_formed_and_scriptless() {
        let html = render_results("nothing here", &[]);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("No results"));
        assert!(!html.to_lowercase().contains("<script"));
    }
}
