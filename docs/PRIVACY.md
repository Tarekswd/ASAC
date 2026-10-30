# ASAC query-privacy policy (v1: no logs)

This documents the **no-query-log policy** that PLAN.md 4.3 #6 requires for ASAC v1, verified by code review,
so it can be published. It is the table-stakes guarantee; the by-construction flagship (PIR + mixnet, PLAN 4.4)
is separate and still in progress.

## The policy

**ASAC keeps no record of what anyone searched.** No per-query logs, on disk or in memory. The server reads a
request, runs the search, returns the HTML page, and keeps nothing tied to the query. Only non-query-specific,
aggregate information may exist (e.g. a total request counter), never the query text, never per-query rows.

## Where this is enforced (code review, 2026-10-30)

Audited the serving path — `asac-server`, `asac-site`, `asac-index`, `asac-frontend`:

- **`asac-site` / `asac-frontend`**: no logging, no file writes, no persistence of any kind. They turn a query
  into HTML and return it. [verified: no `println`/`eprintln`/`tracing`/`File`/`OpenOptions` in either crate,
  grep 2026-10-30]
- **`asac-index`**: persists only **crawled documents** (title/url/body/links), never queries. Search builds a
  transient searcher and returns results. An **unparseable query returns empty results, not an error**
  ([`AsacReader::search`](../crates/asac-index/src/lib.rs)), so the serving path has no query-derived error to
  log.
- **`asac-server`**: logs only operational lines (bootstrap progress, crawled page count, the `.onion`
  address, shutdown). The request path logs **nothing** on success. On failure it logs a fixed string
  (`accept failed` / `request failed: search failed`); the underlying error — which could echo the query — is
  **discarded before logging** ([`serve_one`](../crates/asac-server/src/main.rs)). The requested target/query
  is never logged.

`asac-demo` is a local developer CLI (not the onion service) and does print the query it was given to its own
terminal; that is the operator's local tool, outside the served-service policy.

## Honest limits

- This covers **query content at the ASAC service**. It does **not** hide, by itself, the metadata of *that*
  or *when* a user connected — a user's Tor guard sees an encrypted connection. Defeating a global passive
  observer is the job of the mixnet/cover-traffic layer in PLAN 4.4, not of this policy.
- It is a **property of this code, not a promise about a deployment.** An operator who patches in logging, or
  runs a modified build, breaks it. The guarantee is "the published build keeps no query logs," which is why
  the flagship (PIR) aims to make profiling impossible *by construction* rather than by policy.
- The persistent index directory holds crawled public content, not user queries.
