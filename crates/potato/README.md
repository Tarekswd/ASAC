# potato (Phase 0 skeleton)

The first real code for the Potato client: a workspace crate that depends on
[`arti-client`](https://crates.io/crates/arti-client) and proves it can reach the Tor network.

## Requirements

- **Rust 1.92 or newer.** The Arti 0.47 crates require it (verified 2026-10-26). `rust-toolchain.toml`
  pins this automatically if you use [rustup](https://rustup.rs). A standalone older toolchain will not compile.

## Build and test

```sh
cargo test -p potato     # offline: checks the default Tor config builds
cargo run  -p potato      # online: bootstraps Tor and fetches example.com over it (takes a while)
```

## SOCKS proxy

`arti-client` does not yet expose an embeddable SOCKS API, so for now the SOCKS proxy is Arti's own binary:

```sh
cargo install arti        # or: cargo run -p arti -- proxy
arti proxy                 # SOCKS proxy on 127.0.0.1:9150
```

Phase 1 wraps this proxy with Potato's policy layer (force CGO, Conflux, strict stream isolation); see
`../../PLAN.md` sections 3 and 5.
