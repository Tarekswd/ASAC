//! Private information retrieval (PIR) for Potarek's trustless query privacy (PLAN.md 4.4).
//!
//! The goal: the server answers a lookup **without learning which record was looked up**. This first scheme is
//! *trivial PIR*: the server hands over the entire (small) database and the client reads the record locally.
//! It is information-theoretically private — the server receives no query at all, so it cannot learn the
//! client's interest — at the cost of O(N) bandwidth.
//!
//! shortcut: trivial O(N)-bandwidth PIR; correct and fully private but only viable over a small top-N index.
//! Upgrade to a sublinear single-server scheme (e.g. a PIANO-style construction) when the index grows — the
//! `PirServer`/`PirClient` split here is the seam to swap behind.

/// A small key -> value database to be served under PIR. Keys are searchable terms/ids; values are the
/// serialized result records.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PirDatabase {
    entries: Vec<(String, Vec<u8>)>,
}

impl PirDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or overwrite a record. Later inserts for the same key win (last-writer), so the database holds at
    /// most one value per key.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Vec<u8>>) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The server side of trivial PIR.
///
/// Its only operation, [`PirServer::snapshot`], takes **no query argument** — that type-level fact *is* the
/// privacy guarantee: the server cannot condition on, log, or leak which record a client wants, because it is
/// never told.
pub struct PirServer {
    db: PirDatabase,
}

impl PirServer {
    pub fn new(db: PirDatabase) -> Self {
        Self { db }
    }

    /// Serve the entire database. No index/query parameter by design.
    pub fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
        self.db.entries.clone()
    }
}

/// The client side: fetch the snapshot once, then answer lookups locally. The server never sees these lookups.
pub struct PirClient {
    snapshot: Vec<(String, Vec<u8>)>,
}

impl PirClient {
    pub fn from_snapshot(snapshot: Vec<(String, Vec<u8>)>) -> Self {
        Self { snapshot }
    }

    /// Look up a key locally. This call does not touch the network or the server.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.snapshot
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db() -> PirDatabase {
        let mut db = PirDatabase::new();
        db.insert("weather", b"weather results".to_vec());
        db.insert("recipes", b"recipe results".to_vec());
        db.insert("maps", b"map results".to_vec());
        db
    }

    #[test]
    fn client_retrieves_correct_record() {
        let server = PirServer::new(sample_db());
        let client = PirClient::from_snapshot(server.snapshot());
        assert_eq!(client.get("recipes"), Some(&b"recipe results"[..]));
        assert_eq!(client.get("weather"), Some(&b"weather results"[..]));
        assert_eq!(client.get("absent"), None);
    }

    #[test]
    fn insert_is_last_writer_and_unique() {
        let mut db = PirDatabase::new();
        db.insert("k", b"v1".to_vec());
        db.insert("k", b"v2".to_vec());
        assert_eq!(db.len(), 1);
        let client = PirClient::from_snapshot(PirServer::new(db).snapshot());
        assert_eq!(client.get("k"), Some(&b"v2"[..]));
    }

    /// The privacy guarantee, structurally: the server's response does not depend on what the client wants.
    /// Two clients interested in different keys receive byte-identical snapshots, and the server is never told
    /// which key either one reads. (There is no server API that accepts a query.)
    #[test]
    fn server_response_is_independent_of_query() {
        let server = PirServer::new(sample_db());
        let snap_for_alice = server.snapshot();
        let snap_for_bob = server.snapshot();
        assert_eq!(snap_for_alice, snap_for_bob);

        let alice = PirClient::from_snapshot(snap_for_alice);
        let bob = PirClient::from_snapshot(snap_for_bob);
        // Alice and Bob read different records, but nothing about those reads reached the server.
        assert_eq!(alice.get("weather"), Some(&b"weather results"[..]));
        assert_eq!(bob.get("maps"), Some(&b"map results"[..]));
    }
}
