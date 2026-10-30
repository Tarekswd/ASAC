//! Client-side query stream: real searches hidden among vetted decoys, on a Poisson schedule.
//!
//! This wires together the two client-side defenses that were built but unconnected:
//! - the [`PoissonScheduler`](asac_mix::PoissonScheduler) cover-traffic pacing (flagship F4) — so the *timing*
//!   of sends is independent of real traffic, and
//! - the [`DecoyGenerator`](asac_decoy::DecoyGenerator) vetted decoy queries (PLAN.md 4.3 #7) — so the
//!   *content* of each send is a plausible, benign query whether it is real or cover.
//!
//! Each tick emits one outbound query at a Poisson-distributed delay: the user's real query if one is queued,
//! otherwise a decoy drawn from the caller's benign term list. An observer of the client's query stream sees a
//! constant-rate sequence of plausible searches and cannot pick the real ones out — not by timing (F4) and
//! not by content (decoys look like real queries). Decoys are the only "poisoning" ASAC does, and only of the
//! user's own stream (asac-decoy's hard rule).
//!
//! This is the local emission layer; it does not itself route over Tor/mixes (that is the live transport,
//! FLAGSHIP-DESIGN.md F4-live). Swap the scheduler's PRNG for a CSPRNG in deployment (see asac-mix).

use asac_decoy::{DecoyGenerator, DecoyRng};
use asac_mix::{Payload, PoissonScheduler};

/// Produces the client's outbound query stream (real queries interleaved with decoys), Poisson-paced.
pub struct DecoyClient<'a> {
    scheduler: PoissonScheduler,
    generator: DecoyGenerator<'a>,
    rng: DecoyRng,
    max_words: usize,
}

impl<'a> DecoyClient<'a> {
    /// New client sending at `rate` queries per unit time, drawing decoys of up to `max_words` terms from the
    /// vetted `terms`. Returns `None` if `terms` is empty (an empty decoy list is a trust-boundary bug).
    pub fn new(terms: &'a [&'a str], rate: f64, seed: u64, max_words: usize) -> Option<Self> {
        let generator = DecoyGenerator::new(terms)?;
        Some(Self {
            scheduler: PoissonScheduler::new(rate, seed),
            generator,
            rng: DecoyRng::new(seed ^ 0xDEC0_0000_DEC0_0000),
            max_words,
        })
    }

    /// Queue a real user query to be sent at one of the next ticks (hidden among decoys).
    pub fn enqueue_real(&mut self, query: &str) {
        self.scheduler.enqueue(query.as_bytes().to_vec());
    }

    /// The next outbound query: the delay to wait, and the query text — the next real query if one is queued,
    /// otherwise a freshly generated decoy. Observably identical in timing and shape either way.
    pub fn next_query(&mut self) -> (f64, String) {
        match self.scheduler.next_send() {
            (delay, Payload::Real(bytes)) => (delay, String::from_utf8_lossy(&bytes).into_owned()),
            (delay, Payload::Cover) => (delay, self.generator.one(&mut self.rng, self.max_words)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERMS: &[&str] = &["weather", "recipes", "news", "maps", "dictionary"];

    #[test]
    fn empty_terms_rejected() {
        assert!(DecoyClient::new(&[], 1.0, 1, 3).is_none());
    }

    #[test]
    fn real_queries_come_back_then_decoys_fill_in() {
        let mut c = DecoyClient::new(TERMS, 2.0, 7, 3).unwrap();
        c.enqueue_real("private search");
        c.enqueue_real("onion routing");

        let (_, q0) = c.next_query();
        let (_, q1) = c.next_query();
        assert_eq!(q0, "private search", "first real query sent first");
        assert_eq!(q1, "onion routing");

        // After the real queue drains, every tick is a decoy from the vetted list.
        for _ in 0..50 {
            let (delay, q) = c.next_query();
            assert!(delay > 0.0);
            assert!(!q.is_empty());
            for word in q.split(' ') {
                assert!(TERMS.contains(&word), "decoy leaked a non-benign term: {word}");
            }
        }
    }

    #[test]
    fn timing_does_not_depend_on_whether_a_real_query_is_queued() {
        // Same seed/rate: the delay schedule is identical whether or not real queries are present, so an
        // observer timing the stream cannot tell a searching client from an idle one.
        let mut idle = DecoyClient::new(TERMS, 3.0, 99, 2).unwrap();
        let mut busy = DecoyClient::new(TERMS, 3.0, 99, 2).unwrap();
        busy.enqueue_real("secret query");
        for _ in 0..30 {
            let (d_idle, _) = idle.next_query();
            let (d_busy, _) = busy.next_query();
            assert_eq!(d_idle, d_busy);
        }
    }
}
