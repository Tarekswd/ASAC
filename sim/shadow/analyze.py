#!/usr/bin/env python3
"""F5 analysis: from the observer's timings.csv, test whether the searcher is distinguishable from the
cover-only clients by send timing.

Usage: analyze.py <timings.csv> [--searcher-ip IP]

The observer logged (elapsed_ms, src_ip, len) for every packet. Each emitting host is one src_ip. We compute
per-host timing stats and run a simple timing adversary: guess the searcher as the host whose mean inter-send
gap deviates most from the group median. With N emitting hosts, chance is 1/N; because the asac-mix schedule
is independent of whether a host is really searching, the adversary should do no better than chance, and the
per-host inter-send distributions should look alike (all ~ Exponential(rate), mean ~ 1/rate).

This scores ONE run (one searcher placement) = one trial; a full accuracy estimate needs many runs. Pass the
searcher's Shadow IP with --searcher-ip to score the guess against ground truth.
"""
import csv
import statistics
import sys


def load(path):
    rows = []
    with open(path) as f:
        r = csv.reader(f)
        header = next(r, None)
        for row in r:
            if len(row) != 3:
                continue
            try:
                rows.append((int(row[0]), row[1], int(row[2])))
            except ValueError:
                continue
    return rows


def per_host_gaps(rows):
    by_ip = {}
    for ms, ip, _ in rows:
        by_ip.setdefault(ip, []).append(ms)
    gaps = {}
    for ip, times in by_ip.items():
        times.sort()
        gaps[ip] = [b - a for a, b in zip(times, times[1:])]
    return by_ip, gaps


def ks_two_sample(a, b):
    """Two-sample Kolmogorov-Smirnov statistic D and the ~5%-significance critical value.

    D is the max gap between the two empirical CDFs. If D <= crit, the two inter-send-gap samples are not
    distinguishable at the 5% level -- i.e. the searcher's timing looks like the cover clients'. Stdlib only.
    """
    a, b = sorted(a), sorted(b)
    na, nb = len(a), len(b)
    if na == 0 or nb == 0:
        return 0.0, 0.0
    i = j = 0
    d = 0.0
    while i < na and j < nb:
        x = min(a[i], b[j])
        while i < na and a[i] == x:
            i += 1
        while j < nb and b[j] == x:
            j += 1
        d = max(d, abs(i / na - j / nb))
    crit = 1.36 * ((na + nb) / (na * nb)) ** 0.5
    return d, crit


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        sys.exit(1)
    path = args[0]
    searcher_ip = None
    if "--searcher-ip" in args:
        searcher_ip = args[args.index("--searcher-ip") + 1]

    rows = load(path)
    by_ip, gaps = per_host_gaps(rows)
    hosts = sorted(by_ip)
    n = len(hosts)
    if n < 2:
        print(f"need >=2 emitting hosts, found {n}")
        sys.exit(1)

    print(f"{'src_ip':<16} {'packets':>8} {'mean_gap_ms':>12} {'std_gap_ms':>11}")
    means = {}
    for ip in hosts:
        g = gaps[ip]
        mean = statistics.mean(g) if g else 0.0
        std = statistics.pstdev(g) if len(g) > 1 else 0.0
        means[ip] = mean
        print(f"{ip:<16} {len(by_ip[ip]):>8} {mean:>12.1f} {std:>11.1f}")

    # Simple timing adversary: pick the host whose mean gap is farthest from the group median.
    median = statistics.median(means.values())
    guess = max(hosts, key=lambda ip: abs(means[ip] - median))
    chance = 1.0 / n
    print(f"\nemitting hosts N = {n}  (chance = 1/N = {chance:.3f})")
    print(f"timing-adversary guess (max deviation from median mean-gap): {guess}")
    if searcher_ip:
        print(f"actual searcher: {searcher_ip}  -> guess {'CORRECT' if guess == searcher_ip else 'WRONG'} (one trial)")
        # Timing-independence (FLAGSHIP-DESIGN.md §7, O5): the searcher's inter-send gaps should be
        # distributionally indistinguishable from the cover clients'. KS-test the searcher against the pooled
        # cover gaps.
        if searcher_ip in gaps:
            cover = [g for ip in hosts if ip != searcher_ip for g in gaps[ip]]
            d, crit = ks_two_sample(gaps[searcher_ip], cover)
            verdict = "indistinguishable at 5%" if d <= crit else "DISTINGUISHABLE -- investigate"
            print(f"timing-independence KS: D={d:.3f} crit(5%)={crit:.3f} -> {verdict}")
        else:
            print(f"(no packets recorded from searcher {searcher_ip}; cannot run the KS test)")
    else:
        print("pass --searcher-ip <IP> to score the guess; many runs are needed for an accuracy estimate.")


if __name__ == "__main__":
    main()
