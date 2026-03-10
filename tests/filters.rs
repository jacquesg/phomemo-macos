//! Integration tests for Phomemo CUPS raster filters.
//!
//! Each test constructs a synthetic CUPS RaS3 raster stream, pipes it through
//! the filter binary, and verifies the ESC/POS output byte-by-byte.

use std::io::Write;
use std::process::{Command, Stdio};

// ---------------------------------------------------------------------------
// RaS3 construction helpers
// ---------------------------------------------------------------------------

// Header field offsets (must stay in sync with lib.rs).
const HEADER_SIZE: usize = 1796;
const OFF_ADVANCE_DISTANCE: usize = 256;
const OFF_CUPS_WIDTH: usize = 372;
const OFF_CUPS_HEIGHT: usize = 376;
const OFF_CUPS_MEDIA_TYPE: usize = 380;
const OFF_CUPS_BITS_PER_PIXEL: usize = 388;
const OFF_CUPS_COLOUR_SPACE: usize = 400;
const OFF_CUPS_NUM_COLOURS: usize = 420;

/// Build a single-page CUPS RaS3 stream (little-endian).
fn build_ras3(
    width: u32,
    height: u32,
    media_type: u32,
    advance_distance: u32,
    pixels: &[u8],
) -> Vec<u8> {
    assert_eq!(
        pixels.len(),
        (width * height) as usize,
        "pixel data length must equal width * height"
    );

    let mut buf = Vec::new();
    buf.extend_from_slice(b"3SaR"); // LE magic

    let mut header = vec![0u8; HEADER_SIZE];
    header[OFF_CUPS_WIDTH..][..4].copy_from_slice(&width.to_le_bytes());
    header[OFF_CUPS_HEIGHT..][..4].copy_from_slice(&height.to_le_bytes());
    header[OFF_CUPS_MEDIA_TYPE..][..4].copy_from_slice(&media_type.to_le_bytes());
    header[OFF_CUPS_BITS_PER_PIXEL..][..4].copy_from_slice(&8u32.to_le_bytes());
    header[OFF_CUPS_COLOUR_SPACE..][..4].copy_from_slice(&0u32.to_le_bytes());
    header[OFF_CUPS_NUM_COLOURS..][..4].copy_from_slice(&1u32.to_le_bytes());
    header[OFF_ADVANCE_DISTANCE..][..4].copy_from_slice(&advance_distance.to_le_bytes());

    buf.extend_from_slice(&header);
    buf.extend_from_slice(pixels);
    buf
}

// ---------------------------------------------------------------------------
// Filter runner
// ---------------------------------------------------------------------------

/// Run a filter binary, piping `input` to stdin, and return stdout.
fn run_filter(bin: &str, input: &[u8]) -> Vec<u8> {
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn {bin}: {e}"));

    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input).unwrap();
        // stdin dropped here → pipe closed → filter reads EOF
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{bin} exited with {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

// ---------------------------------------------------------------------------
// Byte-pattern helpers
// ---------------------------------------------------------------------------

/// Find the first occurrence of `needle` in `haystack`.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Read a little-endian u16 from `data` at `offset`.
fn le_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

/// GS v 0 command: \x1d v 0 \x00
const GS_V_0: &[u8] = b"\x1dv0\x00";

// ===========================================================================
// M110 filter tests
// ===========================================================================

#[test]
fn m110_header_speed_density_media() {
    let input = build_ras3(8, 1, 10, 0, &vec![128u8; 8]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    // Speed
    assert_eq!(&out[0..4], b"\x1b\x4e\x0d\x05", "speed command");
    // Density
    assert_eq!(&out[4..8], b"\x1b\x4e\x04\x0a", "density command");
    // Media type header + value
    assert_eq!(&out[8..10], b"\x1f\x11", "media type prefix");
    assert_eq!(out[10], 10, "media type = 10 (LabelWithGaps)");
}

#[test]
fn m110_media_type_passed_through() {
    // Media type 11 = Continuous
    let input = build_ras3(8, 1, 11, 0, &vec![128u8; 8]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);
    assert_eq!(out[10], 11);

    // Media type 38 = LabelWithMarks
    let input = build_ras3(8, 1, 38, 0, &vec![128u8; 8]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);
    assert_eq!(out[10], 38);
}

#[test]
fn m110_raster_dimensions_padded() {
    // 16px wide → 2 bytes, padded to 48 bytes (384-dot print head).
    let input = build_ras3(16, 4, 10, 0, &vec![0u8; 16 * 4]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    assert_eq!(
        le_u16(&out, gs + 4),
        48,
        "width should be padded to 48 bytes"
    );
    assert_eq!(le_u16(&out, gs + 6), 4, "height should be 4");
}

#[test]
fn m110_left_padding_narrow_image() {
    // 8px wide (1 byte), all black. Print head is 48 bytes.
    // Paper is right-aligned → 47 bytes of left padding + 1 byte of data.
    let input = build_ras3(8, 1, 10, 0, &vec![0u8; 8]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    let raster = gs + 8; // skip GS v 0 (4) + width (2) + height (2)

    // 47 bytes of zero left-padding
    assert!(
        out[raster..raster + 47].iter().all(|&b| b == 0),
        "left padding should be all zeros"
    );
    // Last byte: all-black → all bits set (bit 1 = print)
    assert_eq!(out[raster + 47], 0xFF, "all-black should produce 0xFF");
}

#[test]
fn m110_full_width_no_padding() {
    // 384px wide = 48 bytes, matches print head exactly → no padding.
    let input = build_ras3(384, 1, 10, 0, &vec![0u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    assert_eq!(
        le_u16(&out, gs + 4),
        48,
        "width = 48 bytes, no extra padding"
    );

    let raster = gs + 8;
    assert!(
        out[raster..raster + 48].iter().all(|&b| b == 0xFF),
        "all-black full-width should produce all-FF"
    );
}

#[test]
fn m110_all_white_produces_zero_raster() {
    let input = build_ras3(384, 1, 10, 0, &vec![255u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    let raster = gs + 8;
    assert!(
        out[raster..raster + 48].iter().all(|&b| b == 0),
        "all-white should produce all-zero raster"
    );
}

#[test]
fn m110_footer_present() {
    let input = build_ras3(8, 1, 10, 0, &vec![128u8; 8]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    let len = out.len();
    assert_eq!(&out[len - 8..len - 4], b"\x1f\xf0\x05\x00", "footer 1");
    assert_eq!(&out[len - 4..], b"\x1f\xf0\x03\x00", "footer 2");
}

#[test]
fn m110_golden_8x1_black() {
    // Golden-file test: exact byte-for-byte output for a known input.
    // 8x1 all-black image, media type 10, no advance.
    let input = build_ras3(8, 1, 10, 0, &vec![0u8; 8]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm110"), &input);

    let mut expected = Vec::new();
    expected.extend_from_slice(b"\x1b\x4e\x0d\x05"); // speed
    expected.extend_from_slice(b"\x1b\x4e\x04\x0a"); // density
    expected.extend_from_slice(b"\x1f\x11\x0a"); // media type 10
    expected.extend_from_slice(b"\x1dv0\x00"); // GS v 0
    expected.extend_from_slice(&48u16.to_le_bytes()); // width: 48 bytes
    expected.extend_from_slice(&1u16.to_le_bytes()); // height: 1 line
    expected.extend_from_slice(&vec![0u8; 47]); // left padding
    expected.push(0xFF); // 8 black pixels
    expected.extend_from_slice(b"\x1f\xf0\x05\x00"); // footer 1
    expected.extend_from_slice(b"\x1f\xf0\x03\x00"); // footer 2

    assert_eq!(out, expected, "golden output mismatch for 8x1 black M110");
}

// ===========================================================================
// M02 filter tests
// ===========================================================================

#[test]
fn m02_header_structure() {
    let input = build_ras3(384, 1, 0, 0, &vec![128u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    assert_eq!(&out[0..2], b"\x1b\x40", "ESC @ initialise");
    assert_eq!(&out[2..5], b"\x1b\x61\x01", "ESC a centre justify");
    assert_eq!(&out[5..9], b"\x1f\x11\x02\x04", "init command");
}

#[test]
fn m02_inverted_polarity_black() {
    // M02 inverts: black pixels (0) → grayscale inverted to 255 →
    // to_1bit(255) → bit 0 → raster byte 0x00.
    let input = build_ras3(384, 1, 0, 0, &vec![0u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    let gs = find_bytes(&out, b"\x1d\x76\x30\x00").expect("GS v 0 not found");
    let raster = gs + 8;
    assert!(
        out[raster..raster + 48].iter().all(|&b| b == 0x00),
        "M02: all-black should produce all-zero (inverted polarity)"
    );
}

#[test]
fn m02_inverted_polarity_white() {
    // M02 inverts: white pixels (255) → grayscale inverted to 0 →
    // to_1bit(0) → bit 1 → raster byte 0xFF.
    let input = build_ras3(384, 1, 0, 0, &vec![255u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    let gs = find_bytes(&out, b"\x1d\x76\x30\x00").expect("GS v 0 not found");
    let raster = gs + 8;
    assert!(
        out[raster..raster + 48].iter().all(|&b| b == 0xFF),
        "M02: all-white should produce all-FF (inverted polarity)"
    );
}

#[test]
fn m02_linefeed_byte_substitution() {
    // A raster byte of 0x0a (LineFeed) must be substituted with 0x14
    // to prevent the printer from interpreting it as a control character.
    //
    // To produce 0x0a (0b00001010) after M02 inversion + to_1bit:
    // - Pixels at positions 4 and 6 must be >= 128 (white after inversion)
    // - All other pixels must be < 128 (black after inversion)
    let mut pixels = vec![0u8; 384];
    pixels[4] = 255;
    pixels[6] = 255;

    let input = build_ras3(384, 1, 0, 0, &pixels);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    let gs = find_bytes(&out, b"\x1d\x76\x30\x00").expect("GS v 0 not found");
    let raster = gs + 8;
    assert_eq!(
        out[raster], 0x14,
        "0x0a raster byte should be substituted with 0x14"
    );
}

#[test]
fn m02_block_chunking_at_256_lines() {
    // Images taller than 256 lines are split into multiple GS v 0 blocks.
    // 300 lines → block 1: 256 lines (encoded as 255), block 2: 44 lines (encoded as 43).
    let input = build_ras3(384, 300, 0, 0, &vec![128u8; 384 * 300]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    let gs_cmd = b"\x1d\x76\x30\x00";
    let block_count = out.windows(gs_cmd.len()).filter(|w| *w == gs_cmd).count();
    assert_eq!(block_count, 2, "300 lines should produce 2 GS v 0 blocks");

    // First block: lines-1 = 255
    let gs1 = find_bytes(&out, gs_cmd).unwrap();
    assert_eq!(le_u16(&out, gs1 + 6), 255, "first block: 256 lines (255)");

    // Second block: lines-1 = 43
    let gs2 = find_bytes(&out[gs1 + 4..], gs_cmd).unwrap() + gs1 + 4;
    assert_eq!(le_u16(&out, gs2 + 6), 43, "second block: 44 lines (43)");
}

#[test]
fn m02_feed_lines_default() {
    // advance_distance = 0 → default feed = 2.
    let input = build_ras3(384, 1, 0, 0, &vec![128u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    // Footer: \x1b\x64<feed> twice, then 4 × \x1f\x11<xx> = 18 bytes from end.
    let len = out.len();
    assert_eq!(
        &out[len - 18..len - 15],
        b"\x1b\x64\x02",
        "feed = 2 (first)"
    );
    assert_eq!(
        &out[len - 15..len - 12],
        b"\x1b\x64\x02",
        "feed = 2 (second)"
    );
}

#[test]
fn m02_feed_lines_custom() {
    // advance_distance = 5 → feed = 5.
    let input = build_ras3(384, 1, 0, 5, &vec![128u8; 384]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    let len = out.len();
    assert_eq!(
        &out[len - 18..len - 15],
        b"\x1b\x64\x05",
        "feed = 5 (first)"
    );
    assert_eq!(
        &out[len - 15..len - 12],
        b"\x1b\x64\x05",
        "feed = 5 (second)"
    );
}

#[test]
fn m02_resize_to_384() {
    // 192px wide image (half width) should be resized to 384px.
    // Height scales proportionally: 2 → 4 lines.
    let input = build_ras3(192, 2, 0, 0, &vec![0u8; 192 * 2]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopm02"), &input);

    let gs = find_bytes(&out, b"\x1d\x76\x30\x00").expect("GS v 0 not found");
    assert_eq!(le_u16(&out, gs + 4), 48, "width should be 48 bytes (384px)");
    // height-1 = 3 (4 lines after 2x resize)
    assert_eq!(le_u16(&out, gs + 6), 3, "height should be 4 lines (3)");
}

// ===========================================================================
// D30 filter tests
// ===========================================================================

#[test]
fn d30_init_sequence() {
    let input = build_ras3(8, 4, 0, 0, &vec![128u8; 8 * 4]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopd30"), &input);

    assert_eq!(&out[0..4], b"\x1f\x11\x24\x00", "D30 init");
    assert_eq!(&out[4..6], b"\x1b\x40", "ESC @");
}

#[test]
fn d30_rotation_swaps_dimensions() {
    // Input: 8x4 (W=8, H=4).
    // After 90° CCW rotation: new_W=4, new_H=8.
    // bytes_per_line = ceil(4/8) = 1.
    let input = build_ras3(8, 4, 0, 0, &vec![128u8; 8 * 4]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopd30"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    assert_eq!(
        le_u16(&out, gs + 4),
        1,
        "rotated width = ceil(4/8) = 1 byte"
    );
    assert_eq!(le_u16(&out, gs + 6), 8, "rotated height = old width = 8");
}

#[test]
fn d30_feed_padding_zeros() {
    // 8x2 image, advance_distance = 3.
    // After rotation: new_W=2 (1 byte), new_H=8.
    // Feed padding = 3 lines × 1 byte = 3 bytes of zeros.
    let input = build_ras3(8, 2, 0, 3, &vec![128u8; 8 * 2]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopd30"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    let raster_start = gs + 8;
    let raster_bytes = 8; // 8 rows × 1 byte
    let padding_start = raster_start + raster_bytes;
    let padding = &out[padding_start..padding_start + 3];

    assert!(
        padding.iter().all(|&b| b == 0),
        "feed padding should be zeros"
    );
    assert_eq!(
        out.len(),
        padding_start + 3,
        "output should end after feed padding"
    );
}

#[test]
fn d30_no_feed_padding_when_zero() {
    // advance_distance = 0 → no feed padding.
    let input = build_ras3(8, 2, 0, 0, &vec![128u8; 8 * 2]);
    let out = run_filter(env!("CARGO_BIN_EXE_rastertopd30"), &input);

    let gs = find_bytes(&out, GS_V_0).expect("GS v 0 not found");
    let raster_start = gs + 8;
    let raster_bytes = 8; // 8 rows × 1 byte
    assert_eq!(
        out.len(),
        raster_start + raster_bytes,
        "no feed padding when advance_distance = 0"
    );
}
