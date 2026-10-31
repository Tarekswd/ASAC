# Outreach drafts

Short, honest cold-email templates for the people and groups in [EXTERNAL-PLAN.md](EXTERNAL-PLAN.md)
("Who to contact"). They are deliberately plain: say what exists, what is unproven, and the one thing
you want back. Adapt the bracketed bits. Send from a real address, link the public repo (once it is
published — item 9), and attach or link `docs/PIR-SECURITY.md` and `docs/FLAGSHIP-DESIGN.md` where noted.

One rule for all of them: **do not claim the privacy property is proven.** The honest line is "designed so
that a single honest-but-curious server learns nothing about the query, and we want that checked," not
"mathematically unable to profile you." The second is the goal; the first is where the project actually is.

---

## Reusable one-paragraph description

> ASAC is an open-source onion-service search engine built on Arti (the Rust Tor implementation). The query
> path uses private information retrieval (PIR): a keyword PIR step over a bucketed term index, composed with
> a document PIR step, so the design goal is that a single honest-but-curious server answers a search without
> learning which term or document was requested. It is composed entirely from audited primitives (BLAKE3,
> ChaCha20, the PIANO single-server PIR construction, Sphinx via Nym for transport) — no new cryptography.
> The engine works end to end (crawl over Tor, safety filtering, BM25 + PageRank ranking, onion service) and
> the PIR layer is implemented and unit-tested, but the formal privacy argument has not been externally
> reviewed, which is exactly what I am looking for.

---

## 1. Cryptographer / PETS review (EXTERNAL-PLAN item 4; obligations O1, O3, O4, O5)

**To:** an applied-crypto researcher, or the Tor research list, or a PoPETs-adjacent contact.
**Attach:** `docs/PIR-SECURITY.md` (the draft argument + open obligations).

> Subject: PIR-based private search on Arti — would value a second pair of eyes on the security argument
>
> Hi [name],
>
> I've built an open-source onion-service search engine whose query path is private information retrieval:
> a keyword-PIR step composed with a document-PIR step over PIANO, the goal being that a single
> honest-but-curious server learns nothing about the query. [repo link]
>
> The construction is composition of standard primitives only — no new crypto. I've written up the security
> argument and, honestly, the open obligations I can't discharge myself: [list the O1/O3/O4/O5 one-liners].
> Before I'd ever put a privacy claim in front of users I want these checked by someone who does this for a
> living.
>
> Would you be willing to look at the ~[N]-page argument (attached), or point me to the right person or venue?
> I'm aiming at a PoPETs submission once it holds up. No rush and no expectation — I know this is an ask.
>
> Thanks either way,
> [name]

---

## 2. Mixnet transport — Nym (item 3)

**To:** github.com/nymtech (an issue/discussion on `nym-sdk`), or the Nym community channel.

> Subject: Using nym-sdk as the Sphinx transport for a private search client — sanity check
>
> Hi,
>
> I'm building an open-source private search engine on Arti and want the client's cover-traffic stream to ride
> a real deployed mixnet rather than anything I'd roll myself. I've got a `MixTransport` seam with Poisson
> cover traffic and fixed-size framing already; I want to implement `send()` by submitting each framed packet
> as a Nym Sphinx packet.
>
> Two questions before I wire it up: (1) is `nym-sdk` the right entry point for "send these fixed-size
> payloads through the mixnet" from a Rust client, and (2) is there anything about packet sizing or rate that
> I should match to Nym's own cover-traffic model so I'm not fighting it? Repo + design doc: [links].
>
> Thanks,
> [name]

---

## 3. Tor / Arti upstream (items 6, 7 — later phases)

**To:** `tor-dev@lists.torproject.org`, or the Tor Forum / GitLab. Follow the list's norms; lead with the
specific technical question, not the pitch.

> Subject: [arti] Question on launch_onion_service + HS PoW for a search service
>
> Hi all,
>
> I'm running an onion service built on arti-client [version] and have a couple of questions I couldn't settle
> from the docs: [the specific, concrete questions — e.g. recommended config for enabling HS PoW under load,
> or the right pattern for X]. Minimal context: it's an open-source private search engine; repo at [link].
>
> Happy to file this on GitLab instead if that's the better venue — just let me know.
>
> Thanks,
> [name]

---

## 4. Content-safety hash-matching partner (item 5 — launch blocker)

**To:** NCMEC / Thorn / IWF / the Tech Coalition, via their official partner or contact forms (not a
personal address). This is a gate, not a nicety: a public search index needs known-CSAM hash matching
before launch.

> Subject: Hash-matching partnership enquiry — open-source search engine, pre-launch
>
> Hello,
>
> I'm the developer of an open-source search engine preparing for a public launch, and I want known-CSAM
> hash matching in place before any index goes live — not after. The engine already has a pluggable safety
> filter stage; I need access to a maintained hash set (e.g. PhotoDNA / your hash-sharing programme) and to
> understand the eligibility and compliance requirements for a project of this size.
>
> Could you point me to the right process for a small independent project to apply, and the obligations that
> come with access? I'd rather learn the requirements early and build to them.
>
> Thank you,
> [name] — [contact], [repo link]

---

## 5. Security audit firm (item 8 — later phase)

**To:** Radically Open Security or Cure53, via their public contact page.

> Subject: Scoping enquiry — independent security audit of an Arti-based private search engine
>
> Hi,
>
> I'm building an open-source onion-service search engine on Arti whose novelty is a PIR-based private query
> path (composition of audited primitives, no new crypto). Ahead of a public release I'd like to scope an
> independent audit covering the Tor/onion integration, the PIR implementation, and the client cover-traffic
> transport.
>
> Could you share roughly what a scoping conversation and engagement would look like for a project this size,
> and your availability window? Repo and design docs: [links]. I understand this may be premature and am happy
> to come back when it's review-ready.
>
> Thanks,
> [name]

---

## 6. Funding (optional — to pay for audits / partners / time)

**To:** NLnet / NGI Zero (open call) or OTF's Internet Freedom Fund. Match their application form; this is
just the gist you'd adapt into it. (Note the EXTERNAL-PLAN caveat on OTF's 2025 funding disruption.)

> Project: ASAC — an open-source, privacy-preserving search engine on Tor/Arti.
>
> What it is: an onion-service search engine whose query path uses private information retrieval so that,
> by design, the server answering a search does not learn what was searched. Built entirely from audited
> primitives; fully open source.
>
> What the grant would fund: [pick — an external cryptographic review of the PIR security argument / an
> independent security audit / development time to complete the mixnet transport and network-level anonymity
> measurement]. Each has a concrete deliverable: [e.g. a published PoPETs-track write-up / a public audit
> report / measured anonymity-set numbers from a Shadow simulation].
>
> Why it fits [NGI Zero / Internet Freedom Fund]: it's open, privacy-by-design internet infrastructure in the
> Tor ecosystem. Repo and design docs: [links].

---

## 7. Legal review (item 5 — launch blocker)

A public search index raises intermediary-liability, takedown, and jurisdiction questions that are not
engineering questions. EFF is a starting point for *pointers* to counsel, not a legal-services provider —
ask for direction, not representation.

> Subject: Pointer request — launching an open-source search engine, intermediary-liability questions
>
> Hi,
>
> I'm preparing to launch an open-source search engine and want to get the legal questions right before it
> goes public: intermediary liability for indexed content, takedown handling, and jurisdiction. I'm not
> asking you to act as counsel — just hoping you can point me toward the right resources or practitioners for
> a small independent project. Repo: [link].
>
> Thank you,
> [name]
