# Potato Routing and the ASAC Search Engine: Implementation Plan

Status: draft v0.2, 2026-10-26
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

## 0.1 North star (what makes this worth doing)

Potato as a Tor client is "compose Arti well" — useful, but not novel. The differentiator is **ASAC: a
search engine that is *mathematically unable* to profile you** — privacy you do not have to trust the operator
for. Every existing onion search engine offers a *no-log policy* you must take on faith [verified: Ahmia,
DuckDuckGo onion, and newer "no-log" engines all advertise exactly this]; ASAC aims for operator-blindness
**by construction**.

**The idea that makes this reachable:** search is *latency-tolerant*. A query is tiny and infrequent and can
wait seconds. Tor cannot defeat a global passive adversary because general browsing needs low latency — but
the heavyweight defenses that *do* resist a global adversary (mixnet-style mixing and cover traffic) and that
hide the query itself (private information retrieval) are exactly affordable when you only need them for
search. So ASAC deploys, for the narrow search path, the strong defenses Tor's general-purpose design
cannot: **PIR** (the operator cannot learn *what* you searched) + **mixnet-style cover traffic** (a network
observer cannot learn *that/when* you searched, or link you to the server) + **decoy queries**.

**Honest scope (do not overclaim):** this does not "solve mass surveillance" for general traffic. It targets
*provable query-content privacy* (PIR — a clean cryptographic guarantee) and *measured metadata-unobservability*
(mixing + cover traffic, under stated assumptions: enough cover volume, a mixing-delay budget, an honest
fraction of mix nodes). General browsing through Potato keeps Tor's usual global-adversary limit. The research
contribution is the *combination, applied to the latency-tolerant search niche, with every leak measured* — the
detailed design is in section 4.4.

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
- Search-engine operators, including us. ASAC must not be able to link queries to users.

Out of scope (same as Tor; say this openly to users):
- A global passive adversary watching both ends. Tor's low-latency design cannot stop end-to-end timing
  correlation **(unverified)**. Potato's "Fortress" mode only *reduces* the risk.
  - **Exception — the search path (ASAC).** Because search tolerates latency (see 0.1), ASAC pushes
    *past* Tor's global-adversary limit for queries: PIR for query content and mixnet-style cover traffic for
    metadata. This is in scope *for search only*, under stated assumptions, and is the project's flagship
    research goal (section 4.4). General browsing through Potato keeps the Tor limit above.
- A compromised endpoint device or browser fingerprinting. That is the browser's job.
- **Detecting whether someone is watching your screen, or whether spyware or monitoring software is on your
  device.** A privacy client cannot do this reliably: if the endpoint is already compromised it can lie to us,
  and a false "you are safe" is more dangerous than saying nothing. This is an operating-system and physical-
  security problem, not something Potato or ASAC will claim to solve. See the note in 3.10.

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
- **No PHP and no JavaScript in the stack.** Implementation is Rust (plus unavoidable build and config files). The
  ASAC results page is static, server-rendered HTML with **no client-side JavaScript** — which also removes a
  major fingerprinting and exploit surface and matches the existing "no JS needed" front-end rule (4.2). If a
  dynamic UI ever becomes unavoidable, prefer server-side rendering, or a Rust-to-WASM path, over JavaScript, and
  never PHP.

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
- Library: a vetted ML-KEM implementation. RustCrypto's pure-Rust `ml-kem` crate implements FIPS 203 (final) and
  is the leading candidate [verified: crates.io/crates/ml-kem, github.com/RustCrypto/KEMs]; confirm its audit
  status before shipping. Never a homemade one.
- Size problem: ML-KEM-768 has a 1184-byte encapsulation key and a 1088-byte ciphertext [verified: NIST FIPS 203],
  both far larger than one 509-byte cell body, so handshakes must span cells. Read
  `proposals/249-large-create-cells.txt` for the existing approach.
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
- Reuse Equi-X PoW (`crates/equix`) for ASAC's onion service.
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

### 3.11 Circuit rotation (route "shuffling")
- The request "shuffle the route every N seconds" maps onto Tor's existing behavior: Tor already rotates the
  circuit used for *new* connections once the current one has been dirty for `MaxCircuitDirtiness`, which
  defaults to 10 minutes [verified: forum.torproject.org/t/.../11121, tor.stackexchange.com/questions/262]. So
  Potato tunes this existing knob per mode; it does not invent a new IP-shuffling protocol.
- **Faster is not more private.** Rotating circuits more often means touching more guards and exits per hour,
  which *grows* the number of relays that see you and the surface for correlation, and it pays a fresh
  circuit-build latency each time. So a short interval is a privacy *cost*, not a win.
- **The guard (first hop) is deliberately sticky** and must stay so: persistent guards exist precisely to avoid
  reshuffling the entry point (`spec/guard-spec/algorithm.md`). Potato only varies middle/exit selection within
  the mode's rules (3.3), never the guard on a timer.
- Potato rule: expose rotation as a documented, bounded per-mode setting with a sane default (start from Tor's
  10 minutes), not a "new identity every few seconds" toggle. Already-open streams are never migrated
  mid-connection; rotation applies to new streams, as in Tor.

---

## 4. ASAC search engine

### 4.1 Goal
A search engine that indexes onion services (and optionally the clear web through Potato), runs as an onion
service itself, and is designed so its operators are **mathematically unable to profile users** — operator-blind
by construction (section 4.4), not merely promising not to log.

### 4.2 Architecture

```
user (Tor Browser / Potato client)
        |  onion circuit (Balanced mode)
        v
 ASAC front end  (v3 onion service, no JS needed, no cookies, PoW on)
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
   - **Clear-web crawling (the optional path in 4.1) uses Scrapling**, a Python web-scraping framework
     [verified: github.com/D4Vinci/Scrapling], routed through a Potato/Tor SOCKS proxy. **Hard constraints,
     because several of Scrapling's headline features fight this project's goals:** use only its plain HTTP
     fetcher and HTML parser — JavaScript rendering **off**, stealth browsers **off**, anti-bot / CAPTCHA
     bypass **off**, residential-IP features **off** [verified: those features documented in the Scrapling
     README]. Those exist to evade bot protection and defeat `robots.txt`; crawling here stays polite and
     robots-respecting. Onion services keep the Rust `arti-client` path above — Scrapling's browser mode is
     unnecessary there and a JavaScript/fingerprinting risk the plan forbids (2.3, 3.9). Python is allowed by
     2.3 (only PHP and JavaScript are banned); this is the one place the stack is not Rust.
2. **Content safety filter, before indexing (required, not optional).**
   - Hash-match against known-illegal-content lists through an industry partner, keyword and classifier
     filters, and a manual abuse-report process. Existing onion search engines keep blocklists **(unverified)**.
   - Get legal advice in your jurisdiction before crawling. **This is a launch blocker.**
3. **Indexer**: Tantivy, a Rust full-text search library with Lucene-style BM25 scoring [verified:
   github.com/quickwit-oss/tantivy]. Store only title, URL, extracted text and outgoing links.
4. **Ranking**: v1 BM25. v2 adds link-graph authority (PageRank-style) and freshness. v3 adds spam and phishing
   detection, since cloned scam onion sites are common **(unverified)**.
5. **Front end**: served as a v3 onion service with Equi-X PoW, a plain HTML results page, no third-party
   resources, and no cookies.
6. **Privacy of queries.**
   - v1: no query logs. Only aggregate counters in memory, never per-query. Documented and published.
   - v1: Potato circuits already hide the user's IP from ASAC.
   - v2 and beyond: this is the **flagship** — see 4.4. The no-log policy above is table stakes (every onion
     search engine claims it); 4.4 makes profiling impossible *by construction*, not just promised.

### 4.4 Provable, trustless query privacy (the differentiator)

Goal: an operator (including us) and a network observer **cannot** learn what a user searched — not by promise,
by construction. Three composed mechanisms, each with an honest limit:

1. **PIR (private information retrieval)** — the server answers a query without learning which query it
   answered [verified: PIR definition, CACM; single-server schemes like PIANO, 2024]. A clean cryptographic
   guarantee on query *content*. Limit: PIR is expensive and today practical mainly over a *small* index — so
   prototype over a curated top-N / most-common-queries index first, not the full crawl **(unverified: current
   PIR scale)**.
2. **Mixnet-style cover traffic on the search path** — a Loopix-style layer so a global passive observer
   cannot tell *that* or *when* a user searched, or link them to ASAC [verified: mixnets resist the global
   passive adversary that Tor cannot, via mixing + cover traffic; Loopix, USENIX Security 2017]. Limit: costs
   latency and bandwidth, and needs enough cover volume and an honest fraction of mix nodes; this is why it is
   confined to the latency-tolerant search path, not general browsing.
3. **Decoy queries** (4.3 item 7) — client-side noise, defense in depth.

**Why this is reachable here but not in Tor:** search tolerates seconds of latency, so the mixing delay and
cover traffic a global adversary forces are affordable; general web browsing cannot pay that cost, which is why
Tor leaves the global adversary out of scope. See 0.1.

**Research / award path:** build the PIR prototype over a small index, add the mix transport, *measure* the
query-content guarantee and the metadata anonymity set in Shadow, and write it up. Target a published
PoPETs/PETS result (the Caspar Bowden PET Award is given for published PET papers) **(unverified: current
PoPETs deadlines)**. Do not claim a speed or general-anonymity breakthrough; claim provable query privacy plus
measured search-path unobservability.
7. **Decoy queries (optional, client-side, off by default).** The Potato client can mix the user's real searches
   with random, plausible-looking decoy searches, so anyone watching query *timing and volume* cannot pick the
   real ones out of the noise. This is a TrackMeNot / query-obfuscation style defense **(unverified)**. It is the
   only form of "data poisoning" ASAC ships: the user poisons *their own* query stream to protect themselves,
   never anyone else's data, index, or system.
   - Honest limits, all of which must be documented to the user:
     - It costs bandwidth and server load, so rate-limit it and keep crawler-style politeness. Fortress mode may
       turn it on by default; other modes leave it off.
     - Decoys must be drawn from a realistic query distribution, or they are trivially filtered out again
       **(unverified)**. A fixed or obviously-synthetic word list defeats the purpose.
     - It adds **nothing** against ASAC itself, which keeps no query logs (item 6). Its value is against a
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

### Phase 4: ASAC v1 (weeks 25-40, in parallel with Phase 3)
- Legal review and content-safety pipeline first.
- Crawler, indexer, BM25 ranking, onion front end with PoW.
- Seed with a small, manually curated list of onion services.
- **Gate:** the safety filter is tested, the no-log policy is verified by code review, and a load test passes
  with PoW enabled.

### Phase 5: Hardening and audit (weeks 41-52)
- External security audit of Potato and ASAC.
- Reproducible builds, signed releases, a vulnerability disclosure policy.
- Closed beta, measuring real-world latency.
- **Gate:** all critical and high audit findings are fixed.

### Phase 6 (optional): Separate Potato network
- Only if Phases 1-5 show something that the Tor network cannot support, and only with a real plan for enough
  relays and users. Otherwise, contribute the improvements upstream to Tor. That is the higher-privacy outcome.

### Phase 7: ASAC v2
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
| ASAC query latency | Load test over onion | Under 2 s p90 including the circuit (assumption, tune it) |

---

## 7. Team and skills

- 1-2 Rust network engineers (Arti internals, async Rust).
- 1 applied cryptographer, at least part time (PQ handshake, CGO review, formal models).
- 1 measurement and simulation engineer (Shadow, statistics).
- 1 search engineer (crawling, Tantivy, ranking).
- Legal counsel, and a trust-and-safety process for ASAC.
- External auditors in Phase 5.

---

## 8. Risk register

| Risk | Impact | Mitigation |
|---|---|---|
| Small anonymity set (Potato users are distinguishable) | Users deanonymized | Rule 3.9, run on the Tor network, upstream features |
| Latency-aware routing leaks location | Users located | Middle-only, capped bias, Phase 3 gate |
| PQ handshake bug | Broken secrecy | Hybrid construction, vetted library, formal model, audit |
| Conflux memory DoS on exits | Relay instability | memquota, prop 329 OOM rules, `cfx_enabled` kill switch |
| ASAC indexes illegal content | Legal and moral harm | Pre-index filter, legal review, abuse process (launch blocker) |
| ASAC becomes a surveillance point | Query privacy lost | No logs, onion-only, published policy, PIR research |
| Forking Arti drifts from upstream | Security fixes missed | Depend on crates, keep a minimal fork, rebase monthly |
| Overly aggressive congestion control | Harms other Tor users | Tune in Shadow only, keep Vegas fairness |

---

## 9. First three concrete tasks

1. Create the Rust workspace `potato/` with `arti-client` pinned, and get a SOCKS proxy running through it.
2. Run a Shadow simulation of vanilla Arti to produce baseline latency numbers.
3. Read these fully: `proposals/359-cgo-redux.md`, `proposals/355-revisiting-pq.md`,
   `proposals/329-traffic-splitting.md` and `proposals/324-rtt-congestion-control.txt`.
   Then write the threat-model document.

---

## 10. Research log

Kept so the "research as we go" improvements and their sources stay auditable. Each entry is dated and sourced;
`[verified: ...]` means checked against the cited source, not recalled from memory.

### 2026-10-26 (v0.2)
- ML-KEM-768 sizes confirmed: 1184-byte encapsulation key, 1088-byte ciphertext, 32-byte shared secret
  [verified: NIST FIPS 203]. Both exceed the 509-byte cell body, so the multi-cell handshake (3.2) is required,
  not optional.
- RustCrypto `ml-kem` crate confirmed to exist (pure Rust, FIPS 203 final) [verified: crates.io/crates/ml-kem].
  Independent audit status still to confirm before shipping.
- Tantivy confirmed as a Rust full-text search library with Lucene-style BM25 [verified:
  github.com/quickwit-oss/tantivy].
- Tor `MaxCircuitDirtiness` default confirmed at 10 minutes [verified: Tor Project forum, Tor StackExchange];
  informs the circuit-rotation rule (3.11).
- Policy added: privacy beats latency (section 0); non-goals recorded (2.3) — no PHP/JavaScript, no
  PostgreSQL-source fork, no hand-written C/asm; optional client-side decoy queries (4.3); and the explicit
  no-local-surveillance-detection stance (3.10).
- Tooling decision: clear-web scraping uses Scrapling [verified: github.com/D4Vinci/Scrapling], restricted to its
  plain HTTP fetcher over Tor with JS rendering and all anti-bot/stealth/CAPTCHA-bypass features off (4.3 #1).

### 2026-10-26 (Phase 0 kickoff)
- Created the Rust workspace with the `potato` binary crate depending on `arti-client` pinned at `=0.47.0`
  [verified: crates.io], and a connectivity smoke test written against the verified 0.47 API
  [verified: docs.rs/arti-client 0.47.0]. Manifests validated offline [verified: `cargo verify-project`].
- **Toolchain blocker found:** Arti 0.47 crates require **Rust 1.92** [verified: `cargo generate-lockfile`
  output], but the build host has a standalone Rust 1.89.0 with no rustup, so it cannot compile here yet.
  Pinned the requirement in `rust-toolchain.toml`; building needs Rust >= 1.92 (install via rustup).
- Confirmed `tor-socksproto v0.47.0` is in the dependency tree [verified: lockfile], the crate Phase 1 reuses
  for the SOCKS proxy. Note: `arti-client` has no embeddable SOCKS API yet, so the proxy is Arti's own
  `arti proxy` binary for now [verified: docs.rs/arti-client].

### 2026-10-26 (Phase 0 build is green)
- The workspace compiles and tests pass on **Rust 1.92.0** (installed via rustup in WSL Ubuntu 24.04)
  [verified: `cargo test` run, 4m20s build]. `potato` builds `arti-client` with **rustls** (no system OpenSSL)
  and **static-sqlite** (SQLite built from bundled source, so no system libsqlite3 is needed); `static` was
  avoided because it would pull in native-tls [verified: docs.rs/arti-client feature docs + build error then fix].
- Expanded into a multi-crate workspace: `potato` (client bin), `potato-policy` (routing modes, 3 tests),
  `potarek-decoy` (decoy-query generator, 4 tests). Added CI (fmt/clippy/test on 1.92), issue/PR templates,
  CODEOWNERS, dependabot, rustfmt, cargo-deny config, and `docs/ARCHITECTURE.md`.
- Note: the Windows host has only standalone Rust 1.89 and no MSVC linker, so the supported build path is
  Linux (WSL or native). Recorded in the crate README.

### 2026-10-26 (Phase 0 test network: chutney)
- Added `testnet/`: a chutney harness (`setup-chutney.sh`, `run-chutney.sh`) and a scripted+documented Shadow
  setup (`shadow/`). Verified this session that a chutney private Tor network **launches and forms circuits**
  [verified: debug run showed clients connecting to the exit, `failure_count == 0`].
- Prerequisite learned: chutney needs **`tor-gencert`** (builds authority keys) in addition to `tor`; both ship
  in the Debian/Ubuntu `tor` package [verified: packages.debian.org file list]. The Tor Expert Bundle ships
  `tor` but not `tor-gencert`, so it cannot build a network from scratch — the harness now requires the system
  `tor` package.
- Limitation: chutney's byte-transfer self-test (`chutney verify`) uses `asyncore`/`asynchat`, removed in
  Python 3.12; the `pyasyncore` backport lets chutney run but does not drive the self-test's I/O loop
  [verified: tasks stay "not done"]. Bootstrap (what Phase 0 needs) is unaffected; run the self-test on Python
  <= 3.11 if needed. Documented in `testnet/README.md`.
- With the system `tor` installed, a fresh network's authorities reach 100% and publish a full consensus
  [verified: run showed authorities at 100%, all nodes in `md_cons/ns_cons`, "Dir info cached"], but relays and
  clients can stall at 95% "Establishing a Tor circuit" under WSL + tor 0.4.9.11 within the wait window. Raised
  the wait to `CHUTNEY_START_TIME=180` in `run-chutney.sh`; the newer tor 0.4.9.14 converged further. Documented
  with mitigations in `testnet/README.md`.
- **Resolved with tor 0.4.9.14** (official Tor apt repo): a full run is green on WSL2 + Python 3.12 —
  "Everything bootstrapped after 52 sec", `BOOTSTRAP_OK`, `Transmission: Success`, `BOOTSTRAP_RC=0 VERIFY_RC=0`
  [verified: run output]. Correction to the earlier note: the byte self-test is NOT broken by the pyasyncore
  backport; it failed only because the old tor had not fully bootstrapped (no circuits -> no traffic). chutney
  tier of Phase 0 is now fully verified end-to-end.
- Shadow remains scripted-but-not-run (needs a sudo build; WSL2 support unverified).

### 2026-10-26 (Phase 0 benchmark harness)
- Added `crates/potato-bench`: a SOCKS5 benchmark measuring connect/circuit-setup time, TTFB, total fetch time,
  and throughput; point `--proxy` at vanilla Arti's SOCKS (baseline), a chutney client port, or Potato later.
  Dependency-light (tokio + anyhow), SOCKS5 encoder unit-tested (2 tests pass) [verified: `cargo test`].
- Live-validated end to end against real Tor via the system daemon on 127.0.0.1:9050: 3/3 fetches of
  example.com succeeded, median TTFB ~228 ms [verified: run output]. Circuit-build time is left to Arti's
  own stats (PLAN.md 6), and throughput is only meaningful for a large download (guarded against tiny-response
  divide-by-zero) — documented in the crate README.
- Clippy is not installed in the local WSL toolchain (rustup `--profile minimal`); CI installs it.
- **Vanilla-Arti baseline measured** (arti CLI v2 `proxy`, SOCKS 9150, example.com, 5 reps): median TTFB
  **235 ms**, steady-state connect 144-409 ms; the first connect (~14.7 s) includes Arti's full bootstrap
  [verified: run output + arti log "Sufficiently bootstrapped; proxy now functional"]. Comparable to the
  C-tor path (median 228 ms). Throughput needs a large target; the tiny page reads 0.0 by the guard. This is
  the Phase 0 baseline the gate asks for; Shadow-side baseline still pending.

### 2026-10-26 (Phase 0 threat-model doc + toolchain verified)
- Wrote the standalone threat model as `docs/THREAT-MODEL.md` (adversaries + guarantees, explicit out-of-scope,
  assumptions/trust boundaries, modes-vs-threats, residual risks), expanding PLAN.md 2.1 — the last non-tooling
  Phase 0 deliverable.
- Verified the dev toolchain end to end [verified: install check]: Rust 1.92 + clippy + rustfmt, tor 0.4.9.14 +
  tor-gencert, arti 2.7.0, libssl/ libsqlite3/ libclang/ glib (t64) dev libs, python3-networkx, the asyncore
  backports, and cargo-deny/fuzz/audit. Only `libc-dbg` (a Shadow dep) may still be missing; the Shadow build
  will confirm.

### 2026-10-26 (Shadow verified running)
- Built Shadow 3.3.0 from source and ran a minimal simulation on WSL2 (kernel 6.6): "Running/Finished
  simulation", and the simulated host executed and printed output [verified: run output, SHADOW_RC=0]. No
  kernel tuning and no `libc-dbg` needed on Ubuntu 24.04. Both test-network tiers (chutney + Shadow) now work.
- Phase 0 deliverables complete: Rust workspace, test network (chutney green + Shadow running), benchmark
  harness + live Arti baseline (235 ms median TTFB), and the standalone threat model. Remaining for the Phase 0
  *gate*: the baseline measured **inside Shadow** (a tornettools-generated Tor-network simulation), now
  unblocked — a scaled measurement run, not a tooling gap.

### 2026-10-26 (Phase 0 complete: in-Shadow baseline measured)
- Built `tgen` from source and ran Shadow's Simple Tor Network example: a full simulated Tor network (authority
  + 4 relays + 2 exits + tor client) completed 30 sim-minutes in ~28 s real, 20 client streams succeeded; a
  1 MiB download over the simulated Tor path gave TTFB ~800 ms, last-byte ~2.1 s [verified: run output,
  "Shadow completed successfully"]. Self-consistent in-Shadow baseline (fixed 50 ms-latency topology; not
  directly comparable to live numbers).
- **Phase 0 gate satisfied**: baseline measured both in Shadow (above) and on the live network (vanilla Arti,
  235 ms median TTFB). Caveat: the in-Shadow run uses C tor (the standard Shadow example); measuring Arti
  specifically inside Shadow is a follow-up refinement, not a Phase 0 blocker.
- **Phase 0 is complete.** Deliverables: Rust workspace on Arti, two-tier test network (chutney + Shadow, both
  verified running), benchmark harness + both baselines, and the standalone threat model.

### 2026-10-26 (Phase 1 kickoff: Balanced-mode config)
- Added `crates/potato/config/balanced.toml` + `run-balanced.sh`: Balanced mode expressed as a config over
  Arti's own SOCKS proxy (`socks_listen=9150`, `vanguards.mode="lite"`, `channel.padding="normal"`,
  `circuit_timing.max_dirtiness="10 minutes"`, preemptive-circuit tuning), each key mapped to a PLAN section
  and verified against arti 2.7.0's example config. Verified live: arti accepts the config and the proxy
  bootstraps and listens on 127.0.0.1:9150 [verified: run — "Successfully reloaded configuration", "proxy now
  functional", SOCKS on 9150].
- **Scope correction to Phase 1:** Arti 2.7.0's client config exposes **no CGO or Conflux toggle** [verified:
  not present in arti-example-config.toml]. CGO is negotiated per hop and Conflux is consensus-driven, so both
  are internal; "force CGO / enable Conflux" is upstream work, not a client-config change, and Balanced relies
  on Arti's defaults. Strict stream isolation is driven by the SOCKS client (Tor Browser isolates by
  first-party automatically). The fingerprint and page-load gates still stand.

### 2026-10-26 (Phase 1 Balanced vs baseline measurement)
- Benchmarked example.com through the Balanced proxy (SOCKS 9150): median TTFB ~480 ms over 5 reps
  [verified: run], versus vanilla Arti's ~235 ms [verified: earlier run]. **Balanced was not faster.** This
  matches the scope finding: the latency wins (Conflux, Vegas tuning) are not client-config-exposed, while
  vanguards-lite + full padding add cost. Caveat: n=5 on the live network is high-variance and the page is
  tiny, so this is directional only — a rigorous comparison belongs in Shadow with many reps.
- **Consequence:** the Phase 1 gate "lower median/p90 page-load than baseline" is not achievable by
  configuration alone. Realistic options: (a) treat Balanced as "Tor-equivalent privacy, not faster" and drop
  the speed gate; (b) pursue upstream Arti support for client-controlled Conflux/Vegas before claiming a speed
  win.
- **Decision (2026-10-26): option (a) — privacy over speed.** Balanced is "Tor-equivalent privacy with
  vanguards-lite on"; the Phase 1 "faster than baseline" speed gate is dropped. Conflux/Vegas latency work, if
  pursued, goes upstream to Arti, never claimed as a Potato speed win. Phase 1's remaining item is the
  wire-fingerprint check (Balanced must stay indistinguishable from stock Arti).

### 2026-10-26 (Phase 1 complete: wire-fingerprint check)
- Fingerprint check done by config-equivalence (`docs/FINGERPRINT.md`): Potato Balanced is the stock `arti`
  binary + `balanced.toml`, and every config value equals Arti 2.7.0's default [verified: arti-example-config
  .toml], so Balanced is wire-indistinguishable from stock Arti. Dropped the one deviation
  (`min_exit_circs_for_port` 3 to 2). An empirical cell-trace comparison is deferred to the Phase 3 Shadow work.
- **Phase 1 complete.** Balanced client = a stock-Arti-equivalent privacy posture over Arti's SOCKS proxy
  (vanguards-lite, full padding, standard rotation), pinned explicitly; speed gate dropped (privacy over speed).
  Deliverables: `potato-policy` modes, `balanced.toml` + run script (verified bootstrapping on 9150), the
  Balanced-vs-baseline benchmark, and the fingerprint analysis.

### 2026-10-26 (North-star pivot: trustless query privacy)
- Reframed the project around a defensible differentiator (new sections 0.1 and 4.4, plus `docs/
  DIFFERENTIATION.md`): ASAC as a search engine that is *mathematically unable* to profile users, versus
  the no-log *policies* every existing onion search engine advertises [verified: Ahmia/DuckDuckGo-onion/others].
- Key insight recorded: search is latency-tolerant, so it can afford the anti-global-adversary defenses Tor
  cannot — mixnet-style cover traffic (metadata) + PIR (query content) + decoy queries [verified: mixnets
  resist the GPA via mixing/cover traffic (Loopix); PIR hides the query from the server (PIANO/CACM)]. Carved a
  search-path exception into the threat model (2.1), honestly scoped: provable query-*content* privacy +
  *measured* metadata-unobservability under stated assumptions, NOT a general "mass surveillance solved" claim.
- This becomes the flagship research/paper track (PoPETs/PETS). The routing layer stays "compose Arti well."

### Open items to research in upcoming phases
- Phase 0: confirm `chutney` is still the current Tor test-network tool, and whether Shadow runs Arti directly
  **(unverified)**.
- Phase 2: ML-KEM crate audit status; exact `proposals/249` large-cell handshake mechanics; formal-analysis tool
  choice (Tamarin vs ProVerif).
- Phase 3: survey published latency-aware-routing attacks to bound `k` in 3.3; measure cover-traffic cost.
- Phase 4: realistic decoy-query distribution sources; onion-content blocklist partners and the legal review.
