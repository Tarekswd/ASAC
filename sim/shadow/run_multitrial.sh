#!/usr/bin/env bash
# F5 multi-trial measurement. Runs the Shadow sim N times, each with a DIFFERENT client as the real searcher,
# and aggregates the timing adversary's accuracy vs. chance (1/N). Turns the single-trial demo into a measured
# result. Run from this directory with the built binaries beside it:
#   cargo build -p asac-emit -p asac-observe
#   cp ../../target/debug/asac-emit ../../target/debug/asac-observe .
#   SHADOW=/path/to/shadow ./run_multitrial.sh      # SHADOW defaults to `shadow` on PATH
set -u
SHADOW="${SHADOW:-shadow}"
N="${N:-6}"

gen_yaml() { # $1 = searcher client index (1..N) -> writes mix.yaml
python3 - "$1" "$N" > mix.yaml <<'PY'
import sys
searcher, n = int(sys.argv[1]), int(sys.argv[2])
print("general:\n  stop_time: 90 s\nnetwork:\n  graph:\n    type: gml\n    inline: |")
print("      graph [\n        node [\n          id 0")
print('          host_bandwidth_down "100 Mbit"\n          host_bandwidth_up "100 Mbit"')
print("        ]\n        edge [\n          source 0\n          target 0")
print('          latency "150 ms"\n          packet_loss 0.0\n        ]\n      ]')
print("hosts:")
print("  observer:\n    network_node_id: 0\n    processes:\n      - path: ./asac-observe")
print("        args: --listen --out timings.csv\n        start_time: 1s\n        expected_final_state: running")
for k in range(1, n + 1):
    q = ' --query "the real search"' if k == searcher else ''
    print(f"  client{k}:\n    network_node_id: 0\n    processes:\n      - path: ./asac-emit")
    print(f"        args: --observer observer --rate 2 --seed 1{k}{q}")
    print("        start_time: 5s\n        expected_final_state: running")
PY
}

correct=0; trials=0
for k in $(seq 1 "$N"); do
  gen_yaml "$k"
  rm -rf shadow.data
  timeout 300 "$SHADOW" mix.yaml > "run$k.log" 2>&1
  # IPs auto-assign alphabetically: client1..clientN = 11.0.0.1..N, observer = 11.0.0.(N+1).
  res=$(python3 analyze.py shadow.data/hosts/observer/timings.csv --searcher-ip "11.0.0.$k" 2>&1)
  echo "--- trial $k: searcher = client$k (11.0.0.$k) ---"
  echo "$res" | grep -aE 'guess|CORRECT|WRONG|KS'
  echo "$res" | grep -aq 'guess CORRECT' && correct=$((correct + 1))
  trials=$((trials + 1))
done
echo
echo "=== F5 MULTI-TRIAL: adversary correct on $correct/$trials; chance = 1/$N = $(python3 -c "print(f'{1/$N:.3f}')") ==="
