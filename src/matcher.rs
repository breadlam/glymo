//! Stage-2: pick the best-matching glyph from a [`SymbolSet`].
//!
//! Inputs: a `[`Patch`]` of 32 RGB sub-pixels and a non-empty
//! [`SymbolSet`]. Output: a [`Match`] carrying the chosen Unicode
//! codepoint and the `fg`/`bg` RGB colours.
//!
//! # Algorithm
//!
//! 1. Run stage-1 [`analyze`] on the patch:
//!    - [`Analysis::Uniform`]: the query bitmap is forced all-0
//!      (mean is dark → emits SPACE if in pool) or all-1 (mean is
//!      light → emits FULL_BLOCK if in pool), with `fg == bg == mean`.
//!      The Hamming search still runs — it naturally lands on the
//!      lowest- or highest-popcount glyph in the pool.
//!    - [`Analysis::Bimodal`]: build a 32-bit query by thresholding
//!      each sub-pixel against `threshold`.
//! 2. Walk the pool, finding the glyph whose bitmap has the smallest
//!    Hamming distance from the query. Each comparison is one
//!    `XOR + POPCNT` = ~2 instructions. **Popcount-class pruning** —
//!    by the triangle inequality, `|popcount(query) − popcount(s)| ≤
//!    Hamming(query, s.bitmap)`, so candidates whose popcount differs
//!    from the query's by more than the current best can be skipped
//!    without computing the XOR.
//!
//! The matcher is a **pure function** of `(patch, set)` — no history,
//! no other state. The output's stability against small input noise
//! comes from the discrete-binary representation: small RGB
//! perturbations either don't flip any sub-pixel across the threshold
//! (no change at all) or flip 1-2 bits in the query (which rarely
//! moves the Hamming-nearest neighbour, given typical inter-candidate
//! distances).

use crate::bitmap::{Bitmap, HEIGHT, TOTAL, WIDTH};
use crate::color::{analyze, luminance, Analysis, Patch, Rgb};
use crate::repertoire::SymbolSet;

/// Result of matching one cell.
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
            // Mean-dark → query 0 (best match: SPACE / low-popcount).
            // Mean-light → query all-1 (best match: FULL_BLOCK / high-popcount).
            let q = if mean_lum >= 128 { u32::MAX } else { 0 };
            (Bitmap(q), mean, mean)
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
/// against `threshold`. Bit set ⇔ `lum > threshold` (fg cluster).
pub fn query_from_patch(patch: &Patch, threshold: u8) -> Bitmap {
    let mut v: u32 = 0;
    for row in 0..HEIGHT {
        for col in 0..WIDTH {
            let i = row * WIDTH + col;
            if luminance(patch[i]) > threshold {
                v |= 1u32 << Bitmap::bit_index(row, col);
            }
        }
    }
    debug_assert_eq!(TOTAL, HEIGHT * WIDTH);
    Bitmap(v)
}

/// Find the pool member nearest to `query` in Hamming distance.
/// Popcount-class pruning skips candidates that can't possibly beat the
/// current best; uses the triangle inequality
/// `|popcount(a) − popcount(b)| ≤ Hamming(a, b)`. On ties the
/// lower-codepoint candidate wins (because the pool is codepoint-sorted
/// and the comparison is strict-less-than).
pub fn hamming_nearest(set: &SymbolSet, query: Bitmap) -> char {
    let symbols = set.symbols();
    debug_assert!(!symbols.is_empty());

    let query_pc = query.popcount() as i32;
    // Initialise with the first candidate so we never return a sentinel.
    let mut best_d = symbols[0].bitmap.hamming(query);
    let mut best_cp = symbols[0].codepoint;
    for s in &symbols[1..] {
        // Popcount-class lower bound.
        let pc_lower_bound = (s.popcount as i32 - query_pc).unsigned_abs();
        if pc_lower_bound >= best_d {
            continue;
        }
        let d = s.bitmap.hamming(query);
        if d < best_d {
            best_d = d;
            best_cp = s.codepoint;
        }
    }
    best_cp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repertoire::Repertoire;

    fn pool() -> SymbolSet {
        SymbolSet::build(Repertoire::CONSERVATIVE)
    }

    fn fill(c: Rgb) -> Patch {
        [c; TOTAL]
    }

    fn split_upper_lower(upper: Rgb, lower: Rgb) -> Patch {
        let mut p = [Rgb::default(); TOTAL];
        for row in 0..HEIGHT {
            for col in 0..WIDTH {
                p[row * WIDTH + col] = if row < 4 { upper } else { lower };
            }
        }
        p
    }

    fn split_left_right(left: Rgb, right: Rgb) -> Patch {
        let mut p = [Rgb::default(); TOTAL];
        for row in 0..HEIGHT {
            for col in 0..WIDTH {
                p[row * WIDTH + col] = if col < 2 { left } else { right };
            }
        }
        p
    }

    #[test]
    fn all_black_picks_space() {
        let m = match_cell(&pool(), &fill(Rgb(0, 0, 0)));
        assert_eq!(m.codepoint, '\u{0020}', "all-black should be SPACE");
        // fg/bg both the mean (black).
        assert_eq!(m.fg, Rgb(0, 0, 0));
        assert_eq!(m.bg, Rgb(0, 0, 0));
    }

    #[test]
    fn all_white_picks_full_block() {
        let m = match_cell(&pool(), &fill(Rgb(255, 255, 255)));
        assert_eq!(m.codepoint, '\u{2588}', "all-white should be FULL_BLOCK");
        assert_eq!(m.fg, Rgb(255, 255, 255));
    }

    #[test]
    fn black_upper_white_lower_picks_lower_half() {
        // Upper rows are dark (bg cluster), lower rows light (fg cluster).
        // The fg-bit pattern fills the bottom 4 rows → U+2584 LOWER HALF.
        let p = split_upper_lower(Rgb(0, 0, 0), Rgb(255, 255, 255));
        let m = match_cell(&pool(), &p);
        assert_eq!(m.codepoint, '\u{2584}', "should pick LOWER HALF BLOCK");
        assert_eq!(m.fg, Rgb(255, 255, 255));
        assert_eq!(m.bg, Rgb(0, 0, 0));
    }

    #[test]
    fn white_upper_black_lower_picks_upper_half() {
        let p = split_upper_lower(Rgb(255, 255, 255), Rgb(0, 0, 0));
        let m = match_cell(&pool(), &p);
        assert_eq!(m.codepoint, '\u{2580}', "should pick UPPER HALF BLOCK");
    }

    #[test]
    fn black_left_white_right_picks_right_half() {
        let p = split_left_right(Rgb(0, 0, 0), Rgb(255, 255, 255));
        let m = match_cell(&pool(), &p);
        // Right cols are fg → U+2590 RIGHT HALF BLOCK.
        assert_eq!(m.codepoint, '\u{2590}', "should pick RIGHT HALF BLOCK");
    }

    #[test]
    fn white_left_black_right_picks_left_half() {
        let p = split_left_right(Rgb(255, 255, 255), Rgb(0, 0, 0));
        let m = match_cell(&pool(), &p);
        assert_eq!(m.codepoint, '\u{258C}', "should pick LEFT HALF BLOCK");
    }

    #[test]
    fn lower_quadrant_pattern_picks_quadrant_glyph() {
        // Only the bottom-left 2 cols × 4 rows are lit.
        let mut p = [Rgb(0, 0, 0); TOTAL];
        for row in 4..8 {
            for col in 0..2 {
                p[row * WIDTH + col] = Rgb(255, 255, 255);
            }
        }
        let m = match_cell(&pool(), &p);
        // U+2596 = QUADRANT LOWER LEFT.
        assert_eq!(m.codepoint, '\u{2596}');
    }

    #[test]
    fn lower_one_eighth_pattern_picks_eighth_glyph() {
        // Only the very bottom row is lit.
        let mut p = [Rgb(0, 0, 0); TOTAL];
        for col in 0..WIDTH {
            p[7 * WIDTH + col] = Rgb(255, 255, 255);
        }
        let m = match_cell(&pool(), &p);
        // U+2581 = LOWER ONE EIGHTH BLOCK.
        assert_eq!(m.codepoint, '\u{2581}');
    }

    #[test]
    fn matcher_is_deterministic() {
        // Same patch twice → identical match. (Trivial for a pure
        // function, but it's the property the design hinges on, so we
        // assert it explicitly.)
        let p = split_upper_lower(Rgb(40, 80, 120), Rgb(180, 200, 220));
        let a = match_cell(&pool(), &p);
        let b = match_cell(&pool(), &p);
        assert_eq!(a, b);
    }

    #[test]
    fn small_noise_doesnt_flip_codepoint_on_clean_split() {
        // Black upper half, white lower half — clearly bimodal.
        // Adding ±2-unit noise per pixel must not move the chosen glyph:
        // every sub-pixel stays on its side of the (~128) threshold.
        let p_clean = split_upper_lower(Rgb(10, 10, 10), Rgb(245, 245, 245));
        let m_clean = match_cell(&pool(), &p_clean);
        let mut p_noisy = p_clean;
        for i in 0..TOTAL {
            // Simulate ±2-unit decode jitter per channel.
            let bump = ((i as u32 * 31337) % 5) as i32 - 2;
            for ch in 0..3 {
                let v = match ch {
                    0 => p_noisy[i].0,
                    1 => p_noisy[i].1,
                    _ => p_noisy[i].2,
                } as i32 + bump;
                let v = v.clamp(0, 255) as u8;
                match ch {
                    0 => p_noisy[i].0 = v,
                    1 => p_noisy[i].1 = v,
                    _ => p_noisy[i].2 = v,
                }
            }
        }
        let m_noisy = match_cell(&pool(), &p_noisy);
        assert_eq!(
            m_clean.codepoint, m_noisy.codepoint,
            "small noise must not flip glyph on a clearly bimodal patch"
        );
    }
}
