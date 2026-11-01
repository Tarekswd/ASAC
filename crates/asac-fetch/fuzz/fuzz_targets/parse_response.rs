#![no_main]
//! Fuzz the HTTP response parser with arbitrary bytes: it eats responses from untrusted servers (any .onion),
//! so it must never panic or over-read. Run: `cargo +nightly fuzz run parse_response` (from crates/asac-fetch).
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    asac_fetch::__fuzz_parse(data);
});
