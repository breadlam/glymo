# Font survey (for the rasterization step)

*Originally part of [`bit-string-matching-research.md`](bit-string-matching-research.md); split out 2026-05-30. Captures the why behind glymo's reference-font choice for [`symbol_mode`](../src/symbol_mode/).*

The dedup pass needs bitmaps. glymo currently *synthesizes* them
(block elements from geometry, octants from position bitfields), but
full Unicode requires rasterizing each codepoint via a font — and we
don't control the user's terminal font. Survey of what's actually
deployed:

| Platform / client | Default mono font | License | Notes |
|---|---|---|---|
| macOS Terminal.app | Menlo | Apple proprietary | Can't vendor |
| macOS iTerm2 | Monaco → user-customised | Apple proprietary | Can't vendor |
| Windows Terminal | **Cascadia Mono** | SIL OFL | Vendor-OK |
| Windows legacy console | Consolas | MS proprietary | Can't vendor |
| Linux GNOME / Ubuntu | Ubuntu Mono (override) | Ubuntu Font License | Vendor-OK |
| Linux fontconfig fallback | **DejaVu Sans Mono** | mod. Bitstream Vera (MIT-like) | Vendor-OK |
| Fedora / most distros | DejaVu Sans Mono | (same) | Vendor-OK |
| iOS Blink Shell | Pragmata Pro | Commercial | Can't vendor; bundles many free |
| iOS Termius | user-pick from bundle | varies | Bundles DejaVu, Fira, JetBrains, Cascadia, Meslo, Source Code Pro, Ubuntu |
| Android Termux | Android system mono (DroidSansMono) | Apache 2.0 | Vendor-OK |
| Android JuiceSSH | built-in, no font picker | proprietary | – |

**License-clean, broadly-shipped candidates:** Cascadia Mono, DejaVu
Sans Mono, Liberation Mono, Source Code Pro, Fira Mono, JetBrains
Mono, Ubuntu Mono, Inconsolata, Hack, Iosevka — all SIL OFL or
equivalent.

## Why font choice matters less than expected

At 4×8 binary sub-pixels, almost all letterform detail is washed
out. Two same-codepoint glyphs from different fonts produce
**identical 32-bit signatures** more often than not, because the
binary signature only encodes "is there ink in each 2×2 zone."
Distinguishing fonts at 4×8 happens only at:

- **Stroke weight extremes** (Thin vs Black weights).
- **Cap height and x-height differences** (rare).
- **Letter-form quirks** that span multiple zones (e.g. single- vs
  double-storey lowercase `a`).

The shape that gets picked by the matcher is therefore stable across
fonts for the *vast majority* of codepoints; the user's terminal
then renders that codepoint with whatever font it has. The
rasterizer-time font choice is a *modelling* decision (what glyph
shapes are in glymo's universe), not a *rendering* decision (what
the user sees).

## Recommendation: DejaVu Sans Mono as the reference

Rationale:

1. **Most universal open mono font** — the fontconfig default on
   nearly every Linux distro, bundled in iOS Termius, deployable
   everywhere by license.
2. **3 341 Unicode characters** — broad enough that the pool sees
   real coverage; CJK is excluded by our BMP-width-1 scope anyway.
3. **License is MIT-ish** — derivative use including commercial OK;
   only naming restriction is "don't call your modified version
   Bitstream Vera" — we're not modifying, just rasterizing.
4. **~340 KB regular weight** — vendor in `glymo/assets/` once,
   reproducible across machines, no system-font dependency.
5. **Pre-Unicode-16** — does not cover the new octants block, which
   is *desired*: glymo already synthesizes octants procedurally,
   and a font-rendered version would mismatch the procedural one.

The reference-font choice is documented and revisitable: if real-
device testing shows the matcher picking codepoints that render as
tofu on common mobile clients, we can switch reference fonts or add
a "multi-font intersection" pass (only keep glyphs whose 4×8
signature is identical across N fonts → maximum cross-terminal
portability).

## Implementation plan

1. Vendor `DejaVuSansMono.ttf` into `glymo/assets/`.
2. Add `fontdue` (pure-Rust rasterizer, MIT) as a glymo dep.
3. New module `glymo::unicode_pool`:
   - Filter Unicode 16 BMP to (general-category ∈ {L,N,S,P}) ∧
     (East-Asian-width ∈ {N, Na, A}) ∧ ¬combining ∧ ¬private-use.
   - Rasterize each at a cell-pixel size large enough to capture
     detail (e.g. 16×32 px), then box-average + Otsu-threshold to
     4×8 binary. (Same downsample geometry as the runtime matcher's
     input pipeline.)
   - Dedup by signature into a `HashMap<u32, char>`; lowest
     codepoint wins on collision.
   - Merge with the existing procedural pools (`BLOCK`, `OCTANT`) —
     procedural entries win on signature collision because their
     geometry is exact.
4. Store result as a generated `glymo::unicode_pool::TABLE: &[(char,
   u32)]` const slice in a build-time generated `.rs` file
   (committed). Generation is via `cargo run --bin gen_unicode_pool`,
   not `build.rs` — keeps the runtime build fast and lets us inspect
   the generated table.
5. `Repertoire::UNICODE` flag in `repertoire.rs` to pull this pool
   in alongside / instead of `RICH`.
