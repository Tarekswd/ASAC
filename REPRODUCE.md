# Reproducing ASAC's claims

Every claim in this repo is meant to be checkable. This page lists the exact commands. Toolchain: the
workspace pins **Rust 1.92** via `rust-toolchain.toml` (an Arti 0.47 requirement); install `rustup` and the
toolchain is selected automatically.

## 1. Build, lint, test the workspace (offline, no network)

```sh
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings   # the CI gate; must be warning-clean
cargo test --workspace                                   # ~90 unit/integration tests, all offline
```

What this covers: the engine pipeline (crawl logic, safety filter, index, ranking, request handling), and the
flagship mechanisms — single-server PIANO PIR (incl. a randomized roundtrip property test), PIR-by-keyword, the
private corpus, the Loopix-style cover-traffic scheduler, and the decoy client. Network parsers (`asac-fetch`)
have adversarial robustness tests (malformed/hostile responses must not panic or over-read).

## 2. PIR cost numbers (FLAGSHIP-DESIGN.md §4)

The measured per-query PIR costs (constant response size, `~sqrt(n)` server indices) come from the PIR layer;
exercise it directly with the examples:

```sh
cargo run -p asac-pir --example search_demo
cargo run -p asac-pir --example private_search
```

## 3. The private-query security argument

Read [docs/PIR-SECURITY.md](docs/PIR-SECURITY.md). §6 is the written reduction (keyword-privacy ⇒ PIANO
index-hiding, §6.3) and the vocabulary-leakage bound (§6.4). It is a **draft for external review**; O4 (the
PIANO *implementation*) and O5 (wall-clock timing) are explicitly left open.

## 4. Live onion service, end to end (needs network + Tor reachability)

```sh
cargo run -p asac-server -- mysearch https://example.com      # prints an http://<56-char>.onion/ address
# from a second client (Tor Browser, or a second arti client) fetch:
#   http://<onion>/search?q=test     -> a rendered results page
```

Verified this way with a second `arti` client fetching `/search` end to end. Connecting to a `.onion` requires
`asac-fetch`'s `onion-service-client` feature (enabled) and a graceful stream close (see `asac-server`).

## 5. Metadata-unobservability measurement (F5, needs Shadow)

[Shadow](https://shadow.github.io) 3.3.0+ (build-from-source; a Linux host — WSL2 works with a determinism
caveat). See [sim/shadow/README.md](sim/shadow/README.md).

```sh
cargo build -p asac-emit -p asac-observe
cp target/debug/asac-emit target/debug/asac-observe sim/shadow/
cd sim/shadow
shadow mix-anonymity.yaml
python3 analyze.py shadow.data/hosts/observer/timings.csv --searcher-ip 11.0.0.7
```

This reports the timing adversary's guess (target: no better than chance, 1/N) and a two-sample KS
timing-independence test (the searcher's send-gap distribution vs. the cover clients'). A single run is **one
trial**; the multi-trial runner rotates the searcher across hosts and aggregates accuracy.

## 6. The Nym mixnet transport (excluded crate; needs the stable toolchain)

`crates/asac-mix-nym` is excluded from the workspace and built separately (it needs rustc ≥ 1.94 and vendors a
yanked dep — see its `Cargo.toml`):

```sh
cargo +stable test --manifest-path crates/asac-mix-nym/Cargo.toml   # builds the nym-sdk tree; offline test passes
```

## What is NOT reproducible here

A *proven* privacy guarantee. The mechanisms are implemented, tested, and measured, and the argument is
written down — but it has **not** been externally reviewed. See [docs/EXTERNAL-PLAN.md](docs/EXTERNAL-PLAN.md)
for what still needs outside experts.
