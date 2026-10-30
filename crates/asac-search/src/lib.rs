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
//!
//! [`PrivateKeyword`] is the next step (FLAGSHIP-DESIGN.md F1): PIR-by-keyword with **no public term
//! directory** — the client hashes a secret term to a bucket and PIR-fetches it, so the server learns neither
//! the bucket nor the term, and every response is the same fixed size.

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

// ---------------------------------------------------------------------------
// F1 (FLAGSHIP-DESIGN.md): PIR-by-keyword over PIANO.
//
// Unlike `PrivateSearch`, this does NOT publish a term->row directory. The client hashes a term to a bucket
// index and PIR-fetches that bucket; PIR hides *which* bucket is fetched, so the server never learns the
// bucket and hence never learns the term (Chor-Gilboa-Naor: a conventional search structure composed with an
// underlying PIR scheme [Cryptology ePrint 1998/003]). Each bucket is a fixed-width list of
// (term-fingerprint, posting-list) entries, so every response is identical in size regardless of the term or
// how popular it is -- no popularity side channel.
//
// shortcut: fixed bucket capacity ([`MAX_ENTRIES_PER_BUCKET`]) and posting length ([`MAX_DOCS_PER_TERM`]);
// entries beyond those are dropped at build time. Grow the buckets, or add a padded overflow chain, when a
// real corpus needs it (FLAGSHIP-DESIGN.md F2/F3). Query-privacy is the empirically-tested mechanism; the
// formal argument and external review are the F6 gate.

const FP_LEN: usize = 8;
const MAX_DOCS_PER_TERM: usize = 16;
const MAX_ENTRIES_PER_BUCKET: usize = 8;
const ENTRY_LEN: usize = FP_LEN + 1 + MAX_DOCS_PER_TERM * 4; // fingerprint + count + doc-ids (u32 LE)
const BUCKET_WIDTH: usize = MAX_ENTRIES_PER_BUCKET * ENTRY_LEN;

/// A private keyword index: term -> posting list (doc ids), retrievable without the server learning the term.
pub struct PrivateKeyword {
    n_buckets: usize, // perfect square (PIANO requirement)
    perm: Permutation,
    server_db: SingleServerDb,
    hints: ClientState,
}

/// term -> (bucket index, 8-byte fingerprint). The hash is public; PIR is what hides which bucket is fetched.
fn locate(term: &str, n_buckets: usize) -> (usize, [u8; FP_LEN]) {
    let h = blake3::hash(term.as_bytes());
    let h = h.as_bytes();
    let bucket = (u64::from_le_bytes(h[0..8].try_into().unwrap()) % n_buckets as u64) as usize;
    let mut fp = [0u8; FP_LEN];
    fp.copy_from_slice(&h[8..8 + FP_LEN]);
    (bucket, fp)
}

impl PrivateKeyword {
    /// Build the PIR keyword table from `entries` (each a term and its posting list of doc ids). The number of
    /// lookups the result can serve is bounded by PIANO's query budget, which grows with sqrt(buckets) — a
    /// realistic corpus has plenty of headroom; a handful of terms does not.
    pub fn build<S: AsRef<str>>(entries: &[(S, Vec<u32>)], seed: u64) -> Self {
        let n = perfect_square_at_least(entries.len().max(1));
        let mut buckets = vec![vec![0u8; BUCKET_WIDTH]; n];
        let mut fill = vec![0usize; n];

        for (term, posting) in entries {
            let (bucket, fp) = locate(term.as_ref(), n);
            if fill[bucket] >= MAX_ENTRIES_PER_BUCKET {
                continue; // bucket full: entry dropped (see shortcut)
            }
            let off = fill[bucket] * ENTRY_LEN;
            let e = &mut buckets[bucket][off..off + ENTRY_LEN];
            e[0..FP_LEN].copy_from_slice(&fp);
            let count = posting.len().min(MAX_DOCS_PER_TERM);
            e[FP_LEN] = count as u8;
            for (j, &doc) in posting.iter().take(MAX_DOCS_PER_TERM).enumerate() {
                let p = FP_LEN + 1 + j * 4;
                e[p..p + 4].copy_from_slice(&doc.to_le_bytes());
            }
            fill[bucket] += 1;
        }

        let perm = Permutation::from_seed(n, seed);
        let mut permuted = vec![Vec::new(); n];
        for (logical, rec) in buckets.iter().enumerate() {
            permuted[perm.physical(logical)] = rec.clone();
        }
        let server_db = SingleServerDb::from_records(&permuted);
        let s = (n as f64).sqrt() as usize;
        let hints = preprocess_streaming(
            n,
            BUCKET_WIDTH,
            Params::for_sqrt_n(s),
            seed.wrapping_add(1),
            permuted.iter().cloned(),
        );
        Self { n_buckets: n, perm, server_db, hints }
    }

    /// Privately fetch `term`'s posting list. The server never learns `term`. Returns `None` if absent.
    /// Consumes one PIR query from the hint pool.
    pub fn lookup(&mut self, term: &str) -> Option<Vec<u32>> {
        let (bucket, fp) = locate(term, self.n_buckets);
        let server = Server::new(&self.server_db);
        let rec = logical_query(&mut self.hints, &self.perm, bucket, &server);

        for slot in 0..MAX_ENTRIES_PER_BUCKET {
            let off = slot * ENTRY_LEN;
            let entry = &rec[off..off + ENTRY_LEN];
            if entry[0..FP_LEN] == fp && fp != [0u8; FP_LEN] {
                let count = (entry[FP_LEN] as usize).min(MAX_DOCS_PER_TERM);
                let docs = (0..count)
                    .map(|j| {
                        let p = FP_LEN + 1 + j * 4;
                        u32::from_le_bytes(entry[p..p + 4].try_into().unwrap())
                    })
                    .collect();
                return Some(docs);
            }
        }
        None
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

    #[test]
    fn keyword_pir_returns_posting_lists_for_a_secret_term() {
        // A realistic-ish corpus so PIANO's query budget (scales with sqrt(buckets)) covers several lookups.
        let mut entries: Vec<(String, Vec<u32>)> =
            (0..32).map(|i| (format!("filler{i}"), vec![i])).collect();
        entries.push(("rust".to_string(), vec![1, 2, 3]));
        entries.push(("tor".to_string(), vec![7]));
        entries.push(("pasta".to_string(), vec![9, 10, 11, 12]));
        entries.push(("privacy".to_string(), vec![4, 5]));

        let mut kw = PrivateKeyword::build(&entries, 42);
        assert_eq!(kw.lookup("rust"), Some(vec![1, 2, 3]));
        assert_eq!(kw.lookup("pasta"), Some(vec![9, 10, 11, 12]));
        assert_eq!(kw.lookup("tor"), Some(vec![7]));
        assert_eq!(kw.lookup("privacy"), Some(vec![4, 5]));
    }

    #[test]
    fn keyword_pir_absent_term_returns_none() {
        let entries = vec![("rust", vec![1u32]), ("tor", vec![2])];
        let mut kw = PrivateKeyword::build(&entries, 7);
        assert_eq!(kw.lookup("nonexistent"), None);
    }

    #[test]
    fn keyword_pir_posting_list_is_capped() {
        let big: Vec<u32> = (0..100).collect();
        let entries = vec![("many", big)];
        let mut kw = PrivateKeyword::build(&entries, 1);
        let got = kw.lookup("many").expect("present");
        assert_eq!(got.len(), MAX_DOCS_PER_TERM, "posting list is truncated to the fixed cap");
        assert_eq!(got[0], 0);
    }
}
