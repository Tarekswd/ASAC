# Running the ASAC metadata measurement on a realistic Tor network (tornettools + Shadow)

This is the runbook for **MILESTONES.md M2** — taking ASAC's metadata-unobservability measurement from the
hand-built loopback harness in this directory to a **realistic, real-metrics-modeled Tor network at
configurable scale**, using [`shadow/tornettools`](https://github.com/shadow/tornettools) + the
[Shadow](https://github.com/shadow/shadow) simulator. Suggested by Rob Jansen (NRL / Shadow maintainer).

## Hard constraints (read first)

- **x86-64 Linux only.** Shadow supports Ubuntu 22.04/24.04 or Debian 12/13 on **x86-64** — not ARM. So the
  Oracle "always free" ARM box will **not** work. (Source: shadow.github.io/docs/guide/supported_platforms.)
- **RAM is the bottleneck.** A 1% network (`--network_scale 0.01`) for ~60 sim-minutes can need **~30 GiB RAM**
  (tornettools README). Free x86 tiers (~1 GiB) and a 7-8 GiB laptop/WSL only suffice for a tiny smoke test.
  For a real measurement, use a credit-funded cloud VM (new AWS/Azure accounts include ~$200 credit) or
  university HPC with 32-64 GiB.

## One-shot provisioning

On a fresh Ubuntu 22.04/24.04 x86-64 box:

```sh
# from the ASAC repo:
DATA_MONTH=2023-04 SCALE=0.001 ./sim/shadow/provision_tornettools.sh
```

This installs apt deps + Rust, builds and installs **Shadow**, **tgen**, **oniontrace**, and **C-tor**,
installs **tornettools** in a venv, downloads one month of Tor network data, then `stage`s and `generate`s a
`tornet-$SCALE` network. (It does not run the big simulation.) Pick a `DATA_MONTH` that has data at
collector.torproject.org; start with `SCALE=0.001`.

## Step 1 - smoke test with stock tgen first

Prove the pipeline and surface Shadow compatibility problems before involving ASAC:

```sh
cd ~/asac-shadow
tornettools simulate tornet-0.001
grep -ri "unimplemented\|unsupported syscall\|not implemented" tornet-0.001/shadow.data/ | head
```

If a genuine blocker shows up (an unimplemented syscall/kernel feature Arti or tokio needs), open a thread at
<https://github.com/shadow/shadow/discussions> — the community helps there.

## Step 2 - swap tgen -> ASAC (the one ASAC-specific change)

tornettools wires its onion-service hosts to run `tgen`. Replace those with ASAC's binaries:

```sh
# build ASAC release binaries (in the ASAC repo, on the same Linux box):
cargo build --release -p asac-server -p asac-emit -p asac-observe

# inspect the generated config to find the onion server + client host names:
python3 sim/shadow/patch_asac_config.py ~/asac-shadow/tornet-0.001/shadow.config.yaml --list

# dry-run the swap for the hosts you identified, then add --write to apply:
python3 sim/shadow/patch_asac_config.py ~/asac-shadow/tornet-0.001/shadow.config.yaml \
  --asac-bin-dir $(pwd)/target/release \
  --server-host <ONION_SERVER_HOST> --server-args "<nickname/port>" \
  --client-host <CLIENT_HOST>       --client-bin asac-observe --client-args "<args>" \
  --write
```

Everything else (the C-tor relays, directory authorities, scale, timing model) stays as generated. The
`--list` step matters: the exact host names and process schema come from your tornettools version, so confirm
them before `--write`. `patch_asac_config.py --selftest` verifies the swap logic.

> Note: ASAC connects via **Arti** (Rust Tor), while the simulated relays are **C-tor**. That is fine — ASAC's
> onion client/server are just application processes talking to the Tor network — but it is the part to watch
> in the Step 1 syscall check.

## Step 3 - run, parse, analyze

```sh
tornettools simulate tornet-0.001
tornettools parse    tornet-0.001
tornettools plot     tornet-0.001 --prefix pdfs
```

Feed the per-host send timings from `asac-observe` into this directory's existing KS timing-independence
analysis (`analyze.py`) to extend the loopback result (adversary at chance 1/N, KS-indistinguishable) to a
realistically-modeled network. Once a small net runs ASAC cleanly, regenerate at larger `--network_scale` on a
big-RAM box and raise the trial count N well beyond 6 for a tighter bound.

## Citation

Cite tornettools' paper in any write-up: **Rob Jansen, Justin Tracey, Ian Goldberg,
*"Once is Never Enough: Foundations for Sound Statistical Inference in Tor Network Experimentation"*, USENIX
Security 2021.**

---
*Status: this runbook is the documented path (verified against the Shadow/tgen/oniontrace/tornettools docs,
2026-11-25) plus Rob Jansen's guidance. It has not yet been executed end to end with ASAC; Steps 1-2 are where
real integration issues (Shadow syscall support, config schema) will show up.*
