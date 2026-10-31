# F5 — network-level anonymity measurement in Shadow

**Status: harness spec + config template. NOT run in-repo** (needs Shadow installed, and a built emitter
binary; see prerequisites). The *scheduler-level* version of this measurement is already done in-repo and
tested — `asac_mix::timing_observer_accuracy` shows a timing-only observer is at chance (1/N). This directory
is the plan for the *network-level* version: the same question with packets actually crossing a simulated
network and a global observer of the links (FLAGSHIP-DESIGN.md §7, EXTERNAL-PLAN.md item 2).

## What it measures
1. **Anonymity set** — with `N` client hosts, exactly one actually searching and the rest idle (all emitting
   the Poisson cover stream from `asac-client`), how well can an observer of every link identify the searcher?
   Target: no better than chance (`1/N`), i.e. the anonymity set is the full population.
2. **Timing independence** — the distribution of a client's send times is statistically the same whether it is
   searching or idle (a stronger, network-level restatement of the in-repo scheduler test).

## Prerequisites (the external parts)
- [Shadow](https://shadow.github.io) installed (Linux).
- An **emitter binary** `asac-emit` (to be written, ~a few lines): runs a `asac_client::DecoyClient`, framing
  each tick with `asac_mix::frame`, and sends it through a `MixTransport`. For an isolated measurement a
  loopback/socket transport to the observer is enough; for the full path, the Nym transport (EXTERNAL-PLAN
  item 3).
- An **observer/collector**: records per-host send timestamps (Shadow can capture pcap, or the emitter logs).
- An analysis script: from the recorded timings, estimate the observer's identification accuracy and run the
  timing-independence test; compare to the targets above.

## Topology
`mix-anonymity.yaml` (template, this directory): one `observer` host and `N` `client*` hosts sharing a host
profile via a YAML anchor. One client is launched with the "real query" argument; the rest emit only cover.
The network graph is a single switch (gml inline) — latency/bandwidth can be tuned to model Tor+mix delays.

## Run (once the prerequisites exist)
```sh
# build the emitter for the Shadow host environment, then:
shadow sim/shadow/mix-anonymity.yaml
# -> shadow.data/ ; run the analysis script over it to get accuracy + the timing test.
```

## Honesty note
The config here is **schema-grounded but unexecuted** — it follows the Shadow config spec but has not been run,
and the emitter/observer/analysis pieces are not built. Treat it as the starting point a run completes, not a
validated result. No anonymity claim follows until this is actually run and the numbers meet the targets, as
part of the F6 review (FLAGSHIP-DESIGN.md §8).
