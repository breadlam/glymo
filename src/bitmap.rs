//! 4×8 sub-pixel bitmap as a `u32`.
//!
//! Each terminal cell is divided into 4 columns × 8 rows = 32 sub-pixels.
//! A `Bitmap` records which sub-pixels are foreground (bit set) vs
//! background (bit clear). Bit addressing is `row * 4 + col`, row 0 at
//! the top, col 0 at the left — so bit 0 is the top-left sub-pixel and
//! bit 31 is the bottom-right.
//!
//! The convention is internal: the matcher's external contract is
//! `(patch) → (codepoint, fg, bg)`, not bit layout.

/// Sub-cell grid width in sub-pixels.
pub const WIDTH: usize = 4;

/// Sub-cell grid height in sub-pixels.
pub const HEIGHT: usize = 8;

/// Total sub-pixels per cell.
pub const TOTAL: usize = WIDTH * HEIGHT;

/// Compact 4×8 bitmap. One `u32` per cell — exactly the right size for
/// SIMD batching over candidates and a single-instruction popcount on
/// any modern CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bitmap(pub u32);

impl Bitmap {
    /// All sub-pixels background.
    pub const EMPTY: Bitmap = Bitmap(0);

    /// All sub-pixels foreground.
    pub const FULL: Bitmap = Bitmap(u32::MAX);

    /// Bit position for sub-pixel `(row, col)` in the underlying `u32`.
    #[inline]
    pub const fn bit_index(row: usize, col: usize) -> usize {
        row * WIDTH + col
    }

    /// Build by sub-pixel predicate. The closure is called once per
    /// sub-pixel in row-major order.
    pub fn from_grid(mut f: impl FnMut(usize, usize) -> bool) -> Self {
        let mut v: u32 = 0;
        for row in 0..HEIGHT {
            for col in 0..WIDTH {
                if f(row, col) {
                    v |= 1u32 << Self::bit_index(row, col);
                }
            }
        }
        Bitmap(v)
    }

    /// Bitmap covering the half-open rectangle `[r0..r1) × [c0..c1)`.
    /// The natural constructor for halves, eighths, and quadrants — all
    /// the block-family glyphs decompose into one or a few rects.
    pub fn from_rect(r0: usize, c0: usize, r1: usize, c1: usize) -> Self {
        Self::from_grid(|r, c| r >= r0 && r < r1 && c >= c0 && c < c1)
    }

    /// Number of foreground sub-pixels.
    #[inline]
    pub const fn popcount(self) -> u32 {
        self.0.count_ones()
    }

    /// Hamming distance: number of sub-pixel positions where `self` and
    /// `other` disagree. The matcher's inner-loop comparison.
    #[inline]
    pub const fn hamming(self, other: Bitmap) -> u32 {
        (self.0 ^ other.0).count_ones()
    }

    /// Foreground state of one sub-pixel.
    #[inline]
    pub const fn get(self, row: usize, col: usize) -> bool {
        (self.0 >> Self::bit_index(row, col)) & 1 == 1
    }

    /// Bit-wise union (used to compose multi-rectangle glyphs like the
    /// three-corner quadrants).
    #[inline]
    pub const fn union(self, other: Bitmap) -> Bitmap {
        Bitmap(self.0 | other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_full() {
        assert_eq!(Bitmap::EMPTY.popcount(), 0);
        assert_eq!(Bitmap::FULL.popcount(), TOTAL as u32);
        // Maximally different.
        assert_eq!(Bitmap::EMPTY.hamming(Bitmap::FULL), TOTAL as u32);
        // Self-hamming is zero.
        assert_eq!(Bitmap::FULL.hamming(Bitmap::FULL), 0);
    }

    #[test]
    fn bit_addressing_top_left_and_bottom_right() {
        let tl = Bitmap::from_grid(|r, c| r == 0 && c == 0);
        assert_eq!(tl.0, 1, "top-left is bit 0");
        assert!(tl.get(0, 0));
        assert!(!tl.get(0, 1));

        let br = Bitmap::from_grid(|r, c| r == HEIGHT - 1 && c == WIDTH - 1);
        assert_eq!(br.0, 1u32 << (TOTAL - 1), "bottom-right is the top bit");
        assert!(br.get(HEIGHT - 1, WIDTH - 1));
    }

    #[test]
    fn upper_half_via_rect() {
        let upper = Bitmap::from_rect(0, 0, 4, WIDTH);
        assert_eq!(upper.popcount(), 16, "top 4 rows × 4 cols = 16 sub-pixels");
        // Bottom 16 bits = top of cell (row-major); high 16 bits = clear.
        assert_eq!(upper.0 & 0x0000_FFFF, 0x0000_FFFF);
        assert_eq!(upper.0 & 0xFFFF_0000, 0);
    }

    #[test]
    fn hamming_basic() {
        let upper = Bitmap::from_rect(0, 0, 4, WIDTH);
        let lower = Bitmap::from_rect(4, 0, 8, WIDTH);
        assert_eq!(upper.hamming(lower), 32, "disjoint halves = full disagreement");
        assert_eq!(upper.hamming(upper), 0);
    }

    #[test]
    fn union_composes() {
        let upper = Bitmap::from_rect(0, 0, 4, WIDTH);
        let lower = Bitmap::from_rect(4, 0, 8, WIDTH);
        assert_eq!(upper.union(lower), Bitmap::FULL);
    }
}
