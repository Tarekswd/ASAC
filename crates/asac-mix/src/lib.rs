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

// ---------------------------------------------------------------------------
// F4-live transport seam (FLAGSHIP-DESIGN.md Gap C / EXTERNAL-PLAN.md item 3).
//
// Decision: the live transport is Nym (a deployed Rust Sphinx mixnet), not our own mixes — we never roll
// crypto (Sphinx is per-hop public-key crypto) and a real anonymity set needs a crowd. The Nym backend lives
// behind `MixTransport`; it needs the network + nym-sdk, so it is not in-repo. What *is* here and testable:
// the fixed-size framing that makes a real query and a cover message identical on the wire, and the glue that
// turns a scheduler tick into a wire packet.

/// Why a mixnet send failed. The Nym backend maps its errors onto this.
#[derive(Debug)]
pub struct MixError(pub String);

/// The seam to a live mixnet. Implemented by the Nym backend (external); the scheduler feeds it fixed-size
/// framed packets so the transport — and any network observer — cannot tell a real query from cover.
pub trait MixTransport {
    fn send(&self, packet: &[u8]) -> Result<(), MixError>;
}

/// Frame a payload into a fixed-`packet_size` packet: a 4-byte little-endian length, the payload, then zero
/// padding. Every packet is the same size regardless of payload (cover frames an empty payload to the same
/// size), so length reveals nothing. Returns `None` if the payload does not fit.
pub fn frame(payload: &[u8], packet_size: usize) -> Option<Vec<u8>> {
    if 4 + payload.len() > packet_size {
        return None;
    }
    let mut pkt = Vec::with_capacity(packet_size);
    pkt.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    pkt.extend_from_slice(payload);
    pkt.resize(packet_size, 0);
    Some(pkt)
}

/// Recover the payload from a framed packet (inverse of [`frame`]); `None` if malformed.
pub fn unframe(packet: &[u8]) -> Option<Vec<u8>> {
    let len = u32::from_le_bytes(packet.get(0..4)?.try_into().ok()?) as usize;
    packet.get(4..4 + len).map(<[u8]>::to_vec)
}

/// Next scheduler tick as a wire packet: the delay to wait, and a fixed-size framed packet — the real queued
/// message if any, else cover, indistinguishable by size. `None` if a real payload exceeds `packet_size`.
pub fn next_packet(scheduler: &mut PoissonScheduler, packet_size: usize) -> Option<(f64, Vec<u8>)> {
    let (delay, payload) = scheduler.next_send();
    let bytes: &[u8] = match &payload {
        Payload::Real(b) => b,
        Payload::Cover => &[],
    };
    frame(bytes, packet_size).map(|pkt| (delay, pkt))
}

/// Measure the anonymity the scheduler provides against a **timing-only** observer (FLAGSHIP-DESIGN.md F5, at
/// the scheduler level). Simulates `n_clients`, exactly one of which is actually searching (`real_messages`
/// queued), each running its own rate-`rate` scheduler for `ticks` ticks. The observer sees only each client's
/// send *timing* (content is hidden by PIR/encryption) and guesses the searcher by the simplest timing
/// statistic (total delay). Returns the observer's success rate over `trials`.
///
/// Because the send schedule is independent of whether a client is searching, the observer has no signal and
/// its success approaches `1/n_clients` (chance) — i.e. the anonymity set is the full population. This is the
/// scheduler-level result; full network anonymity (routing through mixes) needs a Shadow network simulation.
pub fn timing_observer_accuracy(
    n_clients: usize,
    real_messages: usize,
    ticks: usize,
    trials: usize,
    rate: f64,
    base_seed: u64,
) -> f64 {
    assert!(n_clients >= 2 && ticks >= 1 && trials >= 1);
    let mut correct = 0usize;
    for t in 0..trials {
        let real = (t.wrapping_mul(2_654_435_761)) % n_clients; // which client searches this trial
        let mut totals = vec![0.0f64; n_clients];
        for (c, total) in totals.iter_mut().enumerate() {
            let seed = base_seed
                ^ (t as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ (c as u64).wrapping_mul(0xD1B5_4A32_D192_ED03);
            let mut sch = PoissonScheduler::new(rate, seed);
            if c == real {
                for _ in 0..real_messages {
                    sch.enqueue(vec![0u8; 8]);
                }
            }
            // Observer records timing only; the Real/Cover distinction is hidden from it.
            for _ in 0..ticks {
                let (delay, _payload) = sch.next_send();
                *total += delay;
            }
        }
        // Observer's best timing-based guess: the client with the largest total delay.
        let guess = totals
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap();
        if guess == real {
            correct += 1;
        }
    }
    correct as f64 / trials as f64
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

    #[test]
    fn frame_roundtrips_and_is_fixed_size() {
        let p = frame(b"private search", 64).unwrap();
        assert_eq!(p.len(), 64);
        assert_eq!(unframe(&p).unwrap(), b"private search");

        // Cover (empty payload) frames to the same size as a real payload — no length side channel.
        assert_eq!(frame(&[], 64).unwrap().len(), frame(b"anything", 64).unwrap().len());
        assert_eq!(unframe(&frame(&[], 64).unwrap()).unwrap(), b"");

        // Too big for the packet.
        assert!(frame(&[0u8; 100], 64).is_none());
    }

    #[test]
    fn every_wire_packet_is_the_same_size_real_or_cover() {
        use std::cell::RefCell;

        struct Fake {
            lens: RefCell<Vec<usize>>,
        }
        impl MixTransport for Fake {
            fn send(&self, packet: &[u8]) -> Result<(), MixError> {
                self.lens.borrow_mut().push(packet.len());
                Ok(())
            }
        }

        let transport = Fake { lens: RefCell::new(Vec::new()) };
        let mut sched = PoissonScheduler::new(2.0, 5);
        sched.enqueue(b"the real query".to_vec());

        // One real message then all cover; the observer sees a uniform stream of 128-byte packets.
        for _ in 0..25 {
            let (_delay, pkt) = next_packet(&mut sched, 128).unwrap();
            transport.send(&pkt).unwrap();
        }
        let lens = transport.lens.borrow();
        assert_eq!(lens.len(), 25);
        assert!(lens.iter().all(|&l| l == 128), "real and cover packets must all be 128 bytes on the wire");
    }

    #[test]
    fn timing_observer_is_at_chance() {
        // With 10 clients, a timing-only observer should identify the searcher ~10% of the time — no better
        // than guessing, because the send schedule does not depend on real traffic. Anonymity set = N.
        let n = 10;
        let acc = timing_observer_accuracy(n, 3, 200, 4000, 2.0, 0xA5AC);
        let chance = 1.0 / n as f64;
        assert!((acc - chance).abs() < 0.03, "observer accuracy {acc} should be ~{chance} (chance)");
    }
}
