# ASAC

**ASAC — Anti-Surveillance, Anti-Corruption.** A privacy-first routing layer and onion-service search engine,
built on Tor and [Arti](https://gitlab.torproject.org/tpo/core/arti).

> **Status: working prototype, not audited.** The Balanced client (Phases 0-1) and the full search engine
> pipeline — crawl over Tor, safety filter, BM25 index with persistence/freshness, authority ranking, and a
> concurrent v3 onion-service front end — are built and tested, and live crawling over Tor (http + https) is
> verified end to end. The flagship private-search mechanisms (single-server PIANO PIR, PIR-by-keyword, a
> private corpus, a Loopix-style cover-traffic scheduler, decoy queries) are built and unit-tested, with their
> cost measured. **They are tested mechanisms, not a proven guarantee:** no external cryptographer review yet,
> so do not rely on this for real-world privacy. See [PLAN.md](PLAN.md), [docs/FLAGSHIP-DESIGN.md](docs/FLAGSHIP-DESIGN.md),
> and [docs/PIR-SECURITY.md](docs/PIR-SECURITY.md).

---

## What this is

ASAC is two pieces that work together:

- **Potato routing** — a client and protocol layer that runs *on the existing Tor network*, turning the best
  existing Tor/Arti features (CGO relay crypto, Conflux multipath, Vegas congestion control, post-quantum
  handshakes, padding) on together, tuning them, and measuring the result. It exposes **modes** so the user
  picks the latency/privacy trade-off, and each mode's leak is written down, not hidden.
- **ASAC search engine** — a search engine that indexes onion services, runs *as* an onion service, and is
  designed so its operators cannot build user profiles (no query logs, onion-only, PoW-protected).

See **[PLAN.md](PLAN.md)** for the full design, threat model, phased roadmap, and measurement plan.

## Design principles

These are load-bearing, not decoration (full detail in [PLAN.md](PLAN.md)):

1. **Privacy comes from the crowd, not only the cryptography.** ASAC starts on the Tor network rather than
   as a tiny separate network, because a new network with few users is *less* private however good its crypto.
2. **Privacy and latency pull in opposite directions.** Every latency trick leaks something, so trade-offs are
   explicit modes with documented leaks — never hidden defaults.
3. **Never roll your own crypto.** Everything builds on Arti and audited libraries. ASAC's new parts are
   *policy and composition*, not new primitives.

## Modes

| Mode | Hops | For |
|---|---|---|
| **Swift** | 3 | Lower latency, with a small, bounded, documented location leak |
| **Balanced** (default) | 3 | Tor-equivalent privacy with the best existing speedups on |
| **Fortress** | 3 + vanguards-lite | Maximum resistance: cover traffic and timing jitter, at a bandwidth/latency cost |

**Privacy beats latency.** Balanced is the default and Fortress is the showcase; Swift's latency features are
optional, gated, and the first thing cut if they weaken privacy. An optional client-side **decoy-query** feature
can also obscure your real searches by mixing in random ones (see PLAN.md section 4.3).

## Status and roadmap

The work is phased, and each phase ends with a gate that must pass before the next starts. The first three
concrete tasks and the full phase plan are in [PLAN.md](PLAN.md) sections 5 and 9. In short:

- **Phase 0** — Rust workspace on Arti, private test network (Shadow + chutney), benchmark baseline.
- **Phase 1** — Potato client, Balanced mode.
- **Phase 2** — hybrid post-quantum handshake.
- **Phase 3+** — Swift/Fortress modes, the ASAC search engine, external audit.

The engine and the flagship *mechanisms* are built and tested in-repo. The parts that still need a network
simulator, a live deployment, a cryptographer, or legal/partner agreements — and the concrete steps for each —
are laid out in **[docs/EXTERNAL-PLAN.md](docs/EXTERNAL-PLAN.md)**.

## Building

**Rust 1.92+ on Linux** (native or WSL). Arti 0.47 requires Rust 1.92, and the build is Linux-first; a
`rust-toolchain.toml` pins the compiler if you use [rustup](https://rustup.rs).

```sh
cargo test --workspace                          # build everything and run the tests
cargo run  -p potato                            # bootstrap Tor, fetch example.com over it (needs network)
cargo run  -p asac-demo -- http://example.com/ domain   # crawl over Tor -> index -> private-ish query (needs network)
cargo run  -p asac-server -- asac http://<seed>/        # serve ASAC as a v3 onion service (needs network)
#   ASAC_RECRAWL_SECS=3600 refreshes the index periodically; --features pow enables Equi-X PoW.
```

The client uses pure-Rust **rustls** TLS and a **bundled static SQLite**, so no system OpenSSL or libsqlite3
is needed. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the crate layout.

### Workspace crates

Full detail and status in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

**Potato routing**

| Crate | What it is |
|-------|------------|
| `potato` | Client binary: bootstraps a Tor client via `arti-client`. |
| `potato-policy` | Routing modes (Swift/Balanced/Fortress) and the privacy-over-latency rules. |
| `potato-bench` | SOCKS5 benchmark (connect/TTFB/throughput). |

**Search engine**

| Crate | What it is |
|-------|------------|
| `asac-crawler` | Polite, robots-respecting crawl core; fetch behind a trait. |
| `asac-fetch` | Live fetcher: HTTP(S) over Tor via `arti-client` (redirects, gzip). |
| `asac-safety` | Pre-index content-safety filter (host/hash/keyword blocklists + partner seam); fails closed. |
| `asac-index` | BM25 full-text index (Tantivy): persistence, upsert, freshness. |
| `asac-rank` | Ranking v2: PageRank authority + BM25xauthority x freshness blend. |
| `asac-frontend` | Self-contained HTML results page (no JS/cookies/3rd-party). |
| `asac-site` | Search glue: request/query -> index -> rendered page. |
| `asac-server` | Serve ASAC as a v3 onion service; concurrent, graceful shutdown, optional re-crawl + PoW. |
| `asac-demo` | One-shot live end-to-end (crawl over Tor -> index -> query -> HTML). |

**Flagship privacy (PLAN 4.4)**

| Crate | What it is |
|-------|------------|
| `asac-pir` | PIR (trivial, 2-server XOR, single-server PIANO) + sharding, so the server can't learn the query. |
| `asac-search` | Private search: PIR-by-keyword + a private corpus over the real index. |
| `asac-mix` | Loopix-style Poisson cover-traffic send scheduler. |
| `asac-decoy` | Client-side decoy-query generator (vetted term list). |
| `asac-client` | Client query stream: real searches interleaved with decoys, Poisson-paced. |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md) first. Because this is privacy and cryptography software, correctness and
careful review matter more than speed of merging.

## Security

**Do not open public issues for security vulnerabilities.** See [SECURITY.md](SECURITY.md) for how to report
privately. Note the threat model in [PLAN.md](PLAN.md) section 2.1: like Tor, ASAC does not defend against a
global passive adversary watching both ends of a connection.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. This matches Arti's `MIT OR Apache-2.0` licensing, which ASAC builds on. Unless you state
otherwise, any contribution you submit for inclusion shall be dual-licensed as above, without additional terms.
