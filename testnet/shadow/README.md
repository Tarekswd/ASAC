# Shadow simulation

[Shadow](https://shadow.github.io) is a discrete-event network simulator that runs the **real** `tor` (and
Arti) binaries in simulated time, so you can model a large private Tor network on one machine and measure
latency and privacy properties (PLAN.md sections 3.3, 3.4, 6).

## Verified running under WSL2 (2026-10-26)

Shadow **3.3.0** was built from source and a minimal simulation ran on WSL2 (kernel 6.6, well above Shadow's
5.15 minimum) — no extra kernel tuning and no `libc-dbg` needed on Ubuntu 24.04 noble:

```text
Shadow 3.3.0
... Running simulation / Finished simulation
hello-from-shadow        # the simulated host executed /bin/echo
```

The upstream **Simple Tor Network** example (`~/shadow/examples/docs/tor/`, needs `tgen` built from source +
the system `tor`) also runs green here: a full simulated Tor network (authority + 4 relays + 2 exits + a tor
client) completed 30 simulated minutes in ~28 s real, with 20 successful client streams. A 1 MiB download over
the simulated Tor path measured TTFB ~800 ms and last-byte ~2.1 s — a self-consistent in-Shadow baseline (the
topology uses a fixed 50 ms link latency, so it is not directly comparable to live-network numbers).

## Why this is not auto-installed by the setup scripts

Shadow builds from source and needs system development packages (installed with `sudo`), which this project's
setup scripts never assume. So this directory **documents and scripts** the setup rather than running sudo
unattended.

## Verified setup (run on a Linux host or a sudo-capable WSL)

1. Build/install Shadow from source following the official guide:
   <https://shadow.github.io/docs/guide/install/> (clone `github.com/shadow/shadow`, install its listed
   dependencies, `./setup build --clean && ./setup install`).
2. Install a `tor` binary (the official-repo `tor` package used by the chutney tier works).
3. Follow the Simple Tor Network example:
   <https://shadow.github.io/docs/guide/getting_started_tor.html>.
4. For realistic, scaled Tor topologies, use **tornettools**
   (<https://github.com/shadow/tornettools>), the maintained tool for generating Shadow Tor networks.

`setup-shadow.sh` clones Shadow and tornettools and prints the dependency-install command for Ubuntu, but
stops short of running sudo. Review it, then run the printed steps yourself.
