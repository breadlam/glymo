//! Symbol mode: 8×16 sub-grid, 128-bit signature matcher.
//!
//! This is the high-resolution sibling of the top-level (4×8) matcher.
//! Trade-off summary, established empirically against three reference
//! mono fonts and a detailed source frame:
//!
//! | Mode   | Sub-grid | Bits | Pool | Use case                  |
//! |--------|----------|------|------|---------------------------|
//! | Block  | 4×8      | 32   | ~250 | Clean geometric mosaic    |
//! | Symbol | **8×16** | 128  |~1300 | Textural, fine detail     |
//!
//! Symbol mode samples each terminal cell at 4× the resolution of
//! block mode, then matches against a pool of ~1300 unique letter,
//! geometric-shape, box-drawing, and CJK-halfwidth glyph signatures
//! pre-rendered offline from DejaVu Sans Mono + Noto Sans CJK and
//! checked in as a `const` table (see [`generated`]). The runtime
//! pool build is a `Vec::with_capacity` + copy — no font dep at use.
//!
//! Cell-width assumption (same as block mode): every glyph emitted is
//! width-1 in every common terminal. The block allowlist and General-
//! Category exclusions at pool-generation time guarantee this.
//!
//! Public surface mirrors block mode:
//! - [`Bitmap`] — 128-bit `[u64; 2]` sub-pixel signature
//! - [`Symbol`] — `(codepoint, bitmap, popcount)` glyph candidate
//! - [`SymbolSet`] — the runtime pool
//! - [`Patch`] — `[Rgb; 128]`, one cell's worth of source sub-pixels
//! - [`match_cell`] — patch → (codepoint, fg, bg)
//! - [`patches_from_rgb24`] — RGB image → grid of patches
//!
//! [`generated`]: crate::symbol_mode::generated

pub mod bitmap;
pub mod color;
pub mod generated;
pub mod matcher;
pub mod pool;
pub mod sample;
pub mod symbol;

pub use bitmap::Bitmap;
pub use color::Patch;
pub use matcher::{match_cell, Match};
pub use pool::SymbolSet;
pub use sample::patches_from_rgb24;
pub use symbol::Symbol;
