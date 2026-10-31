//! Sharded single-server PIR: a large record set split into K independent PIANO instances (FLAGSHIP-DESIGN.md
//! F3, scale).
//!
//! Each shard is its own [`SingleServerDb`] + hints, so preprocessing memory stays bounded per shard and the
//! total query budget is `K * sqrt(n/K) = sqrt(nK)` — larger than a single DB's `sqrt(n)`, which is the
//! budget limit that otherwise caps how many lookups a prototype serves. Shards preprocess independently (and
//! could be parallelized).
//!
//! shortcut / privacy note: [`ShardedDb::get`] queries only the shard holding the record, which reveals *which
//! shard* was accessed — a coarse `log2(K)`-bit leak about the record index. That is fine when the shard is
//! not sensitive (e.g. sharding a public corpus for scale), but for full query privacy the caller must query
//! *every* shard each time (cover) and keep only the real answer, trading K-fold cost for zero shard leak.
//! This module provides the scale primitive; the cover-all-shards policy is the caller's to add.

use crate::single_server::{
    logical_query, preprocess_streaming, ClientState, Params, Permutation, Server, SingleServerDb,
};

/// Smallest perfect square >= n, and at least 4 (PIANO needs sqrt(n) >= 2).
fn perfect_square_at_least(n: usize) -> usize {
    let s = ((n as f64).sqrt().ceil() as usize).max(2);
    s * s
}

/// A record set partitioned into independent PIANO shards. Logical record `i` lives in shard `i / shard_size`
/// at local index `i % shard_size`.
pub struct ShardedDb {
    shards: Vec<SingleServerDb>,
    hints: Vec<ClientState>,
    perms: Vec<Permutation>,
    shard_size: usize,
    total: usize,
}

impl ShardedDb {
    /// Build `shards` PIANO instances over `records` (each fixed to `width` bytes). `shards` is clamped to at
    /// least 1; empty trailing shards are omitted when there are fewer records than `shards * shard_size`.
    pub fn build(records: &[Vec<u8>], width: usize, shards: usize, seed: u64) -> Self {
        let shards = shards.max(1);
        let total = records.len();
        let shard_size = total.div_ceil(shards).max(1);

        let mut dbs = Vec::new();
        let mut hints = Vec::new();
        let mut perms = Vec::new();
        let mut start = 0;
        let mut s = 0u64;
        while start < total {
            let end = (start + shard_size).min(total);
            let slice = &records[start..end];
            let n = perfect_square_at_least(slice.len());
            let mut recs: Vec<Vec<u8>> = slice
                .iter()
                .map(|r| {
                    let mut v = r.clone();
                    v.truncate(width);
                    v.resize(width, 0);
                    v
                })
                .collect();
            while recs.len() < n {
                recs.push(vec![0u8; width]);
            }
            let shard_seed = seed.wrapping_add(s.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let perm = Permutation::from_seed(n, shard_seed);
            let mut permuted = vec![Vec::new(); n];
            for (logical, rec) in recs.iter().enumerate() {
                permuted[perm.physical(logical)] = rec.clone();
            }
            let db = SingleServerDb::from_records(&permuted);
            let sq = (n as f64).sqrt() as usize;
            let hint = preprocess_streaming(
                n,
                width,
                Params::for_sqrt_n(sq),
                shard_seed.wrapping_add(1),
                permuted.iter().cloned(),
            );
            dbs.push(db);
            hints.push(hint);
            perms.push(perm);
            start = end;
            s += 1;
        }

        Self {
            shards: dbs,
            hints,
            perms,
            shard_size,
            total,
        }
    }

    /// Number of logical records.
    pub fn len(&self) -> usize {
        self.total
    }

    /// Whether the DB holds no records.
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Privately fetch logical record `i` from its shard (one PIANO query). Returns `None` if out of range.
    /// Reveals which shard was queried — see the module privacy note.
    pub fn get(&mut self, i: usize) -> Option<Vec<u8>> {
        if i >= self.total {
            return None;
        }
        let shard = i / self.shard_size;
        let local = i % self.shard_size;
        let server = Server::new(&self.shards[shard]);
        Some(logical_query(
            &mut self.hints[shard],
            &self.perms[shard],
            local,
            &server,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged_records(n: usize, width: usize) -> Vec<Vec<u8>> {
        (0..n)
            .map(|i| {
                let mut v = vec![0u8; width];
                v[..8].copy_from_slice(&(i as u64).to_le_bytes());
                v
            })
            .collect()
    }

    #[test]
    fn retrieves_records_across_shards() {
        let records = tagged_records(40, 16);
        let mut db = ShardedDb::build(&records, 16, 4, 1234);
        assert_eq!(db.len(), 40);
        for &i in &[0usize, 7, 15, 23, 39] {
            let got = db.get(i).expect("in range");
            assert_eq!(
                &got[..8],
                &(i as u64).to_le_bytes(),
                "record {i} must round-trip from its shard"
            );
        }
        assert!(db.get(40).is_none(), "out of range");
    }

    #[test]
    fn single_shard_degenerates_to_plain_pir() {
        let records = tagged_records(9, 16);
        let mut db = ShardedDb::build(&records, 16, 1, 7);
        assert_eq!(db.len(), 9);
        assert_eq!(&db.get(5).unwrap()[..8], &5u64.to_le_bytes());
    }
}
