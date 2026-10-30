//! Ranking v2 signal: link-graph authority via PageRank, plus a BM25xauthority blend (PLAN.md 4.3 #4).
//!
//! The crawler yields each page's URL and outgoing links; [`Authority`] computes a PageRank score per page
//! (how much the crawled web points at it) and exposes it normalized to `0..=1`. The search path blends that
//! with the BM25 relevance score ([`blend`]), so a strongly-linked page outranks an equally-relevant obscure
//! one. Pure and dependency-free, so it is fully unit-tested offline.
//!
//! shortcut: authority is over the *crawled* subgraph only (external links don't count), recomputed per crawl
//! and held in memory. Freshness (the other half of v2 ranking) is not here yet — it needs a per-doc crawl
//! timestamp. Add persistence/incremental recompute when the index outgrows memory.

use std::collections::HashMap;

const DAMPING: f32 = 0.85;
const ITERS: usize = 30;

/// PageRank over a directed link graph. `pages` is each page's `(url, outgoing_links)`; only links to URLs
/// that are themselves pages form edges. Dangling pages (no in-graph links) have their mass redistributed
/// uniformly. Scores sum to ~1.
pub fn pagerank(pages: &[(String, Vec<String>)]) -> HashMap<String, f32> {
    let n = pages.len();
    if n == 0 {
        return HashMap::new();
    }
    let index: HashMap<&str, usize> =
        pages.iter().enumerate().map(|(i, (u, _))| (u.as_str(), i)).collect();
    // Out-edges to known pages only, deduped (self-links dropped would need extra work; kept simple).
    let out: Vec<Vec<usize>> = pages
        .iter()
        .map(|(_, links)| {
            let mut e: Vec<usize> = links.iter().filter_map(|l| index.get(l.as_str()).copied()).collect();
            e.sort_unstable();
            e.dedup();
            e
        })
        .collect();

    let mut rank = vec![1.0f32 / n as f32; n];
    let base = (1.0 - DAMPING) / n as f32;
    for _ in 0..ITERS {
        let mut next = vec![base; n];
        let mut dangling = 0.0f32;
        for i in 0..n {
            if out[i].is_empty() {
                dangling += rank[i];
            } else {
                let share = DAMPING * rank[i] / out[i].len() as f32;
                for &j in &out[i] {
                    next[j] += share;
                }
            }
        }
        let dangle_share = DAMPING * dangling / n as f32;
        for v in next.iter_mut() {
            *v += dangle_share;
        }
        rank = next;
    }
    pages.iter().enumerate().map(|(i, (u, _))| (u.clone(), rank[i])).collect()
}

/// Per-URL authority, normalized so the most-linked page is `1.0`.
pub struct Authority {
    scores: HashMap<String, f32>,
    max: f32,
}

impl Authority {
    /// Compute authority from the crawl's `(url, links)` pairs.
    pub fn from_pages(pages: &[(String, Vec<String>)]) -> Self {
        let scores = pagerank(pages);
        let max = scores.values().copied().fold(0.0f32, f32::max);
        Self { scores, max }
    }

    /// An empty authority table (every page scores 0; blending is then a no-op).
    pub fn empty() -> Self {
        Self { scores: HashMap::new(), max: 0.0 }
    }

    /// Authority of `url` normalized to `0..=1` (0 if unknown or no links anywhere).
    pub fn norm(&self, url: &str) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            self.scores.get(url).copied().unwrap_or(0.0) / self.max
        }
    }
}

/// Blend a BM25 relevance score with normalized authority (`0..=1`). `weight` is how much a maximally-linked
/// page's score is boosted (e.g. 0.5 = up to +50%). Relevance stays the dominant term.
pub fn blend(bm25: f32, authority_norm: f32, weight: f32) -> f32 {
    bm25 * (1.0 + weight * authority_norm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(u: &str, links: &[&str]) -> (String, Vec<String>) {
        (u.to_string(), links.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn most_linked_page_wins() {
        // B and C both link to A; A links to B. A should have the highest authority.
        let pages = [page("A", &["B"]), page("B", &["A"]), page("C", &["A"])];
        let r = pagerank(&pages);
        assert!(r["A"] > r["B"], "A: {}, B: {}", r["A"], r["B"]);
        assert!(r["A"] > r["C"], "A: {}, C: {}", r["A"], r["C"]);
        let sum: f32 = r.values().sum();
        assert!((sum - 1.0).abs() < 1e-3, "scores should sum to ~1, got {sum}");
    }

    #[test]
    fn external_and_dangling_links_are_handled() {
        // D links only off-graph; E has no links. Must not panic, scores still sum to ~1.
        let pages = [page("D", &["http://external/x"]), page("E", &[])];
        let r = pagerank(&pages);
        let sum: f32 = r.values().sum();
        assert!((sum - 1.0).abs() < 1e-3, "sum {sum}");
    }

    #[test]
    fn empty_authority_is_a_no_op() {
        let a = Authority::empty();
        assert_eq!(a.norm("anything"), 0.0);
        assert_eq!(blend(2.0, a.norm("anything"), 0.5), 2.0);
    }

    #[test]
    fn authority_can_reorder_equal_relevance() {
        let pages = [page("A", &["B"]), page("B", &["A"]), page("C", &["A"])];
        let a = Authority::from_pages(&pages);
        // Two equally-relevant hits (bm25 = 1.0): the higher-authority URL blends higher.
        let hi = blend(1.0, a.norm("A"), 0.5);
        let lo = blend(1.0, a.norm("C"), 0.5);
        assert!(hi > lo, "A ({hi}) should outrank C ({lo}) at equal relevance");
    }
}
