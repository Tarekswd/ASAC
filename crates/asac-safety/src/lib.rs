//! Pre-index content-safety filter for the ASAC search engine (PLAN.md 4.3 #2) — a **launch blocker**.
//!
//! Every crawled document passes through [`ContentFilter::check`] *before* it is indexed, and the filter
//! **fails closed**: anything it cannot clear is rejected, never indexed. Two layers:
//!
//! 1. Local blocklists (exact content-hash + keyword) — always on, cheap, but necessary-not-sufficient.
//! 2. The authoritative [`HashMatchBackend`] — exact/perceptual hash matching against an industry
//!    known-illegal-content list (e.g. CSAM) via a vetted partner. **Required for production**, needs a legal
//!    agreement, and is deliberately NOT bundled here. The type system requires a backend to construct a
//!    filter, so you cannot run this without wiring one.
//!
//! This crate is not a substitute for legal review or a trust-and-safety process; it is the code seam those
//! processes plug into.

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
    blocked_hashes: HashSet<[u8; 32]>,
    blocked_terms: Vec<String>, // stored lowercased
    backend: B,
}

impl<B: HashMatchBackend> ContentFilter<B> {
    pub fn new(backend: B) -> Self {
        Self { blocked_hashes: HashSet::new(), blocked_terms: Vec::new(), backend }
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

    /// Decide whether a document may be indexed. Order: exact-hash blocklist, partner hash match, then keyword
    /// blocklist. Returns [`Verdict::Allow`] only if every layer clears it.
    pub fn check(&self, doc_bytes: &[u8], text: &str) -> Verdict {
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

    #[test]
    fn clean_document_is_allowed() {
        let f = ContentFilter::new(NoKnownIllegal);
        assert_eq!(f.check(b"hello world", "hello world"), Verdict::Allow);
    }

    #[test]
    fn exact_hash_blocklist_blocks() {
        let bad = b"forbidden bytes";
        let h = *blake3::hash(bad).as_bytes();
        let f = ContentFilter::new(NoKnownIllegal).block_hash(h);
        assert!(matches!(f.check(bad, "anything"), Verdict::Block(_)));
    }

    #[test]
    fn keyword_blocklist_is_case_insensitive() {
        let f = ContentFilter::new(NoKnownIllegal).block_term("Forbidden");
        assert!(matches!(f.check(b"x", "this is FORBIDDEN content"), Verdict::Block(_)));
        assert_eq!(f.check(b"x", "this is fine"), Verdict::Allow);
    }

    #[test]
    fn partner_backend_blocks_known_illegal() {
        let bad = b"known-bad";
        let h = *blake3::hash(bad).as_bytes();
        let f = ContentFilter::new(Flags(vec![h]));
        assert!(matches!(f.check(bad, "innocuous text"), Verdict::Block(_)));
        assert_eq!(f.check(b"something else", "innocuous text"), Verdict::Allow);
    }
}
