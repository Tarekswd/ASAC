# Potarek

**A privacy-first routing layer and onion-service search engine, built on Tor and [Arti](https://gitlab.torproject.org/tpo/core/arti).**

> **Status: planning / design draft. There is no working code yet.** This repository currently holds
> the design ([PLAN.md](PLAN.md)) and project documents. Do not rely on anything here for real privacy.

---

## What this is

Potarek is two pieces that work together:

- **Potato routing** — a client and protocol layer that runs *on the existing Tor network*, turning the best
  existing Tor/Arti features (CGO relay crypto, Conflux multipath, Vegas congestion control, post-quantum
  handshakes, padding) on together, tuning them, and measuring the result. It exposes **modes** so the user
  picks the latency/privacy trade-off, and each mode's leak is written down, not hidden.
- **Potarek search engine** — a search engine that indexes onion services, runs *as* an onion service, and is
  designed so its operators cannot build user profiles (no query logs, onion-only, PoW-protected).

See **[PLAN.md](PLAN.md)** for the full design, threat model, phased roadmap, and measurement plan.

## Design principles

These are load-bearing, not decoration (full detail in [PLAN.md](PLAN.md)):

1. **Privacy comes from the crowd, not only the cryptography.** Potarek starts on the Tor network rather than
   as a tiny separate network, because a new network with few users is *less* private however good its crypto.
2. **Privacy and latency pull in opposite directions.** Every latency trick leaks something, so trade-offs are
   explicit modes with documented leaks — never hidden defaults.
3. **Never roll your own crypto.** Everything builds on Arti and audited libraries. Potarek's new parts are
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
- **Phase 3+** — Swift/Fortress modes, the Potarek search engine, external audit.

## Building

**Rust 1.92+ on Linux** (native or WSL). Arti 0.47 requires Rust 1.92, and the build is Linux-first; a
`rust-toolchain.toml` pins the compiler if you use [rustup](https://rustup.rs).

```sh
cargo test --workspace     # build everything and run the tests
cargo run  -p potato        # bootstrap Tor and fetch example.com over it (needs network)
```

The client uses pure-Rust **rustls** TLS and a **bundled static SQLite**, so no system OpenSSL or libsqlite3
is needed. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the crate layout.

### Workspace crates

| Crate | What it is |
|-------|------------|
| `potato` | Client binary: bootstraps a Tor client via `arti-client`; SOCKS proxy is Arti's `arti proxy` for now. |
| `potato-policy` | Routing modes (Swift/Balanced/Fortress) and the privacy-over-latency rules. |
| `potarek-decoy` | Client-side decoy-query generator (self-directed query obfuscation). |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md) first. Because this is privacy and cryptography software, correctness and
careful review matter more than speed of merging.

## Security

**Do not open public issues for security vulnerabilities.** See [SECURITY.md](SECURITY.md) for how to report
privately. Note the threat model in [PLAN.md](PLAN.md) section 2.1: like Tor, Potarek does not defend against a
global passive adversary watching both ends of a connection.

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. This matches Arti's `MIT OR Apache-2.0` licensing, which Potarek builds on. Unless you state
otherwise, any contribution you submit for inclusion shall be dual-licensed as above, without additional terms.
