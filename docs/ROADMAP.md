# ASAC implementation roadmap

A consolidated, prioritized plan to take ASAC from "working, honestly-scoped prototype" to a credible,
externally-reviewed, shippable private search system. It folds together the engineering backlog
([PLAN.md](../PLAN.md)), the external-dependency plan ([EXTERNAL-PLAN.md](EXTERNAL-PLAN.md)), the security
argument ([PIR-SECURITY.md](PIR-SECURITY.md)), and the findings of the in-repo security audit.

Each phase states its **goal**, **tasks**, and the **gate** that must pass before the claim it unlocks can be
made. Honesty rule throughout: no privacy guarantee is claimed until Phase 2's external review passes.

---

## Where we are today (verified in-repo)

- Engine end to end: crawl over Tor -> safety filter (fail-closed) -> BM25 + PageRank + freshness ranking ->
  v3 onion service; a second arti client fetches `/search` end to end.
- Flagship mechanisms built + unit-tested: single-server PIANO PIR (with a randomized roundtrip property
  test), PIR-by-keyword, private corpus, Loopix-style cover-traffic scheduler, decoy client.
- Metadata unobservability **measured** (scheduler/loopback level): over 6 Shadow trials the timing adversary
  is at chance (1/6) and the searcher's send-gap distribution is KS-indistinguishable from cover clients.
- Nym mixnet transport (`asac-mix-nym`) compiles against the SDK (excluded crate; vendored-dep workarounds).
- Security posture: no `unsafe` (forbidden workspace-wide), bounded concurrency + per-request timeout on the
  onion server, bounded untrusted reads, adversarial parser tests, `cargo audit` in CI.
- Written security reduction drafted (PIR-SECURITY.md §6); **not yet externally reviewed**.

---

## Security audit findings (status)

| ID | Finding | Status |
|----|---------|--------|
| V1 | Onion server: unbounded request tasks + no timeout (DoS) | **Fixed** (semaphore cap + per-request timeout + test) |
| V2 | Arti-tree advisories: `rsa` RUSTSEC-2023-0071, `crypto-bigint` RUSTSEC-2026-0387 (timing) | Disclosed + tracked (upstream, no fix); ignored in `.cargo/audit.toml`, re-review each Arti bump |
| V3 | Unmaintained deps (`bincode`, `paste`) | Tracked (warnings) |
| V4 | PIR query panics on budget exhaustion (`logical_query` `.expect`) | **Open** — Phase 1 (return a graceful error, don't panic) |
| E1 | `cargo audit` in CI | **Done** |
| E2 | Forbid `unsafe` workspace-wide | **Done** |
| E3 | Safety hash over fetched bytes, not extracted text | **Done**; media hook scaffolded (inert, off by default) |

---

## Phase 0 — close the open audit items (small, in-repo)

**Goal:** no known in-repo defects before outreach.
- **V4:** make `asac_pir::single_server::{query, logical_query}` (and the `PrivateKeyword`/`PrivateCorpus`
  callers) return a `Result`/`Option` on budget exhaustion instead of `.expect(...)` panicking; map it to a
  graceful "capacity exceeded, re-preprocess" path. Add a test that exhausting the budget errors, not panics.
- Add **`cargo-deny`** (licenses + banned/duplicate crates + sources) alongside `cargo audit`.
- Add real **`cargo-fuzz`** targets for the HTTP response parser and the PIR wire-decoders (the adversarial
  unit tests are a floor, not a fuzzer).
- **Gate:** workspace green (fmt/clippy/test), no panics on any public input path, fuzz targets build.

## Phase 1 — prove the flagship (evidence + review)

**Goal:** turn "designed + measured" into "externally reviewed," so a bounded privacy claim becomes defensible.
- Finish the written reduction (PIR-SECURITY.md O1/O3 are drafted; tighten per reviewer feedback).
- **Full-path F5:** route the Shadow emitters through the Nym transport (not loopback UDP) and re-run the
  multi-trial measurement; scale N well beyond 6 for a tighter accuracy bound; add the §7 wall-clock test at
  scale.
- Engage an applied cryptographer (see OUTREACH.md) to review O1-O5; incorporate findings.
- Submit the measured write-up to PoPETs/PETS.
- **Gate:** external review of PIR-SECURITY.md passes; measured anonymity numbers meet the documented targets.
  Only then does any public "server cannot learn your query" claim ship.

## Phase 2 — live metadata transport (Nym)

**Goal:** real mixnet transport, not just a seam.
- Wire the scheduler's send loop to `NymTransport`; run the `#[ignore]`d `live_send` against a gateway; pin a
  known-good nym release (track the filed `tokio_with_wasm` yank issue) and drop the vendored workaround when
  upstream republishes.
- Measure end-to-end latency/overhead; document the cost in FLAGSHIP-DESIGN.
- **Gate:** packets verified traversing the live mixnet; full-path F5 (Phase 1) uses this transport.

## Phase 3 — safety + legal (launch blockers)

**Goal:** be fit to run a public index.
- Sign a content-safety hash-matching partner; implement `HashMatchBackend` against their API.
- Only then, turn on the **media stage** behind the existing inert `CrawlConfig::fetch_media` hook: fetch
  referenced media over Tor into memory, **sandbox the image decoders** (separate process / seccomp / a
  memory-safe decoder), perceptual-hash via the partner (PDQ/PhotoDNA), hash-and-discard, report hits. Keep it
  off by default; the HTML-only crawler stays the safe default.
- Obtain legal counsel in the operating jurisdiction before any public crawl; stand up the abuse-report process
  on the existing `block_host` takedown hook.
- **Gate:** Phase 4 launch gate (PLAN §8) — these are blockers, not optional.

## Phase 4 — product & reach (optional, post-flagship)

**Goal:** make it usable by real people without weakening privacy.
- **Browser/WASM client:** ship the PIR client + cover-traffic + UI as `wasm32`, talking to the ASAC server
  over a wasm-capable transport (Nym's browser SDK, or a WebSocket->Tor bridge). The index/server stay native.
  Research the Tor-in-browser leg (the open problem).
- Optional `.potato` petname/alias layer in the client (displays `name.potato`, resolves to the real `.onion`;
  the suffix stays `.onion` on the wire — RFC 7686 is fixed).
- Usability: result quality, crawl coverage, operator docs.

## Phase 5 — release engineering & audit

**Goal:** ship-readiness.
- Reproducible builds, signed releases, a published vulnerability-disclosure policy and `SECURITY.md` triage.
- External security audit of Potato + ASAC (Radically Open Security / Cure53); fix all critical/high.
- Closed beta measuring real-world latency and the anonymity set with real users (privacy comes from the
  crowd).
- **Gate:** PLAN Phase 5 gate.

---

## Dependencies & shortest credible path

Phase 0 is independent and should land first (it's cheap and removes the one open defect). The flagship's
credibility runs **Phase 2 (transport) -> Phase 1 full-path measurement -> Phase 1 review**; the code-side
PIR obligations can proceed in parallel. Phase 3 (safety + legal) is the true launch blocker and is purely
external. Phases 4-5 are post-flagship. See EXTERNAL-PLAN.md for the per-item external contacts.
