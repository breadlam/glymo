//! 8×16 box-averaged downsampling.
//!
//! Identical algorithm to `crate::sample` but targets the
//! symbol-mode sub-grid (`WIDTH = 8`, `HEIGHT = 16`).

use crate::symbol_mode::bitmap::{HEIGHT, TOTAL, WIDTH};
use crate::symbol_mode::color::{Patch, Rgb};

/// Downscale a row-major `src_w × src_h` RGB24 source into a grid of
/// `grid_cols × grid_rows` 8×16 patches.
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
                let y1 = ((dst_y + 1) * src_h / dst_h).max(y0 + 1).min(src_h);
                for sub_col in 0..WIDTH {
                    let dst_x = cell_col * WIDTH + sub_col;
                    let x0 = dst_x * src_w / dst_w;
                    let x1 = ((dst_x + 1) * src_w / dst_w).max(x0 + 1).min(src_w);
                    let (mut sr, mut sg, mut sb, mut n) = (0u32, 0u32, 0u32, 0u32);
                    for y in y0..y1 {
                        let row_off = y * src_w * 3;
                        for x in x0..x1 {
                            let i = row_off + x * 3;
                            sr += pixels[i] as u32;
                            sg += pixels[i + 1] as u32;
                            sb += pixels[i + 2] as u32;
                            n += 1;
                        }
                    }
                    if n > 0 {
                        patch[sub_row * WIDTH + sub_col] = Rgb(
                            (sr / n) as u8,
                            (sg / n) as u8,
                            (sb / n) as u8,
                        );
                    }
                }
            }
        }
    }
    patches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_cell_one_pixel_passes_through() {
        // 8×16 px source = 1 cell at 8×16 sub-grid (1:1 source/sub-pixel).
        let mut px = vec![0u8; 8 * 16 * 3];
        for i in 0..8 * 16 { px[i * 3] = 200; px[i * 3 + 1] = 100; px[i * 3 + 2] = 50; }
        let patches = patches_from_rgb24(&px, 8, 16, 1, 1);
        assert_eq!(patches.len(), 1);
        for p in &patches[0] {
            assert_eq!(*p, Rgb(200, 100, 50));
        }
    }
}
