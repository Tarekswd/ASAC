# Outreach drafts

Short, honest cold-email templates for the people and groups in [EXTERNAL-PLAN.md](EXTERNAL-PLAN.md)
("Who to contact"). They are deliberately plain: say what exists, what is unproven, and the one thing
you want back. Adapt the bracketed bits. Send from a real address, link the public repo
(https://github.com/Tarekswd/ASAC), and attach or link `docs/PIR-SECURITY.md` and `docs/FLAGSHIP-DESIGN.md`
where noted.

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
> The engine works end to end and is verified live (crawl over Tor, safety filtering, BM25 + PageRank ranking,
> and a second client fetching results from the running onion service); the PIR layer is implemented and
> unit-tested; the Nym mixnet transport compiles against the SDK; and the metadata-timing harness runs in
> Shadow. A written security reduction is drafted (`docs/PIR-SECURITY.md` §6) but has not been externally
> reviewed — which is exactly what I am looking for.

---

## 1. Cryptographer / PETS review (EXTERNAL-PLAN item 4; O1/O3 drafted, O4/O5 open)

**To:** an applied-crypto researcher, or the Tor research list, or a PoPETs-adjacent contact.
**Attach:** `docs/PIR-SECURITY.md` (the argument, incl. the §6 reductions) and `docs/FLAGSHIP-DESIGN.md`
(design, threat model, measured PIR costs, the Shadow measurement plan).

> Subject: PIR-based private search on Arti — a second pair of eyes on the query-privacy argument
>
> Hi [name],
>
> I've built an open-source onion-service search engine (ASAC) whose query path is private information
> retrieval: a keyword-PIR step composed with a document-PIR step over PIANO single-server PIR, so the design
> goal is that a single honest-but-curious server answers a search without learning which term or documents
> were requested. It's composition of standard primitives only — no new crypto.
> Repo: https://github.com/Tarekswd/ASAC
>
> I've written the argument as a game-based reduction (`docs/PIR-SECURITY.md` §6): keyword-privacy reduces to
> PIANO's index-hiding (§6.3), with the public term→bucket hash hoisted out explicitly, plus a
> vocabulary-leakage bound (§6.4). Two obligations I can't discharge myself remain, and that's really what I'm
> asking about:
>   - O4 — our *implementation* of PIANO (not just the paper's scheme) carries its own proof obligations;
>   - O5 — wall-clock timing, which the logical argument doesn't cover. We measure timing-independence in a
>     Shadow simulation (a first run puts a timing adversary at chance), but I'd value your view on whether
>     that's sufficient.
> I'd also be grateful if you'd sanity-check the §6.3 reduction and the §6.4 bound themselves.
>
> Before I put any privacy claim in front of users I want this reviewed by someone who does it for a living.
> Would you be willing to look at the argument (~[N] pages), or point me to the right person or venue? I'm
> aiming at a PoPETs submission once it holds up. No rush and no expectation — I know this is an ask.
>
> Thanks either way,
> [name] — [your email]

---

## 2. Mixnet transport — Nym (item 3)

**To:** github.com/nymtech (an issue/discussion on `nym-sdk`), or the Nym community channel.

> Subject: nym-sdk as the Sphinx transport for a private search client — build issue + a sizing question
>
> Hi,
>
> I'm building an open-source private search engine on Arti (ASAC) and route the client's Poisson cover-traffic
> stream over Nym rather than rolling my own mixes. I've implemented a `MixTransport` backend on `nym-sdk`
> (`send_message` of fixed-size framed packets) and it compiles against the SDK.
>
> One blocker and one question: (1) building `nym-sdk` from the latest release tag pulls a *yanked*
> `tokio_with_wasm` 0.10.1 and won't resolve on crates.io either — I filed/plan to file a GitHub issue with a
> minimal repro; is there a recommended known-good version/tag to depend on meanwhile? (2) Is there a packet
> size or send-rate I should match to Nym's own cover-traffic model so I'm not fighting it? Repo:
> https://github.com/Tarekswd/ASAC
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
