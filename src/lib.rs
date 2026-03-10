//! Shared library for Phomemo CUPS raster filters.
//!
//! Provides CUPS RaS3 raster parsing and pure-Rust image processing
//! routines (threshold, resize, rotate) with zero external dependencies.

use std::fmt;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of a CUPS raster page header in bytes.
const HEADER_SIZE: usize = 1796;

/// Big-endian magic: `RaS3`.
const MAGIC_BE: [u8; 4] = *b"RaS3";

/// Little-endian magic: `3SaR`.
const MAGIC_LE: [u8; 4] = *b"3SaR";

// Byte offsets into the 1796-byte page header for the fields we need.
// The first 256 bytes are four 64-byte strings; u32 fields follow.
const OFF_ADVANCE_DISTANCE: usize = 256; // field[4]
const OFF_CUPS_WIDTH: usize = 372; // field[33]
const OFF_CUPS_HEIGHT: usize = 376; // field[34]
const OFF_CUPS_MEDIA_TYPE: usize = 380; // field[35]
const OFF_CUPS_BITS_PER_PIXEL: usize = 388; // field[37]
const OFF_CUPS_COLOUR_SPACE: usize = 400; // cupsColorSpace (after cupsColorOrder at 396)
const OFF_CUPS_NUM_COLOURS: usize = 420; // cupsNumColors (after cupsRowStep at 416)

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during RaS3 parsing.
#[derive(Debug)]
pub enum RasterError {
    /// No data was provided.
    Empty,
    /// The data is too short to contain the expected structure.
    TooShort { expected: usize, actual: usize },
    /// The magic bytes are not a recognised CUPS raster signature.
    BadMagic([u8; 4]),
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "no data received"),
            Self::TooShort { expected, actual } => {
                write!(f, "data too short: need {expected} bytes, got {actual}")
            }
            Self::BadMagic(m) => {
                write!(
                    f,
                    "bad magic: {:02x} {:02x} {:02x} {:02x}",
                    m[0], m[1], m[2], m[3]
                )
            }
        }
    }
}

impl std::error::Error for RasterError {}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// A single page extracted from a CUPS raster stream.
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub media_type: u32,
    pub colour_space: u32,
    pub num_colours: u32,
    pub advance_distance: u32,
    /// Raw grayscale pixel data (1 byte per pixel, row-major).
    pub data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Byte-order helpers
// ---------------------------------------------------------------------------

/// Byte order determined from the raster stream magic.
#[derive(Clone, Copy)]
enum ByteOrder {
    Big,
    Little,
}

/// Read a `u32` from a byte slice at the given offset using the specified
/// byte order.
fn read_u32(buf: &[u8], offset: usize, order: ByteOrder) -> u32 {
    let bytes: [u8; 4] = [
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ];
    match order {
        ByteOrder::Big => u32::from_be_bytes(bytes),
        ByteOrder::Little => u32::from_le_bytes(bytes),
    }
}

// ---------------------------------------------------------------------------
// RaS3 parser
// ---------------------------------------------------------------------------

/// Parse a CUPS RaS3 raster stream into a list of pages.
///
/// The entire input (including the 4-byte magic) must be provided as a
/// contiguous byte slice.
///
/// # Errors
///
/// Returns [`RasterError`] if the input is empty, truncated, or has an
/// unrecognised magic signature.
pub fn parse_ras3(input: &[u8]) -> Result<Vec<Page>, RasterError> {
    if input.is_empty() {
        return Err(RasterError::Empty);
    }
    if input.len() < 4 {
        return Err(RasterError::TooShort {
            expected: 4,
            actual: input.len(),
        });
    }

    let magic: [u8; 4] = [input[0], input[1], input[2], input[3]];
    let order = match magic {
        MAGIC_BE => ByteOrder::Big,
        MAGIC_LE => ByteOrder::Little,
        _ => return Err(RasterError::BadMagic(magic)),
    };

    let mut pos: usize = 4;
    let mut pages = Vec::new();

    while pos < input.len() {
        // Ensure we can read the full header.
        if pos + HEADER_SIZE > input.len() {
            return Err(RasterError::TooShort {
                expected: pos + HEADER_SIZE,
                actual: input.len(),
            });
        }

        let header = &input[pos..pos + HEADER_SIZE];
        let width = read_u32(header, OFF_CUPS_WIDTH, order);
        let height = read_u32(header, OFF_CUPS_HEIGHT, order);
        let media_type = read_u32(header, OFF_CUPS_MEDIA_TYPE, order);
        let bits_per_pixel = read_u32(header, OFF_CUPS_BITS_PER_PIXEL, order);
        let colour_space = read_u32(header, OFF_CUPS_COLOUR_SPACE, order);
        let num_colours = read_u32(header, OFF_CUPS_NUM_COLOURS, order);
        let advance_distance = read_u32(header, OFF_ADVANCE_DISTANCE, order);

        if width == 0 || height == 0 {
            return Err(RasterError::TooShort {
                expected: pos + HEADER_SIZE + 1,
                actual: pos + HEADER_SIZE,
            });
        }

        let img_bytes = (width as usize) * (height as usize) * (bits_per_pixel as usize) / 8;
        let data_start = pos + HEADER_SIZE;
        let data_end = data_start + img_bytes;

        if data_end > input.len() {
            return Err(RasterError::TooShort {
                expected: data_end,
                actual: input.len(),
            });
        }

        pages.push(Page {
            width,
            height,
            media_type,
            colour_space,
            num_colours,
            advance_distance,
            data: input[data_start..data_end].to_vec(),
        });

        pos = data_end;
    }

    Ok(pages)
}

// ---------------------------------------------------------------------------
// Image processing
// ---------------------------------------------------------------------------

/// Convert a grayscale image to packed 1-bit data.
///
/// Pixels are packed 8 per byte, MSB first. A pixel value <= 127 produces a
/// 1-bit (black); a value > 127 produces a 0-bit (white). Each row is padded
/// to a byte boundary.
///
/// Returns the packed byte buffer. The number of bytes per row is
/// `(width + 7) / 8`.
pub fn to_1bit(data: &[u8], width: u32) -> Vec<u8> {
    let w = width as usize;
    let bytes_per_row = w.div_ceil(8);
    let height = data.len() / w;
    let mut out = Vec::with_capacity(bytes_per_row * height);

    for row in 0..height {
        let row_start = row * w;
        for byte_idx in 0..bytes_per_row {
            let mut byte: u8 = 0;
            for bit in 0..8 {
                let col = byte_idx * 8 + bit;
                if col < w && data[row_start + col] <= 127 {
                    byte |= 1 << (7 - bit);
                }
            }
            out.push(byte);
        }
    }

    out
}

/// Resize a grayscale image to `dst_width` pixels wide using
/// nearest-neighbour interpolation. The height is scaled proportionally.
///
/// Returns `(new_data, dst_width, dst_height)`.
pub fn resize_nearest(
    data: &[u8],
    src_width: u32,
    src_height: u32,
    dst_width: u32,
) -> (Vec<u8>, u32, u32) {
    let sw = src_width as usize;
    let sh = src_height as usize;
    let dw = dst_width as usize;
    let dh = (sh * dw) / sw;

    let mut out = vec![0u8; dw * dh];

    for oy in 0..dh {
        let sy = oy * sh / dh;
        for ox in 0..dw {
            let sx = ox * sw / dw;
            out[oy * dw + ox] = data[sy * sw + sx];
        }
    }

    (out, dst_width, dh as u32)
}

/// Rotate a grayscale image 90 degrees counter-clockwise.
///
/// Input dimensions are `width` x `height` (row-major). The output has
/// dimensions `height` x `width` (i.e. the old height becomes the new width).
///
/// Returns `(new_data, new_width, new_height)`.
pub fn rotate_90_ccw(data: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let w = width as usize;
    let h = height as usize;
    // Output: new_width = h, new_height = w
    let new_w = h;
    let new_h = w;
    let mut out = vec![0u8; new_w * new_h];

    for out_row in 0..new_h {
        for out_col in 0..new_w {
            // CCW 90°: output[out_row][out_col] = input[out_col][W - 1 - out_row]
            out[out_row * new_w + out_col] = data[out_col * w + (w - 1 - out_row)];
        }
    }

    (out, new_w as u32, new_h as u32)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_1bit_simple() {
        // 8 pixels wide: [0, 0, 0, 0, 255, 255, 255, 255]
        // Pixels <= 127 → bit=1, > 127 → bit=0
        // Bits: 1 1 1 1 0 0 0 0 = 0xF0
        let data = vec![0, 0, 0, 0, 255, 255, 255, 255];
        let result = to_1bit(&data, 8);
        assert_eq!(result, vec![0xF0]);
    }

    #[test]
    fn test_to_1bit_padding() {
        // 3 pixels wide: [0, 255, 0]
        // Bits: 1 0 1 0 0 0 0 0 = 0xA0
        let data = vec![0, 255, 0];
        let result = to_1bit(&data, 3);
        assert_eq!(result, vec![0xA0]);
    }

    #[test]
    fn test_to_1bit_threshold_boundary() {
        // Exactly 127 → bit=1 (black), 128 → bit=0 (white)
        let data = vec![127, 128];
        let result = to_1bit(&data, 2);
        // Bits: 1 0 0 0 0 0 0 0 = 0x80
        assert_eq!(result, vec![0x80]);
    }

    #[test]
    fn test_resize_nearest() {
        // 4x2 image → resize to 2px wide → 2x1
        let data = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let (out, w, h) = resize_nearest(&data, 4, 2, 2);
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        // oy=0: sy=0*2/1=0, ox=0: sx=0*4/2=0 → 10, ox=1: sx=1*4/2=2 → 30
        assert_eq!(out, vec![10, 30]);
    }

    #[test]
    fn test_rotate_90_ccw() {
        // Input 3x2 (W=3, H=2):
        //   A B C     (row 0: indices 0,1,2)
        //   D E F     (row 1: indices 3,4,5)
        //
        // CCW 90° → 2x3 (new_W=2, new_H=3):
        //   C F
        //   B E
        //   A D
        let data = vec![1, 2, 3, 4, 5, 6]; // A=1,B=2,C=3,D=4,E=5,F=6
        let (out, w, h) = rotate_90_ccw(&data, 3, 2);
        assert_eq!(w, 2);
        assert_eq!(h, 3);
        assert_eq!(out, vec![3, 6, 2, 5, 1, 4]);
    }

    #[test]
    fn test_parse_ras3_empty() {
        assert!(parse_ras3(&[]).is_err());
    }

    #[test]
    fn test_parse_ras3_bad_magic() {
        assert!(parse_ras3(b"XXXX").is_err());
    }

    #[test]
    fn test_parse_ras3_le_single_page() {
        // Build a minimal valid RaS3 stream: magic + header + 4 pixels.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"3SaR"); // LE magic

        // 1796-byte header (zeroed, then set the fields we care about).
        let mut header = vec![0u8; HEADER_SIZE];

        // cupsWidth = 2 at offset 372
        header[OFF_CUPS_WIDTH..OFF_CUPS_WIDTH + 4].copy_from_slice(&2u32.to_le_bytes());
        // cupsHeight = 2 at offset 376
        header[OFF_CUPS_HEIGHT..OFF_CUPS_HEIGHT + 4].copy_from_slice(&2u32.to_le_bytes());
        // cupsBitsPerPixel = 8 at offset 388
        header[OFF_CUPS_BITS_PER_PIXEL..OFF_CUPS_BITS_PER_PIXEL + 4]
            .copy_from_slice(&8u32.to_le_bytes());
        // cupsNumColours = 1 at offset 408
        header[OFF_CUPS_NUM_COLOURS..OFF_CUPS_NUM_COLOURS + 4].copy_from_slice(&1u32.to_le_bytes());

        buf.extend_from_slice(&header);

        // 2x2 grayscale image: 4 bytes.
        buf.extend_from_slice(&[10, 20, 30, 40]);

        let pages = parse_ras3(&buf).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].width, 2);
        assert_eq!(pages[0].height, 2);
        assert_eq!(pages[0].num_colours, 1);
        assert_eq!(pages[0].data, vec![10, 20, 30, 40]);
    }
}
