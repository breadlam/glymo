//! Render a PNG to ANSI via glymo. The first visual checkpoint for the
//! matcher — useful for eye-checking glyph + colour choices against the
//! source image, and for diffing against other matchers (chafa, aalib,
//! half-block) on the same input.
//!
//! Usage:
//!   cargo run --release --example render_png -- INPUT.png COLS ROWS
//!
//! `COLS × ROWS` is the terminal cell grid to render into. The PNG is
//! box-averaged down to `COLS*4 × ROWS*8` sub-pixels, then each cell
//! matched independently. Output is truecolor ANSI (`38;2;…` / `48;2;…`)
//! to stdout; redirect to a file or `cat` it in a 24-bit-colour terminal.

use std::io::{BufWriter, Write};

use glymo::{match_cell, patches_from_rgb24, Repertoire, SymbolSet};
use png::ColorType;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: render_png INPUT.png COLS ROWS");
        std::process::exit(2);
    }
    let path = &args[1];
    let cols: usize = args[2].parse().expect("COLS must be a positive integer");
    let rows: usize = args[3].parse().expect("ROWS must be a positive integer");
    assert!(cols > 0 && rows > 0);

    // Decode PNG → RGB24. Strip alpha (composite onto black) and expand
    // greyscale to RGB so the matcher sees a uniform format.
    let file = std::fs::File::open(path).expect("open PNG");
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().expect("read PNG info");
    let info = reader.info().clone();
    let (src_w, src_h) = (info.width as usize, info.height as usize);
    let mut frame = vec![0u8; reader.output_buffer_size()];
    let oi = reader.next_frame(&mut frame).expect("decode PNG frame");

    let rgb24: Vec<u8> = match oi.color_type {
        ColorType::Rgb => frame[..oi.buffer_size()].to_vec(),
        ColorType::Rgba => {
            // Composite onto black so transparent regions don't leak
            // into Otsu's cluster means.
            let n = src_w * src_h;
            let mut v = Vec::with_capacity(n * 3);
            for px in frame[..oi.buffer_size()].chunks_exact(4) {
                let a = px[3] as u32;
                let blend = |c: u8| ((c as u32 * a) / 255) as u8;
                v.push(blend(px[0]));
                v.push(blend(px[1]));
                v.push(blend(px[2]));
            }
            v
        }
        ColorType::Grayscale => {
            let mut v = Vec::with_capacity(src_w * src_h * 3);
            for &g in &frame[..oi.buffer_size()] {
                v.extend_from_slice(&[g, g, g]);
            }
            v
        }
        ColorType::GrayscaleAlpha => {
            let mut v = Vec::with_capacity(src_w * src_h * 3);
            for px in frame[..oi.buffer_size()].chunks_exact(2) {
                let g = ((px[0] as u32 * px[1] as u32) / 255) as u8;
                v.extend_from_slice(&[g, g, g]);
            }
            v
        }
        other => {
            eprintln!("unsupported PNG colour type: {other:?}");
            std::process::exit(3);
        }
    };

    eprintln!("source: {src_w}×{src_h} px → grid {cols}×{rows} cells");

    // Pool: the conservative (block-family) repertoire — the only one
    // built so far. Adding octants / braille widens this later without
    // changing call shape.
    let pool = SymbolSet::build(Repertoire::CONSERVATIVE);
    eprintln!("pool: {} glyphs", pool.len());

    let t0 = std::time::Instant::now();
    let patches = patches_from_rgb24(&rgb24, src_w, src_h, cols, rows);
    let t_sample = t0.elapsed();

    let t1 = std::time::Instant::now();
    let matches: Vec<_> = patches.iter().map(|p| match_cell(&pool, p)).collect();
    let t_match = t1.elapsed();

    eprintln!(
        "downsample: {:.2} ms   match: {:.2} ms   cells: {}",
        t_sample.as_secs_f64() * 1000.0,
        t_match.as_secs_f64() * 1000.0,
        patches.len(),
    );

    // Emit row-major to stdout. Reset SGR at end of each row so the
    // user's terminal isn't left in a weird state if they ^C mid-output.
    let stdout = std::io::stdout().lock();
    let mut w = BufWriter::new(stdout);
    for row in 0..rows {
        for col in 0..cols {
            let m = matches[row * cols + col];
            // Truecolor SGR for fg + bg + the chosen codepoint. No SGR-
            // delta optimisation here — keep the example readable; the
            // server-side writer has its own SGR-delta path.
            write!(
                w,
                "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m{}",
                m.fg.0, m.fg.1, m.fg.2,
                m.bg.0, m.bg.1, m.bg.2,
                m.codepoint
            )
            .unwrap();
        }
        writeln!(w, "\x1b[0m").unwrap();
    }
}
