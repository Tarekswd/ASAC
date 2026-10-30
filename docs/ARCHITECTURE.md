# Architecture

This maps the codebase to [PLAN.md](../PLAN.md). The plan is the source of truth for *why*; this file
is the source of truth for *where*.

## Workspace layout

```
potarek/
├── Cargo.toml              # workspace
├── rust-toolchain.toml     # pins Rust 1.92 (Arti 0.47 requirement)
├── crates/
│   ├── potato/             # binary: client entry point + connectivity (Phase 0/1)
│   ├── potato-policy/      # lib: routing Modes + privacy-over-latency policy (PLAN 2.2, 3.3, 3.7)
│   ├── potato-bench/       # bin: SOCKS5 benchmark (connect/TTFB/throughput) (PLAN 6)
│   ├── asac-decoy/      # lib: client-side decoy-query generator (PLAN 4.3 item 7)
│   ├── asac-pir/        # lib: private information retrieval for trustless query privacy (PLAN 4.4)
│   ├── asac-safety/     # lib: pre-index content-safety filter, launch blocker (PLAN 4.3 #2)
│   ├── asac-index/      # lib: BM25 full-text index (Tantivy), safety-gated (PLAN 4.3 #3)
│   ├── asac-search/     # lib: index results served over PIR, server never learns the query (PLAN 4.3 #6)
│   ├── asac-crawler/    # lib: polite robots-respecting crawl core, fetch behind a trait (PLAN 4.3 #1)
│   ├── asac-frontend/   # lib: self-contained HTML results page, no JS/cookies/3rd-party (PLAN 4.3 #5)
│   ├── asac-fetch/      # lib: live Fetcher backend - HTTP over Tor via arti-client (PLAN 4.3 #1)
│   ├── asac-site/       # lib: search glue - request/query -> index -> rendered HTML page
│   ├── asac-demo/       # bin: live end-to-end (crawl over Tor -> index -> query -> HTML)
│   ├── asac-server/     # bin: serve ASAC as a v3 onion service (arti launch_onion_service)
│   ├── asac-rank/       # lib: link-graph authority (PageRank) + BM25xauthority blend, ranking v2 (PLAN 4.3 #4)
│   ├── asac-mix/        # lib: Loopix-style Poisson cover-traffic send scheduler, flagship F4 (PLAN 4.4)
│   └── asac-client/     # lib: client query stream - real searches interleaved with decoys, Poisson-paced (4.3 #7 + F4)
└── docs/
```

## Crates that exist today

| Crate | Kind | Responsibility | Plan ref | Status |
|-------|------|----------------|----------|--------|
| `potato` | bin | Bootstrap a Tor client via `arti-client`; smoke-test connectivity. SOCKS proxy is Arti's `arti proxy` for now. | 3, 5 (Phase 0/1) | skeleton |
| `potato-policy` | lib | `Mode` enum (Swift/Balanced/Fortress) and the rules each mode implies. | 2.2 | real, tested |
| `potato-bench` | bin | SOCKS5 benchmark: connect/TTFB/total/throughput through any proxy. | 6 | real, tested + live |
| `asac-decoy` | lib | Generate benign decoy queries from a caller-supplied vetted term list. Consumed by asac-client. | 4.3 #7 | real, tested |
| `asac-client` | lib | Client query stream: real searches interleaved with asac-decoy decoys, paced by the asac-mix Poisson scheduler, so an observer can't pick real queries by timing (F4) or content (decoys). | 4.3 #7 / 4.4 | real, tested |
| `asac-pir` | lib | PIR so the server can't learn the query. First scheme: trivial PIR (server sends whole small index). | 4.4 | real, tested |
| `asac-safety` | lib | Pre-index content-safety filter: host blocklist (abuse-report takedowns) + exact-hash + keyword blocklists + required partner hash-match seam; fails closed by construction. | 4.3 #2 | real, tested |
| `asac-index` | lib | BM25 full-text index (Tantivy); inserts gated by asac-safety; stores title/url/body/links. In-RAM (`new`) or persistent on disk (`open_or_create`); inserts upsert by URL so re-crawls refresh. | 4.3 #3 | real, tested |
| `asac-search` | lib | Private search over asac-pir PIR. `PrivateSearch`: top-1-per-term cache over a public vocabulary. `PrivateKeyword` (flagship F1): PIR-by-keyword — client hashes a secret term to a bucket and PIR-fetches it, so the server learns neither bucket nor term, with constant-size responses. `PrivateCorpus` (F2): composes F1 with a second doc-id PIR so a secret term privately fetches its matching document records (server learns neither term nor docs). | 4.3 #6 / 4.4 | real, tested |
| `asac-crawler` | lib | Polite, robots-respecting crawl core: fetch behind a `Fetcher` trait (onion = arti-client, clear-web = Scrapling over Tor), extract title/text/links (HTML only, no JS), scope + frontier, output feeds asac-index. | 4.3 #1 | real, tested (core; Scrapling clear-web backend pending) |
| `asac-frontend` | lib | Renders the results page as one self-contained HTML document (no JS, no third-party resources, no cookies); escapes all untrusted query/result data and only links http(s) URLs. | 4.3 #5 | real, tested (served live by asac-server; Equi-X PoW via its opt-in `pow` feature) |
| `asac-fetch` | lib | Live `Fetcher` backend for asac-crawler: HTTP(S) over Tor via `arti-client` (onion + clear-web), bridging the sync trait to arti's async client. https uses rustls/ring + Mozilla roots over the Tor stream; follows up to 5 redirects; de-chunks and reads UTF-8 bodies (no gzip). | 4.3 #1 | real; response parsing + redirects tested, live http+https verified |
| `asac-site` | lib | Search glue: request target/query -> top BM25 candidates from `asac-index` -> re-ranked by `asac-rank` authority -> `asac-frontend` HTML page. The function a live front end calls per request. | 4.3 #5/#6 | real, tested |
| `asac-rank` | lib | Ranking v2: link-graph authority (PageRank over crawled links), a BM25xauthority blend, and a freshness decay (half-life toward a floor). asac-site ranks by BM25 x authority x freshness. | 4.3 #4 | real, tested |
| `asac-mix` | lib | Loopix-style rate-λ Poisson send scheduler with cover traffic; the send schedule is independent of real traffic (timing-unlinkable). Flagship F4 metadata layer; live mix transport pending. | 4.4 | real, tested (scheduler core) |
| `asac-demo` | bin | Live end-to-end: crawl a seed over Tor, index it (safety-gated), answer one query as an HTML page. | 4 | real; manual (needs live Tor) |
| `asac-server` | bin | Serve ASAC as a v3 onion service: crawl seeds, persist+upsert into an on-disk index, then answer each request with a rendered HTML page over the onion stream (`arti` `launch_onion_service`). http on the onion; one task per request; graceful Ctrl-C shutdown; Equi-X PoW via the opt-in `pow` feature (LGPL `equix`). | 4.3 #5 | real; request parse/response + config tested, live serving manual |

## Crates planned (not yet created)

These are named here so the module boundaries are agreed before code lands. Each arrives with its phase
(PLAN.md section 5), not before:

| Planned crate | Responsibility | Plan ref | Phase |
|---------------|----------------|----------|-------|
| `potato-proxy` | Wrap Arti's SOCKS proxy with the policy layer (force CGO, Conflux, strict isolation). | 3.1, 3.4, 3.9 | 1 |
| `potato-pq` | Hybrid post-quantum ntor-v3 + ML-KEM handshake negotiation. | 3.2 | 2 |

## Hard rules the layout enforces

- **No new crypto primitives.** Crypto comes from Arti and audited libraries; our crates compose policy
  (PLAN.md 2.3).
- **No PHP or JavaScript.** Rust everywhere, except Scrapling (Python) for the optional clear-web crawl,
  restricted to HTTP-only over Tor (PLAN.md 2.3, 4.3 #1).
- **TLS is rustls** (pure-Rust), not native-tls, to avoid a system OpenSSL dependency and stay memory-safe.
