# Test network (Phase 0)

Two tiers of private Tor network for developing and measuring Potato, isolated from the real Tor network
(PLAN.md section 5, Phase 0). Neither touches the public network.

| Tier | Tool | Use | Runs here? |
|------|------|-----|------------|
| Functional | **chutney** | A handful of real `tor` relays on localhost. Fast, good for correctness and integration. | Yes (needs the `tor` package). |
| Scaled / simulated | **Shadow** | Discrete-event simulation of a large Tor network in simulated time, running the real `tor`/Arti binaries. For latency/privacy measurement. | Needs a build with sudo deps; see `shadow/`. |

## chutney

### Prerequisite: tor + tor-gencert

chutney drives real `tor` relays and uses **`tor-gencert`** to build the directory-authority keys for a fresh
network. Both ship in the Debian/Ubuntu `tor` package (which also pulls in libevent), so install it once:

```sh
sudo apt-get update && sudo apt-get install -y tor
```

(The Tor Expert Bundle is not enough here: it ships `tor` but not `tor-gencert`, so it cannot build a network
from scratch.)

### Run

`setup-chutney.sh` checks the prerequisites, clones chutney, and installs the Python backports it needs.
`run-chutney.sh` brings a small network up, waits for it to bootstrap, runs the self-test, and tears it down.

```sh
./setup-chutney.sh
./run-chutney.sh      # prints BOOTSTRAP_OK and exits 0 when the network is up
```

The chutney clone and generated network state are git-ignored.

### Verified result

With tor **0.4.9.14** (from the official Tor apt repo), a full run is green on WSL2 + Python 3.12:

```text
tor: /usr/sbin/tor (Tor version 0.4.9.14.)
Everything bootstrapped after 52 sec
BOOTSTRAP_OK: private Tor network is up and forming circuits
Transmission: Success
BOOTSTRAP_RC=0 VERIFY_RC=0
```

Both the bootstrap and chutney's byte-transfer self-test pass. The `pyasyncore`/`pyasynchat` backports that
`setup-chutney.sh` installs are enough for the self-test on Python 3.12 — it passes once the network has fully
bootstrapped.

### Use the official-repo tor (0.4.9.14+), not the distro package

The Ubuntu `tor` 0.4.9.11 package converged slowly under WSL: authorities reached 100% and published a full
consensus, but relays/clients could stall at 95% "Establishing a Tor circuit" within the wait window. The
current stable tor from the official repo bootstraps fully (52 s above). Install it from
<https://support.torproject.org/apt/>. `run-chutney.sh` also raises the wait to `CHUTNEY_START_TIME=180`
(override with a larger value if your machine is slow).

## Shadow (scaled simulation)

Shadow builds from source and needs system dev packages (hence sudo), so it is not installed automatically.
See [`shadow/README.md`](shadow/README.md) for the verified setup, and the upstream Tor example at
<https://shadow.github.io/docs/guide/getting_started_tor.html>.
