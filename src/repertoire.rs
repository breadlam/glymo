//! Symbol repertoires: predefined pools of [`Symbol`]s.
//!
//! [`Repertoire`] is a bit-flag set selecting which glyph families to
//! include in a pool. [`SymbolSet::build`] materialises the pool,
//! deduplicating by bitmap (identical bitmaps collapse to the lowest
//! codepoint) and sorting by codepoint for deterministic iteration.

use crate::bitmap::{Bitmap, WIDTH};
use crate::symbol::Symbol;

/// Bit-flag set selecting which symbol families a pool includes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repertoire(pub u32);

impl Repertoire {
    /// `U+0020 SPACE` (empty bitmap).
    pub const SPACE: Repertoire = Repertoire(1 << 0);

    /// Block-family glyphs that fit cleanly at the 4×8 sub-grid:
    /// full block, four halves, six lower eighths (1/8…3/8 and
    /// 5/8…7/8 — 4/8 is the lower half), upper 1/8, and ten quadrant
    /// combinations (singles, diagonals, three-corners).
    ///
    /// Horizontal eighths (left 1/8…7/8 and right 1/8) are omitted —
    /// at 4 sub-pixel columns they would need fractional addressing.
    pub const BLOCK: Repertoire = Repertoire(1 << 1);

    /// All 256 Braille patterns (U+2800..U+28FF). Each Braille cell
    /// holds 8 dots in a 2×4 grid; each dot maps to a 2×2 zone of our
    /// sub-pixel grid. Decades-old, near-universal font support — the
    /// safe choice for "rich" pool support before octants are
    /// universally rendered.
    pub const BRAILLE: Repertoire = Repertoire(1 << 2);

    /// Block-only — narrow universal-support tier.
    pub const CONSERVATIVE: Repertoire = Repertoire(0b011);

    /// Block + Braille — broad font support with a ~270-glyph pool.
    pub const RICH: Repertoire = Repertoire(0b111);

    pub const fn contains(self, other: Repertoire) -> bool {
        (self.0 & other.0) == other.0
    }
}

impl std::ops::BitOr for Repertoire {
    type Output = Repertoire;
    fn bitor(self, rhs: Repertoire) -> Repertoire {
        Repertoire(self.0 | rhs.0)
    }
}

fn push_space(v: &mut Vec<Symbol>) {
    v.push(Symbol::new('\u{0020}', Bitmap::EMPTY));
}

fn push_block(v: &mut Vec<Symbol>) {
    // Full block.
    v.push(Symbol::new('\u{2588}', Bitmap::FULL));

    // Halves. Upper / lower split the 8 rows at row 4; left / right
    // split the 4 columns at col 2.
    v.push(Symbol::new('\u{2580}', Bitmap::from_rect(0, 0, 4, WIDTH))); // upper
    v.push(Symbol::new('\u{2584}', Bitmap::from_rect(4, 0, 8, WIDTH))); // lower
    v.push(Symbol::new('\u{258C}', Bitmap::from_rect(0, 0, 8, 2)));     // left
    v.push(Symbol::new('\u{2590}', Bitmap::from_rect(0, 2, 8, WIDTH))); // right

    // Lower k/8 for k ∈ {1,2,3,5,6,7}. k=4 is the lower half (U+2584)
    // already pushed above; skipping it avoids a bitmap duplicate.
    for (cp, k) in [
        (0x2581u32, 1usize),
        (0x2582, 2),
        (0x2583, 3),
        (0x2585, 5),
        (0x2586, 6),
        (0x2587, 7),
    ] {
        v.push(Symbol::from_u32(cp, Bitmap::from_rect(8 - k, 0, 8, WIDTH)));
    }

    // Upper 1/8 (U+2594 — top row only).
    v.push(Symbol::new('\u{2594}', Bitmap::from_rect(0, 0, 1, WIDTH)));

    // Quadrants. Each quadrant is a 4-row × 2-col rect; 4 singles,
    // 2 diagonals, 4 three-corners.
    let q_ul = Bitmap::from_rect(0, 0, 4, 2);
    let q_ur = Bitmap::from_rect(0, 2, 4, WIDTH);
    let q_ll = Bitmap::from_rect(4, 0, 8, 2);
    let q_lr = Bitmap::from_rect(4, 2, 8, WIDTH);

    v.push(Symbol::new('\u{2596}', q_ll));                                 // lower-left
    v.push(Symbol::new('\u{2597}', q_lr));                                 // lower-right
    v.push(Symbol::new('\u{2598}', q_ul));                                 // upper-left
    v.push(Symbol::new('\u{259D}', q_ur));                                 // upper-right
    v.push(Symbol::new('\u{259A}', q_ul.union(q_lr)));                     // UL+LR diagonal
    v.push(Symbol::new('\u{259E}', q_ur.union(q_ll)));                     // UR+LL diagonal
    v.push(Symbol::new('\u{2599}', q_ul.union(q_ll).union(q_lr)));         // missing UR
    v.push(Symbol::new('\u{259B}', q_ul.union(q_ur).union(q_ll)));         // missing LR
    v.push(Symbol::new('\u{259C}', q_ul.union(q_ur).union(q_lr)));         // missing LL
    v.push(Symbol::new('\u{259F}', q_ur.union(q_ll).union(q_lr)));         // missing UL
}

/// Braille codepoint convention: `U+2800 + dot_bitfield`, where
/// bits 0..7 map to dots 1..8. The dot layout within a Braille cell:
///
/// ```text
///   dot 1 | dot 4    ← top row
///   dot 2 | dot 5
///   dot 3 | dot 6
///   dot 7 | dot 8    ← bottom row
/// ```
///
/// Each dot occupies a 2×2 zone in our 4×8 sub-grid (8 zones × 4
/// sub-pixels each = 32 = TOTAL).
fn braille_dot_zone(dot: u8) -> Bitmap {
    // (zone_col, zone_row) — 2 cols of zones, 4 rows of zones.
    let (zc, zr) = match dot {
        1 => (0, 0),
        2 => (0, 1),
        3 => (0, 2),
        4 => (1, 0),
        5 => (1, 1),
        6 => (1, 2),
        7 => (0, 3),
        8 => (1, 3),
        _ => unreachable!("braille dot must be 1..=8"),
    };
    let r0 = zr * 2;
    let c0 = zc * 2;
    Bitmap::from_rect(r0, c0, r0 + 2, c0 + 2)
}

fn push_braille(v: &mut Vec<Symbol>) {
    // All 256 patterns. The dedup pass collapses any whose bitmaps match
    // an earlier (lower-codepoint) entry — e.g. the all-empty Braille
    // pattern (U+2800) collapses to SPACE (U+0020) if SPACE is in the
    // pool; the all-dots pattern (U+28FF) collapses to FULL BLOCK
    // (U+2588) if present. Patterns matching block-family bitmaps
    // (halves, quadrants, eighths) similarly defer to their lower-
    // codepoint block representative.
    for pattern in 0u32..256 {
        let mut bm = Bitmap::EMPTY;
        for bit in 0..8 {
            if pattern & (1 << bit) != 0 {
                bm = bm.union(braille_dot_zone((bit + 1) as u8));
            }
        }
        v.push(Symbol::from_u32(0x2800 + pattern, bm));
    }
}

/// A deduplicated, codepoint-sorted pool of glyphs. The matcher iterates
/// this; encoder-side pool pruning produces a `SymbolSet` covering only
/// the glyphs a particular content actually uses.
#[derive(Debug, Clone)]
pub struct SymbolSet {
    symbols: Vec<Symbol>,
}

impl SymbolSet {
    /// Materialise a pool for the requested families. Glyphs are
    /// deduplicated by bitmap (the lowest codepoint sharing a bitmap
    /// is the representative) and sorted ascending by codepoint, so
    /// the resulting set is bit-identical across runs.
    pub fn build(rep: Repertoire) -> Self {
        let mut all = Vec::new();
        if rep.contains(Repertoire::SPACE) {
            push_space(&mut all);
        }
        if rep.contains(Repertoire::BLOCK) {
            push_block(&mut all);
        }
        if rep.contains(Repertoire::BRAILLE) {
            push_braille(&mut all);
        }

        all.sort_by_key(|s| s.codepoint as u32);
        let mut symbols: Vec<Symbol> = Vec::with_capacity(all.len());
        for s in &all {
            if !symbols.iter().any(|t| t.bitmap == s.bitmap) {
                symbols.push(*s);
            }
        }
        SymbolSet { symbols }
    }

    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(set: &SymbolSet, cp: u32) -> &Symbol {
        set.symbols()
            .iter()
            .find(|s| s.codepoint as u32 == cp)
            .unwrap_or_else(|| panic!("U+{cp:04X} missing from pool"))
    }

    #[test]
    fn conservative_includes_space_and_full_block() {
        let set = SymbolSet::build(Repertoire::CONSERVATIVE);
        assert!(!set.is_empty());
        assert!(set.symbols().iter().any(|s| s.codepoint == ' '));
        assert!(set.symbols().iter().any(|s| s.codepoint == '\u{2588}'));
    }

    #[test]
    fn pool_is_strictly_codepoint_sorted() {
        let set = SymbolSet::build(Repertoire::CONSERVATIVE);
        let mut prev: u32 = 0;
        for s in set.symbols() {
            let cp = s.codepoint as u32;
            assert!(cp > prev, "not ascending: U+{prev:04X} then U+{cp:04X}");
            prev = cp;
        }
    }

    #[test]
    fn dedup_yields_unique_bitmaps() {
        let set = SymbolSet::build(Repertoire::CONSERVATIVE);
        let mut seen = std::collections::HashSet::new();
        for s in set.symbols() {
            assert!(
                seen.insert(s.bitmap.0),
                "duplicate bitmap 0x{:08x} survived dedup",
                s.bitmap.0
            );
        }
    }

    #[test]
    fn block_family_popcounts_match_geometry() {
        let set = SymbolSet::build(Repertoire::CONSERVATIVE);
        let pc = |cp: u32| lookup(&set, cp).popcount;

        // Known truths at 4×8 sub-grid.
        assert_eq!(pc(0x0020), 0, "space empty");
        assert_eq!(pc(0x2588), 32, "full block = all 32 sub-pixels");
        assert_eq!(pc(0x2580), 16, "upper half = top 4 rows × 4 cols");
        assert_eq!(pc(0x2584), 16, "lower half");
        assert_eq!(pc(0x258C), 16, "left half = 8 rows × 2 cols");
        assert_eq!(pc(0x2590), 16, "right half");
        assert_eq!(pc(0x2581), 4, "lower 1/8 = 1 row × 4 cols");
        assert_eq!(pc(0x2587), 28, "lower 7/8 = 7 rows × 4 cols");
        assert_eq!(pc(0x2594), 4, "upper 1/8 = 1 row × 4 cols");
        assert_eq!(pc(0x2596), 8, "lower-left quadrant = 4 rows × 2 cols");
        assert_eq!(pc(0x259E), 16, "UR + LL diagonal pair");
        assert_eq!(pc(0x2599), 24, "three corners, missing UR");
    }

    #[test]
    fn halves_compose_to_full_block() {
        let set = SymbolSet::build(Repertoire::CONSERVATIVE);
        let upper = lookup(&set, 0x2580).bitmap;
        let lower = lookup(&set, 0x2584).bitmap;
        assert_eq!(upper.union(lower), Bitmap::FULL);
        let left = lookup(&set, 0x258C).bitmap;
        let right = lookup(&set, 0x2590).bitmap;
        assert_eq!(left.union(right), Bitmap::FULL);
    }

    // ─── Braille tests ─────────────────────────────────────────────────

    #[test]
    fn rich_pool_grows_over_conservative() {
        let conservative = SymbolSet::build(Repertoire::CONSERVATIVE);
        let rich = SymbolSet::build(Repertoire::RICH);
        assert!(
            rich.len() > conservative.len(),
            "RICH must add glyphs over CONSERVATIVE"
        );
        // Every conservative-pool member must still appear in rich (its
        // codepoint is lower, so it wins dedup).
        for s in conservative.symbols() {
            assert!(
                rich.symbols().iter().any(|t| t.codepoint == s.codepoint),
                "conservative glyph U+{:04X} dropped from RICH pool",
                s.codepoint as u32
            );
        }
    }

    #[test]
    fn braille_dot_zone_popcount() {
        // Each dot covers a 2×2 zone = 4 sub-pixels.
        for dot in 1..=8u8 {
            assert_eq!(braille_dot_zone(dot).popcount(), 4, "dot {dot}");
        }
    }

    #[test]
    fn braille_dots_partition_the_cell() {
        // All 8 dots together cover the entire 4×8 grid exactly once.
        let mut total = Bitmap::EMPTY;
        for dot in 1..=8u8 {
            let z = braille_dot_zone(dot);
            // No overlap with previously-seen dots.
            assert_eq!(z.0 & total.0, 0, "dot {dot} overlaps prior dots");
            total = total.union(z);
        }
        assert_eq!(total, Bitmap::FULL, "8 dots must tile the cell");
    }

    #[test]
    fn braille_codepoints_match_unicode_convention() {
        // U+2800 = no dots = empty bitmap.
        // U+2801 = dot 1 only = top-left 2×2 zone.
        // U+28FF = all 8 dots = full cell.
        let rich = SymbolSet::build(Repertoire::RICH);
        // U+2800 (empty Braille) should dedup AWAY in favour of SPACE.
        assert!(
            !rich.symbols().iter().any(|s| s.codepoint as u32 == 0x2800),
            "U+2800 (empty Braille) must dedup to SPACE U+0020"
        );
        // U+28FF (all dots) should dedup to FULL BLOCK U+2588.
        assert!(
            !rich.symbols().iter().any(|s| s.codepoint as u32 == 0x28FF),
            "U+28FF (all dots) must dedup to FULL BLOCK U+2588"
        );
        // U+2801 (dot 1 only) is a unique pattern — must survive.
        let s = lookup(&rich, 0x2801);
        assert_eq!(s.popcount, 4, "single Braille dot = 4 sub-pixels");
        // Its bitmap is the top-left 2×2 zone.
        assert_eq!(s.bitmap, Bitmap::from_rect(0, 0, 2, 2));
    }

    #[test]
    fn rich_pool_size_in_expected_range() {
        // 23 (block + space, deduped) + 256 (Braille) − (collisions) =
        // roughly 270–280. Assert a sane band, not an exact count, so the
        // dedup logic can evolve without churn.
        let n = SymbolSet::build(Repertoire::RICH).len();
        assert!(
            n >= 260 && n <= 280,
            "RICH pool size = {n}, expected 260..280"
        );
    }
}
