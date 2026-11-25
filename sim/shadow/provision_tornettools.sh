#!/usr/bin/env bash
# Provision a Linux box to run ASAC's full-path metadata measurement in Shadow via tornettools (MILESTONES.md M2).
#
# TARGET: Ubuntu 22.04 or 24.04, x86-64 ONLY. Shadow supports only Linux x86-64 Ubuntu 22.04/24.04 or Debian
#         12/13 -- it does NOT run on ARM, so the Oracle "always free" ARM box will not work. A realistic
#         (scale=0.01, i.e. 1%) Tor network can need ~30 GiB RAM; start tiny (scale=0.001) as a smoke test.
# SOURCES (verified 2026-11-25): shadow.github.io/docs/guide (install_dependencies, install_shadow,
#          supported_platforms); github.com/shadow/{tgen,oniontrace,tornettools} READMEs.
#
# Usage:   DATA_MONTH=2023-04 SCALE=0.001 ./provision_tornettools.sh
# Re-runnable: clones are skipped if already present; builds re-run.
set -euo pipefail

DATA_MONTH="${DATA_MONTH:-2023-04}"   # a month with data at collector.torproject.org; update to a recent one
SCALE="${SCALE:-0.001}"               # smoke-test scale; raise only on a big-RAM box (1% can need ~30 GiB)
PREFIX="$HOME/.local"
WORK="${WORK:-$HOME/asac-shadow}"     # Shadow forbids spaces in its build path -- keep WORK space-free
JOBS="$(nproc)"

echo ">>> arch check (Shadow is x86-64 only)"
test "$(uname -m)" = "x86_64" || { echo "ERROR: need x86_64, this host is $(uname -m). Shadow will not run."; exit 1; }

echo ">>> apt dependencies (Shadow + tgen + oniontrace + C-tor + tornettools helpers)"
sudo apt-get update
sudo apt-get install -y \
  git curl wget build-essential automake cmake pkg-config make \
  libclang-dev libglib2.0-0 libglib2.0-dev libigraph-dev \
  libssl-dev libevent-dev zlib1g zlib1g-dev \
  python3 python3-pip python3-venv python3-networkx \
  findutils netbase util-linux xz-utils faketime dstat procps

echo ">>> rust toolchain (rustup)"
command -v cargo >/dev/null 2>&1 || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
# shellcheck disable=SC1091
source "$HOME/.cargo/env" 2>/dev/null || true

mkdir -p "$WORK"; cd "$WORK"
export PATH="$PREFIX/bin:$PATH"

echo ">>> build + install Shadow"
[ -d shadow ] || git clone https://github.com/shadow/shadow.git
( cd shadow && ./setup build --clean --test && ./setup test && ./setup install )

echo ">>> build + install tgen"
[ -d tgen ] || git clone https://github.com/shadow/tgen.git
( cd tgen && mkdir -p build && cd build && cmake .. -DCMAKE_INSTALL_PREFIX="$PREFIX" && make -j"$JOBS" && make install )

echo ">>> build + install oniontrace"
[ -d oniontrace ] || git clone https://github.com/shadow/oniontrace.git
( cd oniontrace && mkdir -p build && cd build && cmake .. -DCMAKE_INSTALL_PREFIX="$PREFIX" && make -j"$JOBS" && make install )

echo ">>> build C-tor (tornettools needs tor + tor-gencert for relay keys; relays in the sim are C-tor)"
[ -d tor ] || git clone https://git.torproject.org/tor.git   # mirror: https://gitlab.torproject.org/tpo/core/tor.git
( cd tor && ./autogen.sh && ./configure --disable-asciidoc --disable-unittests --disable-manpage --disable-html-manual && make -j"$JOBS" )
export PATH="$PATH:$WORK/tor/src/core/or:$WORK/tor/src/app:$WORK/tor/src/tools"

echo ">>> install tornettools into a venv"
python3 -m venv toolsenv
# shellcheck disable=SC1091
source toolsenv/bin/activate
[ -d tornettools ] || git clone https://github.com/shadow/tornettools.git
( cd tornettools && pip install -r requirements.txt && pip install --ignore-installed . )

echo ">>> download Tor network data for $DATA_MONTH"
base="https://collector.torproject.org/archive/relay-descriptors"
wget -nc "$base/consensuses/consensuses-$DATA_MONTH.tar.xz"
wget -nc "$base/server-descriptors/server-descriptors-$DATA_MONTH.tar.xz"
wget -nc "https://metrics.torproject.org/userstats-relay-country.csv"
wget -nc "https://collector.torproject.org/archive/onionperf/onionperf-$DATA_MONTH.tar.xz"
wget -nc -O "bandwidth-$DATA_MONTH.csv" "https://metrics.torproject.org/bandwidth.csv?start=${DATA_MONTH}-01&end=${DATA_MONTH}-28"
tar xaf "consensuses-$DATA_MONTH.tar.xz"
tar xaf "server-descriptors-$DATA_MONTH.tar.xz"
tar xaf "onionperf-$DATA_MONTH.tar.xz"
[ -d tmodel-ccs2018.github.io ] || git clone https://github.com/tmodel-ccs2018/tmodel-ccs2018.github.io.git

echo ">>> stage (process relay + user info)"
tornettools stage \
  "consensuses-$DATA_MONTH" \
  "server-descriptors-$DATA_MONTH" \
  userstats-relay-country.csv \
  tmodel-ccs2018.github.io \
  --onionperf_data_path "onionperf-$DATA_MONTH" \
  --bandwidth_data_path "bandwidth-$DATA_MONTH.csv" \
  --geoip_path tor/src/config/geoip

echo ">>> generate a scale=$SCALE network"
tornettools generate \
  relayinfo_staging_*.json \
  userinfo_staging_*.json \
  networkinfo_staging.gml \
  tmodel-ccs2018.github.io \
  --network_scale "$SCALE" \
  --prefix "tornet-$SCALE"

cat <<EOF

================ DONE: toolchain built, scale=$SCALE network generated in $WORK/tornet-$SCALE ================
Next (full detail in the ASAC repo at sim/shadow/TORNETTOOLS.md):
  1. SMOKE TEST with the stock tgen config first, to prove the pipeline + catch Shadow syscall gaps:
       tornettools simulate tornet-$SCALE
     Then grep the logs for unimplemented syscalls / kernel features:
       grep -ri "unimplemented\\|unsupported syscall\\|not implemented" tornet-$SCALE/shadow.data/ | head
     If a genuine blocker, open a thread at github.com/shadow/shadow/discussions.
  2. SWAP tgen -> ASAC: build ASAC release binaries in the ASAC repo, then patch the generated config:
       cargo build --release -p asac-server -p asac-emit -p asac-observe
       python3 <asac-repo>/sim/shadow/patch_asac_config.py tornet-$SCALE/shadow.config.yaml \\
         --asac-bin-dir <asac-repo>/target/release --list        # inspect hosts/processes first
       # then re-run with --server-host/--client-host you identified, add --write to apply.
  3. RE-RUN + analyze:
       tornettools simulate tornet-$SCALE && tornettools parse tornet-$SCALE
       tornettools plot tornet-$SCALE --prefix pdfs
  WARNING: scale=0.01 (1%) can need ~30 GiB RAM for ~60 sim-minutes. Raise SCALE only on a big-RAM box.
===========================================================================================================
EOF
