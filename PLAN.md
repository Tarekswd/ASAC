# Potato Routing and the Potarek Search Engine: Implementation Plan

Status: draft v0.1, 2026-10-26
Sources: Tor spec repo `torspec` at commit `f0cda49b` (2026-10-22) and Arti at commit `c87b1abe` (2026-10-22), both cloned and read for this plan.
Paths like `spec/tor-spec/...` and `proposals/...` refer to the torspec repo, and `crates/...` to Arti.
Anything marked **(unverified)** comes from memory, not from the sources. Check it before relying on it.

---

## 0. Read this first: three hard truths

1. **Privacy comes from the crowd, not only from the cryptography.** A new network with 50 users and 20 relays is
   *less* private than Tor, however good its crypto. So Potato starts as a **client and protocol layer that runs
   on the existing Tor network**. It only becomes its own network if a phase gate shows that is worth it (Phase 6).
2. **"More privacy" and "lower latency" pull in opposite directions.** Every latency trick (fewer hops, routing
   by latency, multipath) leaks something. Potato therefore has **modes**: the user picks the trade-off, and each
   mode's leak is written down, not hidden. **Project priority (set 2026-10-26): privacy beats latency.** Every
   latency feature is optional and gated, and none ships if it measurably weakens privacy. When the two conflict,
   favor the more private option and leave the speed on the table. The default experience is tuned for privacy,
   not for benchmarks.
3. **Never write crypto from scratch.** Everything is built on Arti (Tor's Rust implementation, licensed
   `MIT OR Apache-2.0`, `crates/tor-proto/Cargo.toml:7`) and on audited libraries. Potato's new parts are
   *policy and composition*, not new primitives.

---

## 1. What Tor already gives us (the base we build on)

| Area | What Tor does today | Source |
|---|---|---|
| Cells | Fixed 514-byte cells with a 509-byte body | `spec/tor-spec/preliminaries.md:44-48` |
| Circuit handshake | CREATE2 using ntor-v3, which every relay supports and every client uses | `spec/tor-spec/create-created-cells.md:50,206` |
| Handshake extensions | ntor-v3 can carry extra data | `spec/tor-spec/create-created-cells.md:296-306,518-520` |
| Relay crypto ("tor1") | Layered stream cipher, client encrypts for hop N...1. AES-128-CTR with SHA1 (AES-256-CTR for v3 onion services) | `spec/tor-spec/routing-relay-cells.md:32-40`, `crates/tor-proto/src/crypto/cell/tor1.rs:38` |
| New relay crypto (CGO) | Non-malleable onion encryption with a 128-bit tag (tor1 has a 16-bit digest), forward security, expected to be faster. Proposal status: Open. Already implemented in Arti. | `proposals/359-cgo-redux.md`, `crates/tor-proto/src/crypto/cell/cgo.rs:1-11` |
| Post-quantum | Options for hybrid ntor + ML-KEM handshakes. Informational only, not deployed. | `proposals/355-revisiting-pq.md:1-62` |
| Path rules | No two relays from the same family or the same /16 (IPv4) or /32 (IPv6) | `spec/path-spec/path-selection-constraints.md:23-28` |
| Path length | 3 hops; circuit build timeout learned from a Pareto fit | `spec/path-spec/learning-timeouts.md:29-31,61` |
| Guards | Persistent sampled, filtered and primary guard sets | `spec/guard-spec/algorithm.md:20-97` |
| Vanguards | Extra fixed layers for onion services. Vanguards-Lite must be the default. | `spec/vanguards-spec/index.md:42-57` |
| Congestion control | RTT-based TOR_VEGAS replaced fixed SENDME windows, which capped a stream at about 500 KB/s. Default since 0.4.8. | https://spec.torproject.org/proposals/324-rtt-congestion-control.html |
| Multipath (Conflux) | Two linked circuits with different guard and middle relays. MinRTT and LowRTT schedulers. Kill switch `cfx_enabled`. | https://spec.torproject.org/proposals/329-traffic-splitting.html |
| Padding | Event-driven padding state machines on client and relay, with histograms | `spec/padding-spec/circuit-level-padding.md:6-31` |
| DoS defence | Equi-X proof of work for onion services, plus memory-exhaustion defences | `spec/hspow-spec/v1-equix.md:1-11`, `spec/dos-spec/` |
| Arti crates | `tor-proto` (has `conflux/`, `congestion/`, `crypto/cell/`), `tor-circmgr` (has `path/`, `timeouts/`, `preemptive.rs`), `tor-guardmgr`, `tor-hsclient`, `tor-hsservice`, `arti-client`, `equix`, `tor-memquota` | `ls arti/crates`, `ls crates/tor-proto/src`, `ls crates/tor-circmgr/src` |

**Key finding:** most "lower latency" ideas (Vegas, Conflux) and "stronger crypto" ideas (CGO) already exist in
Tor or Arti, but some are still off by default or unfinished. The cheapest big win for Potato is to **turn the
best existing pieces on together, tune them, and measure the result**, not to invent new ones.

---

## 2. Potato design goals

### 2.1 Threat model

Protect against:
- A local network observer (ISP, Wi-Fi operator), who must not learn the destination.
- A single malicious relay, which must not learn both source and destination.
- Some malicious relays, which must not be able to tag or modify cells (CGO is non-malleable).
- A destination server, which must not learn the client's IP address.
- A quantum adversary recording traffic now and decrypting it later (hybrid PQ handshake).
- Search-engine operators, including us. Potarek must not be able to link queries to users.

Out of scope (same as Tor; say this openly to users):
- A global passive adversary watching both ends. Tor's low-latency design cannot stop end-to-end timing
  correlation **(unverified)**. Potato's "Fortress" mode only *reduces* the risk.
- A compromised endpoint device or browser fingerprinting. That is the browser's job.
- **Detecting whether someone is watching your screen, or whether spyware or monitoring software is on your
  device.** A privacy client cannot do this reliably: if the endpoint is already compromised it can lie to us,
  and a false "you are safe" is more dangerous than saying nothing. This is an operating-system and physical-
  security problem, not something Potato or Potarek will claim to solve. See the note in 3.10.

### 2.2 Modes

| Mode | Hops | Latency features | Privacy features | Leak written down |
|---|---|---|---|---|
| **Swift** | 3 | Conflux MinRTT, bounded latency-aware middle choice, preemptive circuits, tuned Vegas | CGO, hybrid PQ, family rules | Latency-aware choice leaks a little about client location (bounded, see 3.3) |
| **Balanced** (default) | 3 | Conflux, tuned Vegas, preemptive circuits | CGO, hybrid PQ, padding machines, strict stream isolation | Same as Tor plus the Conflux linking signal (prop 329, security concerns) |
| **Fortress** | 3 + vanguards-lite | No latency-aware choice | Everything in Balanced, plus constant-rate cover traffic and timing jitter | Higher latency and bandwidth cost |

Because privacy beats latency here (section 0), **Balanced is the default and Fortress is the showcase mode.**
Swift exists only for users who knowingly accept its documented location leak, and it is the first thing cut if
its 3.3 gate does not pass.

### 2.3 Non-goals and rejected alternatives

Decisions recorded so they are not re-litigated. Each is a deliberate "no", with the reason:

- **No new cryptographic primitives.** Enhancement means *finishing, enabling and getting external review of*
  what Arti already has (CGO, hybrid PQ), not inventing ciphers or handshakes. This is truth 3 restated.
- **No forking the PostgreSQL (or any database) source to build a custom store.** The index is Tantivy, which is
  the right tool for BM25 search (4.3). If a relational store is ever needed, depend on PostgreSQL as software,
  never fork its source: a fork is a permanent security-patch and maintenance burden for no privacy gain.
- **No hand-written C or assembly in Potato's own code.** Memory safety is a core privacy property here, and the
  audited crypto libraries already use assembly internally where it matters **(unverified)**. Dropping to C or
  asm would add memory-safety and timing-side-channel risk. Reconsider only for one measured hotspot, and only
  behind a vetted library with constant-time guarantees.
- **No claim to detect local surveillance of the user's device** (see 2.1 out-of-scope and 3.10).

---

## 3. Potato protocol components

Each item below includes: what it is, where it lives in Arti, what is new, and how we know it works.

### 3.1 Relay crypto: CGO everywhere
- Use Arti's existing `crates/tor-proto/src/crypto/cell/cgo.rs` and negotiate it per hop.
- New work: a policy that forces CGO in every mode, and refuses tor1 hops when the user sets `require-cgo`.
- Note: Arti documents that CGO *decryption* is malleable even though encryption is not
  (`cgo.rs:10-11`). Read proposal 359 fully before relying on its tagging resistance in both directions.
- Check: Arti's own test vectors, plus an interop test against C tor on a private test network.

### 3.2 Handshake: hybrid post-quantum ntor-v3
- Base: ntor-v3 (`create-created-cells.md:296`) combined with ML-KEM, following the options in `proposals/355`.
- Library: a vetted ML-KEM implementation, such as RustCrypto `ml-kem` **(unverified)**. Never a homemade one.
- Size problem: ML-KEM keys and ciphertexts are larger than one 509-byte cell body **(unverified, check the
  ML-KEM-768 sizes)**, so handshakes must span cells. Read `proposals/249-large-create-cells.txt` for the
  existing approach.
- Hybrid rule: the session key = KDF(x25519 secret || ML-KEM secret), so breaking one still leaves the other.
- Check: formal model in Tamarin or ProVerif **(unverified tool choice)**, plus fuzzing the cell parser.

### 3.3 Latency-aware path selection (the risky part)
- Problem: choosing relays by latency to *you* leaks where *you* are. Published attacks exist on naive
  latency-aware routing **(unverified)**.
- Potato rule: the guard and exit stay chosen by bandwidth weight exactly as in Tor. Only the **middle** relay may
  be biased, with a hard cap: no relay's selection probability may change by more than a factor `k` (start with
  `k = 2`) from its bandwidth weight.
- Latency source: RTTs that congestion control already measures (prop 324), not new active probes, so we add no
  new traffic fingerprint.
- Lives in: `crates/tor-circmgr/src/path/`.
- Check: simulate with the Shadow simulator (prop 329 also tunes its schedulers with Shadow), then measure
  (a) the latency gain and (b) the attacker's location-guessing advantage. **Gate: if (b) is not small, Swift
  mode ships without this feature.**
- **Priority note:** given section 0, this feature is deprioritized. It is the most privacy-risky item in the
  plan and buys only latency, which we have said we do not need. Treat it as optional and expect it to be cut.

### 3.4 Multipath: Conflux tuned
- Use Arti's `crates/tor-proto/src/conflux/`.
- Swift uses MinRTT and Balanced uses LowRTT, both from prop 329. Keep the reorder-queue memory limits.
- Respect the `cfx_enabled` kill switch.

### 3.5 Congestion control: Vegas tuning
- Keep TOR_VEGAS. Tune `cc_cwnd_init`, `cc_sendme_inc` and the Vegas alpha/beta thresholds (prop 324 parameter
  list) for faster ramp-up, in Shadow only.
- Never send unfair (overly aggressive) congestion control on the live network. It hurts other users, and prop 324
  notes it raises relay memory use.

### 3.6 Fewer round trips at setup
- Preemptive circuits are already in `crates/tor-circmgr/src/preemptive.rs`. Tune pool size per mode.
- Research item: ntor-v3 extra data could carry an early stream request to save one round trip. This needs
  replay-safety analysis first, because early data in TLS 1.3 has known replay problems **(unverified)**.
  Behind a feature flag until reviewed.
- Circuit build timeouts: keep Tor's Pareto learner (`learning-timeouts.md`) and retry quickly on slow builds.

### 3.7 Traffic analysis resistance
- Padding: write Potato padding machines in Tor's existing state-machine format
  (`padding-spec/circuit-level-padding.md`), not a new padding system.
- Fortress mode: constant-rate cover cells per circuit plus small random sending delays. Cost model required:
  bandwidth overhead in percent and added latency.
- Strict stream isolation: one circuit per first-party site, per Tor's stream-isolation rules
  (`spec/path-spec/stream-isolation.md`).

### 3.8 DoS and resource safety
- Reuse Equi-X PoW (`crates/equix`) for Potarek's onion service.
- Reuse `tor-memquota` for every new queue (Conflux reorder queue, PQ handshake reassembly buffers).

### 3.9 Wire compatibility and versioning
- Every Potato feature is negotiated through Tor's subprotocol versioning (`spec/tor-spec/subprotocol-versioning.md`).
- **Potato clients must look like normal Arti clients to relays.** A unique Potato fingerprint would shrink
  the anonymity set to "Potato users". This rule beats any feature in this plan.

### 3.10 What we deliberately do not build: local-surveillance detection
- A recurring request is "tell me if someone is watching my browser or my device." Potato does not do this, and
  saying why is part of being honest with users.
- If the device is already compromised, any detector running on it can be fed false data, so a "clean" result
  means nothing. A false sense of safety is worse than none for an anonymity tool.
- Webcam-based "is someone looking at my screen" detection would mean turning on the camera and running a face
  detector — adding an attack surface and a privacy risk to *defend* privacy. We will not.
- Honest guidance we can give users instead: use full-disk encryption, keep the OS patched, use the OS's own
  "which apps used the camera/mic" indicators, a privacy screen filter against shoulder-surfing, and Tor
  Browser's own anti-fingerprinting. These live in the user's OS and browser, not in Potato. **(unverified:
  specific OS feature names not checked this session.)**

---

## 4. Potarek search engine

### 4.1 Goal
A search engine that indexes onion services (and optionally the clear web through Potato), runs as an onion
service itself, and is designed so its operators cannot build user profiles.

### 4.2 Architecture

```
user (Tor Browser / Potato client)
        |  onion circuit (Balanced mode)
        v
 Potarek front end  (v3 onion service, no JS needed, no cookies, PoW on)
        |
 query service  ->  index shards (Tantivy, BM25)
        ^
 indexer  <-  content filter  <-  crawler (arti-client, Potato circuits)
                                     ^
                          seed list + discovered links
```

### 4.3 Components

1. **Crawler**: Rust, using the `arti-client` crate (exists, `ls arti/crates`).
   - Separate circuits per target onion service (stream isolation), polite rate limits, respects `robots.txt`.
   - Fetches HTML and text only. Never runs JavaScript. Never downloads media.
2. **Content safety filter, before indexing (required, not optional).**
   - Hash-match against known-illegal-content lists through an industry partner, keyword and classifier
     filters, and a manual abuse-report process. Existing onion search engines keep blocklists **(unverified)**.
   - Get legal advice in your jurisdiction before crawling. **This is a launch blocker.**
3. **Indexer**: Tantivy (Rust full-text search, BM25) **(unverified)**. Store only title, URL, extracted text and
   outgoing links.
4. **Ranking**: v1 BM25. v2 adds link-graph authority (PageRank-style) and freshness. v3 adds spam and phishing
   detection, since cloned scam onion sites are common **(unverified)**.
5. **Front end**: served as a v3 onion service with Equi-X PoW, a plain HTML results page, no third-party
   resources, and no cookies.
6. **Privacy of queries.**
   - v1: no query logs. Only aggregate counters in memory, never per-query. Documented and published.
   - v1: Potato circuits already hide the user's IP from Potarek.
   - v2 research: private information retrieval (PIR), so the server cannot see the query even in memory. Practical
     only for small indexes today **(unverified)**. Prototype on a top-N result cache first.
7. **Decoy queries (optional, client-side, off by default).** The Potato client can mix the user's real searches
   with random, plausible-looking decoy searches, so anyone watching query *timing and volume* cannot pick the
   real ones out of the noise. This is a TrackMeNot / query-obfuscation style defense **(unverified)**. It is the
   only form of "data poisoning" Potarek ships: the user poisons *their own* query stream to protect themselves,
   never anyone else's data, index, or system.
   - Honest limits, all of which must be documented to the user:
     - It costs bandwidth and server load, so rate-limit it and keep crawler-style politeness. Fortress mode may
       turn it on by default; other modes leave it off.
     - Decoys must be drawn from a realistic query distribution, or they are trivially filtered out again
       **(unverified)**. A fixed or obviously-synthetic word list defeats the purpose.
     - It adds **nothing** against Potarek itself, which keeps no query logs (item 6). Its value is against a
       network observer, or a future compromise of a server that was supposed to be log-free.
     - Decoys must be drawn only from a vetted, benign term list and must never issue searches that could cause
       real-world harm (illegal, harassing, or dangerous terms). This constraint is part of the feature, not an
       afterthought.
   - Check: a statistical test that an observer cannot separate real from decoy queries above chance, given the
     chosen decoy distribution.

---

## 5. Phased roadmap

Each phase ends with a gate. Don't start the next phase until the gate passes.

### Phase 0: Foundations (weeks 1-4)
- Set up a Rust workspace that depends on Arti crates (pin a version). Fork only if needed.
- Private test network: run a local C tor and Arti network (Tor's `chutney` tool **(unverified)**) plus Shadow.
- Benchmark harness that records time-to-first-byte, page load time, throughput and circuit build time.
- Write the threat model (section 2.1) as a standalone document.
- **Gate:** a vanilla Arti baseline is measured in Shadow and on the live network.

### Phase 1: Potato client, Balanced mode (weeks 5-12)
- Policy layer: force CGO, enable Conflux, add strict isolation, tune preemptive circuits.
- Reuse Arti's SOCKS proxy (`tor-socksproto`) so browsers can connect.
- Fingerprint test: compare Potato's and plain Arti's wire behaviour as relays see it.
- **Gate:** Shadow shows lower median and p90 page-load time than the baseline, with no wire-distinguishability
  from Arti beyond negotiated features.

### Phase 2: Hybrid post-quantum handshake (weeks 13-24)
- Implement the hybrid ntor-v3 + ML-KEM option chosen from proposal 355.
- Multi-cell handshake reassembly with memquota limits.
- Formal model, fuzzing, and test vectors.
- Engage the Tor community: a proposal upstream is better than a private fork, because a private handshake makes
  you a fingerprintable minority.
- **Gate:** the formal model passes, fuzzing finds no crashes in 72 hours, and an external cryptographer reviews it.

### Phase 3: Swift and Fortress modes (weeks 25-36)
- Swift: bounded latency-aware middle selection (3.3) and Conflux MinRTT.
- Fortress: padding machines and cover traffic (3.7).
- Measure the location-inference advantage for Swift and the cost for Fortress.
- **Gate:** each mode's leak and cost is quantified and documented. Swift ships only if the 3.3 gate passes.

### Phase 4: Potarek v1 (weeks 25-40, in parallel with Phase 3)
- Legal review and content-safety pipeline first.
- Crawler, indexer, BM25 ranking, onion front end with PoW.
- Seed with a small, manually curated list of onion services.
- **Gate:** the safety filter is tested, the no-log policy is verified by code review, and a load test passes
  with PoW enabled.

### Phase 5: Hardening and audit (weeks 41-52)
- External security audit of Potato and Potarek.
- Reproducible builds, signed releases, a vulnerability disclosure policy.
- Closed beta, measuring real-world latency.
- **Gate:** all critical and high audit findings are fixed.

### Phase 6 (optional): Separate Potato network
- Only if Phases 1-5 show something that the Tor network cannot support, and only with a real plan for enough
  relays and users. Otherwise, contribute the improvements upstream to Tor. That is the higher-privacy outcome.

### Phase 7: Potarek v2
- Link-graph ranking, spam and phishing detection, and the PIR prototype.

---

## 6. Measurement plan

| Metric | Tool | Target |
|---|---|---|
| Time to first byte, median and p90 | Shadow and live benchmark harness | Below the vanilla Arti baseline (Balanced, Swift) |
| Circuit build time | Arti timeout stats | No worse than baseline |
| Throughput | Bulk download test | No worse than baseline |
| Location-inference advantage (Swift) | Shadow plus an attacker model | Below a documented threshold |
| Wire distinguishability | Packet and cell trace comparison against Arti | None beyond negotiated features |
| Relay memory under Conflux and PQ | memquota stats | Within limits, no OOM kills under load |
| Potarek query latency | Load test over onion | Under 2 s p90 including the circuit (assumption, tune it) |

---

## 7. Team and skills

- 1-2 Rust network engineers (Arti internals, async Rust).
- 1 applied cryptographer, at least part time (PQ handshake, CGO review, formal models).
- 1 measurement and simulation engineer (Shadow, statistics).
- 1 search engineer (crawling, Tantivy, ranking).
- Legal counsel, and a trust-and-safety process for Potarek.
- External auditors in Phase 5.

---

## 8. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| Small anonymity set (Potato users are distinguishable) | Users deanonymized | Rule 3.9, run on the Tor network, upstream features |
| Latency-aware routing leaks location | Users located | Middle-only, capped bias, Phase 3 gate |
| PQ handshake bug | Broken secrecy | Hybrid construction, vetted library, formal model, audit |
| Conflux memory DoS on exits | Relay instability | memquota, prop 329 OOM rules, `cfx_enabled` kill switch |
| Potarek indexes illegal content | Legal and moral harm | Pre-index filter, legal review, abuse process (launch blocker) |
| Potarek becomes a surveillance point | Query privacy lost | No logs, onion-only, published policy, PIR research |
| Forking Arti drifts from upstream | Security fixes missed | Depend on crates, keep a minimal fork, rebase monthly |
| Overly aggressive congestion control | Harms other Tor users | Tune in Shadow only, keep Vegas fairness |

---

## 9. First three concrete tasks

1. Create the Rust workspace `potato/` with `arti-client` pinned, and get a SOCKS proxy running through it.
2. Run a Shadow simulation of vanilla Arti to produce baseline latency numbers.
3. Read these fully: `proposals/359-cgo-redux.md`, `proposals/355-revisiting-pq.md`,
   `proposals/329-traffic-splitting.md` and `proposals/324-rtt-congestion-control.txt`.
   Then write the threat-model document.
