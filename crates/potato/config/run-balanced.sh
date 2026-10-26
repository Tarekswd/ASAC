#!/usr/bin/env bash
# Start the Potato Balanced-mode SOCKS proxy: Arti's proxy with config/balanced.toml.
# Point Tor Browser / apps at SOCKS 127.0.0.1:9150. Ctrl-C to stop.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
command -v arti >/dev/null 2>&1 || { echo "arti not found; install it (see docs/INSTALL.md)"; exit 1; }
exec arti proxy -c "$HERE/balanced.toml"
