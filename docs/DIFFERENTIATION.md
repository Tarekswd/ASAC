# What makes ASAC different

## The claim

**A search engine that is *mathematically unable* to profile you.** Not a promise not to log — an architecture
where the operator (including us) and a network observer *cannot* learn what you searched, by construction.

## Why that is new

Every existing onion/Tor search engine competes on a **no-log policy** — Ahmia, DuckDuckGo's onion service,
and newer "no-log" engines all advertise it [verified: their own descriptions]. A policy is something you must
*trust*. No deployed onion search engine offers a *cryptographic* guarantee that the operator cannot see the
query. That gap is the opening.

A better Tor *client* is **not** the differentiator: Potato's routing layer is "compose Arti well," and the
real latency/crypto wins (Conflux, Vegas, CGO, PQ) live inside Arti, not here. Competing there would be
overclaiming. We compete on query privacy instead.

## The idea that makes it reachable

Tor cannot defeat a **global passive adversary** (an observer who sees both ends and correlates by timing)
because general web browsing needs **low latency**. The defenses that *do* resist such an adversary —
mixnet-style mixing and cover traffic — cost latency and bandwidth, so Tor cannot use them for general traffic
[verified: mixnets (e.g. Loopix) resist the global passive adversary that Tor does not, via mixing + cover
traffic].

**Search is different: it is latency-tolerant.** A query is tiny and infrequent and can wait seconds. That
latency budget is exactly what the strong defenses need. So for the narrow search path we can afford what Tor
cannot:

1. **PIR (private information retrieval)** — the server answers without learning which query it answered
   [verified: PIR definition, CACM; single-server schemes such as PIANO, 2024]. Guarantee on query *content*.
2. **Mixnet-style cover traffic** — a network observer cannot tell *that/when* you searched, or link you to the
   server. Guarantee on *metadata* (under stated assumptions).
3. **Decoy queries** — client-side noise, defense in depth.

## Honest scope (so it survives expert review)

- This does **not** "solve mass surveillance" for general browsing. General traffic through Potato keeps Tor's
  usual global-adversary limit.
- PIR is expensive: the first prototype is over a **small** curated index, not the full crawl.
- The metadata guarantee is **measured, with assumptions** (cover volume, mixing delay, honest mix fraction),
  not absolute.
- We claim provable query-*content* privacy and *measured* search-path unobservability — nothing more.

## Why it can win recognition

The field rewards novel, **measured, reproducible** results published at venues like PoPETs/PETS; the Caspar
Bowden PET Award ($3,000) goes to outstanding *published* privacy-tech papers [verified: petsymposium.org/award].
The path: build the PIR + mix-transport prototype, measure the guarantees in Shadow (reproducible artifacts),
publish. The project's existing habit of writing down and measuring every leak is itself a credibility
differentiator.

See PLAN.md sections 0.1 and 4.4 for the technical design.
