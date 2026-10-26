# Single-server sublinear PIR: research + solo-dev plan

Goal: make Potarek's "operator cannot see your query" (PLAN.md 4.4) hold with a **single** server (no
non-collusion assumption) and **sublinear** online cost, so it works for a realistically sized index.

## What the research says

- **Linear server compute is unavoidable without preprocessing** (Beimel-Ishai-Malkin): if the server does not
  touch an entry, it learns the query is not that entry [verified: CMU PIANO blog]. So a sublinear scheme must
  **preprocess**.
- **Homomorphic-encryption PIR** (OnionPIR, Spiral) gets tiny communication but still **linear** server
  compute [verified: PIANO blog] — too slow for a large index.
- **LWE/lattice PIR** (SimplePIR, FrodoPIR) is fast but built on learning-with-errors cryptography [verified:
  MIT SimplePIR paper "Ring LWE"; FrodoPIR "learning with errors"] — hard for a solo developer to implement
  correctly and safely.
- **PIANO** (Zhou et al., IEEE S&P 2024, eprint 2023/452): single-server, preprocessing, **symmetric-crypto
  only** (PRFs/seeds), O(√n) online compute + communication, O~(√n) client storage, ~150 LOC core, 12 ms on a
  100 GB DB [verified: PIANO blog]. Its authors are building a **private search engine** on ~Wikipedia-scale
  data with it [verified: PIANO blog].

**Choice: PIANO.** It is the only one a solo developer can realistically implement correctly (no lattice math),
it is the fastest, and it was literally designed for the private-search-engine use case.

### PIANO in one paragraph (as we will build it)

Treat the index as `n` fixed-width records; the group operation is **XOR**. Split indices into √n chunks of √n.
*Preprocess* (client downloads the DB once, streaming): store ~O(√n) "equations", each selecting one entry per
chunk and keeping the **XOR of those entries** plus the seed that generated the selection; also store O(log n)
**replacement** entries per chunk. Then delete the DB. *Query Xᵢ*: find a stored equation that selects index i;
its stored XOR `Y` includes Xᵢ. Ask the server for the XOR of the same indices but with i **replaced** by a
same-chunk replacement r (so the server sees √n random-looking indices, one per chunk); call it `Y'`. Then
`Xᵢ = Y ⊕ Y' ⊕ Xᵣ`. *Refresh* the consumed equation and replacement so later queries stay private; a DB
permutation (shared seed) makes adaptive queries look random. Each preprocessing supports √n queries; pipeline
the next preprocessing underneath.

### Honest limitations (bake into any claim)

- Preprocessing reveals the **whole DB** to the client [verified: PIANO blog] — fine for Potarek because the
  index is **public** anyway, but it rules out per-user permissioned data.
- PIANO assumes a **static** DB; updates are an open problem [verified: PIANO blog]. Fine for a search index
  rebuilt periodically.
- One-time O(n) download + O~(n) client compute at preprocessing; only the *online* path is sublinear.

## Solo-developer plan (each milestone has a test gate)

- **M1 — recovery math (static DB, correctness only).** Preprocessing equations + replacement subtraction over
  XOR; recover every record correctly. **Not yet private** (coverage is deterministic, no permutation/refresh).
  Gate: for all i, recovered == DB[i]. *(Starting now.)*
- **M2 — random equations + multi-query refresh.** Seed-generated random equations; backup equations to refresh
  a consumed equation "conditioned on containing i"; support √n queries per preprocessing. Gate: √n queries all
  correct; no equation/replacement reused.
- **M3 — adaptive-query privacy.** DB permutation via a shared seed so queries look random; write down the
  privacy argument; note malicious-server affects correctness not privacy. Gate: a statistical test that the
  server's per-query indices are ~uniform per chunk and independent of the true index.
- **M4 — streaming preprocessing.** Download the DB in chunks, update hints, discard — bounded memory. Gate:
  peak memory ≪ DB size; hints identical to the non-streaming build.
- **M5 — integration + measurement.** Wire to a small real index (terms→records); benchmark online cost vs the
  trivial and 2-server schemes; measure in Shadow. Gate: online query is sublinear in practice on the prototype
  index.
- **M6 — write-up.** A PoPETs-style note + reproducible artifacts (the award/pitch path, PLAN.md 4.4).

Reference implementation to check against: the PIANO authors' code (eprint 2023/452). Do not copy blindly —
re-derive, test against a naive baseline, and keep every assumption written down.
