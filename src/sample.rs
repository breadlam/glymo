//! Source-image → matcher input: box-averaged downsampling.
//!
//! Given an arbitrary `src_w × src_h` RGB24 image and a target
//! `grid_cols × grid_rows` cell grid, produce one [`Patch`] per cell.
//! Each patch sub-pixel is the box-average of the source-pixel
//! rectangle that maps to it. When the source is **smaller** than the
//! target sub-pixel grid, individual source pixels get sampled by
//! multiple sub-pixels (nearest-neighbor upscale, fallback for the
//! degenerate case).
//!
//! The output grid is row-major: `patches[row * grid_cols + col]` is
//! the cell at terminal position (`col`, `row`). Within each patch,
//! sub-pixels follow the [`crate::bitmap::Bitmap`] convention
//! (`index = sub_row * 4 + sub_col`).

use crate::bitmap::{HEIGHT, TOTAL, WIDTH};
use crate::color::{Patch, Rgb};

/// Downscale a row-major `src_w × src_h` RGB24 source into a grid of
/// `grid_cols × grid_rows` patches.
///
/// # Panics
/// - `pixels.len() != src_w * src_h * 3`
/// - any dimension is zero
pub fn patches_from_rgb24(
    pixels: &[u8],
    src_w: usize,
    src_h: usize,
    grid_cols: usize,
    grid_rows: usize,
) -> Vec<Patch> {
    assert!(src_w > 0 && src_h > 0, "source dims must be positive");
    assert!(grid_cols > 0 && grid_rows > 0, "grid dims must be positive");
    assert_eq!(
        pixels.len(),
        src_w * src_h * 3,
        "RGB24 buffer size mismatch: have {}, expected {}",
        pixels.len(),
        src_w * src_h * 3
    );

    let dst_w = grid_cols * WIDTH;
    let dst_h = grid_rows * HEIGHT;
    let mut patches = vec![[Rgb::default(); TOTAL]; grid_cols * grid_rows];

    for cell_row in 0..grid_rows {
        for cell_col in 0..grid_cols {
            let patch = &mut patches[cell_row * grid_cols + cell_col];
            for sub_row in 0..HEIGHT {
                let dst_y = cell_row * HEIGHT + sub_row;
                let y0 = dst_y * src_h / dst_h;
                // y1 ≥ y0 + 1 so the averaging window is never empty,
                // even when the source is smaller than the target grid
                // (in which case nearest-neighbor degenerates correctly).
                let y1 = ((dst_y + 1) * src_h / dst_h).max(y0 + 1).min(src_h);
                for sub_col in 0..WIDTH {
                    let dst_x = cell_col * WIDTH + sub_col;
                    let x0 = dst_x * src_w / dst_w;
                    let x1 = ((dst_x + 1) * src_w / dst_w).max(x0 + 1).min(src_w);

                    let mut sum_r = 0u32;
                    let mut sum_g = 0u32;
                    let mut sum_b = 0u32;
                    let mut n = 0u32;
                    for y in y0..y1 {
                        let row_off = y * src_w;
                        for x in x0..x1 {
                            let i = (row_off + x) * 3;
                            sum_r += pixels[i] as u32;
                            sum_g += pixels[i + 1] as u32;
                            sum_b += pixels[i + 2] as u32;
                            n += 1;
                        }
                    }
                    // n ≥ 1 always (y1 > y0, x1 > x0, both clamped by
                    // `.min(src_*)` which is ≥ 1 since src dims > 0).
                    patch[sub_row * WIDTH + sub_col] = Rgb(
                        (sum_r / n) as u8,
                        (sum_g / n) as u8,
                        (sum_b / n) as u8,
                    );
                }
            }
        }
    }
    patches
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an RGB24 source buffer of `w × h` with a per-pixel closure.
    fn build_src(w: usize, h: usize, mut f: impl FnMut(usize, usize) -> Rgb) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let p = f(x, y);
                v.push(p.0);
                v.push(p.1);
                v.push(p.2);
            }
        }
        v
    }

    #[test]
    fn identity_one_cell_no_resampling() {
        // Source exactly matches one cell's sub-grid: 4×8 = WIDTH × HEIGHT.
        let src = build_src(WIDTH, HEIGHT, |x, y| {
            // Encode position so each sub-pixel is distinct.
            Rgb((x * 60) as u8, (y * 30) as u8, 0)
        });
        let patches = patches_from_rgb24(&src, WIDTH, HEIGHT, 1, 1);
        assert_eq!(patches.len(), 1);
        for sub_row in 0..HEIGHT {
            for sub_col in 0..WIDTH {
                let want = Rgb((sub_col * 60) as u8, (sub_row * 30) as u8, 0);
                let got = patches[0][sub_row * WIDTH + sub_col];
                assert_eq!(got, want, "sub-pixel ({sub_col},{sub_row}) mismatch");
            }
        }
    }

    #[test]
    fn solid_color_stays_solid() {
        let src = build_src(80, 24, |_, _| Rgb(100, 150, 200));
        let patches = patches_from_rgb24(&src, 80, 24, 10, 3);
        for p in &patches {
            for sp in p {
                assert_eq!(*sp, Rgb(100, 150, 200));
            }
        }
    }

    #[test]
    fn top_half_black_bottom_white_at_one_cell() {
        // 4-wide × 8-tall source, top 4 rows black, bottom 4 white.
        let src = build_src(4, 8, |_, y| {
            if y < 4 { Rgb(0, 0, 0) } else { Rgb(255, 255, 255) }
        });
        let patches = patches_from_rgb24(&src, 4, 8, 1, 1);
        let p = patches[0];
        // Top half sub-pixels = black; bottom half = white.
        for sub_row in 0..HEIGHT {
            for sub_col in 0..WIDTH {
                let got = p[sub_row * WIDTH + sub_col];
                let want = if sub_row < 4 { Rgb(0, 0, 0) } else { Rgb(255, 255, 255) };
                assert_eq!(got, want);
            }
        }
    }

    #[test]
    fn box_average_collapses_4_to_1() {
        // 16-wide × 32-tall source, 4× resolution per sub-pixel.
        // Make each 4×4 source block uniform with a distinct colour.
        let src = build_src(16, 32, |x, y| {
            let bx = x / 4;
            let by = y / 4;
            Rgb((bx * 60) as u8, (by * 30) as u8, 0)
        });
        let patches = patches_from_rgb24(&src, 16, 32, 1, 1);
        // Each output sub-pixel averages a uniform 4×4 region → matches
        // exactly.
        for sub_row in 0..HEIGHT {
            for sub_col in 0..WIDTH {
                let want = Rgb((sub_col * 60) as u8, (sub_row * 30) as u8, 0);
                let got = patches[0][sub_row * WIDTH + sub_col];
                assert_eq!(got, want, "sub-pixel ({sub_col},{sub_row}) mismatch");
            }
        }
    }

    #[test]
    fn upscale_degenerates_to_nearest_neighbor() {
        // Source SMALLER than the sub-grid; every output sub-pixel
        // should sample some valid source pixel (no panic on empty
        // range).
        let src = build_src(2, 4, |x, _| Rgb((x * 127) as u8, 0, 0));
        let patches = patches_from_rgb24(&src, 2, 4, 1, 1);
        let p = patches[0];
        // The left half of the cell maps to source x=0 (red=0), the
        // right half to source x=1 (red=127).
        for sub_row in 0..HEIGHT {
            for sub_col in 0..WIDTH {
                let got = p[sub_row * WIDTH + sub_col];
                let expected_red = if sub_col < 2 { 0 } else { 127 };
                assert_eq!(got.0, expected_red, "sub-pixel ({sub_col},{sub_row})");
            }
        }
    }

    #[test]
    fn grid_layout_is_row_major() {
        // 2 cells wide, 1 cell tall. Left cell solid red, right cell
        // solid blue. After downsample, patches[0] = red, patches[1] = blue.
        let src = build_src(8, 8, |x, _| {
            if x < 4 { Rgb(255, 0, 0) } else { Rgb(0, 0, 255) }
        });
        let patches = patches_from_rgb24(&src, 8, 8, 2, 1);
        assert_eq!(patches.len(), 2);
        for sp in &patches[0] {
            assert_eq!(*sp, Rgb(255, 0, 0), "left cell should be red");
        }
        for sp in &patches[1] {
            assert_eq!(*sp, Rgb(0, 0, 255), "right cell should be blue");
        }
    }

    #[test]
    fn integrates_with_matcher() {
        // End-to-end smoke: build a source with a clear upper/lower
        // split, downsample to one cell, run the matcher, expect
        // LOWER HALF BLOCK (bright pixels on bottom = fg cluster).
        use crate::matcher::match_cell;
        use crate::repertoire::{Repertoire, SymbolSet};

        let src = build_src(16, 32, |_, y| {
            if y < 16 { Rgb(20, 20, 20) } else { Rgb(220, 220, 220) }
        });
        let patches = patches_from_rgb24(&src, 16, 32, 1, 1);
        let pool = SymbolSet::build(Repertoire::CONSERVATIVE);
        let m = match_cell(&pool, &patches[0]);
        assert_eq!(m.codepoint, '\u{2584}', "should pick LOWER HALF BLOCK");
        assert!(m.fg.0 > 150 && m.bg.0 < 100);
    }
}
