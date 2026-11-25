# ASAC — research and development milestones

*Drafted for a grant application (closest funder fit: NLnet / NGI Zero Commons Fund — see
[EXTERNAL-PLAN.md](EXTERNAL-PLAN.md) §Funding). The fund pays out **per completed milestone**, over **up to one
year**, for **R&D with an emphasis on development**, where every deliverable ships under a recognised
open-source / open-access licence. Verified at nlnet.nl/commonsfund/guideforapplicants and nlnet.nl/officehour,
2026-11-01. The matching cost breakdown is in [BUDGET.md](BUDGET.md).*

> **Eligibility note:** the existing ASAC prototype (engine end-to-end over Tor, flagship mechanisms built +
> unit-tested, scheduler-level metadata measured) is **unfunded prior work** and is context only. This grant
> covers **only the future milestones listed here** — none of this work has been started, in line with the
> fund's "work funded from the acceptance date" rule.

## Scope

Take ASAC's flagship private-search claim from **"built and measured in-process"** to **"running over a live
mixnet and externally reviewed"** — the credibility path in [ROADMAP.md](ROADMAP.md) (Phase 2 transport →
Phase 1 full-path measurement → Phase 1 review), which the roadmap names as the shortest credible path for the
flagship [ROADMAP.md:109-112]. Each milestone produces a public, open-licensed artifact that is its payment
gate.

## Milestones

| # | Milestone | Key tasks | Open deliverable (the completion gate) | Roadmap |
|---|-----------|-----------|----------------------------------------|---------|
| **M1** | **Live Nym mixnet transport** | Wire the Poisson scheduler's send loop to `NymTransport`; pass the `#[ignore]`d `live_send` against a real gateway; pin a known-good nym release and drop the vendored `tokio_with_wasm` workaround once upstream republishes; measure end-to-end latency/overhead. | Merged transport code + a measured latency/overhead note in FLAGSHIP-DESIGN; packets verified traversing the live mixnet. | Phase 2 [ROADMAP.md:65-72] |
| **M2** | **Full-path metadata measurement at scale** | Adopt [`shadow/tornettools`](https://github.com/shadow/tornettools) to generate a realistic, real-metrics-modeled Tor network at configurable `--network_scale`; swap its generated onion-service client/server config to run ASAC's onion client/server in place of the default `tgen` (verify on a small network first, watching Shadow logs for unimplemented syscalls); route the scheduler through the M1 Nym transport instead of loopback UDP; scale the trial count N well beyond 6 for a tighter accuracy bound; add the wall-clock timing test at scale; re-run the KS timing-independence analysis. | tornettools-based `sim/shadow/` harness + published multi-trial results (adversary accuracy vs. the 1/N chance line, KS at the new scale), reproducible via REPRODUCE.md. Cite *Once is Never Enough* (Jansen, Tracey, Goldberg, USENIX Sec 2021). | Phase 1 [ROADMAP.md:57-59] |
| **M3** | **Complete + externally review the PIR query-privacy argument** | Finish the written reduction (discharge O1/O3, then O4/O5) and the code-side obligations (O2 fixed-K padding, O6 CSPRNG); engage an applied cryptographer to review O1–O5; incorporate findings. | Final PIR-SECURITY.md with all obligations closed, plus a reviewer's written sign-off / incorporated feedback. | Phase 1 [ROADMAP.md:56,60]; [EXTERNAL-PLAN.md:60-67] |
| **M4** | **Open artifact + peer-review submission** | Package a reproducible research artifact (build/test, PIR cost numbers, Shadow results from M2); submit the measured write-up to PoPETs/PETS (artifact-review track). | Public tagged artifact release + submission receipt. Only on passing M3+M4 does any "the server cannot learn your query" claim ship publicly. | Phase 1 gate [ROADMAP.md:61-63]; [EXTERNAL-PLAN.md:61] |

**Optional stretch (fits only if the grant cap allows, else a natural follow-on grant):**

| # | Milestone | Key tasks | Open deliverable | Roadmap |
|---|-----------|-----------|------------------|---------|
| **M5** | **Production content-safety seam** | Harden the `HashMatchBackend` integration path and the `block_host` abuse-report tooling so a partner backend can be dropped in; keep the HTML-only crawler the safe default (media stage stays inert). *(The partner agreement and legal counsel themselves are external and not code — they are not funded here.)* | A documented, test-covered partner-backend integration seam + operator abuse-report runbook. | Phase 3 code-side [ROADMAP.md:74-84] |

**Honesty gate (carried from the roadmap):** no public privacy guarantee is claimed until M3's external review
passes and M2's measured numbers meet the documented targets [ROADMAP.md:9,62-63].
