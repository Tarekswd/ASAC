# ASAC Flagship Design: Provable, Trustless Query Privacy

**Status: design / spec.** Nothing here is implemented beyond the existing `asac-pir` PIANO prototype and the
`asac-search` top-1-per-term cache. This document plans the work that turns ASAC's slogan — "a search engine
mathematically unable to profile you" — into something defensible. It is the plan for PLAN.md §4.4.

## 0. The honesty bar (read first)

We do **not** claim a privacy guarantee until (a) the scheme has a written security argument, (b) an external
cryptographer has reviewed it, and (c) the implementation is measured, not just asserted. Until then this is
"query privacy by construction, prototype stage," never "proven." Any public messaging that outruns this bar
is a bug. The slogan is the *target*, not the current state.

## 1. Goal and adversary

**Goal.** Neither the ASAC operator (including us) nor a network observer can learn *what* a user searched, by
construction rather than by promise. Two distinct properties:

- **Query content** — the server answers a query without learning which query it answered. (Cryptographic:
  PIR.)
- **Query metadata** — a global passive observer cannot tell *that* or *when* a user searched, or link them to
  ASAC. (Mixnet + cover traffic.)

**Adversary.** The operator (honest-but-curious, and we assume *also* compromised), plus a global passive
network observer — the adversary Tor explicitly leaves out of scope. We buy that stronger model only on the
**search path**, which tolerates seconds of latency; general browsing cannot pay the cost (see THREAT-MODEL.md
and PLAN §0.1).

Out of scope: hiding that a user runs ASAC at all from their own guard/ISP (that is Tor's job and its limits
apply); protecting against a user's compromised endpoint; correctness/poisoning of the index itself.

## 2. Where we are today, and the gap

- **`asac-pir`** implements single-server PIANO PIR [verified: Zhou et al., "PIANO", IEEE S&P 2024, eprint
  2023/452] over a small fixed-width record set: symmetric-crypto-only, √n client hint storage, streaming
  preprocessing. The *mechanism* works and is tested; it has **no formal proof in-repo** and is small-scale.
- **`asac-search`** maps a query term to a row **locally** against a **public** vocabulary directory, then
  PIR-fetches that row. So the server does not see the term — but the *set of possible terms is public*, each
  term resolves to a single cached top result, and nothing hides query metadata.

Three gaps stand between that and the flagship: **(A) private term matching over a secret query**, **(B)
scale**, and **(C) metadata unobservability**. None is a one-commit job.

## 3. Gap A — private term matching (PIR-by-keyword)

**Problem.** Real search is: term → posting list (matching docs) → ranked results. Today the term→rows map is
public and client-side. The flagship needs the client to retrieve a term's posting list *without the server
learning the term*, ideally without even publishing the full vocabulary.

**Prior art (verified this session):**

- **Chor, Gilboa, Naor, "Private Information Retrieval by Keywords"** [verified: Cryptology ePrint 1998/003].
  The foundational, modular recipe: combine any conventional search structure (hash table / trie / B-tree)
  with any underlying PIR scheme, so a keyword lookup becomes a sequence of PIR position-fetches. Single-server
  compatible. This is the backbone approach.
- **ChalametPIR**, "Simple, Practical Private Information Retrieval for Keyword Queries" [verified: ACM
  10.1145/3658644.3670271, 2024]. A modern single-server keyword-PIR with fast, low-bandwidth keyword queries
  built on a cuckoo-filter-style structure — the most directly reusable modern design for our single-server
  setting.
- **Tiptoe**, "Private Web Search with Tiptoe" [verified: SOSP 2023, eprint 2023/1438, Henzinger et al.]. Full
  private web search by reducing full-text search to private *nearest-neighbor* over semantic embeddings +
  PIR. The state of the art to benchmark against and the strongest evidence private web search is reachable at
  all; LWE-based and heavier than our symmetric PIANO base.

**Recommended direction.** Start with Chor–Gilboa–Naor over our existing PIANO PIR: build an oblivious lookup
table (term-hash → posting-list bucket) and PIR-fetch the bucket. Pad posting lists to **fixed-size buckets**
so the response size leaks nothing about the term's popularity; terms whose lists overflow a bucket spill to a
documented, padded overflow scheme. Keep the embedding/nearest-neighbor (Tiptoe-style) path as a v2 once the
keyword path is proven — it is a larger, LWE-based build and a separate proof surface.

**Leakage to pin down (design obligations):** fixed bucket size (no popularity leak), constant number of PIR
rounds per query (no "how many matches" leak via round count), identical work for a hit and a miss, and no
timing dependence on the term. Each becomes a measurable property in §7.

## 4. Gap B — scale

**Measured** (our PIANO impl, `cargo run -p asac-pir --example pir_cost --release`, 256-byte records, 2026-10-30):

| n | sqrt(n) | preprocess | per query | response | server indices/query |
|---|---|---|---|---|---|
| 64 | 8 | 0.79 ms | 2.7 µs | 256 B | 8 |
| 256 | 16 | 0.96 ms | 1.6 µs | 256 B | 16 |
| 1 024 | 32 | 6.1 ms | 2.2 µs | 256 B | 32 |
| 4 096 | 64 | 47 ms | 6.6 µs | 256 B | 64 |
| 16 384 | 128 | 484 ms | 16 µs | 256 B | 128 |

Reading: **queries are microseconds** (symmetric-only PIANO), the **response is constant-size** regardless of
which record (no size side channel), and the server sees exactly **sqrt(n) indices per query, independent of
the target** (constant query shape). The real cost is **preprocessing**, which scans the whole DB (~linear in
n) — so a per-crawl re-preprocess is cheap at 10^3 records, ~0.5 s at ~16 k, and is the quantity to watch as
the corpus grows. This confirms "practical over a small/curated index" with numbers, and says the scaling work
is about preprocessing + sharding, not query cost. Plan:

- **Top-N first.** Prototype over a curated "most-common-queries" / top-N index (PLAN §4.4 #1), not the full
  crawl. Prove the mechanism and measure cost before scaling.
- **Fixed record width + sharding.** Cap record width; shard the DB so PIANO's √n client hint storage and
  per-query work stay affordable; a query hits one shard chosen by the (private) keyword lookup.
- **Streaming preprocessing** (already in `asac-pir`) to keep client setup memory bounded.
- **Targets** feed PLAN §6: query latency under ~2 s p90 *including* the circuit (assumption, to be tuned);
  client storage O(√n) per shard; server answer size = one fixed bucket.

## 5. Gap C — metadata unobservability (mixnet)

PIR hides content, not the fact of searching. For the global passive observer we add a **Loopix-style** layer
[verified: Piotrowska et al., "The Loopix Anonymity System", USENIX Security 2017, arXiv 1703.00536]:

- **Cover traffic + Poisson mixing** — brief independent per-message delays make packet timings unlinkable and
  resist traffic analysis against a global network adversary (exactly the adversary Tor cannot handle).
- **Loop messages** — clients and mixes send cover loops so real messages hide in a constant-rate stream.

**What it buys:** an observer cannot tell that/when a specific user searched, or link user↔ASAC, within an
anonymity set we will *measure* (§7). **Honest cost:** added latency (the mixing delay) and bandwidth (cover
traffic); it needs enough cover volume and an honest fraction of mix nodes. This is why it is confined to the
latency-tolerant search path, never general browsing.

**Build note.** `asac-mix` holds the client-side cover-traffic + delay scheduler and the transport seam. The
own-mixes-vs-Nym decision (§10) is **made: ride Nym** — a deployed Rust Sphinx mixnet [verified:
github.com/nymtech, `sphinx` is their Rust Sphinx impl, `nym-sdk` on docs.rs]. Rationale: we never roll crypto
and Sphinx is per-hop public-key crypto (principle 3), and a real anonymity set needs a crowd, not our own few
mixes (principle 1 / PLAN §8 risk 1). The live Nym client lives behind `asac_mix::MixTransport`; the scheduler
feeds it fixed-size framed packets (`frame`) so real and cover are identical on the wire.

## 6. Composition

Three layers, each with one job and one honest limit:

1. **PIR (content).** Server can't learn the query term or which result was fetched. Limit: cost; needs the
   keyword-PIR and scale work above.
2. **Mixnet (metadata).** Observer can't learn that/when/who searched. Limit: latency, cover volume, honest-mix
   fraction.
3. **Decoy queries** (`asac-decoy`, already built). Client-side noise; defense in depth. Limit: adds nothing
   against ASAC itself (which keeps no logs — see PRIVACY.md), and is trivially filtered unless decoys are
   drawn from a realistic distribution (PLAN §4.3 #7).

The claim the composition supports — and *only* this — is: **provable query-content privacy plus measured
search-path unobservability.** Not a speed win, not general-browsing anonymity.

## 7. Measurement plan (Shadow)

Ties into PLAN §6. For each property, an experiment, not an assertion:

- **Content indistinguishability** — argue by construction (the PIR security argument) *and* test the
  implementation for side channels: response size constant across terms (hit vs miss, popular vs rare), PIR
  round count constant, wall-clock time independent of the term (statistical test over many queries in
  Shadow). *Partly measured already* (§4 table): the PIR response is a constant 256 B and the server sees a
  constant sqrt(n) indices per query regardless of the target. Still to measure under Shadow: wall-clock
  timing independence from the term.
- **Anonymity set** — in Shadow with N clients and a mix topology, measure the observer's uncertainty
  (entropy / set size) about which client issued a given search, as cover rate and mix count vary. Target: a
  documented minimum set size at a stated cover-traffic budget. *Scheduler level measured already*
  (`asac_mix::timing_observer_accuracy`, test `timing_observer_is_at_chance`): a timing-only observer over 10
  clients identifies the searcher at ~1/10 (chance), i.e. the anonymity set is the full population — because
  the send schedule is independent of real traffic. Still to measure under Shadow: the *network-level* set,
  with routing through real mixes and a global observer of the links.
- **Cost** — added latency (p50/p90) and bandwidth per query vs the no-mix baseline, so the privacy/cost curve
  is explicit.

## 8. Proof obligations and external review (the gate)

Before any "mathematically unable to profile you" claim ships:

- Written security argument for the keyword-PIR construction over PIANO (reduction to the PIANO/PIR assumption
  + the oblivious lookup structure), including the padding/leakage arguments from §3.
- The PIANO prototype's own deferred proof obligations discharged (`asac-pir` module docs flag these).
- **External cryptographer review**, then a published artifact (target: a PoPETs/PETS paper; the Caspar Bowden
  PET Award is for published PET work) [unverified: current PoPETs deadlines]. No marketing claim precedes the
  review.

## 9. Milestones (ordered)

Each is a normal, testable step like the ones already shipped; crates in parentheses.

- **F1 — done.** Oblivious keyword→bucket lookup over PIANO, fixed buckets, in-process; private posting-list
  retrieval for a secret term, no public term directory (`asac_search::PrivateKeyword`). Fixed bucket width =
  constant response size across terms. Tests cover correct posting lists, absent terms, and the posting-list
  cap. Query budget scales with sqrt(buckets) (PIANO). Remaining for the flagship: F2–F6 below.
- **F2 — done.** Compose F1 with a second PIANO instance keyed by doc id (`asac_search::PrivateCorpus`): a
  secret term privately fetches its posting list, then privately fetches each document record — the server
  learns neither the term nor which docs. `PrivateCorpus::from_index` builds the corpus + postings from a real
  `AsacIndex` over a public vocab, postings ordered by **BM25 blended with link authority** (`asac-rank`), so
  the private path returns best-first like the public one — the curated top-N corpus of PLAN 4.4 #1.
- **F3 — sharding + measurement done; Shadow-scale pending.** Cost measured (§4 table). Sharding primitive
  (`asac_pir::sharded::ShardedDb`): K independent PIANO shards, total budget sqrt(nK) > sqrt(n), with the
  shard-index leak documented (query all shards for full privacy). Still to do: width caps tuning and
  Shadow-scale cost on a large corpus.
- **F4 — scheduler + transport seam done; Nym backend pending.** `asac_mix::PoissonScheduler` (traffic-
  independent delay schedule, tested) plus the `MixTransport` seam and fixed-size `frame`/`next_packet`, so
  real and cover packets are identical on the wire (tested). Own-mixes-vs-Nym **decided: Nym**. Remaining
  (external, needs the network + `nym-sdk`): implement `MixTransport` against Nym and run the live send loop.
- **F5 — scheduler level done; network level pending.** Content side channels partly measured (§4 table:
  constant response size + server index count). Scheduler-level anonymity measured
  (`asac_mix::timing_observer_accuracy`): timing-only observer at chance = 1/N. Still to do in Shadow:
  network-level anonymity set with routing through real mixes, and wall-clock timing independence.
- **F6** Security write-up + external review + artifact (§8).

F1–F3 are solo-buildable here. F4–F5 are larger. F6 needs outside people.

## 10. Open questions / risks

- **Own mixes vs existing network (Nym)? — DECIDED: Nym.** Running too few mixes ourselves recreates the
  small-anonymity-set risk (PLAN §8) and would mean rolling Sphinx crypto; riding Nym (deployed Rust Sphinx
  mixnet) gives a real anonymity set and a vetted impl. Integration goes behind `asac_mix::MixTransport`.
- **Vocabulary privacy.** Chor–Gilboa–Naor hides *which* term is looked up but the lookup structure's size and
  the universe of terms may still leak; quantify and decide what is acceptable.
- **Symmetric (PIANO) vs LWE (Tiptoe/SimplePIR).** We chose PIANO for a solo dev (symmetric-only, no lattice
  assumptions). Tiptoe shows embeddings buy relevance but at LWE cost and a bigger proof surface — revisit only
  after the keyword path is proven.
- **Update cost.** PIANO preprocessing is per-DB; a re-crawl changes the DB. Incremental/periodic
  re-preprocessing cost needs measuring (ties to the index's re-crawl, already built).

## References (verified this session, 2026-10-30)

- Chor, Gilboa, Naor. *Private Information Retrieval by Keywords.* Cryptology ePrint 1998/003.
- *Simple, Practical Private Information Retrieval for Keyword Queries* (ChalametPIR). ACM 10.1145/3658644.3670271, 2024.
- Henzinger et al. *Private Web Search with Tiptoe.* SOSP 2023, eprint 2023/1438.
- Piotrowska et al. *The Loopix Anonymity System.* USENIX Security 2017, arXiv 1703.00536.
- Zhou et al. *PIANO: Extremely Simple, Single-Server PIR with Sublinear Server Computation.* IEEE S&P 2024, eprint 2023/452. (Already the basis of `asac-pir`.)
