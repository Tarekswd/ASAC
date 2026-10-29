# Potato / ASAC Threat Model

Status: draft v0.1, 2026-10-26. Standalone expansion of [PLAN.md](../PLAN.md) section 2.1, for reviewers and
contributors. If this document and PLAN.md disagree, PLAN.md is authoritative until this one is ratified.

Claims that merely restate Tor's design are cited to PLAN.md; properties not yet checked against a primary
source are marked **(unverified)** and must be confirmed before anyone relies on them.

## 1. What we protect, and for whom

The asset is the **linkability between a person and their network activity**: which sites or onion services
they reach, and which searches they run on ASAC. The user is anyone using a Potato client (via Tor Browser)
and/or the ASAC search engine. "Harm" is any party learning that *this person* did *this activity* when the
user did not intend it.

Potato is a client and protocol layer on the **existing Tor network**; its guarantees are Tor's guarantees,
tuned and composed, not new ones [proof: PLAN.md section 0]. ASAC is an onion-service search engine designed
so its own operators cannot profile users [proof: PLAN.md 4.1].

## 2. Adversaries we defend against

| Adversary | Capability | Guarantee we aim for | Mechanism |
|---|---|---|---|
| Local network observer (ISP, Wi-Fi) | Sees the user's link-layer traffic | Must not learn the destination | Tor encryption + guard; TLS to the guard [proof: PLAN.md 2.1] |
| Single malicious relay | Controls one hop | Must not learn both source and destination | 3-hop circuits; no single hop sees both ends [proof: PLAN.md 2.1] |
| Some malicious relays | Control several relays | Must not tag or modify cells undetected | CGO non-malleable relay crypto [proof: PLAN.md 3.1, 2.1] |
| Destination server | Receives the connection | Must not learn the client's IP | Exit relay originates the connection [proof: PLAN.md 2.1] |
| Recording quantum adversary | Stores traffic now, decrypts later | Forward secrecy against future quantum attack | Hybrid ntor-v3 + ML-KEM handshake [proof: PLAN.md 3.2, 2.1] |
| Search-engine operator (including us) | Runs ASAC | Must not link queries to users | No query logs; onion-only; client IP hidden by Potato [proof: PLAN.md 4.3 #6, 2.1] |

## 3. Out of scope (and why we say so openly)

These are inherent limits of a low-latency anonymity system or belong to other layers. Claiming otherwise would
be dishonest and dangerous.

- **Global passive adversary** watching both ends: can correlate by timing. Tor's low-latency design cannot
  stop this; Fortress mode only *reduces* it [proof: PLAN.md 2.1]. **(unverified: the precise limits of
  end-to-end correlation resistance.)**
- **Compromised endpoint device** or **browser fingerprinting**: that is the OS's and browser's job, not
  Potato's [proof: PLAN.md 2.1].
- **Detecting local surveillance of the user's device** (spyware, shoulder-surfing): a compromised endpoint can
  lie, so a "you are safe" signal is worse than none; this is an OS/physical-security problem [proof: PLAN.md
  3.10].
- **Denial of service** against availability is a resource-safety concern (Equi-X PoW, memquota), not an
  anonymity guarantee [proof: PLAN.md 3.8].

## 4. Assumptions and trust boundaries

- The Tor network is large and diverse enough that the user is not isolated: **privacy comes from the crowd**,
  so Potato runs on Tor rather than a tiny separate network [proof: PLAN.md section 0].
- The user runs a genuine, unmodified client and an up-to-date Tor Browser; the endpoint is not compromised.
- Audited cryptographic libraries (Arti, RustCrypto `ml-kem`) are correct; Potato adds **no new primitives**,
  only policy and composition [proof: PLAN.md 2.3].
- **Trust boundary:** everything from the user's device up to and including the guard is the user's trust base;
  beyond that, no single party is trusted with both identity and activity.
- Potato clients must be **wire-indistinguishable** from ordinary Arti clients, or the anonymity set shrinks to
  "Potato users" [proof: PLAN.md 3.9].

## 5. Modes versus the threat model

Each mode trades latency for privacy, and each leak is written down rather than hidden [proof: PLAN.md 2.2]. The
project default is **privacy over latency** [proof: PLAN.md section 0].

- **Balanced** (default): Tor-equivalent guarantees with the best existing speedups (CGO, Conflux, tuned Vegas).
  Added leak vs. plain Tor: the Conflux circuit-linking signal [proof: PLAN.md 2.2].
- **Swift**: adds bounded, latency-aware middle-relay selection, which leaks a little about client location;
  capped and gated, and cut if the leak is not small [proof: PLAN.md 2.2, 3.3]. Deprioritized under the
  privacy-first priority.
- **Fortress**: adds constant-rate cover traffic and timing jitter to reduce (not eliminate) timing
  correlation, at a bandwidth and latency cost [proof: PLAN.md 2.2, 3.7]. May enable client-side decoy queries
  by default [proof: PLAN.md 4.3 item 7].

## 6. Residual risks and open questions

- End-to-end timing correlation by a global or near-global observer remains out of scope; quantify how much
  Fortress actually reduces it **(unverified)**.
- Swift's location leak must be measured (Shadow + an attacker model) and bounded before it ships [proof:
  PLAN.md 3.3 gate].
- Decoy queries are a weak defense: they do nothing against a log-free ASAC and are filterable if the decoy
  distribution is unrealistic [proof: PLAN.md 4.3 item 7].
- The hybrid PQ handshake needs a formal model, fuzzing, and external cryptographer review before it is trusted
  [proof: PLAN.md 2 phase 2 gate].
