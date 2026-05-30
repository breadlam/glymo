//! The symbol-mode pool, materialised from the generated const table.

use crate::symbol_mode::bitmap::Bitmap;
use crate::symbol_mode::generated::SYMBOL_POOL;
use crate::symbol_mode::symbol::Symbol;

/// Deduplicated, codepoint-sorted pool of symbol-mode glyphs. The
/// underlying data is the generated const slice — `build()` simply
/// materialises it into owned `Symbol` records the matcher iterates.
#[derive(Debug, Clone)]
pub struct SymbolSet {
    symbols: Vec<Symbol>,
}

impl SymbolSet {
    /// Build the symbol-mode pool from the generated const.
    pub fn build() -> Self {
        let symbols = SYMBOL_POOL
            .iter()
            .map(|&(cp, lo, hi, pc)| {
                Symbol::from_raw(cp, Bitmap::from_words(lo, hi), pc)
            })
            .collect();
        SymbolSet { symbols }
    }

    pub fn symbols(&self) -> &[Symbol] { &self.symbols }
    pub fn len(&self) -> usize { self.symbols.len() }
    pub fn is_empty(&self) -> bool { self.symbols.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_is_nonempty_and_sorted() {
        let set = SymbolSet::build();
        assert!(!set.is_empty());
        assert!(set.len() >= 500, "pool unexpectedly small: {}", set.len());
        let mut prev: u32 = 0;
        for s in set.symbols() {
            let cp = s.codepoint as u32;
            assert!(cp > prev, "pool not sorted: U+{prev:04X} then U+{cp:04X}");
            prev = cp;
        }
    }

    #[test]
    fn pool_includes_space() {
        let set = SymbolSet::build();
        assert!(set.symbols().iter().any(|s| s.codepoint == ' '),
                "pool must include canonical blank U+0020");
    }

    #[test]
    fn popcount_is_consistent() {
        let set = SymbolSet::build();
        for s in set.symbols() {
            assert_eq!(s.popcount, s.bitmap.popcount(),
                       "cached popcount mismatch for U+{:04X}", s.codepoint as u32);
        }
    }
}
