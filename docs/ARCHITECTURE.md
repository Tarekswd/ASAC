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
│   └── potarek-decoy/      # lib: client-side decoy-query generator (PLAN 4.3 item 7)
└── docs/
```

## Crates that exist today

| Crate | Kind | Responsibility | Plan ref | Status |
|-------|------|----------------|----------|--------|
| `potato` | bin | Bootstrap a Tor client via `arti-client`; smoke-test connectivity. SOCKS proxy is Arti's `arti proxy` for now. | 3, 5 (Phase 0/1) | skeleton |
| `potato-policy` | lib | `Mode` enum (Swift/Balanced/Fortress) and the rules each mode implies. | 2.2 | real, tested |
| `potarek-decoy` | lib | Generate benign decoy queries from a caller-supplied vetted term list. | 4.3 | real, tested |

## Crates planned (not yet created)

These are named here so the module boundaries are agreed before code lands. Each arrives with its phase
(PLAN.md section 5), not before:

| Planned crate | Responsibility | Plan ref | Phase |
|---------------|----------------|----------|-------|
| `potato-proxy` | Wrap Arti's SOCKS proxy with the policy layer (force CGO, Conflux, strict isolation). | 3.1, 3.4, 3.9 | 1 |
| `potato-pq` | Hybrid post-quantum ntor-v3 + ML-KEM handshake negotiation. | 3.2 | 2 |
| `potarek-crawler` | Onion crawler (Rust/`arti-client`) + clear-web crawler (Scrapling, HTTP-only over Tor). | 4.3 #1 | 4 |
| `potarek-index` | Full-text index over Tantivy (BM25), storing title/URL/text/links only. | 4.3 #3 | 4 |
| `potarek-frontend` | v3 onion service front end with Equi-X PoW, static HTML, no JS. | 4.3 #5 | 4 |
| `potarek-safety` | Pre-index content-safety filter (launch blocker). | 4.3 #2 | 4 |

## Hard rules the layout enforces

- **No new crypto primitives.** Crypto comes from Arti and audited libraries; our crates compose policy
  (PLAN.md 2.3).
- **No PHP or JavaScript.** Rust everywhere, except Scrapling (Python) for the optional clear-web crawl,
  restricted to HTTP-only over Tor (PLAN.md 2.3, 4.3 #1).
- **TLS is rustls** (pure-Rust), not native-tls, to avoid a system OpenSSL dependency and stay memory-safe.
