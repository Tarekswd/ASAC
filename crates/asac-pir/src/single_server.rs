//! Single-server sublinear PIR, PIANO-style (Zhou et al., IEEE S&P 2024) — **milestone M3 (in progress)** (see
//! `docs/PIR-RESEARCH.md`), building on M2.
//!
//! M3 so far: offset derivation now uses a **CSPRF** (BLAKE3 keyed hash) instead of M2's splitmix stand-in, and
//! a statistical test checks the server-visible offsets are ~uniform. **Still pending for M3:** the shared-seed
//! database *permutation* that makes adaptive (data-dependent) query sequences look uniform, and a formal
//! argument for the refresh distribution — until those land, treat privacy as empirically sanity-checked, not
//! proven.
//!
//! M2 added the privacy *mechanism*: equations are generated from random seeds (not the fixed diagonal), and
//! after each query the consumed equation is **refreshed** from a backup "conditioned on containing the queried
//! index" so the remaining pool's distribution is not skewed. Each equation and replacement is used at most
//! once. This supports a batch of up to ~sqrt(n) queries per preprocessing.
//!
//! What M2 is NOT (that is M3): a *proof* that the refresh yields exactly the right conditional distribution,
//! and the shared-seed database permutation that makes adaptive (data-dependent) queries look uniform. So M2
//! is "the private-scheme machinery, correct and single-use", not yet a formally argued guarantee.
//!
//! Group operation is XOR over fixed-width records. `n` must be a perfect square and sqrt(n) >= 2.
//!
//! shortcut: the shared-seed database permutation for adaptive-query privacy is not implemented yet (next M3
//! step); without it, privacy holds for queries not correlated with the (public) structure.

/// Derive a 32-byte PRF key from a user-supplied master seed (BLAKE3 is a cryptographic hash).
fn derive_key(master_seed: u64) -> [u8; 32] {
    *blake3::hash(&master_seed.to_le_bytes()).as_bytes()
}

/// CSPRF: the offset chosen within `chunk` by equation `eq_id`, in `[0, sqrt_n)`, keyed by the client's secret
/// `key`. Uses BLAKE3's keyed hash (a PRF/MAC), so a server without the key cannot predict or recognise the
/// offsets — this is the cryptographic replacement for M2's splitmix stand-in.
///
/// shortcut: reducing a 64-bit value mod sqrt_n has negligible modulo bias for the small sqrt_n used here;
/// use rejection sampling if sqrt_n ever approaches 2^64.
fn offset(key: &[u8; 32], eq_id: u64, chunk: usize, sqrt_n: usize) -> usize {
    let mut input = [0u8; 16];
    input[..8].copy_from_slice(&eq_id.to_le_bytes());
    input[8..].copy_from_slice(&(chunk as u64).to_le_bytes());
    let digest = blake3::keyed_hash(key, &input);
    let v = u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes"));
    (v % sqrt_n as u64) as usize
}

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
        assert!(sqrt_n >= 2, "need sqrt(n) >= 2");
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

/// The single server: XOR the records at the given indices. It never learns which record the client wants
/// (one random-looking index per chunk).
pub struct Server<'a> {
    db: &'a SingleServerDb,
}

impl<'a> Server<'a> {
    pub fn new(db: &'a SingleServerDb) -> Self {
        Self { db }
    }

    pub fn answer(&self, indices: &[usize]) -> Vec<u8> {
        self.db.xor_at(indices)
    }
}

/// How many hints to preprocess. Over-provisioned for the prototype; PIANO sizes these by a balls-into-bins
/// argument (O(sqrt(n)) primaries, O(log n) backups/replacements per chunk).
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub primaries: usize,
    pub backups_per_chunk: usize,
    pub replacements_per_chunk: usize,
}

impl Params {
    /// Generous defaults that make a batch of sqrt(n) queries succeed with very high probability.
    pub fn for_sqrt_n(s: usize) -> Self {
        Self { primaries: 8 * s, backups_per_chunk: s, replacements_per_chunk: s }
    }
}

struct Equation {
    offsets: Vec<usize>,
    parity: Vec<u8>,
    consumed: bool,
}

struct Backup {
    offsets: Vec<usize>,      // entry at the hole chunk is unused
    parity_without_hole: Vec<u8>,
    consumed: bool,
}

struct Replacement {
    index: usize,
    value: Vec<u8>,
    consumed: bool,
}

/// Client state after preprocessing. The database is dropped; only these sublinear hints remain. Mutable
/// because querying consumes and refreshes hints.
pub struct ClientState {
    sqrt_n: usize,
    equations: Vec<Equation>,
    backups: Vec<Vec<Backup>>,       // backups[chunk] -> backups with their hole in `chunk`
    replacements: Vec<Vec<Replacement>>,
}

/// Preprocess the database into client hints using `master_seed` for all randomness.
pub fn preprocess(db: &SingleServerDb, params: Params, master_seed: u64) -> ClientState {
    let s = db.sqrt_n;
    let key = derive_key(master_seed);

    let equations = (0..params.primaries)
        .map(|k| {
            let offsets: Vec<usize> = (0..s).map(|c| offset(&key, k as u64, c, s)).collect();
            let indices: Vec<usize> = (0..s).map(|c| c * s + offsets[c]).collect();
            Equation { offsets, parity: db.xor_at(&indices), consumed: false }
        })
        .collect();

    let backups = (0..s)
        .map(|hole| {
            (0..params.backups_per_chunk)
                .map(|k| {
                    let eq_id = (1u64 << 63) | ((hole as u64) << 20) | k as u64;
                    let offsets: Vec<usize> =
                        (0..s).map(|c| if c == hole { 0 } else { offset(&key, eq_id, c, s) }).collect();
                    let indices: Vec<usize> =
                        (0..s).filter(|&c| c != hole).map(|c| c * s + offsets[c]).collect();
                    Backup { offsets, parity_without_hole: db.xor_at(&indices), consumed: false }
                })
                .collect()
        })
        .collect();

    // Keep `replacements_per_chunk` distinct offsets per chunk (their values are known to the client).
    let replacements = (0..s)
        .map(|c| {
            (0..params.replacements_per_chunk.min(s))
                .map(|off| Replacement { index: c * s + off, value: db.rows[c * s + off].clone(), consumed: false })
                .collect()
        })
        .collect();

    ClientState { sqrt_n: s, equations, backups, replacements }
}

/// Privately fetch record `i`: one server round-trip, then refresh the consumed hints. Panics only if the
/// prototype runs out of provisioned hints (raise [`Params`] if so).
pub fn query(state: &mut ClientState, i: usize, server: &Server) -> Vec<u8> {
    let s = state.sqrt_n;
    let chunk = i / s;
    let off = i % s;

    // 1. Consume a primary equation that selects index i.
    let pos = state
        .equations
        .iter()
        .position(|e| !e.consumed && e.offsets[chunk] == off)
        .expect("no unconsumed equation covers i; raise Params.primaries");
    let offsets = state.equations[pos].offsets.clone();
    let parity = state.equations[pos].parity.clone();
    state.equations[pos].consumed = true;

    // 2. Consume a replacement in this chunk, distinct from i.
    let rpos = state.replacements[chunk]
        .iter()
        .position(|r| !r.consumed && r.index != i)
        .expect("no unconsumed replacement != i; raise Params.replacements_per_chunk");
    let r_index = state.replacements[chunk][rpos].index;
    let r_value = state.replacements[chunk][rpos].value.clone();
    state.replacements[chunk][rpos].consumed = true;

    // 3. Ask the server for the equation with i swapped for the replacement (server sees one index per chunk).
    let indices: Vec<usize> =
        (0..s).map(|c| if c == chunk { r_index } else { c * s + offsets[c] }).collect();
    let y_prime = server.answer(&indices);

    // 4. Recover: X_i = parity XOR y' XOR X_r.
    let xi: Vec<u8> = parity
        .iter()
        .zip(&y_prime)
        .zip(&r_value)
        .map(|((&p, &y), &r)| p ^ y ^ r)
        .collect();

    // 5. Refresh: turn a backup (hole in this chunk) into a fresh equation that contains i, using the X_i we
    //    just learned. This keeps the pool's size and distribution rather than skewing it.
    if let Some(bpos) = state.backups[chunk].iter().position(|b| !b.consumed) {
        let mut new_offsets = state.backups[chunk][bpos].offsets.clone();
        let base = state.backups[chunk][bpos].parity_without_hole.clone();
        state.backups[chunk][bpos].consumed = true;
        new_offsets[chunk] = off;
        let new_parity: Vec<u8> = base.iter().zip(&xi).map(|(&a, &b)| a ^ b).collect();
        state.equations.push(Equation { offsets: new_offsets, parity: new_parity, consumed: false });
    }

    xi
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(n: usize) -> SingleServerDb {
        let records: Vec<Vec<u8>> = (0..n).map(|k| format!("record-{k:03}").into_bytes()).collect();
        SingleServerDb::from_records(&records)
    }

    fn want(i: usize, width: usize) -> Vec<u8> {
        let mut w = format!("record-{i:03}").into_bytes();
        w.resize(width, 0);
        w
    }

    #[test]
    fn batch_of_distinct_queries_all_correct() {
        let db = db(16);
        let s = 4;
        let mut state = preprocess(&db, Params::for_sqrt_n(s), 0xDEAD_BEEF);
        let server = Server::new(&db);
        // includes two queries in the same chunk (0 and 1) to exercise refresh + repeated consumption.
        for &i in &[0usize, 1, 5, 10, 15] {
            let got = query(&mut state, i, &server);
            assert_eq!(got, want(i, got.len()), "record {i}");
        }
    }

    #[test]
    fn query_refreshes_an_equation_containing_i() {
        let db = db(16);
        let mut state = preprocess(&db, Params::for_sqrt_n(4), 1);
        let server = Server::new(&db);
        let i = 6usize; // chunk 1, offset 2
        let before = state.equations.len();
        let _ = query(&mut state, i, &server);
        // A fresh, unconsumed equation that selects i exists (the refreshed one).
        let refreshed = state
            .equations
            .iter()
            .any(|e| !e.consumed && e.offsets[i / 4] == i % 4);
        assert!(refreshed, "refresh must leave an equation covering i");
        assert_eq!(state.equations.len(), before + 1, "refresh adds one equation");
    }

    #[test]
    fn each_equation_used_at_most_once() {
        // After a batch, the number of consumed primaries equals the number of queries (no double-spend).
        let db = db(16);
        let mut state = preprocess(&db, Params::for_sqrt_n(4), 99);
        let server = Server::new(&db);
        let batch = [2usize, 7, 8, 13];
        for &i in &batch {
            let _ = query(&mut state, i, &server);
        }
        let consumed = state.equations.iter().filter(|e| e.consumed).count();
        assert_eq!(consumed, batch.len());
    }

    #[test]
    fn csprf_offsets_are_roughly_uniform() {
        // Empirical sanity check (NOT a proof): the CSPRF's per-chunk offset is ~uniform over [0, s). This is
        // the randomness the server-visible indices depend on; a broken PRF would show as skew here.
        let key = derive_key(2026);
        let s = 16usize;
        let trials = 16_000u64;
        let mut counts = vec![0usize; s];
        for eq_id in 0..trials {
            counts[offset(&key, eq_id, 0, s)] += 1;
        }
        let expected = trials as f64 / s as f64; // 1000 per bucket
        for (off, &c) in counts.iter().enumerate() {
            let c = c as f64;
            assert!(c > expected * 0.75 && c < expected * 1.25, "offset {off}: {c} far from {expected}");
        }
    }
}
