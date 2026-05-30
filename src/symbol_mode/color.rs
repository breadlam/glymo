//! 8×16 patch type + stage-1 analysis (Otsu fg/bg + threshold).
//!
//! Same algorithm as `crate::color`, but sized for 128 sub-pixels. The
//! `Analysis` enum carries the result; the matcher consumes it.
//!
//! See `crate::color` for the algorithm's full prose. This module only
//! exists because Rust's array sizes are part of the type — the
//! analyzer's *logic* is geometry-independent, only the array length
//! changes.

pub use crate::color::{luminance, Analysis, Rgb, UNIFORM_RANGE};

use crate::symbol_mode::bitmap::TOTAL;

/// One symbol-mode cell's worth of source data: 128 sub-pixels in
/// row-major order, matching the 8×16 [`crate::symbol_mode::Bitmap`]
/// addressing (`index = row * 8 + col`).
pub type Patch = [Rgb; TOTAL];

/// Run stage-1 analysis on a 128-sub-pixel patch. Algorithm identical
/// to `crate::color::analyze` — see there for prose.
pub fn analyze(patch: &Patch) -> Analysis {
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
        if l < lum_min { lum_min = l; }
        if l > lum_max { lum_max = l; }
        sum_r += p.0 as u32;
        sum_g += p.1 as u32;
        sum_b += p.2 as u32;
        sum_lum += l as u32;
    }
    let n = TOTAL as u32;
    let mean_rgb = Rgb((sum_r / n) as u8, (sum_g / n) as u8, (sum_b / n) as u8);
    let mean_lum = (sum_lum / n) as u8;

    if lum_max - lum_min < UNIFORM_RANGE {
        return Analysis::Uniform { mean: mean_rgb, mean_lum };
    }

    // Otsu's between-class-variance maximisation over a 256-bin
    // luminance histogram.
    let mut hist = [0u32; 256];
    for &l in &lums { hist[l as usize] += 1; }
    let total_sum: f64 = hist.iter().enumerate()
        .map(|(i, &c)| i as f64 * c as f64).sum();

    let mut sum_b_f = 0f64;
    let mut wb = 0u32;
    let mut wf;
    let mut max_var = -1f64;
    let mut best_t: u8 = mean_lum;
    for t in 0..256 {
        wb += hist[t];
        if wb == 0 { continue; }
        wf = n - wb;
        if wf == 0 { break; }
        sum_b_f += t as f64 * hist[t] as f64;
        let mb = sum_b_f / wb as f64;
        let mf = (total_sum - sum_b_f) / wf as f64;
        let var = wb as f64 * wf as f64 * (mb - mf) * (mb - mf);
        if var > max_var { max_var = var; best_t = t as u8; }
    }

    let threshold = best_t;
    let (mut fg_r, mut fg_g, mut fg_b, mut fg_n) = (0u32, 0u32, 0u32, 0u32);
    let (mut bg_r, mut bg_g, mut bg_b, mut bg_n) = (0u32, 0u32, 0u32, 0u32);
    for i in 0..TOTAL {
        let p = patch[i];
        if lums[i] > threshold {
            fg_r += p.0 as u32; fg_g += p.1 as u32; fg_b += p.2 as u32; fg_n += 1;
        } else {
            bg_r += p.0 as u32; bg_g += p.1 as u32; bg_b += p.2 as u32; bg_n += 1;
        }
    }
    let fg = if fg_n > 0 {
        Rgb((fg_r / fg_n) as u8, (fg_g / fg_n) as u8, (fg_b / fg_n) as u8)
    } else { Rgb(0, 0, 0) };
    let bg = if bg_n > 0 {
        Rgb((bg_r / bg_n) as u8, (bg_g / bg_n) as u8, (bg_b / bg_n) as u8)
    } else { Rgb(0, 0, 0) };
    Analysis::Bimodal { fg, bg, threshold }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_patch_is_uniform() {
        let p = [Rgb(120, 120, 120); TOTAL];
        match analyze(&p) {
            Analysis::Uniform { mean, .. } => assert_eq!(mean, Rgb(120, 120, 120)),
            _ => panic!("expected Uniform"),
        }
    }

    #[test]
    fn split_patch_is_bimodal() {
        let mut p = [Rgb(0, 0, 0); TOTAL];
        for i in TOTAL / 2..TOTAL { p[i] = Rgb(255, 255, 255); }
        match analyze(&p) {
            Analysis::Bimodal { fg, bg, .. } => {
                assert_eq!(fg, Rgb(255, 255, 255));
                assert_eq!(bg, Rgb(0, 0, 0));
            }
            _ => panic!("expected Bimodal"),
        }
    }
}
