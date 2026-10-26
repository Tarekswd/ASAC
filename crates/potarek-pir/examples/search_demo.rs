//! End-to-end demo of trustless query privacy (PLAN.md 4.4): search a small index so the server never learns
//! the search term. Run: `cargo run -p potarek-pir --example search_demo`
//!
//! shortcut: the two-server demo uses a fixed-seed PRG for the selection vector; production needs a CSPRNG.

use potarek_pir::two_server::{combine, make_queries, TwoServerDb, TwoServerServer};
use potarek_pir::{PirClient, PirDatabase, PirServer};

fn main() {
    // Public index: term -> result bytes. The set of terms is public; which one you search is private.
    let terms = ["weather", "recipes", "maps", "dictionary"];
    let results: Vec<Vec<u8>> = terms
        .iter()
        .map(|t| format!("results for {t}").into_bytes())
        .collect();
    let query = "maps";
    let row = terms.iter().position(|&t| t == query).expect("term in index");

    println!("index has {} terms; searching for \"{query}\" (row {row})\n", terms.len());

    // --- Scheme 1: trivial PIR ---
    let mut db = PirDatabase::new();
    for (t, r) in terms.iter().zip(&results) {
        db.insert(*t, r.clone());
    }
    let server = PirServer::new(db);
    let snapshot = server.snapshot(); // the ONLY server call; it takes no term
    let trivial_download: usize = snapshot.iter().map(|(k, v)| k.len() + v.len()).sum();
    let client = PirClient::from_snapshot(snapshot);
    let hit = client.get(query).map(|b| String::from_utf8_lossy(b).into_owned());
    println!("[trivial PIR]  result: {:?}", hit.expect("hit"));
    println!("[trivial PIR]  server received the term? NO (snapshot() has no query argument)");
    println!("[trivial PIR]  download: {trivial_download} bytes (the WHOLE index -> O(N))\n");

    // --- Scheme 2: two-server XOR PIR ---
    let two = TwoServerDb::from_records(&results);
    let s0 = TwoServerServer::new(&two);
    let s1 = TwoServerServer::new(&two);
    let r0 = seeded_bits(0x00C0_FFEE, two.len()); // demo randomness; CSPRNG in production
    let (q0, q1) = make_queries(two.len(), row, &r0);
    let rec = combine(&s0.answer(&q0), &s1.answer(&q1));
    let rec_str = String::from_utf8_lossy(&rec);
    let two_download = 2 * two.width();
    println!("[2-server PIR] result: {:?}", rec_str.trim_end_matches('\0'));
    println!("[2-server PIR] each server saw only a random selection vector; neither learned row {row}");
    println!("[2-server PIR] download: {two_download} bytes (2 x one record -> O(record), not O(N))\n");

    println!("In both schemes the server(s) never received the term \"{query}\".");
}

fn seeded_bits(seed: u64, n: usize) -> Vec<bool> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33) & 1 == 1
        })
        .collect()
}
