# potato-bench

A small benchmark harness for the **client-observable** metrics in PLAN.md section 6: connect/circuit-setup
time, time-to-first-byte (TTFB), total fetch time, and download throughput. It drives an HTTP GET through a
**SOCKS5 proxy**, so you point it at whatever you want to measure.

```sh
cargo run -p potato-bench -- --proxy 127.0.0.1:9050 --host example.com --port 80 --path / --reps 5
```

Output is one JSON object per rep plus a summary, e.g.:

```text
{"rep":0,"connect_ms":812.4,"ttfb_ms":430.1,"total_ms":690.7,"bytes":1256,"throughput_kib_s":4.7}
{"summary":true,"reps_ok":5,"median_ttfb_ms":441.9}
```

`throughput_kib_s` is in KiB/s and is only meaningful for a **large** download — point `--host`/`--path` at a
sizeable file. For tiny responses the whole body arrives in ~one read, so the harness reports `0.0` rather than
a bogus number (verified against real Tor: example.com's ~931-byte page is too small to measure throughput).

## Getting the vanilla-Arti baseline (Phase 0 gate)

The Phase 0 gate wants a vanilla Arti baseline [PLAN.md section 5]. Measure Arti's SOCKS proxy:

```sh
cargo install arti          # or: cargo run -p arti -- proxy
arti proxy                   # SOCKS5 on 127.0.0.1:9150
# in another shell:
cargo run -p potato-bench -- --proxy 127.0.0.1:9150 --host example.com --port 80 --reps 10
```

You can also point `--proxy` at a chutney client's SOCKS port (see `../../testnet/`) to measure inside the
private test network, or at Potato's proxy later to compare against this baseline.

## What this does not measure

- **Circuit-build time** is an Arti-internal metric (Arti timeout stats), not a client-side timing; read it
  from Arti rather than inferring it here.
- **Full page-load time** (sub-resource fetching, rendering) is a browser concern; `total_ms` here is the time
  to fetch one resource, a useful proxy but not a real page load.
