//! Full-text BM25 index for the ASAC search engine (PLAN.md 4.3 #3), built on Tantivy.
//!
//! Stores only title, URL, extracted body text, and outgoing links (PLAN.md 4.3 #3). **Every insert passes
//! through `asac-safety` first** ([`AsacIndex::add`]): a document the content filter blocks is never indexed.
//! Use [`AsacIndex::new`] for an in-memory index (tests/one-shot) or [`AsacIndex::open_or_create`] for a
//! persistent one on disk that survives restarts. Inserts upsert by URL, so re-crawling refreshes a page
//! rather than duplicating it.

use std::path::Path;

pub use asac_safety::{ContentFilter, HashMatchBackend, Verdict};
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::doc;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT};
use tantivy::{DocAddress, Document, Index, IndexWriter, Score, TantivyDocument, Term};

/// One search hit: the BM25 relevance score and the stored document as JSON.
pub struct Hit {
    pub score: f32,
    pub json: String,
}

/// A search hit with the stored fields parsed out of the JSON, ready for the front end to render.
pub struct DocHit {
    pub score: f32,
    pub title: String,
    pub url: String,
    pub body: String,
    pub links: String,
}

/// Pull the first string value of a stored field out of Tantivy's `to_json` output, which represents each
/// field as an array of values (tolerant of a bare string too).
fn first_str(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::Array(a)) => a.first().and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// A BM25 full-text index. Inserts are gated by a content-safety filter; search ranks by BM25 over title+body.
pub struct AsacIndex {
    index: Index,
    schema: Schema,
    writer: IndexWriter,
    title: Field,
    url: Field,
    body: Field,
    links: Field,
}

impl AsacIndex {
    /// The ASAC schema and its fields (shared by the in-RAM and on-disk constructors).
    fn schema_fields() -> (Schema, Field, Field, Field, Field) {
        let mut b = Schema::builder();
        let title = b.add_text_field("title", TEXT | STORED);
        let url = b.add_text_field("url", STRING | STORED); // URLs stored verbatim, not tokenized (one term)
        let body = b.add_text_field("body", TEXT | STORED);
        let links = b.add_text_field("links", STORED);
        (b.build(), title, url, body, links)
    }

    fn from_index(index: Index, schema: Schema, fields: (Field, Field, Field, Field)) -> tantivy::Result<Self> {
        let writer = index.writer(50_000_000)?;
        let (title, url, body, links) = fields;
        Ok(Self { index, schema, writer, title, url, body, links })
    }

    /// Create an in-memory index with the ASAC schema.
    pub fn new() -> tantivy::Result<Self> {
        let (schema, title, url, body, links) = Self::schema_fields();
        let index = Index::create_in_ram(schema.clone());
        Self::from_index(index, schema, (title, url, body, links))
    }

    /// Open a persistent on-disk index at `dir`, creating it (and the directory) if absent. Reopening the same
    /// directory keeps everything indexed on the previous run.
    pub fn open_or_create(dir: &Path) -> tantivy::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let (schema, title, url, body, links) = Self::schema_fields();
        let index = Index::open_or_create(MmapDirectory::open(dir)?, schema.clone())?;
        Self::from_index(index, schema, (title, url, body, links))
    }

    /// Run the safety filter, then index the document only if it is allowed. Returns the filter's [`Verdict`];
    /// a `Block` means nothing was indexed. Call [`AsacIndex::commit`] to make inserts searchable.
    pub fn add<B: HashMatchBackend>(
        &mut self,
        filter: &ContentFilter<B>,
        title: &str,
        url: &str,
        body: &str,
        links: &str,
    ) -> tantivy::Result<Verdict> {
        // Gate on the body bytes plus the human-visible text (title + body).
        let verdict = filter.check(body.as_bytes(), &format!("{title} {body}"));
        if matches!(verdict, Verdict::Block(_)) {
            return Ok(verdict);
        }
        // Upsert: drop any previously-committed doc with this URL so a re-crawl refreshes rather than dupes.
        self.writer.delete_term(Term::from_field_text(self.url, url));
        self.writer.add_document(doc!(
            self.title => title,
            self.url => url,
            self.body => body,
            self.links => links,
        ))?;
        Ok(verdict)
    }

    /// Flush pending inserts so they become searchable.
    pub fn commit(&mut self) -> tantivy::Result<()> {
        self.writer.commit()?;
        Ok(())
    }

    /// A cheap, cloneable, `Send + Sync` read handle for serving searches concurrently (e.g. one per request).
    /// It reflects everything committed so far and keeps reflecting later commits.
    pub fn reader(&self) -> AsacReader {
        AsacReader {
            index: self.index.clone(),
            schema: self.schema.clone(),
            title: self.title,
            body: self.body,
        }
    }

    /// BM25 search over title + body; returns up to `limit` hits, most relevant first.
    pub fn search(&self, query: &str, limit: usize) -> tantivy::Result<Vec<Hit>> {
        self.reader().search(query, limit)
    }

    /// Like [`AsacIndex::search`] but returns each hit's stored fields already parsed out.
    pub fn search_docs(&self, query: &str, limit: usize) -> tantivy::Result<Vec<DocHit>> {
        self.reader().search_docs(query, limit)
    }
}

/// A read-only, cloneable handle onto an index's committed data. Safe to share across threads/tasks.
#[derive(Clone)]
pub struct AsacReader {
    index: Index,
    schema: Schema,
    title: Field,
    body: Field,
}

impl AsacReader {
    /// BM25 search over title + body; returns up to `limit` hits, most relevant first.
    pub fn search(&self, query: &str, limit: usize) -> tantivy::Result<Vec<Hit>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.title, self.body]);
        let parsed = parser.parse_query(query)?;
        let top: Vec<(Score, DocAddress)> =
            searcher.search(&parsed, &TopDocs::with_limit(limit).order_by_score())?;
        let mut hits = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: TantivyDocument = searcher.doc(addr)?;
            hits.push(Hit { score, json: doc.to_json(&self.schema) });
        }
        Ok(hits)
    }

    /// Like [`AsacReader::search`] but returns each hit's stored fields already parsed out (title/url/body/
    /// links), so callers don't re-parse the JSON.
    pub fn search_docs(&self, query: &str, limit: usize) -> tantivy::Result<Vec<DocHit>> {
        Ok(self
            .search(query, limit)?
            .into_iter()
            .map(|h| {
                let v: serde_json::Value = serde_json::from_str(&h.json).unwrap_or(serde_json::Value::Null);
                DocHit {
                    score: h.score,
                    title: first_str(&v, "title"),
                    url: first_str(&v, "url"),
                    body: first_str(&v, "body"),
                    links: first_str(&v, "links"),
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoKnownIllegal;
    impl HashMatchBackend for NoKnownIllegal {
        fn is_known_illegal(&self, _h: &[u8; 32]) -> bool {
            false
        }
    }

    #[test]
    fn bm25_ranks_relevant_doc_first() {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        idx.add(&f, "The Old Man and the Sea", "http://x/sea",
                "He fished alone in the Gulf Stream chasing a great whale", "").unwrap();
        idx.add(&f, "Rust Programming", "http://x/rust",
                "a systems programming language focused on memory safety", "").unwrap();
        idx.commit().unwrap();

        let hits = idx.search("whale sea", 5).unwrap();
        assert!(!hits.is_empty(), "expected a hit");
        assert!(hits[0].json.contains("Old Man"), "the sea document should rank first");
    }

    #[test]
    fn search_docs_parses_stored_fields() {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        idx.add(&f, "Rust Lang", "http://x/rust", "memory safety ownership", "http://x/a").unwrap();
        idx.commit().unwrap();

        let hits = idx.search_docs("ownership", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Rust Lang");
        assert_eq!(hits[0].url, "http://x/rust");
        assert!(hits[0].body.contains("ownership"));
        assert_eq!(hits[0].links, "http://x/a");
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("asac-index-test-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn on_disk_index_survives_reopen() {
        let dir = temp_dir("persist");
        {
            let mut idx = AsacIndex::open_or_create(&dir).unwrap();
            let f = ContentFilter::new(NoKnownIllegal);
            idx.add(&f, "Persisted", "http://x/p", "durable body text", "").unwrap();
            idx.commit().unwrap();
        } // writer/index dropped, mimicking a restart
        let idx = AsacIndex::open_or_create(&dir).unwrap();
        let hits = idx.search_docs("durable", 5).unwrap();
        assert_eq!(hits.len(), 1, "doc indexed in the previous session must still be there");
        assert_eq!(hits[0].title, "Persisted");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn re_adding_same_url_upserts_not_duplicates() {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal);
        idx.add(&f, "Old Title", "http://x/same", "alpha keyword", "").unwrap();
        idx.commit().unwrap();
        idx.add(&f, "New Title", "http://x/same", "alpha keyword refreshed", "").unwrap();
        idx.commit().unwrap();

        let hits = idx.search_docs("alpha", 10).unwrap();
        assert_eq!(hits.len(), 1, "same URL must not produce a duplicate");
        assert_eq!(hits[0].title, "New Title", "latest crawl wins");
    }

    #[test]
    fn safety_filter_blocks_insert() {
        let mut idx = AsacIndex::new().unwrap();
        let f = ContentFilter::new(NoKnownIllegal).block_term("forbidden");
        let verdict = idx.add(&f, "bad", "http://x/bad", "this page has forbidden content", "").unwrap();
        assert!(matches!(verdict, Verdict::Block(_)), "filter must block the insert");
        idx.commit().unwrap();
        assert!(idx.search("forbidden", 5).unwrap().is_empty(), "blocked doc must not be indexed");
    }
}
