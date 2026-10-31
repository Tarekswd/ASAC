# ASAC Private-Search Security Argument (DRAFT for review)

**Status: DRAFT. This is not a validated proof.** It is the written argument a cryptographer should review
(FLAGSHIP-DESIGN.md F6). Until that review and the open obligations in §5 are discharged, ASAC claims only
"query-content privacy by construction, prototype stage," never a guarantee. Nothing here should be quoted as
a proven property.

## 1. What we argue, and what we don't

**In scope (this document):** *query-content privacy* — a curious or compromised ASAC operator, seeing
everything the server processes, learns nothing about **which term** a user searched or **which documents**
were returned, beyond the explicit leakage in §4.

**Out of scope here:**
- *Metadata privacy* (that/when a user searched, linking user↔ASAC) — that is the mixnet layer
  (`asac-mix`, FLAGSHIP-DESIGN.md Gap C), argued separately.
- *Correctness/availability* of results, index poisoning, and the content-safety filter.
- The user's endpoint, and Tor's own global-passive-adversary limit.

## 2. Building blocks and assumptions

- **PIANO single-server PIR** [Zhou et al., IEEE S&P 2024, eprint 2023/452], implemented in
  `asac-pir::single_server`. We use it as a black box with:
  - **(A1)** *PIANO security*: the server's view of a query reveals nothing about the logical index fetched.
    We rely on the PIANO paper's theorem; we do **not** re-prove it here, and the in-repo implementation's own
    proof obligations are flagged as deferred in the `single_server` module docs.
- **(A2)** *CSPRF is a PRF*: PIANO offsets come from `blake3::keyed_hash` with the client's secret key
  [proof: `crates/asac-pir/src/single_server.rs` `offset()`], assumed a secure pseudo-random function.
- **Keyword→position composition** [Chor, Gilboa, Naor, "Private Information Retrieval by Keywords", eprint
  1998/003]: a keyword lookup is a PIR fetch of a position derived from the term by a public hash.

## 3. The constructions and the argument (sketch)

### 3.1 `PrivateKeyword` (PIR-by-keyword)
The client computes `bucket = H(term) mod n_buckets` with a **public** hash and PIR-fetches that bucket; it
then scans the returned fixed-width bucket for its term's fingerprint.

*Argument:* the server sees only a PIANO query. By **A1** that view is independent of the bucket index, hence
(the hash being a fixed public function) independent of the term. The response is a **constant-width bucket**
regardless of term or popularity — [proof: measured constant 256-B-class response and constant `sqrt(n)`
server indices per query, FLAGSHIP-DESIGN.md §4 table] — so there is no response-size or query-shape side
channel. There is no published term→row directory (unlike `PrivateSearch`), so the *set of possible terms* is
not revealed by the protocol itself (but see §4 on the vocabulary).

### 3.2 `PrivateCorpus` (keyword PIR ∘ document PIR)
Two independent PIANO instances: one keyword table (§3.1), one keyed by document id. `search(term)` fetches the
posting list from the first, then fetches each listed document from the second.

*Argument:* the operator sees two PIANO query streams, each leaking nothing about its index by **A1**; the
instances are independent. Each `search` performs **exactly `fetches_per_query` document fetches** — a corpus
constant `min(MAX, sqrt(n_docs))`, independent of the query (real results first, the rest dummy fetches that
are discarded) — so the fetch *count* reveals nothing about a term's posting-list length (O2, closed in code).

### 3.3 Sharding (`asac-pir::sharded`)
Splitting into K shards and querying only the owning shard reveals the shard index (coarse `log2 K` bits). For
full privacy a caller queries every shard (cover). Documented in the module; not used on the private path by
default.

## 4. Leakage summary (what the operator can still learn)

| Observable | Leaks | Status |
|---|---|---|
| PIANO query (keyword or doc) | nothing about the index (under A1) | argued; constant size/shape **measured** |
| Number of doc fetches per search | nothing (fixed `min(MAX, sqrt(n_docs))`, query-independent) | **closed (O2)** |
| Shard index (if single-shard queried) | coarse bucket of the record | documented; avoid via query-all-shards |
| The public vocabulary / bucket count | the *universe* of searchable terms (not which one) | by design; quantify (O3) |
| Bucket overflow (dropped entries) | availability only, not privacy | correctness, not a leak |

## 5. Open obligations (the F6 review checklist)

- **O1 — Formalize §3.1.** A written reduction: distinguishing the term from the operator's view ⇒ breaking
  PIANO (A1) or the PRF (A2). Make the hoisting of the public hash explicit.
- **O2 — Closed in code.** `PrivateCorpus::search` now performs exactly `fetches_per_query =
  min(MAX_FETCHES_PER_QUERY, sqrt(n_docs))` document fetches every time (real results first, dummy fetches for
  the remainder, discarded), with the dummies spread across the index by a rotating cursor. The count is a
  corpus constant, independent of the query, and `K ≤ sqrt(n)` keeps it within the per-chunk PIANO budget.
  Remaining for review: confirm the dummy indices carry no exploitable correlation, and that truncating a
  posting list longer than `K` (a result cap) is acceptable.
- **O3 — Quantify vocabulary leakage.** The term universe / bucket structure may leak more than "which term";
  bound it.
- **O4 — Discharge PIANO's own deferred obligations** flagged in `single_server` (the in-repo implementation,
  not just the paper's scheme).
- **O5 — Timing side channel.** A1 covers the logical view, not wall-clock. Measure timing independence from
  the term under Shadow (FLAGSHIP-DESIGN.md §7).
- **O6 — PRNG.** `asac-mix`/`asac-decoy` use non-CSPRNGs for pacing/selection (documented); confirm that is
  acceptable for the metadata layer or swap in a CSPRNG.

Only once O1-O6 are resolved and an external cryptographer has reviewed this argument does any public
"mathematically unable to profile you" claim become defensible.
