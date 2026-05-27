//! A glyph candidate: Unicode codepoint + sub-cell bitmap + cached popcount.

use crate::bitmap::Bitmap;

/// One glyph in a [`crate::SymbolSet`]. Its bitmap is the sub-pixel
/// coverage at the 4×8 sub-grid; its codepoint is what the matcher
/// emits when this glyph wins. The popcount is cached at construction
/// because the matcher's inner loop uses it for class-pruning every
/// iteration (`|popcount(a) - popcount(b)| ≤ hamming(a, b)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Symbol {
    pub codepoint: char,
    pub bitmap: Bitmap,
    pub popcount: u32,
}

impl Symbol {
    /// Construct a symbol with a runtime-checked codepoint.
    pub fn new(codepoint: char, bitmap: Bitmap) -> Self {
        Symbol {
            codepoint,
            bitmap,
            popcount: bitmap.popcount(),
        }
    }

    /// Construct from a raw `u32` codepoint. Invalid codepoints map to
    /// `U+FFFD REPLACEMENT CHARACTER` rather than panicking — useful
    /// for table-driven repertoire builders where every entry is
    /// statically known to be valid but you want a const-correct path.
    pub const fn from_u32(codepoint: u32, bitmap: Bitmap) -> Self {
        let codepoint = match char::from_u32(codepoint) {
            Some(c) => c,
            None => '\u{FFFD}',
        };
        Symbol {
            codepoint,
            bitmap,
            popcount: bitmap.0.count_ones(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popcount_is_cached_correctly() {
        let s = Symbol::new('A', Bitmap::from_rect(0, 0, 4, 4));
        assert_eq!(s.popcount, 16);
        // And matches the bitmap's own popcount.
        assert_eq!(s.popcount, s.bitmap.popcount());
    }

    #[test]
    fn space_is_empty() {
        let s = Symbol::new(' ', Bitmap::EMPTY);
        assert_eq!(s.popcount, 0);
    }

    #[test]
    fn from_u32_handles_valid_codepoint() {
        let s = Symbol::from_u32(0x2588, Bitmap::FULL);
        assert_eq!(s.codepoint, '\u{2588}');
        assert_eq!(s.popcount, 32);
    }
}
