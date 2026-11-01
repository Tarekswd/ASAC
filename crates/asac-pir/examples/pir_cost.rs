//! Measure single-server PIANO PIR cost vs database size (FLAGSHIP-DESIGN.md §4/§7: "measure, don't assume").
//!
//! Prints preprocessing time, per-query time, the response size, and the number of indices the server sees
//! per query — the last is constant (= sqrt(n)) regardless of which record is fetched, which is the
//! query-shape side channel we care about. Run: `cargo run -p asac-pir --example pir_cost --release`.

use std::time::Instant;

use asac_pir::single_server::{preprocess, query, Params, Server, SingleServerDb};

fn main() {
    let width = 256usize; // bytes per record
    println!("single-server PIANO PIR cost (record width = {width} bytes)");
    println!(
        "{:>9}  {:>7}  {:>13}  {:>13}  {:>11}  {:>11}",
        "n", "sqrt(n)", "preprocess", "per_query", "resp_bytes", "srv_idx/q"
    );

    for &s in &[8usize, 16, 32, 64, 128] {
        let n = s * s;
        let records: Vec<Vec<u8>> = (0..n)
            .map(|i| {
                let mut v = vec![0u8; width];
                v[..8].copy_from_slice(&(i as u64).to_le_bytes());
                v
            })
            .collect();
        let db = SingleServerDb::from_records(&records);

        let t = Instant::now();
        let mut st = preprocess(&db, Params::for_sqrt_n(s), 1);
        let pre = t.elapsed();

        let server = Server::new(&db);
        let t2 = Instant::now();
        let mut resp = 0usize;
        // One query per chunk, to stay within the per-chunk replacement budget (= sqrt(n)).
        for c in 0..s {
            resp = query(&mut st, c * s, &server).expect("within budget").len();
        }
        let per = t2.elapsed() / s as u32;

        println!("{n:>9}  {s:>7}  {pre:>13?}  {per:>13?}  {resp:>11}  {s:>11}");
    }
}
