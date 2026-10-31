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

- **O1 — Formalize §3.1.** **Drafted in §6.3** (a game-based reduction from keyword-privacy to PIANO
  index-hiding, with the public-hash hoisting made explicit). For review: check the reduction is tight and the
  hybrid composition with the document-PIR step (§3.2) is sound.
- **O2 — Closed in code.** `PrivateCorpus::search` now performs exactly `fetches_per_query =
  min(MAX_FETCHES_PER_QUERY, sqrt(n_docs))` document fetches every time (real results first, dummy fetches for
  the remainder, discarded), with the dummies spread across the index by a rotating cursor. The count is a
  corpus constant, independent of the query, and `K ≤ sqrt(n)` keeps it within the per-chunk PIANO budget.
  Remaining for review: confirm the dummy indices carry no exploitable correlation, and that truncating a
  posting list longer than `K` (a result cap) is acceptable.
- **O3 — Quantify vocabulary leakage.** **Drafted in §6.4** (bounds the term universe and the term-level
  anonymity set; flags the public-hash membership-oracle question). For review: confirm the bound and that no
  cross-system oracle leaks term membership.
- **O4 — Discharge PIANO's own deferred obligations** flagged in `single_server` (the in-repo implementation,
  not just the paper's scheme).
- **O5 — Timing side channel.** A1 covers the logical view, not wall-clock. Measure timing independence from
  the term under Shadow (FLAGSHIP-DESIGN.md §7).
- **O6 — PRNG. Resolved.** `asac-decoy` now uses a **ChaCha20 CSPRNG** (`rand_chacha`) for decoy selection, so
  observing decoys does not reveal the generator state and an adversary cannot predict/filter future decoys;
  deployment must seed it from an unpredictable source. `asac-mix` keeps a fast non-CSPRNG for delay *sampling*
  on purpose: Loopix's resistance comes from the Poisson distribution and cover volume, which are public, not
  from secret delays — so predictable delays do not reveal which sends were real. (Still confirm this
  distribution argument in review.)

Only once O1-O6 are resolved and an external cryptographer has reviewed this argument does any public
"mathematically unable to profile you" claim become defensible.

## 6. Formal arguments (DRAFT, for review)

This section discharges the *written* parts of O1 and O3. It is a draft for a cryptographer to check; it
reduces to the assumptions in §2 and does not re-prove them.

### 6.1 Model and server view

A single honest-but-curious operator runs the server and sees the full protocol transcript. For one
`PrivateKeyword` lookup of term `t` ([proof: `crates/asac-search/src/lib.rs` `locate`/`PrivateKeyword::search`]):

- The client computes `locate(t) = (b, fp)` **locally**, where `b = (blake3(t)[0..8] as u64 LE) mod n_buckets`
  and `fp = blake3(t)[8..16]`. `blake3` here is **unkeyed and public** — a bucketing function, not a secret.
- The client issues one PIANO query `Q` for logical bucket index `b` and receives the whole fixed-width bucket
  (`BUCKET_WIDTH = MAX_ENTRIES_PER_BUCKET · ENTRY_LEN = 8 · 73 = 584` bytes), then matches `fp` **client-side**.

So the only part of the server's view that depends on `t` is the PIANO query `Q(b)`. The response size is a
constant independent of `t`, its popularity, or its posting-list length; the fingerprint match happens on the
client and is never sent to the server. (`PrivateCorpus` composes this with a second, independent PIANO
instance keyed by document id, §3.2; the per-query document-fetch *count* is fixed by O2.)

### 6.2 Assumptions (restated from §2)

- **A1 (PIANO index-hiding).** For the single-server PIANO scheme, the server's view of a query for logical
  index `i` is computationally indistinguishable from its view for any `j`, over the client's secret key and
  randomness. Relied upon from the PIANO paper [Zhou et al., IEEE S&P 2024]; our implementation's own
  obligations are **O4**.
- **A2 (CSPRF).** PIANO's keyed offset function (`blake3::keyed_hash` under the client's secret key) is a
  secure PRF. PIANO's index-hiding (A1) is argued relative to A2. Note this is a *different* use of blake3
  from the *unkeyed* `locate` hash in §6.1, which carries no secret and is not relied on for hiding.

### 6.3 O1 — keyword privacy reduces to PIANO index-hiding

**Game `KW-IND`.** The adversary `A` (the server) outputs two terms `t0, t1`. The challenger samples a bit
`β`, runs the client lookup for `t_β`, and gives `A` the resulting server-side transcript. `A` outputs `β'`.
Define `Adv_KW(A) = |Pr[β' = β] − 1/2|`.

**Claim.** For every `A` there is a reduction `B` of essentially the same running time with
`Adv_KW(A) ≤ Adv_IH(B)`, where `Adv_IH` is advantage against PIANO index-hiding (A1). Hence under A1,
`Adv_KW(A)` is negligible.

**Reduction `B`.** `B` receives `t0, t1` from `A` and computes `b0, b1` via the public `locate` (it needs no
secret to do so).
1. If `b0 = b1`, the server's view is identical for both terms (same bucket queried, same constant-width
   response), so `Adv_KW = 0` on this pair; `B` outputs a random bit.
2. If `b0 ≠ b1`, `B` forwards `(b0, b1)` to its index-hiding challenger, receives the server-view of `Q(b_β)`,
   relays it to `A`, and outputs `A`'s guess. Then `Adv_IH(B) = Adv_KW(A)`.

**Hoisting the public hash.** `locate` is public, deterministic, and evaluated entirely client-side; the
server never receives `t`. The hash influences the server's view *only* through the bucket index `b`, and the
response width is a constant. An operator may freely recompute `locate` on any guessed term, but by A1 it still
cannot tell which `b` was queried — so distinguishing `t0` from `t1` is exactly distinguishing `Q(b0)` from
`Q(b1)`. The fingerprint `fp` lives inside the PIR-fetched bucket and is matched on the client, adding nothing
to the server's view.

**Full chain.** `KW-privacy ⇐ A1 ⇐ A2`: the end-to-end assumption is "`blake3::keyed_hash` is a PRF" plus "the
PIANO scheme, and our implementation (O4), is index-hiding."

**Composition (`PrivateCorpus`).** The document-PIR step is a second, independent PIANO instance; the same
reduction applies to it, and independence gives a two-step hybrid: an adversary distinguishing the whole
transcript implies one distinguishing a single PIANO instance. For review: confirm the hybrid and that the two
instances share no state that could correlate the queries.

**Out of scope here.** Wall-clock timing (**O5**), the result-cap truncation (**O2**), and vocabulary-level
leakage (**O3**, next).

### 6.4 O3 — vocabulary-leakage bound

**What the transcript reveals.** The public parameters `n_buckets`, `BUCKET_WIDTH`, `MAX_ENTRIES_PER_BUCKET =
8`, `FP_LEN = 8`, `MAX_DOCS_PER_TERM = 16` are known. Because each bucket holds at most
`MAX_ENTRIES_PER_BUCKET` entries (overflow dropped at build time), the number of distinct indexed terms is at
most `n_buckets · 8`. That bounds the *size* of the searchable vocabulary; it does not reveal *which* terms are
in it. The term→bucket map is public (anyone can evaluate `locate`), but corpus membership is already known to
the server (it built the index), and a network observer sees only that a fixed-shape PIANO query occurred.

**Term anonymity set.** Under A1 the bucket index is hidden, so from the server's view a lookup is consistent
with every term in the vocabulary: the server's posterior over the queried term equals its prior. The
term-level anonymity set is therefore the full indexed vocabulary (`≤ n_buckets · 8`).

**Caveats to review.**
- *Bucket collisions.* About `|vocab| / n_buckets` terms share a bucket; the 64-bit fingerprint disambiguates
  client-side. A within-bucket fingerprint collision (≤ 8 entries, so probability on the order of `2^-61`) would
  return a wrong posting — a correctness issue, not a privacy leak.
- *Public (unkeyed) hash.* Because `locate` is public, an adversary who could observe whether a specific
  client's query *succeeds* (hit vs. miss) could test membership of guessed terms. The protocol as written does
  not expose hit/miss to the server — matching is client-side and the per-query document-fetch count is fixed
  (O2). **For review:** confirm no cross-system oracle (e.g., follow-on fetch behaviour) leaks hit/miss.
- *Overflow drop.* Dropping entries past `MAX_ENTRIES_PER_BUCKET` is availability, not privacy.

**Residual leakage.** The public parameters bound `|vocab|`; nothing in the transcript reveals which term was
searched. This is the "by design" row of §4's table, now quantified.
