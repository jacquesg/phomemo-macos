//! CUPS raster filter for Phomemo D30 label printer.
//!
//! Reads CUPS RaS3 raster data from stdin, converts to the D30 ESC/POS
//! protocol, and writes binary output to stdout.  The D30 requires a
//! 90-degree counter-clockwise rotation before printing.

use std::io::{self, Read, Write};
use std::process;

use phomemo_filters::{parse_ras3, rotate_90_ccw, to_1bit};

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

        // Rotate 90° CCW, then threshold to 1-bit.
        let (rotated, rot_w, rot_h) = rotate_90_ccw(&page.data, page.width, page.height);
        let packed = to_1bit(&rotated, rot_w);

        let bytes_per_line = (rot_w.div_ceil(8)) as u16;
        let height = rot_h as u16;
        let feed_lines = page.advance_distance;

        // --- Init ---
        out.write_all(b"\x1f\x11\x24\x00")?;
        out.write_all(b"\x1b\x40")?; // ESC @

        // --- Raster: GS v 0 ---
        out.write_all(b"\x1dv0\x00")?;
        out.write_all(&bytes_per_line.to_le_bytes())?;
        out.write_all(&height.to_le_bytes())?;
        out.write_all(&packed)?;

        // --- Feed padding ---
        let padding_bytes = bytes_per_line as usize * feed_lines as usize;
        if padding_bytes > 0 {
            let zeros = vec![0u8; padding_bytes];
            out.write_all(&zeros)?;
        }
    }

    out.flush()?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: rastertopd30: {e}");
        process::exit(1);
    }
}
