//! CUPS raster filter for Phomemo M02, M02 Pro, M02S, and T02 printers.
//!
//! Reads CUPS RaS3 raster data from stdin, converts to the M02/T02 ESC/POS
//! protocol, and writes binary output to stdout.  The M02 print head is 384
//! dots (48 bytes) wide; images are resized to fit.

use std::io::{self, Read, Write};
use std::process;

use phomemo_filters::{parse_ras3, resize_nearest, to_1bit};

/// M02 print head width in pixels.
const PRINT_WIDTH: u32 = 384;

/// Maximum number of raster lines per GS v 0 block.
const MAX_BLOCK_LINES: usize = 256;

/// Bytes per raster line (384 / 8).
const BYTES_PER_LINE: u16 = 48; // 0x30

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

        // Resize to 384px wide if needed.
        let (mut grey, width, height) = if page.width != PRINT_WIDTH {
            let (resized, w, h) = resize_nearest(&page.data, page.width, page.height, PRINT_WIDTH);
            (resized, w, h)
        } else {
            (page.data.clone(), page.width, page.height)
        };

        // The M02 uses inverted bit polarity: bit 0 = print, bit 1 = no print.
        // Invert the grayscale data before 1-bit packing so that to_1bit()
        // (which sets bit 1 for dark pixels) produces the correct output.
        for pixel in grey.iter_mut() {
            *pixel = 255 - *pixel;
        }

        // Threshold to 1-bit.
        let packed = to_1bit(&grey, width);
        let height = height as usize;

        // --- Header ---
        out.write_all(b"\x1b\x40")?; // ESC @: initialise
        out.write_all(b"\x1b\x61\x01")?; // ESC a: centre justify
        out.write_all(b"\x1f\x11\x02\x04")?;

        // --- Raster blocks (max 256 lines each) ---
        let bpl = BYTES_PER_LINE as usize;
        let mut remaining = height;
        let mut line = 0;

        while remaining > 0 {
            let lines = remaining.min(MAX_BLOCK_LINES);

            // GS v 0
            out.write_all(b"\x1d\x76\x30\x00")?;
            out.write_all(&BYTES_PER_LINE.to_le_bytes())?;
            out.write_all(&((lines - 1) as u16).to_le_bytes())?;

            for row in line..line + lines {
                let row_start = row * bpl;
                for col in 0..bpl {
                    let mut byte = packed[row_start + col];
                    // 0x0a is interpreted as LineFeed by the printer — substitute.
                    if byte == 0x0a {
                        byte = 0x14;
                    }
                    out.write_all(&[byte])?;
                }
            }

            line += lines;
            remaining -= lines;
        }

        // --- Footer ---
        let feed = if page.advance_distance == 0 {
            2u8
        } else {
            page.advance_distance as u8
        };
        out.write_all(b"\x1b\x64")?;
        out.write_all(&[feed])?;
        out.write_all(b"\x1b\x64")?;
        out.write_all(&[feed])?;
        out.write_all(b"\x1f\x11\x08")?;
        out.write_all(b"\x1f\x11\x0e")?;
        out.write_all(b"\x1f\x11\x07")?;
        out.write_all(b"\x1f\x11\x09")?;
    }

    out.flush()?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: rastertopm02: {e}");
        process::exit(1);
    }
}
