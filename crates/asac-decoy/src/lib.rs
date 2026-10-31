//! Client-side decoy-query generator: self-directed query obfuscation.
//!
//! This is the only form of "data poisoning" ASAC ships (PLAN.md 4.3 item 7): the user mixes
//! random, benign decoy searches with their real ones so an observer of query timing and volume
//! cannot pick the real queries out. It poisons *the user's own* query stream to protect them,
//! never anyone else's data, index, or system.
//!
//! Hard rule from the plan: decoys are drawn ONLY from a vetted, benign term list supplied by the
//! caller. This crate never invents terms, so it cannot emit a harmful or illegal search.

/// A cryptographically secure PRNG (ChaCha20) for decoy selection. Unlike a fast non-crypto PRNG, observing
/// decoy outputs does not reveal the generator state, so an adversary cannot predict future decoys and filter
/// them out. Deterministic from the seed (so decoy selection is testable); in deployment, seed it from an
/// unpredictable source (e.g. OS entropy) so the stream itself is unpredictable.
pub struct DecoyRng {
    inner: rand_chacha::ChaCha20Rng,
}

impl DecoyRng {
    /// Seed the CSPRNG from a `u64`.
    pub fn new(seed: u64) -> Self {
        use rand_core::SeedableRng;
        Self {
            inner: rand_chacha::ChaCha20Rng::seed_from_u64(seed),
        }
    }

    fn next_u64(&mut self) -> u64 {
        use rand_core::Rng;
        self.inner.next_u64()
    }
}

/// Generates decoy queries by sampling from a caller-supplied benign term list.
pub struct DecoyGenerator<'a> {
    terms: &'a [&'a str],
}

impl<'a> DecoyGenerator<'a> {
    /// Create a generator over a vetted benign term list.
    ///
    /// Returns `None` if the list is empty, because an empty list cannot produce decoys and a
    /// silently-empty generator would be a bug at a trust boundary.
    pub fn new(terms: &'a [&'a str]) -> Option<Self> {
        if terms.is_empty() {
            None
        } else {
            Some(Self { terms })
        }
    }

    /// Produce one decoy query of `1..=max_words` terms joined by single spaces.
    pub fn one(&self, rng: &mut DecoyRng, max_words: usize) -> String {
        let max = max_words.max(1);
        let words = 1 + (rng.next_u64() as usize) % max;
        let mut out = String::new();
        for i in 0..words {
            if i > 0 {
                out.push(' ');
            }
            let idx = (rng.next_u64() as usize) % self.terms.len();
            out.push_str(self.terms[idx]);
        }
        out
    }

    /// Produce `count` decoy queries.
    pub fn many(&self, rng: &mut DecoyRng, count: usize, max_words: usize) -> Vec<String> {
        (0..count).map(|_| self.one(rng, max_words)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERMS: &[&str] = &["weather", "recipes", "news", "maps", "dictionary"];

    #[test]
    fn empty_terms_rejected() {
        assert!(DecoyGenerator::new(&[]).is_none());
    }

    #[test]
    fn decoys_only_use_supplied_terms() {
        let g = DecoyGenerator::new(TERMS).unwrap();
        let mut rng = DecoyRng::new(42);
        for q in g.many(&mut rng, 100, 4) {
            assert!(!q.is_empty());
            for word in q.split(' ') {
                assert!(TERMS.contains(&word), "decoy leaked a non-benign term: {word}");
            }
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let g = DecoyGenerator::new(TERMS).unwrap();
        let mut a = DecoyRng::new(7);
        let mut b = DecoyRng::new(7);
        assert_eq!(g.many(&mut a, 10, 3), g.many(&mut b, 10, 3));
    }

    #[test]
    fn respects_max_words() {
        let g = DecoyGenerator::new(TERMS).unwrap();
        let mut rng = DecoyRng::new(99);
        for q in g.many(&mut rng, 50, 3) {
            assert!(q.split(' ').count() <= 3);
        }
    }
}
