//! Full-text BM25 index for the ASAC search engine (PLAN.md 4.3 #3), built on Tantivy.
//!
//! Stores only title, URL, extracted body text, and outgoing links (PLAN.md 4.3 #3). **Every insert passes
//! through `asac-safety` first** ([`AsacIndex::add`]): a document the content filter blocks is never indexed.
//! This is the in-memory prototype; a real deployment would use a persistent directory and feed the stored
//! records to `asac-pir` for private retrieval.

pub use asac_safety::{ContentFilter, HashMatchBackend, Verdict};
use tantivy::collector::TopDocs;
use tantivy::doc;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT};
use tantivy::{DocAddress, Document, Index, IndexWriter, Score, TantivyDocument};

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
    /// Create an in-memory index with the ASAC schema.
    pub fn new() -> tantivy::Result<Self> {
        let mut b = Schema::builder();
        let title = b.add_text_field("title", TEXT | STORED);
        let url = b.add_text_field("url", STRING | STORED); // URLs stored verbatim, not tokenized
        let body = b.add_text_field("body", TEXT | STORED);
        let links = b.add_text_field("links", STORED);
        let schema = b.build();
        let index = Index::create_in_ram(schema.clone());
        let writer = index.writer(50_000_000)?;
        Ok(Self { index, schema, writer, title, url, body, links })
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

    /// Like [`AsacIndex::search`] but returns each hit's stored fields already parsed out (title/url/body/
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
