//! Private search: the end-to-end ASAC differentiator, in-process.
//!
//! It wires the BM25 index ([`asac_index`]) to single-server PIR ([`asac_pir`]). The server precomputes, for
//! each term in a **public** vocabulary, that term's top BM25 result, and lays those out as a fixed-width,
//! perfect-square record set (a "top-N result cache", PLAN.md 4.3 #6). A client maps its query term to a row
//! **locally** (the vocabulary is public) and fetches the result record via PIR, so **the server never learns
//! the query term**.
//!
//! shortcut: a precomputed top-1-per-term cache over a small vocabulary, fixed record width (results are
//! truncated/padded). Full private ranked retrieval over a large index is the scale-up/research path
//! (PLAN.md 4.4). PIR query privacy here is the empirically-tested mechanism, formal proof pending.

use asac_index::AsacIndex;
use asac_pir::single_server::{
    logical_query, preprocess_streaming, ClientState, Params, Permutation, Server, SingleServerDb,
};

/// Smallest perfect square >= n, and at least 4 (PIR needs sqrt(n) >= 2).
fn perfect_square_at_least(n: usize) -> usize {
    let mut s = (n as f64).sqrt().ceil() as usize;
    if s < 2 {
        s = 2;
    }
    s * s
}

/// A private-search instance: a public term directory, the server's PIR database, and the client's PIR hints.
pub struct PrivateSearch {
    directory: Vec<String>, // row -> term (public); empty string = padding row
    perm: Permutation,
    server_db: SingleServerDb,
    hints: ClientState,
}

impl PrivateSearch {
    /// Build from a populated, safety-gated index and a public vocabulary. Each term's top BM25 result is
    /// precomputed into a fixed-`width` record; rows are padded to a perfect square, permuted, and streamed
    /// into PIR hints.
    pub fn build(index: &AsacIndex, vocab: &[&str], width: usize, seed: u64) -> tantivy::Result<Self> {
        let mut records: Vec<Vec<u8>> = Vec::new();
        let mut directory: Vec<String> = Vec::new();
        for &term in vocab {
            let hits = index.search(term, 1)?;
            let payload = hits.first().map(|h| h.json.clone()).unwrap_or_default();
            let mut rec = payload.into_bytes();
            rec.truncate(width);
            rec.resize(width, 0);
            records.push(rec);
            directory.push(term.to_string());
        }
        // Pad to a perfect-square row count with empty records.
        let n = perfect_square_at_least(records.len());
        while records.len() < n {
            records.push(vec![0u8; width]);
            directory.push(String::new());
        }

        let perm = Permutation::from_seed(n, seed);
        let mut permuted = vec![Vec::new(); n];
        for (logical, rec) in records.iter().enumerate() {
            permuted[perm.physical(logical)] = rec.clone();
        }
        let server_db = SingleServerDb::from_records(&permuted);
        let s = (n as f64).sqrt() as usize;
        let hints =
            preprocess_streaming(n, width, Params::for_sqrt_n(s), seed.wrapping_add(1), permuted.iter().cloned());

        Ok(Self { directory, perm, server_db, hints })
    }

    /// Privately search for `term`: map it to a row locally, then PIR-fetch the cached result. Returns the
    /// stored result (JSON) or `None` if the term is not in the public vocabulary / has no result. The server
    /// never receives `term`. Consumes one PIR query from the hint pool.
    pub fn search(&mut self, term: &str) -> Option<String> {
        let row = self.directory.iter().position(|t| t == term)?;
        let server = Server::new(&self.server_db);
        let rec = logical_query(&mut self.hints, &self.perm, row, &server);
        let text = String::from_utf8_lossy(&rec).trim_end_matches('\0').to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asac_index::{ContentFilter, HashMatchBackend};

    struct NoKnownIllegal;
    impl HashMatchBackend for NoKnownIllegal {
        fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
            false
        }
    }

    fn populated_index() -> AsacIndex {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        idx.add(&f, "Weather Today", "http://x/weather", "sunny forecast rain clouds temperature", "")
            .unwrap();
        idx.add(&f, "Pasta Recipe", "http://x/pasta", "boil pasta tomato sauce garlic basil", "").unwrap();
        idx.add(&f, "Rust Language", "http://x/rust", "systems programming memory safety ownership", "")
            .unwrap();
        idx.commit().unwrap();
        idx
    }

    #[test]
    fn private_search_returns_cached_result() {
        let idx = populated_index();
        let vocab = ["weather", "pasta", "rust"];
        let mut ps = PrivateSearch::build(&idx, &vocab, 256, 7).unwrap();

        let r = ps.search("pasta").expect("a result for 'pasta'");
        assert!(r.contains("Pasta Recipe"), "fetched the wrong cached result: {r}");

        let r2 = ps.search("rust").expect("a result for 'rust'");
        assert!(r2.contains("Rust Language"), "fetched the wrong cached result: {r2}");
    }

    #[test]
    fn unknown_term_is_not_in_the_public_vocabulary() {
        let idx = populated_index();
        let mut ps = PrivateSearch::build(&idx, &["weather"], 256, 1).unwrap();
        assert!(ps.search("not-in-vocab").is_none());
    }
}
