#!/usr/bin/env bash
# Prepare a chutney Tor test network. Checks prerequisites and clones chutney; installs nothing with sudo.
set -euo pipefail
cd "$(dirname "$0")"

# chutney needs both `tor` and `tor-gencert` (the latter builds the directory-authority keys). On
# Debian/Ubuntu both ship in the `tor` package, which also pulls in libevent. Installing it needs root, so
# we check and tell you rather than install it ourselves.
missing=0
command -v tor        >/dev/null 2>&1 || { echo "missing: tor"; missing=1; }
command -v tor-gencert >/dev/null 2>&1 || { echo "missing: tor-gencert"; missing=1; }
if [ "$missing" -ne 0 ]; then
  echo
  echo "Install the Tor package (provides both tor and tor-gencert):"
  echo "    sudo apt-get update && sudo apt-get install -y tor"
  exit 1
fi
echo "tor:         $(command -v tor) ($(tor --version | head -1))"
echo "tor-gencert: $(command -v tor-gencert)"

if [ ! -d chutney ]; then
  echo "cloning chutney..."
  git clone --depth 1 https://github.com/torproject/chutney.git
fi

# chutney still imports asyncore/asynchat, removed from the stdlib in Python 3.12. Supply PyPI backports via
# the user site (no sudo, no venv) when the stdlib lacks them.
if ! python3 -c 'import asyncore, asynchat' 2>/dev/null; then
  echo "installing asyncore/asynchat backports for Python 3.12+ ..."
  python3 -m pip install --user pyasyncore pyasynchat 2>/dev/null \
    || python3 -m pip install --user --break-system-packages pyasyncore pyasynchat
fi

echo "available network flavors:"
ls chutney/networks | sed -n '1,40p'
echo "setup-chutney OK"
