# Safety-filter architecture

*Last updated: 2026-11-01. Source of truth is the code; this doc tracks `crates/asac-safety/src/lib.rs`
and its call sites. Where the two disagree, the code wins.*

ASAC indexes untrusted pages crawled over Tor, so every document must clear a content-safety filter
**before** it is indexed. This document describes what that filter is, where it sits, what it guarantees,
and — just as importantly — what it does **not** cover today.

## 1. Where it sits in the pipeline

```
crawl (HTML only, over Tor)
        │  raw page bytes + extracted title/body/links
        ▼
AsacIndex::add_with_bytes  ──►  ContentFilter::check(url, doc_bytes, "{title} {body}")
        │                              │
        │                     Verdict::Allow │ Verdict::Block(reason)
        ▼                              │
   indexed (BM25)   ◄── Allow only ────┘   Block ⇒ never indexed, reason returned
```

The filter is **not** an optional post-processing step: the index insert *is* the filter call. Indexing
goes through `AsacIndex::add` / `add_with_bytes`, which call `filter.check(...)` first and refuse the insert
on `Verdict::Block` before writing anything to the index. There is no code path that indexes a document
without a `ContentFilter`.

- Call site: `crates/asac-index/src/lib.rs:181` — `let verdict = filter.check(url, doc_bytes, &format!("{title} {body}")); if matches!(verdict, Verdict::Block(_)) { return Ok(verdict); }`.
- The crawl/serve binaries use `add_with_bytes`, which hashes the page **as fetched**: `crates/asac-server/src/main.rs:81`, `crates/asac-demo/src/main.rs:49`.

## 2. The filter: layers and order

`ContentFilter::check` (`crates/asac-safety/src/lib.rs:85`) runs four checks and returns `Verdict::Allow`
only if **every** layer clears the document. Order (first block wins):

| # | Layer | What it matches | Backing store |
|---|-------|-----------------|---------------|
| 1 | **Host blocklist** | the URL's bare host (case-insensitive; any path/port on that host) | in-memory `HashSet<String>` |
| 2 | **Exact-hash blocklist** | BLAKE3 hash of the document bytes | in-memory `HashSet<[u8;32]>` |
| 3 | **Partner hash match** | BLAKE3 hash against an industry known-illegal list (e.g. CSAM) | `HashMatchBackend` trait — external |
| 4 | **Keyword blocklist** | case-insensitive substring of `"{title} {body}"` | `Vec<String>` |

- Host extraction is done without a URL-parser dependency (`host_of`, `crates/asac-safety/src/lib.rs:109`): it
  strips scheme, userinfo, and port, so `http://bad.onion:8080/x?q=1` and `http://bad.onion/y` both match a
  `bad.onion` block.
- The hash layers (2 and 3) both key on `blake3::hash(doc_bytes)` — the same 32-byte hash — so a local
  blocklist entry and a partner-list entry reference identical bytes.

## 3. Fail-closed by construction

Two design choices make "unsafe unless proven safe" the default, enforced by the type system rather than by
discipline:

1. **A backend is mandatory to construct a filter.** `ContentFilter::new(backend)` takes a
   `B: HashMatchBackend` (`crates/asac-safety/src/lib.rs:54`). You cannot build a filter — and therefore
   cannot index anything — without wiring a partner backend in. There is no "no filter" mode.
2. **Block is the terminal state.** `check` returns `Allow` only after falling through all four layers
   (`crates/asac-safety/src/lib.rs:104`); any match short-circuits to `Block(reason)` and the index insert
   returns without writing (`crates/asac-index/src/lib.rs:182`).

The abuse-report / takedown workflow plugs into **layer 1**: an operator who receives a report adds the
reported host via `ContentFilter::block_host`, and every page under that host stops being indexed
(`crates/asac-safety/src/lib.rs:65`; test `host_blocklist_takes_down_every_page_on_the_host`,
`crates/asac-safety/src/lib.rs:171`).

## 4. What the hash layers actually hash

The exact-hash and partner layers hash the document **as fetched** (the raw page body), not the
lossily-extracted index text. `add_with_bytes` passes the raw bytes through to `check`, so a takedown or
partner-list entry references the bytes that were actually served. This is verified by a test that drives the
same indexed text with different fetched bytes and asserts the backend sees different hashes
(`crates/asac-index/src/lib.rs:439`, `safety_hash_is_over_fetched_bytes_not_indexed_text`).

## 5. Honest limits (state these to a content-safety partner)

- **The shipped binaries wire a no-op partner backend.** `asac-server` and `asac-demo` construct the filter
  with `NoKnownIllegal`, whose `is_known_illegal` always returns `false`
  (`crates/asac-server/src/main.rs:40`). Layer 3 is therefore **inert in the current build** — it is a typed
  seam, not a live illegal-content check. Production requires implementing `HashMatchBackend` against a
  vetted partner's API under a legal agreement; it is deliberately not bundled.
- **Media is out of scope.** The crawler is **HTML-only and never downloads image/video bytes**
  (`CrawlConfig::fetch_media` exists but is off and inert — `crates/asac-crawler/src/lib.rs:140`). No media
  bytes ever enter the pipeline, so industry **image / perceptual** CSAM hash sets (e.g. PhotoDNA) do **not**
  apply to the current crawl surface. They would take effect only if a media-fetching crawler were added
  (with sandboxed decoding) and a partner agreement signed. Today's media defences are the host-blocklist
  takedown, keyword, and legal/partner processes.
- **The local blocklists start empty.** Host, exact-hash, and keyword lists are populated by the operator at
  runtime (`block_host` / `block_hash` / `block_term`); the crate ships no default entries.
- **Not a classifier and not legal review.** This crate is the code seam that a trust-and-safety process,
  legal review, and a hash-matching partner plug into. It does not itself decide what is illegal beyond exact
  matches against lists it is given.

## 6. Tests

All in `crates/asac-safety/src/lib.rs` unless noted:

- `clean_document_is_allowed` (:137)
- `exact_hash_blocklist_blocks` (:143)
- `keyword_blocklist_is_case_insensitive` (:151)
- `partner_backend_blocks_known_illegal` (:161)
- `host_blocklist_takes_down_every_page_on_the_host` (:170)
- `host_of_extracts_bare_host` (:185)
- `safety_hash_is_over_fetched_bytes_not_indexed_text` — `crates/asac-index/src/lib.rs:439`
- index insert is actually gated: `crates/asac-index/src/lib.rs:420` (filter must block the insert)
