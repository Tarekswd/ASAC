# Install guide

Everything needed to build and run Potato/ASAC, by phase. Verified on **WSL2 Ubuntu 24.04 (noble),
x86_64**; a native Linux host works the same. The build is Linux-first — on Windows, use WSL2.

## Phase 0 — Foundations (required now)

```sh
# Rust toolchain (rustup); the repo pins 1.92 via rust-toolchain.toml
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add clippy rustfmt

# Build/runtime libs for the Arti client (rustls + static-sqlite avoid most system deps, but these help)
sudo apt-get install -y build-essential pkg-config git python3 curl libssl-dev libsqlite3-dev

# tor + tor-gencert from the OFFICIAL Tor repo (distro 0.4.9.11 converges slowly; use 0.4.9.14+)
# full repo setup: https://support.torproject.org/apt/
sudo apt-get install -y tor deb.torproject.org-keyring

# chutney test network uses Python; on Python 3.12 it needs these backports (user site, no sudo)
python3 -m pip install --user pyasyncore pyasynchat    # add --break-system-packages if PEP668 blocks it

# Arti CLI (SOCKS proxy for the client tier)
cargo install arti --locked --features static-sqlite

# Shadow simulator + tgen (scaled/simulated test network)
sudo apt-get install -y cmake findutils libclang-dev libc-dbg libglib2.0-0t64 libglib2.0-dev \
    make netbase python3 python3-networkx xz-utils util-linux gcc g++ libigraph-dev
# then build from source (no sudo): shadow -> ./setup build && ./setup install ; tgen -> cmake/make/install

# Optional supply-chain / fuzzing tools
cargo install cargo-deny cargo-fuzz cargo-audit
```

Notes verified this session: the Tor Expert Bundle is **not** enough for chutney (it lacks `tor-gencert`);
`libc-dbg` turned out not to be required for the Shadow build on noble; `libglib2.0-0` is named
`libglib2.0-0t64` on noble.

## Phase 1 — Potato Balanced client
No new system installs. Arti crates come via `cargo`. Optional: Tor Browser (kept stock) to drive the proxy.

## Phase 2 — Hybrid post-quantum handshake
- `ml-kem` crate — pulled by cargo (not a manual install).
- Fuzzing: `cargo install cargo-fuzz` + `rustup toolchain install nightly`.
- Formal analysis: **Tamarin** (Haskell/stack + Maude) or **ProVerif** (OCaml) — heavy, pick one.

## Phase 3 — Swift / Fortress modes
Nothing new; reuses Shadow from Phase 0. Scaled sims use **tornettools** (cloned) + Tor metrics data.

## Phase 4 — ASAC search engine
- **Tantivy** — cargo crate (not a manual install).
- **Scrapling** — `pip install scrapling` (HTTP-only over Tor per this project; no browser mode needed).
- **Content-safety filter** — an industry hash-matching service/partner + a classifier; agreements + legal
  counsel, not a package. Launch blocker.

## Phase 5 — Hardening & audit
`gpg` (signed releases), `cargo install cargo-audit`, reproducible-build tooling. External auditors.

## Phases 6–7
No new core installs.
