# Security Policy

Potarek is privacy and anonymity software. A bug here can deanonymize real people, so we treat security
reports seriously and ask you to report them responsibly.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues, pull requests, or discussions.**

Instead, use GitHub's private vulnerability reporting:

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability** to open a private advisory.

> **TODO (project owner):** add a dedicated security contact (for example a monitored email address or a PGP
> key) here, so reporters have an option that does not require a GitHub account.

Please include:

- a description of the issue and the impact you think it has,
- the component affected (routing layer, handshake, search engine, crawler, ...),
- steps to reproduce, or a proof of concept, and
- any suggested fix, if you have one.

## What to expect

- We aim to acknowledge a report within a few days (best effort; this is currently a small project).
- We will work with you on a fix and a coordinated disclosure date.
- We are happy to credit reporters who want credit.

## Scope

This project is in an early **planning stage** — there is little or no running code yet (see
[PLAN.md](PLAN.md)). Reports about the *design* (threat model, protocol choices, the plan itself) are just as
welcome as reports about code, and may be more valuable right now.

## Threat model and known limits

Read [PLAN.md](PLAN.md) section 2.1 for the full threat model. Some limits are inherent to this class of
low-latency anonymity system and are **not** considered vulnerabilities:

- A **global passive adversary** that can watch traffic at both ends can correlate it by timing. Like Tor,
  Potarek's low-latency modes do not defend against this; Fortress mode only reduces the risk.
- A **compromised endpoint device or browser fingerprinting** is out of scope — that is the browser's job.

If you are unsure whether something is in scope, report it privately and let us decide together.
