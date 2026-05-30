//! A symbol-mode glyph candidate: Unicode codepoint + 8×16 bitmap +
//! cached popcount.

use crate::symbol_mode::bitmap::Bitmap;

/// One glyph in a [`crate::symbol_mode::SymbolSet`]. Identical role to
/// [`crate::Symbol`] but holds a 128-bit bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Symbol {
    pub codepoint: char,
    pub bitmap: Bitmap,
    /// Cached `bitmap.popcount()`. The matcher uses it for the
    /// popcount-class lower bound on Hamming distance every iteration.
    pub popcount: u32,
}

impl Symbol {
    /// Construct from a runtime-checked codepoint.
    pub fn new(codepoint: char, bitmap: Bitmap) -> Self {
        Symbol { codepoint, bitmap, popcount: bitmap.popcount() }
    }

    /// Construct from a raw `u32` codepoint and a pre-computed
    /// popcount (saves a recount when loading from the generated const).
    /// Invalid codepoints map to `U+FFFD REPLACEMENT CHARACTER`.
    pub const fn from_raw(codepoint: u32, bitmap: Bitmap, popcount: u32) -> Self {
        let codepoint = match char::from_u32(codepoint) {
            Some(c) => c,
            None => '\u{FFFD}',
        };
        Symbol { codepoint, bitmap, popcount }
    }
}
