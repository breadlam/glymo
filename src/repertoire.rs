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

    /// All 256 Braille patterns (U+2800..U+28FF). Each cell holds 8
    /// dots in a 2×4 grid. **Note:** this revision's bitmap model for
    /// braille is wrong — each dot is mapped to a full 2×2 sub-pixel
    /// zone, but terminals render dots small with whitespace around
    /// them. Reasonable matching surface is sparse (~1 sub-pixel per
    /// dot at a fixed position within the zone). Kept in for testing /
    /// future fix; not in the default RICH pool.
    pub const BRAILLE: Repertoire = Repertoire(1 << 2);

    /// 230 Unicode 16 octants (U+1CD00..U+1CDE5). Each octant fills
    /// solid 2×2 sub-pixel zones in a 2×4 grid — matches the matcher's
    /// bitmap model directly. The 26 patterns NOT encoded here are
    /// already in [`BLOCK`] (full, halves, quadrants, diagonals, three-
    /// corner, lower 1/4 and 3/4) or are unencoded by Unicode for
    /// reasons internal to the standard ({3,5}, {4,6}, {7}, {8},
    /// {1,2,3,4,5,6}). Together with `BLOCK`, this covers all
    /// 256 octant patterns minus a small handful of mystery omissions.
    pub const OCTANT: Repertoire = Repertoire(1 << 3);

    /// Block-only — narrow universal-support tier.
    pub const CONSERVATIVE: Repertoire = Repertoire(0b0011);

    /// Block + Octant — the default rich pool. ~250 deduped glyphs.
    /// Requires a terminal that renders Unicode 16 (Sep 2024) octants;
    /// recent kitty / foot / wezterm / ghostty / iTerm2 work, mobile
    /// is patchier. Falls back to nearest block-element on un-rendered
    /// codepoints (the client sees tofu, but for matching purposes
    /// every patch still resolves).
    pub const RICH: Repertoire = Repertoire(0b1011);

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

/// Octant 2×4 grid → 4×8 sub-pixel zone, Z-order numbering
/// (1,2 top row; 3,4 second; 5,6 third; 7,8 bottom). Each position
/// occupies a 2×2 zone of sub-pixels.
fn octant_zone(position: u8) -> Bitmap {
    let n = (position - 1) as usize;
    let zone_col = n & 1;
    let zone_row = n >> 1;
    let r0 = zone_row * 2;
    let c0 = zone_col * 2;
    Bitmap::from_rect(r0, c0, r0 + 2, c0 + 2)
}

/// Octant position-bitfields indexed by `(codepoint - U+1CD00)`. Each
/// byte's bit N-1 = position N filled. Generated from the official
/// Unicode 16 "BLOCK OCTANT-N" naming convention. 230 entries
/// covering U+1CD00..U+1CDE5. Patterns not encoded here are either
/// in BLOCK (halves, quadrants, diagonals, three-corners, lower
/// 1/4 and 3/4, plus SPACE / FULL) or omitted by Unicode for
/// internal reasons ({3,5}, {4,6}, {7} alone, {8} alone,
/// {1,2,3,4,5,6}).
const OCTANT_PATTERNS: [u8; 230] = [
    // U+1CD00..U+1CD02 — max position 3 (3 entries)
    0x04, 0x06, 0x07,
    // U+1CD03..U+1CD08 — max position 4 (6 entries)
    0x08, 0x09, 0x0B, 0x0C, 0x0D, 0x0E,
    // U+1CD09..U+1CD17 — max position 5 (15 entries; {3,5}=0x14 omitted)
    0x10, 0x11, 0x12, 0x13, 0x15, 0x16, 0x17,
    0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F,
    // U+1CD18..U+1CD35 — max position 6 (30 entries; {4,6}=0x28 and {1..6}=0x3F omitted)
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F,
    0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
    0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x3E,
    // U+1CD36..U+1CD70 — max position 7 (59 entries; {7}=0x40 omitted,
    // plus LL-quad/left-half/UR+LL-diag/3-corner-missing-LR exclusions)
    0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47,
    0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F,
    0x51, 0x52, 0x53, 0x54, 0x56, 0x57,
    0x58, 0x59, 0x5B, 0x5C, 0x5D, 0x5E,
    0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67,
    0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F,
    0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77,
    0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F,
    // U+1CD71..U+1CDE5 — max position 8 (117 entries; {8}=0x80 omitted,
    // plus LR-quad/right-half/UL+LR-diag/3-corner-missing-{LL,UR,UL}/
    // lower-{1/4,1/2,3/4}/FULL exclusions)
    0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F,
    0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
    0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D, 0x9E, 0x9F,
    0xA1, 0xA2, 0xA3, 0xA4, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAB, 0xAC, 0xAD, 0xAE,
    0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7,
    0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF,
    0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7,
    0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF,
    0xD0, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7,
    0xD8, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF,
    0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7,
    0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF,
    0xF1, 0xF2, 0xF3, 0xF4, 0xF6, 0xF7,
    0xF8, 0xF9, 0xFB, 0xFD, 0xFE,
];

fn push_octants(v: &mut Vec<Symbol>) {
    for (offset, &pattern) in OCTANT_PATTERNS.iter().enumerate() {
        let mut bm = Bitmap::EMPTY;
        for bit in 0..8u8 {
            if pattern & (1 << bit) != 0 {
                bm = bm.union(octant_zone(bit + 1));
            }
        }
        let codepoint = 0x1CD00 + offset as u32;
        v.push(Symbol::from_u32(codepoint, bm));
    }
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
        if rep.contains(Repertoire::OCTANT) {
            push_octants(&mut all);
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

    // ─── Octant tests ──────────────────────────────────────────────────

    #[test]
    fn octant_zone_geometry() {
        // Each octant zone covers a 2×2 sub-pixel block, 8 zones tile
        // the 4×8 cell exactly with no overlap.
        let mut total = Bitmap::EMPTY;
        for pos in 1..=8u8 {
            let z = octant_zone(pos);
            assert_eq!(z.popcount(), 4, "position {pos} = 2×2 = 4 sub-pixels");
            assert_eq!(z.0 & total.0, 0, "position {pos} overlaps prior");
            total = total.union(z);
        }
        assert_eq!(total, Bitmap::FULL, "8 octant zones must tile the cell");
    }

    #[test]
    fn octant_table_has_230_entries() {
        assert_eq!(OCTANT_PATTERNS.len(), 230);
    }

    #[test]
    fn octant_patterns_match_block_element_exclusions() {
        // Every byte in OCTANT_PATTERNS must be a pattern that DOESN'T
        // correspond to an existing block-element bitmap. Spot-check the
        // 18 forbidden patterns are absent.
        let forbidden: &[(u8, &str)] = &[
            (0x00, "empty = SPACE"),
            (0xFF, "full = FULL BLOCK"),
            (0x0F, "{1,2,3,4} = UPPER HALF"),
            (0xF0, "{5,6,7,8} = LOWER HALF"),
            (0x55, "{1,3,5,7} = LEFT HALF"),
            (0xAA, "{2,4,6,8} = RIGHT HALF"),
            (0x05, "{1,3} = UL QUAD"),
            (0x0A, "{2,4} = UR QUAD"),
            (0x50, "{5,7} = LL QUAD"),
            (0xA0, "{6,8} = LR QUAD"),
            (0xA5, "UL+LR diagonal"),
            (0x5A, "UR+LL diagonal"),
            (0xF5, "3-corner missing UR"),
            (0x5F, "3-corner missing LR"),
            (0xAF, "3-corner missing LL"),
            (0xFA, "3-corner missing UL"),
            (0xC0, "{7,8} = LOWER 1/4"),
            (0xFC, "{3,4,5,6,7,8} = LOWER 3/4"),
        ];
        for &(pat, label) in forbidden {
            assert!(
                !OCTANT_PATTERNS.contains(&pat),
                "octant table must not contain 0x{pat:02X} ({label})"
            );
        }
    }

    #[test]
    fn octant_codepoint_zero_picks_position_3() {
        // U+1CD00 is the first octant ("Block Octant-3") — only position
        // 3 should be lit. Position 3 in Z-order = (col 0, row 1) =
        // sub-rows 2-3, sub-cols 0-1.
        let pool = SymbolSet::build(Repertoire::OCTANT | Repertoire::SPACE);
        let s = lookup(&pool, 0x1CD00);
        assert_eq!(s.bitmap, Bitmap::from_rect(2, 0, 4, 2));
        assert_eq!(s.popcount, 4);
    }

    #[test]
    fn octant_block_combination_fills_pool() {
        // BLOCK + OCTANT together should cover all 256 octant patterns
        // (BLOCK provides the patterns octants skip — the 18 exclusions
        // plus SPACE plus FULL) MINUS the 5 mystery patterns Unicode
        // chose not to encode anywhere. So 256 − 5 = 251 unique bitmaps.
        let pool = SymbolSet::build(Repertoire::SPACE | Repertoire::BLOCK | Repertoire::OCTANT);
        // 230 octants + 21 block elements (deduped) = 251, matching.
        assert!(
            pool.len() >= 248 && pool.len() <= 254,
            "BLOCK+OCTANT pool size = {}, expected ~251",
            pool.len()
        );
    }

    #[test]
    fn rich_default_uses_octants_not_braille() {
        let rich = SymbolSet::build(Repertoire::RICH);
        // Octant codepoints should be present.
        assert!(
            rich.symbols().iter().any(|s| s.codepoint == '\u{1CD00}'),
            "RICH pool must include octants"
        );
        // Braille codepoints should NOT be present (BRAILLE flag not
        // in RICH; deferred until bitmap model is fixed).
        assert!(
            !rich.symbols().iter().any(|s| (s.codepoint as u32) & 0xFFFFFF00 == 0x2800),
            "RICH pool must not include Braille (deferred)"
        );
    }

    // ─── Braille tests (using explicit flag, not RICH default) ─────────

    #[test]
    fn braille_dot_zone_popcount() {
        for dot in 1..=8u8 {
            assert_eq!(braille_dot_zone(dot).popcount(), 4, "dot {dot}");
        }
    }

    #[test]
    fn braille_dots_partition_the_cell() {
        let mut total = Bitmap::EMPTY;
        for dot in 1..=8u8 {
            let z = braille_dot_zone(dot);
            assert_eq!(z.0 & total.0, 0, "dot {dot} overlaps prior dots");
            total = total.union(z);
        }
        assert_eq!(total, Bitmap::FULL, "8 dots must tile the cell");
    }

    #[test]
    fn braille_codepoints_match_unicode_convention() {
        let pool = SymbolSet::build(Repertoire::SPACE | Repertoire::BLOCK | Repertoire::BRAILLE);
        // U+2800 (empty) dedups to SPACE.
        assert!(!pool.symbols().iter().any(|s| s.codepoint as u32 == 0x2800));
        // U+28FF (all dots) dedups to FULL BLOCK.
        assert!(!pool.symbols().iter().any(|s| s.codepoint as u32 == 0x28FF));
        // U+2801 (dot 1 only) survives as a unique pattern at this
        // (current-but-known-wrong) bitmap model. Top-left 2×2 zone.
        let s = lookup(&pool, 0x2801);
        assert_eq!(s.popcount, 4);
        assert_eq!(s.bitmap, Bitmap::from_rect(0, 0, 2, 2));
    }
}
