#!/usr/bin/env python3
"""Patch a tornettools-generated Shadow config to run ASAC instead of the default tgen (MILESTONES.md M2).

tornettools wires its onion-service hosts to run `tgen`. This helper rewrites the chosen hosts' process
command(s) to run ASAC binaries instead, leaving the rest of the generated network (C-tor relays, dir auths,
scale, timing) untouched.

Honest limits: the exact generated schema can vary by tornettools version, so ALWAYS run `--list` first to see
the real host/process layout, confirm which hosts are the onion server and client, then swap by name. Dry-run
is the default; nothing is written without `--write`. Requires PyYAML (present in the tornettools venv).

Examples:
  python3 patch_asac_config.py tornet-0.001/shadow.config.yaml --list
  python3 patch_asac_config.py tornet-0.001/shadow.config.yaml --asac-bin-dir ~/asac/target/release \\
      --server-host server1onionserver --client-host markovclient1 --write
  python3 patch_asac_config.py --selftest
"""
import argparse
import sys


def load_hosts(doc):
    """Return the hosts mapping, tolerating the documented Shadow shape {'hosts': {name: {...}}}."""
    hosts = doc.get("hosts") if isinstance(doc, dict) else None
    if not isinstance(hosts, dict):
        raise SystemExit("ERROR: no 'hosts' mapping found. Run with --list on the real file and adjust; the "
                         "generated schema may differ from the expected Shadow 'hosts: {name: {processes: [...]}}'.")
    return hosts


def processes_of(hostcfg):
    procs = hostcfg.get("processes") if isinstance(hostcfg, dict) else None
    return procs if isinstance(procs, list) else []


def list_hosts(hosts):
    for name, cfg in hosts.items():
        procs = processes_of(cfg)
        print(f"- {name}  ({len(procs)} process(es))")
        for p in procs:
            if isinstance(p, dict):
                print(f"    path={p.get('path')!r} args={p.get('args')!r} start_time={p.get('start_time')}")


def swap_host(hosts, host, new_path, new_args):
    """Rewrite every process on `host` to run new_path/new_args. Returns a list of (before, after) tuples."""
    if host not in hosts:
        raise SystemExit(f"ERROR: host {host!r} not in config. Use --list to see the real host names.")
    changes = []
    procs = processes_of(hosts[host])
    if not procs:
        raise SystemExit(f"ERROR: host {host!r} has no 'processes' to rewrite.")
    for p in procs:
        before = (p.get("path"), p.get("args"))
        p["path"] = new_path
        p["args"] = new_args
        changes.append((before, (new_path, new_args)))
    return changes


def selftest():
    cfg = {"hosts": {
        "srv": {"processes": [{"path": "/x/tgen", "args": "server.graphml", "start_time": 60}]},
        "cli": {"processes": [{"path": "/x/tgen", "args": "client.graphml", "start_time": 300}]},
    }}
    hosts = load_hosts(cfg)
    c1 = swap_host(hosts, "srv", "/asac/asac-server", "9050")
    c2 = swap_host(hosts, "cli", "/asac/asac-observe", "--out obs.csv")
    assert cfg["hosts"]["srv"]["processes"][0]["path"] == "/asac/asac-server", c1
    assert cfg["hosts"]["srv"]["processes"][0]["args"] == "9050"
    assert cfg["hosts"]["cli"]["processes"][0]["path"] == "/asac/asac-observe", c2
    assert cfg["hosts"]["srv"]["processes"][0]["start_time"] == 60  # untouched fields preserved
    print("selftest OK")


def main():
    ap = argparse.ArgumentParser(description="Swap tgen -> ASAC in a tornettools Shadow config.")
    ap.add_argument("config", nargs="?", help="path to the generated shadow.config.yaml")
    ap.add_argument("--asac-bin-dir", help="dir holding asac-server / asac-emit / asac-observe (e.g. target/release)")
    ap.add_argument("--server-host", help="host name to run asac-server on")
    ap.add_argument("--client-host", help="host name to run the ASAC client/observer on")
    ap.add_argument("--server-args", default="", help="args for asac-server (e.g. onion nickname / port)")
    ap.add_argument("--client-args", default="", help="args for the client binary")
    ap.add_argument("--client-bin", default="asac-observe", help="client binary name in --asac-bin-dir")
    ap.add_argument("--list", action="store_true", help="print hosts + process commands and exit")
    ap.add_argument("--write", action="store_true", help="write changes back (default is a dry run)")
    ap.add_argument("--selftest", action="store_true", help="run the built-in self-check and exit")
    a = ap.parse_args()

    if a.selftest:
        selftest()
        return
    if not a.config:
        ap.error("config path is required (or use --selftest)")
    try:
        import yaml
    except ImportError:
        raise SystemExit("ERROR: PyYAML not found. Run inside the tornettools venv, or: pip install pyyaml")

    with open(a.config, "r", encoding="utf-8") as f:
        doc = yaml.safe_load(f)
    hosts = load_hosts(doc)

    if a.list or (not a.server_host and not a.client_host):
        list_hosts(hosts)
        if not a.list:
            print("\nNo --server-host/--client-host given; nothing to change. "
                  "Pick host names from the list above.")
        return

    if not a.asac_bin_dir:
        ap.error("--asac-bin-dir is required to swap in ASAC binaries")
    bindir = a.asac_bin_dir.rstrip("/")
    all_changes = []
    if a.server_host:
        all_changes += swap_host(hosts, a.server_host, f"{bindir}/asac-server", a.server_args)
    if a.client_host:
        all_changes += swap_host(hosts, a.client_host, f"{bindir}/{a.client_bin}", a.client_args)

    print("Planned changes:" if not a.write else "Applied changes:")
    for before, after in all_changes:
        print(f"  {before[0]!r} {before[1]!r}  ->  {after[0]!r} {after[1]!r}")

    if a.write:
        import yaml
        with open(a.config, "w", encoding="utf-8") as f:
            yaml.safe_dump(doc, f, sort_keys=False)
        print(f"\nWrote {a.config}. Back up / regenerate with tornettools if you need the original.")
    else:
        print("\nDry run. Re-run with --write to apply.")


if __name__ == "__main__":
    main()
