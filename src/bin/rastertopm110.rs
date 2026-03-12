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

        // Physical page width (stock width) from the raster header.
        // Try the v2 float cupsPageSize first (precise), then fall back
        // to the base-header integer PageSize.  When both are zero the
        // rasteriser did not populate page geometry and we fall back to
        // cupsWidth (no centering).
        let page_bytes = if page.hw_res_x > 0 {
            if page.page_width_pts > 0.0 {
                // v2 float cupsPageSize — precise
                let dots = (page.page_width_pts as f64
                    * page.hw_res_x as f64
                    / 72.0
                    + 0.5) as usize;
                dots.div_ceil(8)
            } else if page.page_size_w > 0 {
                // Base-header integer PageSize — floor to avoid
                // rounding past a byte boundary.
                let dots = (page.page_size_w as usize * page.hw_res_x as usize) / 72;
                (dots / 8).max(src_bpl)
            } else {
                src_bpl
            }
        } else {
            src_bpl
        };

        // Total row width: at least the print head, but wider if the
        // page or raster exceeds it (e.g. M220 wide labels).
        let dst_bpl = HEAD_WIDTH_BYTES.max(page_bytes).max(src_bpl);

        // Right-align the stock on the print head, then centre the
        // raster data within the stock.
        let stock_pad = dst_bpl - page_bytes;
        let margin_left = page_bytes.saturating_sub(src_bpl) / 2;
        let pad_left = stock_pad + margin_left;
        let pad_right = dst_bpl - pad_left - src_bpl;

        // Vertical centering: add blank rows at the top of the raster
        // to push content down.  Only top padding (no bottom) to keep
        // the total height within the label stock bounds.
        let page_height_dots = if page.hw_res_x > 0 {
            if page.page_height_pts > 0.0 {
                (page.page_height_pts as f64 * page.hw_res_x as f64 / 72.0 + 0.5) as usize
            } else if page.page_size_h > 0 {
                (page.page_size_h as usize * page.hw_res_x as usize) / 72
            } else {
                page.height as usize
            }
        } else {
            page.height as usize
        };

        let content_h = page.height as usize;
        let top_pad = page_height_dots.saturating_sub(content_h) / 2;
        let total_height = content_h + top_pad;

        let left_zeros = vec![0u8; pad_left];
        let right_zeros = vec![0u8; pad_right];
        let blank_row = vec![0u8; dst_bpl];

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
        out.write_all(&(total_height as u16).to_le_bytes())?;

        // Top padding: blank rows to push content down
        for _ in 0..top_pad {
            out.write_all(&blank_row)?;
        }
        for row in 0..content_h {
            let start = row * src_bpl;
            let end = start + src_bpl;
            out.write_all(&left_zeros)?;
            out.write_all(&packed[start..end])?;
            out.write_all(&right_zeros)?;
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
