# Shadow simulation

[Shadow](https://shadow.github.io) is a discrete-event network simulator that runs the **real** `tor` (and
Arti) binaries in simulated time, so you can model a large private Tor network on one machine and measure
latency and privacy properties (PLAN.md sections 3.3, 3.4, 6).

## Why this is not auto-installed here

Shadow builds from source and needs system development packages (installed with `sudo`), which this project's
setup scripts never assume. It also has Linux-kernel expectations that can need tuning; under WSL2 that may
require extra configuration {unverified}. So this directory **documents and scripts** the setup rather than
running it unattended.

## Verified setup (run on a Linux host or a sudo-capable WSL)

1. Build/install Shadow from source following the official guide:
   <https://shadow.github.io/docs/guide/install/> (clone `github.com/shadow/shadow`, install its listed
   dependencies, `./setup build --clean && ./setup install`).
2. Install a `tor` binary (the one in `../.toolcache/` from `setup-chutney.sh` works, or the distro package).
3. Follow the Simple Tor Network example:
   <https://shadow.github.io/docs/guide/getting_started_tor.html>.
4. For realistic, scaled Tor topologies, use **tornettools**
   (<https://github.com/shadow/tornettools>), the maintained tool for generating Shadow Tor networks.

`setup-shadow.sh` clones Shadow and tornettools and prints the dependency-install command for Ubuntu, but
stops short of running sudo. Review it, then run the printed steps yourself.
