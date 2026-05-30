//! 8×16 sub-pixel bitmap as two `u64` words.
//!
//! Symbol mode samples each terminal cell at 8 columns × 16 rows = 128
//! sub-pixels. Bit addressing is `row * 8 + col`, row 0 at the top,
//! col 0 at the left — bit 0 of `lo` is the top-left sub-pixel; bit 63
//! of `hi` is the bottom-right.

use crate::symbol_mode::generated;

/// Sub-cell grid width in sub-pixels.
pub const WIDTH: usize = generated::COLS;

/// Sub-cell grid height in sub-pixels.
pub const HEIGHT: usize = generated::ROWS;

/// Total sub-pixels per cell.
pub const TOTAL: usize = WIDTH * HEIGHT;

/// 128-bit bitmap packed as `(lo, hi)` u64 words. Bit indices 0..64
/// live in `lo`, 64..128 in `hi`. Two pipelined `popcount` operations
/// per Hamming comparison — same instruction count as a single `u64`
/// candidate, just both words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bitmap {
    pub lo: u64,
    pub hi: u64,
}

impl Bitmap {
    /// All sub-pixels background.
    pub const EMPTY: Bitmap = Bitmap { lo: 0, hi: 0 };

    /// All sub-pixels foreground.
    pub const FULL: Bitmap = Bitmap { lo: u64::MAX, hi: u64::MAX };

    /// Bit position for sub-pixel `(row, col)` in the packed 128-bit
    /// representation. Use [`Bitmap::test`] to read a single bit.
    #[inline]
    pub const fn bit_index(row: usize, col: usize) -> usize {
        row * WIDTH + col
    }

    /// Construct from packed words. Order: lower 64 bits first.
    #[inline]
    pub const fn from_words(lo: u64, hi: u64) -> Self {
        Bitmap { lo, hi }
    }

    /// True iff bit `i` is set. `i` must be `< TOTAL`.
    #[inline]
    pub const fn test(&self, i: usize) -> bool {
        if i < 64 {
            self.lo & (1u64 << i) != 0
        } else {
            self.hi & (1u64 << (i - 64)) != 0
        }
    }

    /// Number of foreground sub-pixels.
    #[inline]
    pub const fn popcount(&self) -> u32 {
        self.lo.count_ones() + self.hi.count_ones()
    }

    /// Hamming distance to another bitmap. Two XORs + two popcounts.
    /// The matcher's inner loop.
    #[inline]
    pub const fn hamming(&self, other: Bitmap) -> u32 {
        (self.lo ^ other.lo).count_ones() + (self.hi ^ other.hi).count_ones()
    }

    /// Bitwise union (foreground in either).
    #[inline]
    pub const fn union(&self, other: Bitmap) -> Bitmap {
        Bitmap { lo: self.lo | other.lo, hi: self.hi | other.hi }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_full() {
        assert_eq!(Bitmap::EMPTY.popcount(), 0);
        assert_eq!(Bitmap::FULL.popcount(), TOTAL as u32);
        assert_eq!(Bitmap::EMPTY.hamming(Bitmap::FULL), TOTAL as u32);
    }

    #[test]
    fn bit_index_corners() {
        assert_eq!(Bitmap::bit_index(0, 0), 0);
        assert_eq!(Bitmap::bit_index(0, WIDTH - 1), WIDTH - 1);
        assert_eq!(Bitmap::bit_index(HEIGHT - 1, WIDTH - 1), TOTAL - 1);
    }

    #[test]
    fn test_reads_round_trip() {
        let mut bm = Bitmap::EMPTY;
        for i in [0usize, 1, 63, 64, 65, 127] {
            if i < 64 { bm.lo |= 1u64 << i; } else { bm.hi |= 1u64 << (i - 64); }
            assert!(bm.test(i));
        }
    }

    #[test]
    fn hamming_zero_is_equal() {
        let b = Bitmap::from_words(0xDEADBEEFCAFEBABE, 0x0123456789ABCDEF);
        assert_eq!(b.hamming(b), 0);
    }
}
