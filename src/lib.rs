//! glymo — Glyph Mosaic: a fast, deterministic terminal-cell matcher.
//!
//! Given a high-resolution RGB image patch and a pool of candidate Unicode
//! glyphs, glymo picks the `(codepoint, fg, bg)` triple that best
//! represents the patch as one terminal cell. The matcher is a **pure
//! function** of the patch (no temporal state), and its discrete-binary
//! representation gives it Lipschitz stability on noisy input: small
//! input perturbations produce small or zero output changes.
//!
//! # Algorithm
//!
//! Two decoupled stages (color stage and matcher land in subsequent
//! commits; this revision is the type / repertoire scaffolding):
//!
//! 1. **Color (Otsu).** Per-sub-pixel BT.709 luminance, Otsu's
//!    between-class-variance threshold, fg/bg as the cluster RGB means.
//! 2. **Glyph (Hamming).** Threshold each sub-pixel into a `u32` bitmap,
//!    find the pool member nearest in Hamming distance via `XOR + POPCNT`
//!    with popcount-class pruning over candidates.
//!
//! Sub-grid: **4 columns × 8 rows = 32 sub-pixels per cell**. Matches
//! typical terminal cell aspect (1:2 W:H) so each sub-pixel is roughly
//! square on screen.
//!
//! # Crate status
//!
//! Pre-release scaffolding. The [`Bitmap`], [`Symbol`], and
//! [`Repertoire`] types stabilise the data model. The block-family
//! repertoire lands first; octants / braille / sextants follow.

#![forbid(unsafe_code)]

pub mod bitmap;
pub mod repertoire;
pub mod symbol;

pub use bitmap::Bitmap;
pub use repertoire::{Repertoire, SymbolSet};
pub use symbol::Symbol;
