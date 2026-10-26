//! Two-server information-theoretic PIR (the classic Chor-Goldreich-Kushilevitz-Sudan XOR scheme).
//!
//! Improvement over trivial PIR: each server returns **one record's width** (O(W)) instead of the whole
//! database (O(N*W)); the client uploads an N-bit selection vector to each server. Security: a single server
//! sees only a uniformly random selection vector, so it learns nothing about the wanted index — **provided the
//! two servers do not collude**. That non-collusion assumption is the price of this model.
//!
//! The row <-> term mapping is public (the client knows which row it wants); the *query* (which row) is what
//! stays private. Records are padded to a fixed width, which reveals the max record length, never the query.
//!
//! shortcut: production must draw the selection randomness from a CSPRNG; the tests use a small seeded PRG.
//! Upload is still O(N) bits per server, so this is sublinear in *download* only — upgrade to a single-server
//! sublinear scheme when that matters.

/// A database of fixed-width records (padded with zero bytes to the longest record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoServerDb {
    width: usize,
    rows: Vec<Vec<u8>>,
}

impl TwoServerDb {
    /// Build from records, padding each to the width of the longest.
    pub fn from_records(records: &[Vec<u8>]) -> Self {
        let width = records.iter().map(Vec::len).max().unwrap_or(0);
        let rows = records
            .iter()
            .map(|r| {
                let mut padded = r.clone();
                padded.resize(width, 0);
                padded
            })
            .collect();
        Self { width, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn width(&self) -> usize {
        self.width
    }
}

/// One of the two servers. It answers a selection vector by XORing the selected rows together.
pub struct TwoServerServer<'a> {
    db: &'a TwoServerDb,
}

impl<'a> TwoServerServer<'a> {
    pub fn new(db: &'a TwoServerDb) -> Self {
        Self { db }
    }

    /// `answer = XOR of rows[k] for every k with select[k] == true`. The server cannot tell which single row
    /// the client ultimately wants, because `select` is one of two random vectors (see [`make_queries`]).
    pub fn answer(&self, select: &[bool]) -> Vec<u8> {
        assert_eq!(select.len(), self.db.rows.len(), "selection vector length must equal row count");
        let mut acc = vec![0u8; self.db.width];
        for (k, &bit) in select.iter().enumerate() {
            if bit {
                for (a, b) in acc.iter_mut().zip(&self.db.rows[k]) {
                    *a ^= *b;
                }
            }
        }
        acc
    }
}

/// Build the two selection vectors for private index `i` over `n` rows from caller randomness `r0` (n bools).
///
/// Server 0 receives `r0` unchanged; server 1 receives `r0` with bit `i` flipped. Their XOR is therefore the
/// one-hot vector `e_i`, so the two answers XOR to exactly row `i` — yet each vector alone is uniformly random
/// when `r0` is, revealing nothing about `i`.
pub fn make_queries(n: usize, i: usize, r0: &[bool]) -> (Vec<bool>, Vec<bool>) {
    assert!(i < n, "index out of range");
    assert_eq!(r0.len(), n, "randomness length must equal row count");
    let q0 = r0.to_vec();
    let mut q1 = r0.to_vec();
    q1[i] = !q1[i];
    (q0, q1)
}

/// Combine the two servers' answers into the requested record (XOR).
pub fn combine(a0: &[u8], a1: &[u8]) -> Vec<u8> {
    a0.iter().zip(a1).map(|(x, y)| x ^ y).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny seeded PRG (xorshift64*) to produce the random selection vector in tests. NOT a CSPRNG — fine for
    /// tests, never for deployment (see the module shortcut note).
    fn random_bits(seed: u64, n: usize) -> Vec<bool> {
        let mut s = seed | 1;
        (0..n)
            .map(|_| {
                s ^= s >> 12;
                s ^= s << 25;
                s ^= s >> 27;
                (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) & 1 == 1
            })
            .collect()
    }

    fn sample() -> TwoServerDb {
        TwoServerDb::from_records(&[
            b"weather results".to_vec(),
            b"recipe results".to_vec(),
            b"map results".to_vec(),
            b"dictionary results".to_vec(),
        ])
    }

    #[test]
    fn reconstructs_every_record() {
        let db = sample();
        let s0 = TwoServerServer::new(&db);
        let s1 = TwoServerServer::new(&db);
        let originals = [
            &b"weather results"[..],
            &b"recipe results"[..],
            &b"map results"[..],
            &b"dictionary results"[..],
        ];
        for (i, original) in originals.iter().enumerate() {
            let r0 = random_bits(1000 + i as u64, db.len());
            let (q0, q1) = make_queries(db.len(), i, &r0);
            let rec = combine(&s0.answer(&q0), &s1.answer(&q1));
            let mut expected = original.to_vec();
            expected.resize(db.width(), 0);
            assert_eq!(rec, expected, "record {i} mismatch");
        }
    }

    #[test]
    fn server0_view_is_independent_of_index() {
        // Same randomness, different wanted index -> server 0 receives the identical vector, so it cannot tell
        // which index the client wants.
        let r0 = random_bits(7, 4);
        let (q0_a, _) = make_queries(4, 1, &r0);
        let (q0_b, _) = make_queries(4, 3, &r0);
        assert_eq!(q0_a, q0_b);
    }

    #[test]
    fn queries_xor_to_one_hot() {
        let r0 = random_bits(42, 5);
        let i = 2;
        let (q0, q1) = make_queries(5, i, &r0);
        for (k, (&a, &b)) in q0.iter().zip(&q1).enumerate() {
            assert_eq!(a ^ b, k == i, "XOR at {k}");
        }
    }
}
