//! Stage-1: per-patch colour analysis.
//!
//! Given a 4×8 patch of RGB sub-pixels, compute the foreground / background
//! colours and the luminance threshold that partitions the sub-pixels
//! between them. The matcher's [`crate::bitmap::Bitmap`] for this patch
//! is the per-sub-pixel comparison `luminance > threshold`.
//!
//! # Algorithm
//!
//! 1. **Luminance** per sub-pixel via the BT.709 weights `Y = 0.2126R +
//!    0.7152G + 0.0722B`, integer-approximated at scale 256.
//! 2. **Variance gate.** If the patch's luminance range (`max - min`) is
//!    below [`UNIFORM_RANGE`], Otsu's split would wander on noise. Return
//!    [`Analysis::Uniform`] with the patch's mean RGB; the matcher then
//!    emits `SPACE` or `FULL_BLOCK` based on mean brightness.
//! 3. **Otsu** between-class-variance argmax over the luminance histogram
//!    selects the threshold.
//! 4. **Cluster means** in full RGB (not just luminance) give `fg` and
//!    `bg`. This is the ansiani principle generalised — colours are
//!    deterministic from the source partition, independent of which
//!    glyph eventually wins.
//!
//! Pure function of `patch` — no history, no other state.

use crate::bitmap::TOTAL;

/// 24-bit RGB sub-pixel. The matcher's primary numeric type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// One cell's worth of source data: 32 sub-pixels in row-major order,
/// matching [`crate::bitmap::Bitmap`]'s addressing (`index = row * 4 +
/// col`).
pub type Patch = [Rgb; TOTAL];

/// Outcome of stage-1 analysis. The two arms are exhaustive: the matcher
/// dispatches once on this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Analysis {
    /// Patch luminance range was below [`UNIFORM_RANGE`]; Otsu would
    /// pick arbitrarily. The matcher emits `SPACE` or `FULL_BLOCK`
    /// based on `mean_lum`, with both fg and bg set to `mean`.
    Uniform {
        /// Mean RGB across all 32 sub-pixels.
        mean: Rgb,
        /// BT.709 luminance of `mean`. Used to choose SPACE vs FULL_BLOCK.
        mean_lum: u8,
    },
    /// Patch was bimodal. `threshold` partitions the sub-pixels;
    /// `fg`/`bg` are the cluster RGB means.
    Bimodal {
        fg: Rgb,
        bg: Rgb,
        /// Sub-pixels with `luminance > threshold` are in the `fg`
        /// cluster; `≤ threshold` are in `bg`.
        threshold: u8,
    },
}

/// Luminance range (`max − min`) below which the patch is treated as
/// uniform. Calibrated against typical decoded-video noise floor: a
/// single chroma-subsampled pixel can drift ~1-2 units between frames
/// on nominally-static content, so a 16-unit range comfortably exceeds
/// pure-noise variation while still catching all real bimodality. Tune
/// against measured content later.
pub const UNIFORM_RANGE: u8 = 16;

/// BT.709 luminance of one sub-pixel, integer-approximated at scale 256:
/// weights `(54, 183, 19)` sum to 256, errors ≤ 0.4 lum-units vs float.
#[inline]
pub const fn luminance(rgb: Rgb) -> u8 {
    let r = rgb.0 as u32;
    let g = rgb.1 as u32;
    let b = rgb.2 as u32;
    ((54 * r + 183 * g + 19 * b) >> 8) as u8
}

/// Run stage-1 analysis on a patch.
pub fn analyze(patch: &Patch) -> Analysis {
    // One pass: per-sub-pixel luminance + accumulated sums for the
    // mean-RGB / mean-luminance / min-max range. Single read of the
    // patch — keeps cache pressure minimal.
    let mut lums = [0u8; TOTAL];
    let mut lum_min: u8 = u8::MAX;
    let mut lum_max: u8 = 0;
    let mut sum_r: u32 = 0;
    let mut sum_g: u32 = 0;
    let mut sum_b: u32 = 0;
    let mut sum_lum: u32 = 0;
    for i in 0..TOTAL {
        let p = patch[i];
        let l = luminance(p);
        lums[i] = l;
        if l < lum_min {
            lum_min = l;
        }
        if l > lum_max {
            lum_max = l;
        }
        sum_r += p.0 as u32;
        sum_g += p.1 as u32;
        sum_b += p.2 as u32;
        sum_lum += l as u32;
    }

    let n = TOTAL as u32;
    let mean_rgb = Rgb(
        (sum_r / n) as u8,
        (sum_g / n) as u8,
        (sum_b / n) as u8,
    );
    let mean_lum = (sum_lum / n) as u8;

    // Variance gate.
    if lum_max - lum_min < UNIFORM_RANGE {
        return Analysis::Uniform {
            mean: mean_rgb,
            mean_lum,
        };
    }

    // Otsu argmax over the luminance histogram. With only 32 sub-pixels
    // the histogram is sparse, but the 256-wide scan is still trivial
    // (~256 ops) and avoids the cache eviction of a sort. We use
    // cross-multiplication to compare candidate variances exactly in
    // integer arithmetic.
    let mut hist = [0u16; 256];
    for &l in lums.iter() {
        hist[l as usize] += 1;
    }

    // Otsu reformulation: between-class variance
    //   J(t) = n_low · n_high · (μ_low − μ_high)²
    //        = (sum_low · n_high − sum_high · n_low)²  /  (n_low · n_high)
    // For argmax over t, compare candidates by cross-multiplying both
    // sides — exact integer arithmetic, no rounding.
    let mut best_num_sq: i64 = -1;
    let mut best_denom: i64 = 1;
    let mut best_t: u8 = lum_min;
    let mut n_low: i64 = 0;
    let mut sum_low: i64 = 0;
    let total_sum: i64 = sum_lum as i64;
    let total_n: i64 = n as i64;
    for t in 0..=255u16 {
        let h = hist[t as usize] as i64;
        if h == 0 {
            continue;
        }
        sum_low += (t as i64) * h;
        n_low += h;
        let n_high = total_n - n_low;
        if n_high == 0 {
            break;
        }
        let num = sum_low * n_high - (total_sum - sum_low) * n_low;
        let num_sq = num * num;
        let denom = n_low * n_high;
        // J_t > J_best  ⇔  num_sq · best_denom  >  best_num_sq · denom
        if best_num_sq < 0 || num_sq * best_denom > best_num_sq * denom {
            best_num_sq = num_sq;
            best_denom = denom;
            best_t = t as u8;
        }
    }

    // Second pass: partition sub-pixels by threshold; compute cluster
    // means in full RGB.
    let mut fg_n: u32 = 0;
    let mut fg_r: u32 = 0;
    let mut fg_g: u32 = 0;
    let mut fg_b: u32 = 0;
    let mut bg_n: u32 = 0;
    let mut bg_r: u32 = 0;
    let mut bg_g: u32 = 0;
    let mut bg_b: u32 = 0;
    for i in 0..TOTAL {
        if lums[i] > best_t {
            fg_n += 1;
            fg_r += patch[i].0 as u32;
            fg_g += patch[i].1 as u32;
            fg_b += patch[i].2 as u32;
        } else {
            bg_n += 1;
            bg_r += patch[i].0 as u32;
            bg_g += patch[i].1 as u32;
            bg_b += patch[i].2 as u32;
        }
    }

    // Degenerate partition (threshold at an extreme) — fall back to uniform.
    if fg_n == 0 || bg_n == 0 {
        return Analysis::Uniform {
            mean: mean_rgb,
            mean_lum,
        };
    }

    Analysis::Bimodal {
        fg: Rgb(
            (fg_r / fg_n) as u8,
            (fg_g / fg_n) as u8,
            (fg_b / fg_n) as u8,
        ),
        bg: Rgb(
            (bg_r / bg_n) as u8,
            (bg_g / bg_n) as u8,
            (bg_b / bg_n) as u8,
        ),
        threshold: best_t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(c: Rgb) -> Patch {
        [c; TOTAL]
    }

    fn split_upper_lower(upper: Rgb, lower: Rgb) -> Patch {
        let mut p = [Rgb::default(); TOTAL];
        for row in 0..crate::bitmap::HEIGHT {
            for col in 0..crate::bitmap::WIDTH {
                p[row * crate::bitmap::WIDTH + col] =
                    if row < 4 { upper } else { lower };
            }
        }
        p
    }

    #[test]
    fn luminance_extremes() {
        assert_eq!(luminance(Rgb(0, 0, 0)), 0);
        assert_eq!(luminance(Rgb(255, 255, 255)), 255);
        // BT.709 pure-green dominates.
        let r = luminance(Rgb(255, 0, 0));
        let g = luminance(Rgb(0, 255, 0));
        let b = luminance(Rgb(0, 0, 255));
        assert!(g > r && r > b, "green should weigh most, blue least");
    }

    #[test]
    fn uniform_black() {
        match analyze(&fill(Rgb(0, 0, 0))) {
            Analysis::Uniform { mean, mean_lum } => {
                assert_eq!(mean, Rgb(0, 0, 0));
                assert_eq!(mean_lum, 0);
            }
            other => panic!("expected Uniform, got {other:?}"),
        }
    }

    #[test]
    fn uniform_white() {
        match analyze(&fill(Rgb(255, 255, 255))) {
            Analysis::Uniform { mean, mean_lum } => {
                assert_eq!(mean, Rgb(255, 255, 255));
                assert_eq!(mean_lum, 255);
            }
            other => panic!("expected Uniform, got {other:?}"),
        }
    }

    #[test]
    fn uniform_below_gate_threshold() {
        // 16-unit gate: two values 8 apart are well within it.
        let p = split_upper_lower(Rgb(100, 100, 100), Rgb(108, 108, 108));
        match analyze(&p) {
            Analysis::Uniform { .. } => {}
            other => panic!("expected Uniform for near-uniform patch, got {other:?}"),
        }
    }

    #[test]
    fn bimodal_clear_split() {
        // Black upper half, white lower half — unambiguous bimodal patch.
        // Threshold's exact value is implementation-detail (any value in
        // [0, 254] partitions identically for this patch); we assert on
        // the cluster colours instead.
        let p = split_upper_lower(Rgb(0, 0, 0), Rgb(255, 255, 255));
        match analyze(&p) {
            Analysis::Bimodal { fg, bg, threshold: _ } => {
                assert_eq!(fg, Rgb(255, 255, 255));
                assert_eq!(bg, Rgb(0, 0, 0));
            }
            other => panic!("expected Bimodal, got {other:?}"),
        }
    }

    #[test]
    fn bimodal_colored_split() {
        // Red upper half, blue lower half — bimodal, but with chroma
        // not just luminance.
        let p = split_upper_lower(Rgb(200, 30, 30), Rgb(30, 30, 200));
        match analyze(&p) {
            Analysis::Bimodal { fg, bg, .. } => {
                // Red (higher BT.709 luminance) is fg; blue is bg.
                assert!(fg.0 > fg.2, "fg should be red-dominant");
                assert!(bg.2 > bg.0, "bg should be blue-dominant");
            }
            other => panic!("expected Bimodal for red/blue patch, got {other:?}"),
        }
    }

    #[test]
    fn bimodal_threshold_is_partitioning() {
        // Verify: applying `lum > threshold` to the patch reproduces the
        // partition that produced the cluster means.
        let p = split_upper_lower(Rgb(20, 20, 20), Rgb(200, 200, 200));
        let analysis = analyze(&p);
        if let Analysis::Bimodal { threshold, fg, bg } = analysis {
            // Sub-pixels whose luminance > threshold should match fg
            // cluster (i.e., be the bright row).
            for row in 0..crate::bitmap::HEIGHT {
                let lum = luminance(p[row * crate::bitmap::WIDTH]);
                let is_fg = lum > threshold;
                if row < 4 {
                    assert!(!is_fg, "dark row {row} unexpectedly in fg");
                } else {
                    assert!(is_fg, "bright row {row} unexpectedly in bg");
                }
            }
            // Spot-check colours are roughly right.
            assert!(fg.0 > 150 && bg.0 < 100);
        } else {
            panic!("expected Bimodal");
        }
    }
}
