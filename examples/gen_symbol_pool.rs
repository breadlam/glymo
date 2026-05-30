//! Symbol-mode pool generator (offline, developer-run).
//!
//! Rasterises a curated set of Unicode codepoints via fontdue at the
//! 8×16 symbol-mode sub-grid, downsamples each to a 128-bit signature,
//! deduplicates by signature, and writes the result to a generated
//! Rust source file that the runtime crate compiles as a const.
//!
//! The runtime symbol-mode matcher uses this const directly — glymo
//! has no fontdue dependency at runtime.
//!
//! Filter chain (terminal-safe across the popular client matrix):
//!
//! 1. **Block allowlist.** Only codepoints from blocks every common
//!    terminal renders the same way (Latin/Greek/Cyrillic via DejaVu,
//!    Halfwidth Forms via Noto CJK).
//! 2. **Width.** `unicode-width` must agree the codepoint is width-1
//!    in both CJK and non-CJK locales — no Ambiguous-width.
//! 3. **General Category.** Excludes Lm (Modifier Letter), Sk (Modifier
//!    Symbol) and all marks (Mn/Mc/Me). Those categories render at
//!    unpredictable widths or as fallback combining marks.
//! 4. **Ink density.** Glyphs with under 10% lit sub-pixels are tiny
//!    visual marks that the matcher would pick for blank patches and
//!    terminals render unpredictably; the canonical blank `U+0020` is
//!    forced in instead.
//!
//! Usage:
//!   cargo run --release --example gen_symbol_pool -- \
//!       --dejavu /path/to/DejaVuSansMono.ttf \
//!       --cjk    /path/to/NotoSansCJK-Regular.ttc \
//!       --output src/symbol_mode/generated.rs

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use fontdue::{Font, FontSettings};
use unicode_categories::UnicodeCategories;
use unicode_width::UnicodeWidthChar;

const COLS: usize = 8;
const ROWS: usize = 16;
const RASTER_SCALE: usize = 16;
const MIN_INK_DENSITY: f32 = 0.10;

/// Blocks rendered via DejaVu Sans Mono. Every codepoint here is
/// width-1 in every common terminal and DejaVu draws it directly.
const DEJAVU_BLOCKS: &[(u32, u32)] = &[
    (0x0020, 0x007E), // Basic Latin printable
    (0x00A0, 0x00FF), // Latin-1 Supplement (printable)
    (0x0100, 0x017F), // Latin Extended-A
    (0x0180, 0x024F), // Latin Extended-B
    (0x0250, 0x02AF), // IPA Extensions
    (0x0370, 0x03FF), // Greek (basic)
    (0x0400, 0x04FF), // Cyrillic
    (0x0500, 0x052F), // Cyrillic Supplement
    (0x1E00, 0x1EFF), // Latin Extended Additional (precomposed Latin+diacritic)
    (0x1F00, 0x1FFF), // Greek Extended (precomposed Greek+diacritic)
    (0x2500, 0x257F), // Box Drawing
    (0x2580, 0x259F), // Block Elements
    (0x25A0, 0x25FF), // Geometric Shapes
    (0x2C60, 0x2C7F), // Latin Extended-C
    (0xA720, 0xA7FF), // Latin Extended-D
];

/// Blocks rendered via Noto Sans CJK. DejaVu has no glyphs for these;
/// per UAX-11 they are EAW=Narrow or EAW=Halfwidth and render as one
/// cell wide in every terminal regardless of locale.
const CJK_BLOCKS: &[(u32, u32)] = &[
    (0xFE50, 0xFE6F), // Small Form Variants
    (0xFF61, 0xFF9F), // Halfwidth Katakana
    (0xFFA0, 0xFFDC), // Halfwidth Hangul Jamo
    (0xFFE8, 0xFFEE), // Halfwidth Symbol Variants
];

fn in_blocks(c: u32, blocks: &[(u32, u32)]) -> bool {
    blocks.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
}

fn safe(c: char, blocks: &[(u32, u32)]) -> bool {
    let cp = c as u32;
    if !in_blocks(cp, blocks) { return false; }
    if cp == 0x7F { return false; }
    let Some(w) = c.width() else { return false };
    let Some(wc) = c.width_cjk() else { return false };
    if w != 1 || wc != 1 { return false; }
    if c.is_letter_modifier()
        || c.is_symbol_modifier()
        || c.is_mark_nonspacing()
        || c.is_mark_spacing_combining()
        || c.is_mark_enclosing()
    {
        return false;
    }
    true
}

fn render_glyph(font: &Font, glyph: char) -> Vec<u8> {
    let cell_w = COLS * RASTER_SCALE;
    let cell_h = ROWS * RASTER_SCALE;
    let mut cell = vec![0u8; cell_w * cell_h];
    let probe_px = cell_h as f32;
    let lm_probe = font.horizontal_line_metrics(probe_px).unwrap();
    let probe_total = (lm_probe.ascent - lm_probe.descent).max(1.0);
    let px = probe_px * (cell_h as f32) / probe_total;
    let lm = font.horizontal_line_metrics(px).unwrap();
    let baseline = lm.ascent.round() as i32;
    let (metrics, bitmap) = font.rasterize(glyph, px);
    if metrics.width == 0 || metrics.height == 0 { return cell; }
    let x_off = (cell_w as i32 - metrics.width as i32) / 2;
    let y_off = baseline - (metrics.ymin + metrics.height as i32);
    for gy in 0..metrics.height {
        let cy = y_off + gy as i32;
        if cy < 0 || cy as usize >= cell_h { continue; }
        for gx in 0..metrics.width {
            let cx = x_off + gx as i32;
            if cx < 0 || cx as usize >= cell_w { continue; }
            cell[cy as usize * cell_w + cx as usize] = bitmap[gy * metrics.width + gx];
        }
    }
    cell
}

/// Box-average + mean-threshold the rasterised cell to a 128-bit
/// signature. Returns `(lo_word, hi_word)` packed bit `i` =
/// sub-pixel `(row=i/COLS, col=i%COLS)`.
fn downsample_signature(cell: &[u8]) -> (u64, u64) {
    let cell_w = COLS * RASTER_SCALE;
    let cell_h = ROWS * RASTER_SCALE;
    let bin_w = cell_w / COLS;
    let bin_h = cell_h / ROWS;
    let total = COLS * ROWS;
    let mut down = vec![0u32; total];
    for r in 0..ROWS {
        for c in 0..COLS {
            let mut sum = 0u32;
            for dy in 0..bin_h {
                for dx in 0..bin_w {
                    sum += cell[(r * bin_h + dy) * cell_w + (c * bin_w + dx)] as u32;
                }
            }
            down[r * COLS + c] = sum / (bin_w * bin_h) as u32;
        }
    }
    let mean: u32 = down.iter().sum::<u32>() / down.len() as u32;
    let mut lo = 0u64;
    let mut hi = 0u64;
    for (i, &v) in down.iter().enumerate() {
        if v > mean {
            if i < 64 { lo |= 1u64 << i; } else { hi |= 1u64 << (i - 64); }
        }
    }
    (lo, hi)
}

struct Args {
    dejavu: PathBuf,
    cjk: PathBuf,
    output: PathBuf,
}

fn parse_args() -> Args {
    let mut dejavu = None;
    let mut cjk = None;
    let mut output = None;
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--dejavu" => { dejavu = Some(PathBuf::from(&argv[i + 1])); i += 2; }
            "--cjk"    => { cjk = Some(PathBuf::from(&argv[i + 1]));    i += 2; }
            "--output" => { output = Some(PathBuf::from(&argv[i + 1])); i += 2; }
            other => panic!("unknown arg: {other}"),
        }
    }
    Args {
        dejavu: dejavu.expect("--dejavu PATH required"),
        cjk:    cjk.expect("--cjk PATH required"),
        output: output.expect("--output PATH required"),
    }
}

fn main() {
    let args = parse_args();
    eprintln!("Loading fonts…");
    let dejavu_bytes = fs::read(&args.dejavu)
        .unwrap_or_else(|e| panic!("read {}: {e}", args.dejavu.display()));
    let dejavu = Font::from_bytes(dejavu_bytes.as_slice(), FontSettings::default())
        .expect("parse DejaVu");
    let cjk_bytes = fs::read(&args.cjk)
        .unwrap_or_else(|e| panic!("read {}: {e}", args.cjk.display()));
    let cjk = Font::from_bytes(
        cjk_bytes.as_slice(),
        FontSettings { scale: 40.0, collection_index: 2, ..FontSettings::default() },
    )
    .expect("parse Noto CJK");

    let total_bits = (COLS * ROWS) as f32;
    let mut seen: HashMap<(u64, u64), char> = HashMap::new();

    // Canonical blank — guarantees blank patches resolve to U+0020.
    seen.insert((0, 0), ' ');

    let mut try_add = |c: char, font: &Font, blocks: &[(u32, u32)]| -> Option<()> {
        if !safe(c, blocks) { return None; }
        if font.lookup_glyph_index(c) == 0 { return None; }
        let cell = render_glyph(font, c);
        let sig = downsample_signature(&cell);
        let popcount = sig.0.count_ones() + sig.1.count_ones();
        let density = popcount as f32 / total_bits;
        if density < MIN_INK_DENSITY { return None; }
        seen.entry(sig).or_insert(c);
        Some(())
    };

    eprintln!("Rasterising DejaVu blocks…");
    for cp in 0x21u32..=0xA7FF {
        if let Some(c) = char::from_u32(cp) {
            try_add(c, &dejavu, DEJAVU_BLOCKS);
        }
    }
    eprintln!("Rasterising Noto CJK halfwidth blocks…");
    for cp in 0xFE50u32..=0xFFEE {
        if let Some(c) = char::from_u32(cp) {
            try_add(c, &cjk, CJK_BLOCKS);
        }
    }

    let mut entries: Vec<(char, u64, u64, u32)> = seen
        .into_iter()
        .map(|(sig, c)| {
            let pc = sig.0.count_ones() + sig.1.count_ones();
            (c, sig.0, sig.1, pc)
        })
        .collect();
    entries.sort_by_key(|(c, ..)| *c as u32);
    eprintln!("Unique signatures: {}", entries.len());

    eprintln!("Writing {}", args.output.display());
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent).ok();
    }
    let f = fs::File::create(&args.output).expect("create output");
    let mut w = std::io::BufWriter::new(f);
    writeln!(w, "// AUTO-GENERATED by `cargo run --release --example gen_symbol_pool`.").unwrap();
    writeln!(w, "// Do not edit by hand. Re-run the generator to refresh.").unwrap();
    writeln!(w, "//").unwrap();
    writeln!(w, "// Pool generated from:").unwrap();
    writeln!(w, "//   DejaVu Sans Mono blocks: Basic Latin, Latin-1, Latin Ext A/B/C/D,").unwrap();
    writeln!(w, "//     IPA Ext, Greek, Greek Ext, Cyrillic, Cyrillic Supp,").unwrap();
    writeln!(w, "//     Latin Ext Add, Box Drawing, Block Elements, Geometric Shapes").unwrap();
    writeln!(w, "//   Noto Sans CJK blocks:    Small Form Variants, Halfwidth Katakana,").unwrap();
    writeln!(w, "//     Halfwidth Hangul Jamo, Halfwidth Symbol Variants").unwrap();
    writeln!(w, "//").unwrap();
    writeln!(w, "// Filtered by Unicode width (==1 in both locales) and General Category").unwrap();
    writeln!(w, "// (no Lm/Sk/Mn/Mc/Me), with a 10%% ink-density floor.").unwrap();
    writeln!(w).unwrap();
    writeln!(w, "/// Sub-grid columns per terminal cell.").unwrap();
    writeln!(w, "pub const COLS: usize = {};", COLS).unwrap();
    writeln!(w, "/// Sub-grid rows per terminal cell.").unwrap();
    writeln!(w, "pub const ROWS: usize = {};", ROWS).unwrap();
    writeln!(w, "/// Total sub-pixels (signature width in bits).").unwrap();
    writeln!(w, "pub const TOTAL: usize = COLS * ROWS;").unwrap();
    writeln!(w).unwrap();
    writeln!(w, "/// One entry: `(codepoint, bitmap_lo, bitmap_hi, popcount)`.").unwrap();
    writeln!(w, "///").unwrap();
    writeln!(w, "/// Bitmap bit `i` set iff sub-pixel `(row=i/COLS, col=i%COLS)` is").unwrap();
    writeln!(w, "/// foreground. Bits 0..63 live in `bitmap_lo`, 64..127 in `bitmap_hi`.").unwrap();
    writeln!(w, "/// Sorted ascending by codepoint for deterministic iteration.").unwrap();
    writeln!(w, "pub const SYMBOL_POOL: &[(u32, u64, u64, u32)] = &[").unwrap();
    for (c, lo, hi, pc) in &entries {
        writeln!(w, "    (0x{:04X}, 0x{:016X}, 0x{:016X}, {}),", *c as u32, lo, hi, pc).unwrap();
    }
    writeln!(w, "];").unwrap();
    w.flush().unwrap();
    eprintln!("Done.");
}
