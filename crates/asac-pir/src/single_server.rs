//! Single-server sublinear PIR, PIANO-style (Zhou et al., IEEE S&P 2024) — **milestone M3 (in progress)** (see
//! `docs/PIR-RESEARCH.md`), building on M2.
//!
//! M3 adds the cryptographic/analysis ingredients: offsets come from a **CSPRF** (BLAKE3 keyed hash), the
//! shared-seed database **permutation** ([`Permutation`]) puts adaptive query streams into the random-query
//! distribution the analysis assumes, and statistical tests check offset uniformity and that the permutation
//! is a bijection. **Still deferred (M6 / external review):** a *formal proof* that the refresh and permutation
//! together give the exact privacy guarantee — until then, treat privacy as empirically sanity-checked, not
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
//! shortcut: the formal privacy proof is pending (M6); the permutation here provides the mechanism, not a
//! proof, and the modulo reductions assume small sqrt_n / n.

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

/// Draw a pseudorandom u64 from `key` at counter `ctr` (BLAKE3 keyed hash), advancing `ctr`.
fn prng_u64(key: &[u8; 32], ctr: &mut u64) -> u64 {
    let digest = blake3::keyed_hash(key, &ctr.to_le_bytes());
    *ctr = ctr.wrapping_add(1);
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes"))
}

/// A database permutation derived from a shared seed — PIANO's device for the *adaptive*-query setting: the
/// server stores records in a random permuted order whose seed it shares with the client, so an adaptive
/// sequence of logical queries maps to physical positions distributed as the security analysis assumes.
///
/// Note on scope: the server knows the seed, so the permutation does NOT by itself hide a single logical index
/// — the per-query equation/replacement mechanism (M2) does that. The permutation keeps the *query stream*
/// within the analysed random-query distribution. A formal privacy argument for the adaptive setting is
/// deferred to M6 / external review.
///
/// shortcut: Fisher-Yates with a modulo reduction (negligible bias for small n; use rejection sampling for
/// large n).
pub struct Permutation {
    phys: Vec<usize>, // phys[logical] = the physical slot holding that logical record
}

impl Permutation {
    pub fn from_seed(n: usize, seed: u64) -> Self {
        let key = derive_key(seed ^ 0x5041_524D_5554_4531); // domain-separate from the offset key
        let mut phys: Vec<usize> = (0..n).collect();
        let mut ctr = 0u64;
        for j in (1..n).rev() {
            let r = (prng_u64(&key, &mut ctr) % (j as u64 + 1)) as usize;
            phys.swap(j, r);
        }
        Self { phys }
    }

    /// The physical slot holding logical record `i`.
    pub fn physical(&self, i: usize) -> usize {
        self.phys[i]
    }
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

    /// Build the server's database in the order given by `perm`: physical slot `perm.physical(i)` holds logical
    /// record `i`. The client uses the same `perm` to translate logical queries (see [`logical_query`]).
    pub fn from_records_with_permutation(records: &[Vec<u8>], perm: &Permutation) -> Self {
        let n = records.len();
        let mut permuted = vec![Vec::new(); n];
        for (logical, rec) in records.iter().enumerate() {
            permuted[perm.physical(logical)] = rec.clone();
        }
        Self::from_records(&permuted)
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
        Self {
            primaries: 8 * s,
            backups_per_chunk: s,
            replacements_per_chunk: s,
        }
    }
}

#[derive(PartialEq, Eq)]
struct Equation {
    offsets: Vec<usize>,
    parity: Vec<u8>,
    consumed: bool,
}

#[derive(PartialEq, Eq)]
struct Backup {
    offsets: Vec<usize>, // entry at the hole chunk is unused
    parity_without_hole: Vec<u8>,
    consumed: bool,
}

#[derive(PartialEq, Eq)]
struct Replacement {
    index: usize,
    value: Vec<u8>,
    consumed: bool,
}

/// Client state after preprocessing. The database is dropped; only these sublinear hints remain. Mutable
/// because querying consumes and refreshes hints.
#[derive(PartialEq, Eq)]
pub struct ClientState {
    sqrt_n: usize,
    equations: Vec<Equation>,
    backups: Vec<Vec<Backup>>, // backups[chunk] -> backups with their hole in `chunk`
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
            Equation {
                offsets,
                parity: db.xor_at(&indices),
                consumed: false,
            }
        })
        .collect();

    let backups = (0..s)
        .map(|hole| {
            (0..params.backups_per_chunk)
                .map(|k| {
                    let eq_id = (1u64 << 63) | ((hole as u64) << 20) | k as u64;
                    let offsets: Vec<usize> = (0..s)
                        .map(|c| if c == hole { 0 } else { offset(&key, eq_id, c, s) })
                        .collect();
                    let indices: Vec<usize> = (0..s)
                        .filter(|&c| c != hole)
                        .map(|c| c * s + offsets[c])
                        .collect();
                    Backup {
                        offsets,
                        parity_without_hole: db.xor_at(&indices),
                        consumed: false,
                    }
                })
                .collect()
        })
        .collect();

    // Keep `replacements_per_chunk` distinct offsets per chunk (their values are known to the client).
    let replacements = (0..s)
        .map(|c| {
            (0..params.replacements_per_chunk.min(s))
                .map(|off| Replacement {
                    index: c * s + off,
                    value: db.rows[c * s + off].clone(),
                    consumed: false,
                })
                .collect()
        })
        .collect();

    ClientState {
        sqrt_n: s,
        equations,
        backups,
        replacements,
    }
}

/// XOR `src` into `acc` (equal length).
fn xor_into(acc: &mut [u8], src: &[u8]) {
    for (a, b) in acc.iter_mut().zip(src) {
        *a ^= *b;
    }
}

/// Streaming preprocessing (M4): build **exactly the same hints** as [`preprocess`] without ever holding the
/// whole database in memory. Records arrive one at a time from `records` (padded/truncated to `width`) in index
/// order; each is folded into every hint it belongs to and then dropped, so peak memory is O(hints + width),
/// not O(n * width). Identical to the batch build because XOR is order-independent.
pub fn preprocess_streaming(
    n: usize,
    width: usize,
    params: Params,
    master_seed: u64,
    records: impl Iterator<Item = Vec<u8>>,
) -> ClientState {
    let s = (n as f64).sqrt().round() as usize;
    assert_eq!(s * s, n, "record count must be a perfect square");
    assert!(s >= 2, "need sqrt(n) >= 2");
    let key = derive_key(master_seed);

    let mut equations: Vec<Equation> = (0..params.primaries)
        .map(|k| Equation {
            offsets: (0..s).map(|c| offset(&key, k as u64, c, s)).collect(),
            parity: vec![0u8; width],
            consumed: false,
        })
        .collect();

    let mut backups: Vec<Vec<Backup>> = (0..s)
        .map(|hole| {
            (0..params.backups_per_chunk)
                .map(|k| {
                    let eq_id = (1u64 << 63) | ((hole as u64) << 20) | k as u64;
                    Backup {
                        offsets: (0..s)
                            .map(|c| if c == hole { 0 } else { offset(&key, eq_id, c, s) })
                            .collect(),
                        parity_without_hole: vec![0u8; width],
                        consumed: false,
                    }
                })
                .collect()
        })
        .collect();

    let keep = params.replacements_per_chunk.min(s);
    let mut replacements: Vec<Vec<Replacement>> = (0..s)
        .map(|c| {
            (0..keep)
                .map(|off| Replacement {
                    index: c * s + off,
                    value: vec![0u8; width],
                    consumed: false,
                })
                .collect()
        })
        .collect();

    let mut count = 0usize;
    for (idx, rec) in records.enumerate() {
        let mut row = rec;
        row.resize(width, 0);
        let c = idx / s;
        let o = idx % s;
        for eq in equations.iter_mut() {
            if eq.offsets[c] == o {
                xor_into(&mut eq.parity, &row);
            }
        }
        for (hole, bs) in backups.iter_mut().enumerate() {
            if hole == c {
                continue;
            }
            for b in bs.iter_mut() {
                if b.offsets[c] == o {
                    xor_into(&mut b.parity_without_hole, &row);
                }
            }
        }
        if o < keep {
            if let Some(r) = replacements[c].iter_mut().find(|r| r.index == c * s + o) {
                r.value = row;
            }
        }
        count += 1;
    }
    assert_eq!(count, n, "stream must yield exactly n records");

    ClientState {
        sqrt_n: s,
        equations,
        backups,
        replacements,
    }
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
    let indices: Vec<usize> = (0..s)
        .map(|c| if c == chunk { r_index } else { c * s + offsets[c] })
        .collect();
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
        state.equations.push(Equation {
            offsets: new_offsets,
            parity: new_parity,
            consumed: false,
        });
    }

    xi
}

/// Fetch logical record `i` through a shared permutation: translate to its physical slot, then run [`query`].
pub fn logical_query(state: &mut ClientState, perm: &Permutation, i: usize, server: &Server) -> Vec<u8> {
    query(state, perm.physical(i), server)
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
            assert!(
                c > expected * 0.75 && c < expected * 1.25,
                "offset {off}: {c} far from {expected}"
            );
        }
    }

    #[test]
    fn permutation_is_a_bijection() {
        let perm = Permutation::from_seed(100, 42);
        let mut seen = vec![false; 100];
        for i in 0..100 {
            let p = perm.physical(i);
            assert!(!seen[p], "physical slot {p} assigned twice");
            seen[p] = true;
        }
        assert!(seen.into_iter().all(|b| b), "every slot must be used");
    }

    #[test]
    fn permuted_scheme_recovers_logical_records() {
        let records: Vec<Vec<u8>> = (0..16).map(|k| format!("record-{k:03}").into_bytes()).collect();
        let perm = Permutation::from_seed(16, 0x5EED);
        let db = SingleServerDb::from_records_with_permutation(&records, &perm);
        let mut state = preprocess(&db, Params::for_sqrt_n(4), 7);
        let server = Server::new(&db);
        // An arbitrary (adaptive-looking) logical order; each must come back correct through the permutation.
        for &i in &[3usize, 9, 0, 15, 6] {
            let got = logical_query(&mut state, &perm, i, &server);
            let mut want = format!("record-{i:03}").into_bytes();
            want.resize(got.len(), 0);
            assert_eq!(got, want, "logical {i}");
        }
    }

    #[test]
    fn streaming_builds_identical_hints() {
        let records: Vec<Vec<u8>> = (0..16).map(|k| format!("rec-{k:04}").into_bytes()).collect();
        let width = records.iter().map(Vec::len).max().unwrap();
        let params = Params::for_sqrt_n(4);
        let seed = 123;
        let batch = preprocess(&SingleServerDb::from_records(&records), params, seed);
        let streamed = preprocess_streaming(16, width, params, seed, records.iter().cloned());
        assert!(
            batch == streamed,
            "streaming must build byte-identical hints to the batch build"
        );
    }

    #[test]
    fn streamed_hints_answer_queries() {
        let records: Vec<Vec<u8>> = (0..16).map(|k| format!("rec-{k:04}").into_bytes()).collect();
        let width = records.iter().map(Vec::len).max().unwrap();
        let db = SingleServerDb::from_records(&records);
        let mut state = preprocess_streaming(16, width, Params::for_sqrt_n(4), 55, records.iter().cloned());
        let server = Server::new(&db);
        for i in [2usize, 7, 11] {
            let got = query(&mut state, i, &server);
            let mut w = format!("rec-{i:04}").into_bytes();
            w.resize(got.len(), 0);
            assert_eq!(got, w, "record {i}");
        }
    }

    #[test]
    fn end_to_end_private_search_over_term_index() {
        // Full M5 path: public term->row directory, permuted + streamed index, private logical query.
        let terms = [
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p",
        ];
        let width = 8;
        let results: Vec<Vec<u8>> = terms
            .iter()
            .map(|t| {
                let mut v = format!("R-{t}").into_bytes();
                v.resize(width, 0);
                v
            })
            .collect();
        let perm = Permutation::from_seed(16, 1);
        let mut permuted = vec![Vec::new(); 16];
        for (logical, rec) in results.iter().enumerate() {
            permuted[perm.physical(logical)] = rec.clone();
        }
        let db = SingleServerDb::from_records(&permuted);
        let mut state = preprocess_streaming(16, width, Params::for_sqrt_n(4), 2, permuted.iter().cloned());
        let server = Server::new(&db);

        let q = "k";
        let row = terms.iter().position(|&t| t == q).unwrap();
        let got = logical_query(&mut state, &perm, row, &server);
        let mut want = format!("R-{q}").into_bytes();
        want.resize(got.len(), 0);
        assert_eq!(got, want);
    }
}
