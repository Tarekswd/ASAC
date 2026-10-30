//! Loopix-style send scheduler for the ASAC search path (FLAGSHIP-DESIGN.md F4, Gap C).
//!
//! The client emits messages on a **Poisson process of rate `λ`** [verified: Piotrowska et al., "The Loopix
//! Anonymity System", USENIX Security 2017]: inter-send delays are i.i.d. exponential, and at each send the
//! client transmits a real queued message if it has one, otherwise a **cover** message. The crucial property —
//! and what this module makes testable — is that **the delay schedule does not depend on the queue**, so an
//! observer timing the client's sends sees the same constant-rate stream whether or not the user is really
//! searching. Real traffic is hidden in cover traffic; timings are unlinkable.
//!
//! This is the offline-testable core only. The live mix transport (routing, Sphinx-style packets, loop
//! messages back through mixes, provider storage) is later work; see the module's `shortcut`.
//!
//! shortcut: models the *local send schedule*, not the network. Cover messages are a `Cover` marker, not
//! routed loop packets; there is no mix/provider wire protocol yet. Add those when wiring a live mixnet (own
//! deployment vs riding Nym is open — FLAGSHIP-DESIGN.md §10).

use std::collections::VecDeque;

/// What the scheduler says to send at a tick.
#[derive(Debug, PartialEq, Eq)]
pub enum Payload {
    /// A real, queued message.
    Real(Vec<u8>),
    /// A cover message (sent to keep the stream at a constant Poisson rate when nothing real is queued).
    Cover,
}

/// A rate-`λ` Poisson send scheduler with a real-message queue. Deterministic given its seed (for tests and
/// reproducibility); swap in a CSPRNG for deployment.
pub struct PoissonScheduler {
    rate: f64, // λ, sends per unit time
    state: u64,
    queue: VecDeque<Vec<u8>>,
}

/// splitmix64 — small, self-contained PRNG for sampling (not security-critical here; delays only).
fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl PoissonScheduler {
    /// New scheduler sending at `rate` messages per unit time (must be > 0).
    pub fn new(rate: f64, seed: u64) -> Self {
        assert!(rate > 0.0, "rate must be positive");
        Self { rate, state: seed, queue: VecDeque::new() }
    }

    /// Queue a real message to be sent at one of the next ticks.
    pub fn enqueue(&mut self, msg: Vec<u8>) {
        self.queue.push_back(msg);
    }

    /// Draw the next inter-send delay: exponential with mean `1/rate`. Independent of the queue — this is what
    /// makes the schedule reveal nothing about real traffic.
    pub fn next_delay(&mut self) -> f64 {
        // Uniform u in [0,1) from the top 53 bits, then inverse-CDF of the exponential distribution.
        let bits = next_u64(&mut self.state) >> 11;
        let u = bits as f64 / (1u64 << 53) as f64;
        -(1.0 - u).ln() / self.rate
    }

    /// The next tick: the delay to wait, and what to send (a real queued message if any, else cover).
    pub fn next_send(&mut self) -> (f64, Payload) {
        let delay = self.next_delay();
        let payload = self.queue.pop_front().map(Payload::Real).unwrap_or(Payload::Cover);
        (delay, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_are_independent_of_queue_contents() {
        // Same seed/rate; one scheduler has real traffic, the other does not. The delay *schedule* must match
        // tick-for-tick, so timing analysis cannot tell the two apart.
        let mut idle = PoissonScheduler::new(2.0, 777);
        let mut busy = PoissonScheduler::new(2.0, 777);
        busy.enqueue(b"real-1".to_vec());
        busy.enqueue(b"real-2".to_vec());

        for i in 0..20 {
            let (d_idle, p_idle) = idle.next_send();
            let (d_busy, p_busy) = busy.next_send();
            assert_eq!(d_idle, d_busy, "delay at tick {i} must not depend on the queue");
            assert_eq!(p_idle, Payload::Cover, "idle client always covers");
            if i < 2 {
                assert!(matches!(p_busy, Payload::Real(_)), "busy client sends its real messages first");
            } else {
                assert_eq!(p_busy, Payload::Cover, "then covers, same as idle");
            }
        }
    }

    #[test]
    fn mean_delay_approximates_one_over_rate() {
        let rate = 5.0;
        let mut s = PoissonScheduler::new(rate, 42);
        let n = 100_000;
        let mean: f64 = (0..n).map(|_| s.next_delay()).sum::<f64>() / n as f64;
        assert!((mean - 1.0 / rate).abs() < 0.01, "mean delay {mean} should be ~{}", 1.0 / rate);
    }

    #[test]
    fn delays_are_positive() {
        let mut s = PoissonScheduler::new(1.0, 1);
        for _ in 0..1000 {
            assert!(s.next_delay() > 0.0);
        }
    }
}
