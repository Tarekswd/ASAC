# ASAC single-server private search — design & evaluation note

**Status: prototype, not peer-reviewed.** This note documents the construction and *measured* costs of ASAC's
private-search prototype and lists the **open proof obligations** that external cryptographic review must
discharge before any formal privacy claim is made. It is the artifact for a future PoPETs/PETS submission, not
the proof itself.

## 1. Setting and threat model

A **public, static** index of `n` fixed-width records (search terms → result payloads). A client wants record
`i` without the server learning `i`. One server (no non-collusion assumption). The server is honest-but-curious
for *privacy*; a malicious server can harm *correctness* but, by design, not privacy. Out-of-scope items follow
the project threat model ([THREAT-MODEL.md](THREAT-MODEL.md)); in particular this is query-*content* privacy for
the latency-tolerant search path (PLAN.md 4.4), not a general anonymity system.

## 2. Construction (PIANO-style, Zhou et al. IEEE S&P 2024)

- Index split into √n chunks of √n; the group operation is **XOR**.
- **Preprocess (streaming):** the client downloads the index once, one record at a time, accumulating ~O(√n)
  random "equations" (one entry per chunk, offsets from a **BLAKE3-keyed CSPRF**), storing each equation's XOR
  parity plus O(log n)-ish backups and replacements per chunk; it then drops the index. Client storage is
  O~(√n).
- **Query Xᵢ:** find a stored equation selecting `i`; replace `i` with a same-chunk replacement; the server
  returns the XOR of the resulting √n indices; subtract to recover Xᵢ. The consumed equation is **refreshed**
  from a backup conditioned on containing `i`.
- **Permutation:** a shared-seed random permutation maps logical queries to physical slots so an adaptive query
  stream matches the random-query distribution the analysis assumes.

## 3. Implementation

Rust crate `asac-pir`, three schemes for comparison: trivial PIR, 2-server XOR PIR (Chor et al.), and the
single-server PIANO-style scheme (`single_server.rs`). CSPRF = BLAKE3 keyed hash. 15 tests cover recovery,
multi-query refresh/no-reuse, CSPRF offset uniformity, permutation bijection, streaming == batch hints, and
end-to-end term search; clippy clean under `-D warnings`.

## 4. Evaluation (measured, prototype)

Prototype index n = 16 records × 32 B, √n = 4. Per-query **download** [proof: `cargo run -p asac-pir --example
private_search`]:

| Scheme | Servers | Per-query download | Online server work |
|--------|---------|--------------------|---------------------|
| Trivial PIR | 1 | 512 B (whole index, O(n)) | O(n) |
| 2-server XOR | 2 (non-colluding) | 64 B (2 × one record) | O(n) each |
| Single-server (PIANO) | 1 | **32 B (one record) + √n=4 indices up** | O(√n) |

The single-server scheme is the only one that is both single-server *and* sublinear online. Absolute numbers
are illustrative at this scale; the **asymptotics** (O(√n) online, O~(√n) client storage, one server) are the
result. Scaled/Shadow measurement is deferred to the scale-up phase.

## 5. Assumptions and limits (do not overclaim)

- Index is **public, static, perfect-square, fixed-width** in the prototype.
- Preprocessing reveals the whole (public) index to the client — fine here, rules out permissioned data.
- Offsets use a modulo reduction with negligible bias only for small √n; the CSPRF key must stay secret.
- The permutation is the *mechanism* PIANO assumes, not a proof; the server knows the seed.

## 6. Open proof obligations (for external cryptographic review)

These are **not** proven here and must be discharged before any formal privacy claim:

1. **Per-query indistinguishability:** the √n indices sent per query are indistinguishable from a fresh uniform
   equation, so the server cannot identify the real (replaced) chunk.
2. **Refresh distribution:** over a batch of √n queries, the refresh-from-backup keeps the remaining pool
   exactly conditioned correctly, with no cumulative leakage.
3. **Adaptive security:** the shared-seed permutation + refresh bound leakage for adaptive (data-dependent)
   query sequences.
4. **PRF reduction:** reduce offset unpredictability to BLAKE3-keyed-hash PRF security; quantify modulo bias.
5. **Parameters:** coverage probability and backup/replacement counts for a target failure probability.

## 7. Reproducibility

`cargo test -p asac-pir`; `cargo run -p asac-pir --example private_search`; roadmap and milestones in
[PIR-RESEARCH.md](PIR-RESEARCH.md). Reference: PIANO (eprint 2023/452). **Do not deploy for real-world privacy
until the obligations in §6 are discharged.**
