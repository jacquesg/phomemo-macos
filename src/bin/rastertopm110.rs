//! CUPS raster filter for Phomemo M110, M120, M220, and M421 printers.
//!
//! Reads CUPS RaS3 raster data from stdin, converts to the M110 ESC/POS
//! protocol, and writes binary output to stdout.

use std::io::{self, Read, Write};
use std::process;

use phomemo_filters::{parse_ras3, to_1bit};

/// M110/M120 print head width in dots (48mm @ 203 DPI).
const HEAD_WIDTH_DOTS: usize = 384;

/// Print head width in bytes (384 / 8).
const HEAD_WIDTH_BYTES: usize = HEAD_WIDTH_DOTS / 8; // 48

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;

    let pages = parse_ras3(&input)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for page in &pages {
        if page.colour_space != 0 || page.num_colours != 1 {
            return Err("invalid colour space: only monochrome supported".into());
        }

        // Threshold to 1-bit: black (≤127) → bit 1 (print), white (>127) → bit 0.
        let packed = to_1bit(&page.data, page.width);
        let src_bpl = page.width.div_ceil(8) as usize;

        // Pad each raster line to the full print head width.  The label
        // paper is right-aligned against the print head, so all slack
        // goes to left padding.
        let dst_bpl = HEAD_WIDTH_BYTES.max(src_bpl);
        let pad_left = dst_bpl - src_bpl;
        let left_zeros = vec![0u8; pad_left];

        let height = page.height as u16;

        // --- Header ---
        // Speed
        out.write_all(b"\x1b\x4e\x0d\x05")?;
        // Density
        out.write_all(b"\x1b\x4e\x04\x0a")?;
        // Media type
        out.write_all(b"\x1f\x11")?;
        out.write_all(&[page.media_type as u8])?;

        // --- Raster: GS v 0 ---
        out.write_all(b"\x1dv0\x00")?;
        out.write_all(&(dst_bpl as u16).to_le_bytes())?;
        out.write_all(&height.to_le_bytes())?;

        for row in 0..page.height as usize {
            let start = row * src_bpl;
            let end = start + src_bpl;
            out.write_all(&left_zeros)?;
            out.write_all(&packed[start..end])?;
        }

        // --- Footer ---
        out.write_all(b"\x1f\xf0\x05\x00")?;
        out.write_all(b"\x1f\xf0\x03\x00")?;
    }

    out.flush()?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: rastertopm110: {e}");
        process::exit(1);
    }
}
