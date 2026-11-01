# asac-fetch fuzzing

Coverage-guided fuzzing of the untrusted HTTP response parser (`parse_response` via the hidden
`asac_fetch::__fuzz_parse` entry point). The parser consumes responses from arbitrary servers (any `.onion`),
so the invariant is: **never panic, never over-read, on any input.** The in-tree adversarial unit tests are the
CI-gated floor; this harness is the deeper, on-demand check.

## Run (needs a nightly toolchain + cargo-fuzz)

```sh
rustup toolchain install nightly
cargo install cargo-fuzz        # if not already installed
cd crates/asac-fetch
cargo +nightly fuzz run parse_response        # fuzz until a crash or Ctrl-C
cargo +nightly fuzz run parse_response -- -runs=1000000   # bounded run
```

This is **not** part of CI: libFuzzer requires nightly and a sanitizer build, which the project's pinned
stable/1.92 CI does not provide. Run it periodically and before releases; any crash input lands in
`fuzz/artifacts/` for a regression test.
