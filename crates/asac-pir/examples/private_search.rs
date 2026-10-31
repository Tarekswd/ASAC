//! End-to-end private search over a small index using the single-server (PIANO-style) scheme — shared-seed
//! permutation + streaming preprocessing — plus a per-query transfer-cost comparison across the three PIR
//! schemes in this crate. Run: `cargo run -p asac-pir --example private_search`
//!
//! shortcut: a tiny fixed-width, perfect-square index for demonstration; the single-server scheme's privacy is
//! the empirically-tested mechanism, with the formal proof pending (see docs/PIR-RESEARCH.md).

use asac_pir::single_server::{
    logical_query, preprocess_streaming, Params, Permutation, Server, SingleServerDb,
};

fn main() {
    // Public directory: the term set is public; which term you search is private.
    let terms = [
        "weather",
        "recipes",
        "maps",
        "dictionary",
        "news",
        "music",
        "books",
        "travel",
        "health",
        "finance",
        "sports",
        "science",
        "art",
        "history",
        "food",
        "games",
    ]; // 16 = a perfect square
    let n = terms.len();
    let width = 32usize;
    let results: Vec<Vec<u8>> = terms
        .iter()
        .map(|t| {
            let mut v = format!("top results for '{t}'").into_bytes();
            v.resize(width, 0);
            v
        })
        .collect();

    // Server stores the index permuted under a shared seed; the client streams it (one record at a time) into
    // hints, then drops the database.
    let perm = Permutation::from_seed(n, 0xA5AC_5EED);
    let mut permuted = vec![Vec::new(); n];
    for (logical, rec) in results.iter().enumerate() {
        permuted[perm.physical(logical)] = rec.clone();
    }
    let db = SingleServerDb::from_records(&permuted);
    let mut state = preprocess_streaming(n, width, Params::for_sqrt_n(4), 0xBEEF, permuted.iter().cloned());
    let server = Server::new(&db);

    let query = "maps";
    let row = terms.iter().position(|&t| t == query).expect("term in directory");
    let got = logical_query(&mut state, &perm, row, &server);
    println!(
        "search {query:?} -> {}",
        String::from_utf8_lossy(&got).trim_end_matches('\0')
    );
    println!("the server never received the term {query:?} (it saw sqrt(n) random-looking indices).\n");

    let s = (n as f64).sqrt() as usize;
    println!("per-query transfer cost over n={n} records x {width} B:");
    println!(
        "  trivial PIR      : download {} B (the whole index -> O(n))",
        n * width
    );
    println!(
        "  2-server XOR PIR : download {} B (2 x one record) + two {}-bit query vectors",
        2 * width,
        n
    );
    println!(
        "  single-server    : download {width} B (one record) + {s} indices up  <- O(sqrt(n)), one server"
    );
}
