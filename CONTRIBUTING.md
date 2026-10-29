# Contributing to ASAC

Thanks for your interest. ASAC is privacy and cryptography software, so the bar for changes is deliberately
high: **correctness and review matter more than merge speed.**

Please also read the [Code of Conduct](CODE_OF_CONDUCT.md). For anything security-sensitive, follow
[SECURITY.md](SECURITY.md) instead of opening a public issue.

## Before you start

- Read **[PLAN.md](PLAN.md)**, especially the three hard truths (section 0) and the threat model (section 2.1).
  Most design questions are already answered there.
- For a non-trivial change, **open an issue first** to discuss it. This avoids wasted work and keeps changes
  aligned with the phased roadmap (PLAN.md section 5).

## Ground rules specific to this project

These come straight from the plan and are not negotiable without a design discussion:

1. **Never roll your own crypto.** Build on Arti and audited libraries. New work should be *policy and
   composition*, not new primitives (PLAN.md section 0.3).
2. **Don't make Potato clients fingerprintable.** A unique wire fingerprint shrinks the anonymity set to
   "Potato users" and hurts everyone (PLAN.md section 3.9). This rule beats any feature.
3. **Write down what leaks.** If a change trades privacy for latency, document the leak explicitly, the way
   the modes table does (PLAN.md section 2.2).
4. **Measure, don't guess.** Performance and privacy claims need Shadow simulation and/or live measurement
   (PLAN.md section 6), not assertion.
5. **Mark unverified claims.** Anything from memory rather than a checked source is marked `(unverified)` in
   the docs, as in PLAN.md. Keep that habit.

## Pull requests

- Keep each PR focused on one change.
- Explain *why*, not just *what*, in the description, and link the issue it addresses.
- Add or update tests and docs for behavior you change. New non-trivial logic needs at least a test or an
  assert-based self-check.
- Be ready for careful review. Pushback on an anonymity tool is a feature.

## Licensing of contributions

By submitting a contribution you agree it is dual-licensed under
[Apache-2.0](LICENSE-APACHE) OR [MIT](LICENSE-MIT), matching the project license, with no additional terms.
