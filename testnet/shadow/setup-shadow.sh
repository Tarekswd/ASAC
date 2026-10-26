#!/usr/bin/env bash
# Clone Shadow + tornettools and PRINT (do not run) the sudo steps. Review before running anything.
set -euo pipefail
cd "$(dirname "$0")"

if [ ! -d shadow ]; then
  echo "cloning shadow..."
  git clone --depth 1 https://github.com/shadow/shadow.git
fi
if [ ! -d tornettools ]; then
  echo "cloning tornettools..."
  git clone --depth 1 https://github.com/shadow/tornettools.git
fi

cat <<'NEXT'

Shadow and tornettools are cloned. Remaining steps need sudo, so run them yourself after review:

  # 1. Install Shadow's build dependencies (see shadow/docs for the authoritative list):
  cd shadow && ./setup build --clean --test && ./setup install

  # 2. Verify:
  shadow --version

  # 3. Build a Tor simulation with tornettools (see its README), or start from the simple example:
  #    https://shadow.github.io/docs/guide/getting_started_tor.html

This script intentionally does not run sudo or a multi-minute build unattended.
NEXT
