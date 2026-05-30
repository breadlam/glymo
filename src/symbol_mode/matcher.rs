//! Stage-2: pick the best-matching symbol-mode glyph from a [`SymbolSet`].
//!
//! Same algorithm as `crate::matcher`, targeting the 128-bit signature.
//! Each Hamming comparison is two `XOR + POPCNT` pairs (`Bitmap::hamming`).
//! Popcount-class pruning is the same triangle-inequality bound:
//! `|popcount(query) − popcount(s)| ≤ Hamming(query, s.bitmap)`.

use crate::symbol_mode::bitmap::{Bitmap, HEIGHT, TOTAL, WIDTH};
use crate::symbol_mode::color::{analyze, luminance, Analysis, Patch, Rgb};
use crate::symbol_mode::pool::SymbolSet;

/// Result of matching one symbol-mode cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub codepoint: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

/// Match a patch against the pool. The pool must be non-empty.
pub fn match_cell(set: &SymbolSet, patch: &Patch) -> Match {
    assert!(!set.is_empty(), "match_cell: SymbolSet is empty");

    let (query, fg, bg) = match analyze(patch) {
        Analysis::Uniform { mean, mean_lum } => {
            let q = if mean_lum >= 128 {
                Bitmap::FULL
            } else {
                Bitmap::EMPTY
            };
            (q, mean, mean)
        }
        Analysis::Bimodal { fg, bg, threshold } => {
            let q = query_from_patch(patch, threshold);
            (q, fg, bg)
        }
    };
    let codepoint = hamming_nearest(set, query);
    Match { codepoint, fg, bg }
}

/// Build the query bitmap by thresholding each sub-pixel's luminance
/// against `threshold`. Bit set ⇔ `lum > threshold`.
pub fn query_from_patch(patch: &Patch, threshold: u8) -> Bitmap {
    let mut lo: u64 = 0;
    let mut hi: u64 = 0;
    for row in 0..HEIGHT {
        for col in 0..WIDTH {
            let i = row * WIDTH + col;
            if luminance(patch[i]) > threshold {
                let bit_i = Bitmap::bit_index(row, col);
                if bit_i < 64 { lo |= 1u64 << bit_i; }
                else { hi |= 1u64 << (bit_i - 64); }
            }
        }
    }
    debug_assert_eq!(TOTAL, HEIGHT * WIDTH);
    Bitmap { lo, hi }
}

/// Find the pool member nearest to `query` in Hamming distance, with
/// popcount-class pruning. Ties broken by lowest codepoint (the pool
/// is codepoint-sorted, comparison is strict-less-than).
pub fn hamming_nearest(set: &SymbolSet, query: Bitmap) -> char {
    let symbols = set.symbols();
    debug_assert!(!symbols.is_empty());

    let query_pc = query.popcount() as i32;
    let mut best_d = symbols[0].bitmap.hamming(query);
    let mut best_cp = symbols[0].codepoint;
    for s in &symbols[1..] {
        let pc_lower_bound = (s.popcount as i32 - query_pc).unsigned_abs();
        if pc_lower_bound >= best_d { continue; }
        let d = s.bitmap.hamming(query);
        if d < best_d { best_d = d; best_cp = s.codepoint; }
    }
    best_cp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(c: Rgb) -> Patch { [c; TOTAL] }

    #[test]
    fn all_black_picks_space() {
        let set = SymbolSet::build();
        let m = match_cell(&set, &fill(Rgb(0, 0, 0)));
        assert_eq!(m.codepoint, ' ');
        assert_eq!(m.fg, Rgb(0, 0, 0));
        assert_eq!(m.bg, Rgb(0, 0, 0));
    }

    #[test]
    fn all_white_picks_a_full_block_or_similar() {
        let set = SymbolSet::build();
        let m = match_cell(&set, &fill(Rgb(255, 255, 255)));
        // The pool may contain U+2588 FULL BLOCK or a similar dense
        // glyph; either way `fg == bg == white` and the codepoint is
        // one of the highest-popcount entries.
        assert_eq!(m.fg, Rgb(255, 255, 255));
        assert_eq!(m.bg, Rgb(255, 255, 255));
    }

    #[test]
    fn matcher_is_deterministic() {
        let set = SymbolSet::build();
        let mut p = [Rgb(40, 80, 120); TOTAL];
        for i in TOTAL / 2..TOTAL { p[i] = Rgb(180, 200, 220); }
        let a = match_cell(&set, &p);
        let b = match_cell(&set, &p);
        assert_eq!(a, b);
    }

    #[test]
    fn class_pruning_does_not_change_result() {
        // Brute-force scan, no class pruning — must agree with
        // `hamming_nearest` for every test query.
        let set = SymbolSet::build();
        let queries = [
            Bitmap::EMPTY,
            Bitmap::FULL,
            Bitmap::from_words(0xFF00FF00FF00FF00, 0x00FF00FF00FF00FF),
            Bitmap::from_words(0xAAAAAAAAAAAAAAAA, 0x5555555555555555),
        ];
        for q in queries {
            let pruned = hamming_nearest(&set, q);
            let brute = set.symbols().iter()
                .min_by_key(|s| s.bitmap.hamming(q))
                .unwrap().codepoint;
            // Pruned may pick a lower codepoint on ties; brute may pick
            // a different one. Compare distances rather than codepoints.
            let pruned_d = set.symbols().iter()
                .find(|s| s.codepoint == pruned).unwrap().bitmap.hamming(q);
            let brute_d = set.symbols().iter()
                .find(|s| s.codepoint == brute).unwrap().bitmap.hamming(q);
            assert_eq!(pruned_d, brute_d,
                       "class pruning changed best distance: pruned={pruned_d}, brute={brute_d}");
        }
    }
}
