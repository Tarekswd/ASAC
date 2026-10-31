//! Pre-index content-safety filter for the ASAC search engine (PLAN.md 4.3 #2) — a **launch blocker**.
//!
//! Every crawled document passes through [`ContentFilter::check`] *before* it is indexed, and the filter
//! **fails closed**: anything it cannot clear is rejected, never indexed. Two layers:
//!
//! 1. Local blocklists — always on, cheap, necessary-not-sufficient:
//!    - **host blocklist** ([`ContentFilter::block_host`]): the code seam for the **manual abuse-report
//!      process** (PLAN 4.3 #2) — an operator takes down a reported onion/domain and every page under it stops
//!      being indexed;
//!    - exact content-hash and keyword blocklists.
//! 2. The authoritative [`HashMatchBackend`] — exact/perceptual hash matching against an industry
//!    known-illegal-content list (e.g. CSAM) via a vetted partner. **Required for production**, needs a legal
//!    agreement, and is deliberately NOT bundled here. The type system requires a backend to construct a
//!    filter, so you cannot run this without wiring one.
//!
//! This crate is not a substitute for legal review, a classifier, or a trust-and-safety process; it is the
//! code seam those processes plug into.

use std::collections::HashSet;

/// The decision for one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Block(String), // human-readable reason
}

/// The authoritative illegal-content check: hash matching against an industry known-bad list via a vetted
/// partner. Implement this against the partner's API. Not bundled (requires a legal agreement).
pub trait HashMatchBackend {
    /// Return true if the document with this content hash is known to be illegal.
    fn is_known_illegal(&self, content_hash: &[u8; 32]) -> bool;
}

/// A pre-index content-safety filter. Construct with the required partner backend, then optionally add local
/// blocklist entries.
pub struct ContentFilter<B: HashMatchBackend> {
    blocked_hosts: HashSet<String>, // stored lowercased; abuse-report takedowns
    blocked_hashes: HashSet<[u8; 32]>,
    blocked_terms: Vec<String>, // stored lowercased
    backend: B,
}

impl<B: HashMatchBackend> ContentFilter<B> {
    pub fn new(backend: B) -> Self {
        Self {
            blocked_hosts: HashSet::new(),
            blocked_hashes: HashSet::new(),
            blocked_terms: Vec::new(),
            backend,
        }
    }

    /// Block an entire host (the abuse-report / takedown seam): every URL on this host is refused. Pass a bare
    /// host (e.g. `"abc...xyz.onion"` or `"example.com"`), matched case-insensitively.
    pub fn block_host(mut self, host: &str) -> Self {
        self.blocked_hosts.insert(host.to_lowercase());
        self
    }

    /// Add an exact content hash (BLAKE3 of the document bytes) to the local blocklist.
    pub fn block_hash(mut self, hash: [u8; 32]) -> Self {
        self.blocked_hashes.insert(hash);
        self
    }

    /// Add a keyword to the local blocklist (matched case-insensitively as a substring of the document text).
    pub fn block_term(mut self, term: &str) -> Self {
        self.blocked_terms.push(term.to_lowercase());
        self
    }

    /// Decide whether a document at `url` may be indexed. Order: host blocklist (takedowns), exact-hash
    /// blocklist, partner hash match, then keyword blocklist. Returns [`Verdict::Allow`] only if every layer
    /// clears it.
    pub fn check(&self, url: &str, doc_bytes: &[u8], text: &str) -> Verdict {
        if let Some(host) = host_of(url) {
            if self.blocked_hosts.contains(&host.to_lowercase()) {
                return Verdict::Block(format!("blocked host: {host}"));
            }
        }
        let hash = *blake3::hash(doc_bytes).as_bytes();
        if self.blocked_hashes.contains(&hash) {
            return Verdict::Block("exact-hash blocklist".into());
        }
        if self.backend.is_known_illegal(&hash) {
            return Verdict::Block("partner: known illegal content".into());
        }
        let lower = text.to_lowercase();
        for term in &self.blocked_terms {
            if lower.contains(term.as_str()) {
                return Verdict::Block(format!("blocked term: {term}"));
            }
        }
        Verdict::Allow
    }
}

/// Extract the bare host from a URL string (no scheme/userinfo/port/path), without pulling in a URL parser.
fn host_of(url: &str) -> Option<&str> {
    let after_scheme = url.split_once("://")?.1;
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h); // drop any userinfo
    let host = host_port.split(':').next()?; // drop any port
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoKnownIllegal;
    impl HashMatchBackend for NoKnownIllegal {
        fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
            false
        }
    }

    struct Flags(Vec<[u8; 32]>);
    impl HashMatchBackend for Flags {
        fn is_known_illegal(&self, h: &[u8; 32]) -> bool {
            self.0.contains(h)
        }
    }

    const U: &str = "http://site.example/page";

    #[test]
    fn clean_document_is_allowed() {
        let f = ContentFilter::new(NoKnownIllegal);
        assert_eq!(f.check(U, b"hello world", "hello world"), Verdict::Allow);
    }

    #[test]
    fn exact_hash_blocklist_blocks() {
        let bad = b"forbidden bytes";
        let h = *blake3::hash(bad).as_bytes();
        let f = ContentFilter::new(NoKnownIllegal).block_hash(h);
        assert!(matches!(f.check(U, bad, "anything"), Verdict::Block(_)));
    }

    #[test]
    fn keyword_blocklist_is_case_insensitive() {
        let f = ContentFilter::new(NoKnownIllegal).block_term("Forbidden");
        assert!(matches!(
            f.check(U, b"x", "this is FORBIDDEN content"),
            Verdict::Block(_)
        ));
        assert_eq!(f.check(U, b"x", "this is fine"), Verdict::Allow);
    }

    #[test]
    fn partner_backend_blocks_known_illegal() {
        let bad = b"known-bad";
        let h = *blake3::hash(bad).as_bytes();
        let f = ContentFilter::new(Flags(vec![h]));
        assert!(matches!(f.check(U, bad, "innocuous text"), Verdict::Block(_)));
        assert_eq!(f.check(U, b"something else", "innocuous text"), Verdict::Allow);
    }

    #[test]
    fn host_blocklist_takes_down_every_page_on_the_host() {
        let f = ContentFilter::new(NoKnownIllegal).block_host("BAD.onion");
        // case-insensitive, and applies to any path/port on that host
        assert!(matches!(
            f.check("http://bad.onion/a", b"x", "clean"),
            Verdict::Block(_)
        ));
        assert!(matches!(
            f.check("http://bad.onion:8080/b?q=1", b"x", "clean"),
            Verdict::Block(_)
        ));
        assert_eq!(f.check("http://good.onion/a", b"x", "clean"), Verdict::Allow);
    }

    #[test]
    fn host_of_extracts_bare_host() {
        assert_eq!(host_of("http://h.example/a/b?x#y"), Some("h.example"));
        assert_eq!(host_of("https://user:pw@h.example:443/p"), Some("h.example"));
        assert_eq!(host_of("notaurl"), None);
    }
}
