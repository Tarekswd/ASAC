# Proceeding with the work that needs external help or validation

Everything self-contained and self-verifiable in a dev environment is built and tested (engine Phases 0-1 and
4; flagship mechanisms F1-F5 at the in-process level; see ARCHITECTURE.md and FLAGSHIP-DESIGN.md). What
remains cannot be *finished or proven here* — it needs a network simulator, a live Tor deployment, outside
experts, a legal/partner agreement, or a decision. This is the plan for each, with what it needs, the concrete
steps, what it depends on, and the gate it clears.

Ordered roughly by dependency and by how much it unblocks.

## 1. Live onion-service validation (cheapest external step)
- **Needs:** a networked host that can reach Tor, and a second client (Tor Browser, or a second arti client)
  to connect to the service.
- **Steps:** run `cargo run -p asac-server -- <nickname> <seed>`; note the printed `.onion`; from the second
  client open `http://<onion>/search?q=...`; confirm a rendezvous connects and the results page renders. Then
  repeat with `--features pow` and `ASAC_RECRAWL_SECS` set.
- **Depends on:** nothing new (code is done).
- **Clears:** the "live serving" half of PLAN Phase 4 — turns `asac-server`/`asac-fetch` from "compiles +
  response-parsing tested" into "observed working end to end."

## 2. F5 — network-level Shadow simulation
- **Needs:** [Shadow](https://shadow.github.io) installed, plus a mix-transport stub (item 3) to measure the
  full path (the scheduler-level anonymity is already measured in-process — `asac_mix::timing_observer_accuracy`).
- **Scaffold in-repo:** `sim/shadow/` holds the harness spec (`README.md`) and a schema-grounded config
  template (`mix-anonymity.yaml`) — N clients emitting the `asac-client` cover stream, one real searcher, an
  observer. Not run here (needs Shadow + a built emitter).
- **Steps:** build the `asac-emit`/`asac-observe` binaries the template references; run `shadow
  sim/shadow/mix-anonymity.yaml`; compute the observer's identification accuracy (target ~1/N) and the
  wall-clock timing-independence test from FLAGSHIP-DESIGN.md §7; compare to documented targets.
- **Depends on:** item 3 (something to route through); `asac-mix` (done).
- **Clears:** the metadata-unobservability measurement — the evidence half of the flagship claim.

## 3. F4-live — mix transport (Nym backend)
- **Decision: made — ride Nym** (deployed Rust Sphinx mixnet), not our own mixes: we never roll crypto and
  Sphinx is per-hop public-key crypto, and a real anonymity set needs a crowd (FLAGSHIP-DESIGN.md §5/§10).
- **Done in-repo:** the `asac_mix::MixTransport` seam and fixed-size packet framing (`frame`/`next_packet`),
  so real and cover packets are identical on the wire (tested). The scheduler already feeds it.
- **Needs (external):** the `nym-sdk` crate and a live network to test against.
- **Steps:** add `nym-sdk`; implement `MixTransport::send` by submitting each framed packet as a Nym Sphinx
  packet to a gateway; run the live send loop (sleep the scheduler delay, then send); validate in Shadow
  (item 2) or against the live Nym testnet.
- **Depends on:** `asac-mix` seam (done); the Nym network.
- **Clears:** the actual metadata-privacy transport (today only the local send schedule + framing exist).

## 4. F6 — external cryptographer review + published artifact
- **Needs:** an applied cryptographer (PLAN §7 lists this role), then a PETS/PoPETs submission.
- **Steps:** finish discharging the code-side obligations in [PIR-SECURITY.md](PIR-SECURITY.md) that we *can*
  do here (O2 fixed-K padding, O6 CSPRNG swap — both decisions + small code); complete the written reduction
  (O1, O3); engage a reviewer; incorporate findings; submit the measured write-up (Shadow results from item 2
  + the PIR cost numbers already measured). **No public privacy guarantee ships before this.**
- **Depends on:** PIR-SECURITY.md (drafted), items 2 and the O-obligations.
- **Clears:** the honesty-bar gate for claiming provable query privacy (FLAGSHIP-DESIGN.md §0/§8).

## 5. Content-safety partner + legal review (launch blockers)
- **Needs:** legal counsel in the operating jurisdiction, and an agreement with an industry known-illegal-content
  hash-matching partner.
- **Steps:** obtain legal advice before any public crawl (PLAN §4.3, §8); sign the partner agreement; implement
  `asac_safety::HashMatchBackend` against their API (the type seam already forces a backend to exist); stand up
  the manual abuse-report process on top of the `block_host` takedown hook that is already built.
- **Depends on:** nothing in code (the seam is ready).
- **Clears:** the Phase 4 launch gate — these are explicit launch *blockers*, not optional.

## 6. Potato Phase 2 — hybrid post-quantum handshake
- **Needs:** a cryptographer, a formal-methods tool (Tamarin), and engagement with the Tor/Arti community
  (proposal 355) — a private handshake fork would make ASAC a fingerprintable minority (PLAN §3.9).
- **Steps:** implement hybrid ntor-v3 + ML-KEM; multi-cell reassembly with memquota; formal model + 72h fuzzing
  + external review; prefer upstreaming to Arti over a fork.
- **Depends on:** upstream coordination.
- **Clears:** PLAN Phase 2 gate.

## 7. Potato Phase 3 — Swift / Fortress modes
- **Needs:** Arti-internal work (0.47's client config exposes no CGO/Conflux toggle, per the research log) and
  Shadow measurement of the location-inference advantage.
- **Steps:** upstream the needed toggles to Arti (or maintain a minimal fork); implement bounded latency-aware
  middle selection (Swift) and padding/cover traffic (Fortress); measure each mode's leak and cost in Shadow;
  ship Swift only if the §3.3 leak gate passes.
- **Depends on:** Arti upstream; a Shadow testbed.
- **Clears:** PLAN Phase 3 gate.

## 8. Potato Phase 5 — security audit + release engineering
- **Needs:** external auditors.
- **Steps:** freeze a release candidate; reproducible builds, signed releases, a vulnerability-disclosure
  policy; external audit of Potato and ASAC; fix all critical/high findings; closed beta measuring real-world
  latency.
- **Depends on:** everything above being stable.
- **Clears:** PLAN Phase 5 gate (ship readiness).

## 9. Publish the repository
- **Needs:** an interactive terminal with push credentials (this environment's non-interactive shell cannot
  authenticate to GitHub).
- **Steps:** `git push origin main` from an interactive terminal; tag; set up CI to run `cargo test --workspace`
  and `cargo clippy --all-targets -- -D warnings` on every push (the gate this project already holds to locally).
- **Depends on:** nothing.
- **Clears:** the local-only state (all work to date is committed but unpushed).

---

### What this unblocks, shortest path
Item **1** (live validation) and item **9** (push + CI) need nothing but access and can happen immediately.
Item **5** (legal + partner) is the true launch blocker and is purely external. The flagship's credibility
runs through **3 → 2 → 4** (transport, then Shadow measurement, then cryptographer review); the code-side
obligations from PIR-SECURITY.md can be done in parallel in this repo whenever you want to prioritize them.
