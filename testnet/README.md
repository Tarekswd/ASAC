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

### Known limitation: the byte-transfer self-test on Python 3.12

The harness's success criterion is **bootstrap** — a private Tor network that forms circuits, which is what
Phase 0 needs. chutney also has an optional data-transmission self-test (`chutney verify`) that pushes bytes
exit-to-client through its own traffic generator. That generator imports `asyncore`/`asynchat`, removed from
the Python standard library in 3.12. `setup-chutney.sh` installs the `pyasyncore`/`pyasynchat` backports so
chutney *runs*, but the backport does not drive the event loop to completion on 3.12, so the byte self-test
times out even though circuits are fine (verified: tasks stay "not done" with `failure_count == 0`).

This is an upstream chutney/Python-3.12 issue, not a problem with the network. To run the self-test too, use a
Python with a real `asyncore` (<= 3.11), for example:

```sh
sudo apt-get install -y python3.11 python3.11-venv   # or pyenv
python3.11 -m venv ~/.chutney-venv && ~/.chutney-venv/bin/python -m pip install --upgrade pip
# then run chutney with ~/.chutney-venv/bin/python on PATH
```

### Known flakiness: last-5% bootstrap under WSL + tor 0.4.9.11

Observed this session with the Ubuntu `tor` 0.4.9.11 package in WSL2: the directory **authorities reach 100%
and publish a full consensus** (all nodes, `desc md md_cons ns_cons`, "Dir info cached"), but the relays and
clients can stall at **95%, "Establishing a Tor circuit"** within the wait window, so `wait_for_bootstrap`
reports failure even though the network is substantially up (verified: authorities 100%, consensus distributed;
the Tor Expert Bundle's newer tor 0.4.9.14 got further in earlier runs).

`run-chutney.sh` already raises the wait to `CHUTNEY_START_TIME=180`. If you hit this, options in order of
preference:

- Install the current stable `tor` (0.4.9.14+) from the official repo instead of the distro package:
  <https://support.torproject.org/apt/> — the newer tor converged further here.
- Increase the window further: `CHUTNEY_START_TIME=300 ./run-chutney.sh`.
- For development you often do not need 100% of all nodes: once the authorities publish a consensus, you can
  point Arti/Potato at the network and build your own circuits.

## Shadow (scaled simulation)

Shadow builds from source and needs system dev packages (hence sudo), so it is not installed automatically.
See [`shadow/README.md`](shadow/README.md) for the verified setup, and the upstream Tor example at
<https://shadow.github.io/docs/guide/getting_started_tor.html>.
