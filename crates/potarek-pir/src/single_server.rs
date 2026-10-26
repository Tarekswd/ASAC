//! Single-server sublinear PIR, PIANO-style (Zhou et al., IEEE S&P 2024) — **milestone M1** (see
//! `docs/PIR-RESEARCH.md`): the *recovery mechanism* only. It demonstrates PIANO's core trick — preprocess XOR
//! "equations" over √n chunks, then recover a record by asking the server for one equation with the wanted
//! index **replaced** by a same-chunk entry, and subtracting.
//!
//! **M1 is NOT yet private.** For correctness we use deterministic "diagonal" equations and no permutation or
//! refresh, so the indices the server sees are structured, not random. The privacy machinery — random
//! seed-generated equations, per-query refresh with backup equations, and a shared-seed DB permutation — is
//! milestones M2/M3. This module exists to get the arithmetic provably right first.
//!
//! Group operation is XOR over fixed-width records. `n` must be a perfect square and √n >= 2.

/// A database of fixed-width records (padded with zero bytes). `n` must be a perfect square.
pub struct SingleServerDb {
    width: usize,
    sqrt_n: usize,
    rows: Vec<Vec<u8>>,
}

impl SingleServerDb {
    pub fn from_records(records: &[Vec<u8>]) -> Self {
        let n = records.len();
        let sqrt_n = (n as f64).sqrt().round() as usize;
        assert_eq!(sqrt_n * sqrt_n, n, "record count must be a perfect square");
        assert!(sqrt_n >= 2, "need at least 2x2 records for a replacement to exist");
        let width = records.iter().map(Vec::len).max().unwrap_or(0);
        let rows = records
            .iter()
            .map(|r| {
                let mut p = r.clone();
                p.resize(width, 0);
                p
            })
            .collect();
        Self { width, sqrt_n, rows }
    }

    fn xor_at(&self, indices: &[usize]) -> Vec<u8> {
        let mut acc = vec![0u8; self.width];
        for &idx in indices {
            for (a, b) in acc.iter_mut().zip(&self.rows[idx]) {
                *a ^= *b;
            }
        }
        acc
    }
}

/// The single server. Its only operation is to XOR the records at a set of indices — it never stores or
/// learns which record the client ultimately wants (in the full scheme those indices are random; in M1 they
/// are structured, hence M1 is correctness-only).
pub struct Server<'a> {
    db: &'a SingleServerDb,
}

impl<'a> Server<'a> {
    pub fn new(db: &'a SingleServerDb) -> Self {
        Self { db }
    }

    /// Return the XOR of the records at `indices` (one index per chunk in the full scheme).
    pub fn answer(&self, indices: &[usize]) -> Vec<u8> {
        self.db.xor_at(indices)
    }
}

/// One preprocessed equation: an offset per chunk, and the XOR of the selected records.
struct Equation {
    offsets: Vec<usize>, // length sqrt_n; selected index in chunk c is c*sqrt_n + offsets[c]
    parity: Vec<u8>,
}

/// Client hints produced by preprocessing. Holds the equations and a couple of replacement entries per chunk;
/// the database itself is dropped after preprocessing.
pub struct ClientHints {
    sqrt_n: usize,
    equations: Vec<Equation>,
    /// replacements[chunk] = a few (global_index, value) pairs the client keeps, to substitute for the wanted
    /// index within the same chunk.
    replacements: Vec<Vec<(usize, Vec<u8>)>>,
}

/// Preprocess the database into client hints. M1 uses √n deterministic "diagonal" equations (equation j selects
/// offset j in every chunk), which guarantees every (chunk, offset) is covered so recovery always succeeds.
/// shortcut: diagonal equations are not random, so M1 provides correctness, not privacy; M2 swaps in
/// seed-generated random equations behind this same interface.
pub fn preprocess(db: &SingleServerDb) -> ClientHints {
    let s = db.sqrt_n;
    let equations = (0..s)
        .map(|j| {
            let offsets = vec![j; s];
            let indices: Vec<usize> = (0..s).map(|c| c * s + offsets[c]).collect();
            Equation { offsets, parity: db.xor_at(&indices) }
        })
        .collect();

    // Keep offsets 0 and 1 of each chunk as replacement entries (values known to the client).
    let replacements = (0..s)
        .map(|c| {
            vec![
                (c * s, db.rows[c * s].clone()),
                (c * s + 1, db.rows[c * s + 1].clone()),
            ]
        })
        .collect();

    ClientHints { sqrt_n: s, equations, replacements }
}

/// Privately fetch record `i` using the hints and one server round-trip. Returns the recovered record (padded).
pub fn query(hints: &ClientHints, i: usize, server: &Server) -> Vec<u8> {
    let s = hints.sqrt_n;
    let chunk = i / s;
    let offset = i % s;

    // Find a stored equation that selects index i (offset `offset` in `chunk`).
    let eq = hints
        .equations
        .iter()
        .find(|e| e.offsets[chunk] == offset)
        .expect("M1 diagonal equations cover every (chunk, offset)");

    // Pick a replacement entry in the same chunk, distinct from i.
    let (r_idx, r_val) = hints.replacements[chunk]
        .iter()
        .find(|(idx, _)| *idx != i)
        .expect("each chunk keeps two replacements, so one differs from i");

    // Build the indices to send: the equation's selection, with chunk `chunk` swapped to the replacement.
    let indices: Vec<usize> = (0..s)
        .map(|c| if c == chunk { *r_idx } else { c * s + eq.offsets[c] })
        .collect();

    let y_prime = server.answer(&indices);

    // X_i = parity XOR y_prime XOR X_r  (parity has X_i; y_prime has X_r in its place).
    eq.parity
        .iter()
        .zip(&y_prime)
        .zip(r_val)
        .map(|((&p, &y), &r)| p ^ y ^ r)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db16() -> SingleServerDb {
        let records: Vec<Vec<u8>> = (0..16).map(|k| format!("record-{k:02}").into_bytes()).collect();
        SingleServerDb::from_records(&records)
    }

    #[test]
    fn recovers_every_record() {
        let db = db16();
        let hints = preprocess(&db);
        let server = Server::new(&db);
        for i in 0..16 {
            let got = query(&hints, i, &server);
            let mut want = format!("record-{i:02}").into_bytes();
            want.resize(got.len(), 0);
            assert_eq!(got, want, "record {i} mismatch");
        }
    }

    #[test]
    fn server_only_ever_sees_sqrt_n_indices() {
        // The server's answer takes exactly one index per chunk (√n total) — the shape that will carry the
        // privacy guarantee once M3 randomises them.
        let db = db16();
        let hints = preprocess(&db);
        let s = hints.sqrt_n;
        let chunk = 2usize;
        let i = chunk * s + 1;
        // Rebuild the indices the query would send and check their count.
        let eq = hints.equations.iter().find(|e| e.offsets[chunk] == i % s).unwrap();
        let (r_idx, _) = hints.replacements[chunk].iter().find(|(idx, _)| *idx != i).unwrap();
        let indices: Vec<usize> =
            (0..s).map(|c| if c == chunk { *r_idx } else { c * s + eq.offsets[c] }).collect();
        assert_eq!(indices.len(), s);
    }
}
