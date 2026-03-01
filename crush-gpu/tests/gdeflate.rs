//! `GDeflate` codec unit tests (T008, T009, T010).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::unreadable_literal
)]

use crush_gpu::gdeflate::{gdeflate_compress_tile, gdeflate_decompress_tile};

// ---------------------------------------------------------------------------
// T008: Roundtrip tests — various data patterns
// ---------------------------------------------------------------------------

#[test]
fn roundtrip_empty_data() {
    let data = b"";
    let compressed = gdeflate_compress_tile(data).expect("compress empty");
    assert!(compressed.is_empty(), "empty input produces empty output");

    let decompressed = gdeflate_decompress_tile(&compressed, data.len()).expect("decompress empty");
    assert_eq!(decompressed, data);
}

#[test]
fn roundtrip_small_text() {
    let data = b"Hello, GDeflate world!";
    let compressed = gdeflate_compress_tile(data).expect("compress small text");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress small text");
    assert_eq!(decompressed, data.as_slice());
}

#[test]
fn roundtrip_repeated_pattern() {
    let data: Vec<u8> = "abcdefgh".bytes().cycle().take(4096).collect();
    let compressed = gdeflate_compress_tile(&data).expect("compress repeated");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress repeated");
    assert_eq!(decompressed, data);
}

#[test]
fn roundtrip_binary_data() {
    let data: Vec<u8> = (0..=255u8).cycle().take(8192).collect();
    let compressed = gdeflate_compress_tile(&data).expect("compress binary");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress binary");
    assert_eq!(decompressed, data);
}

#[test]
fn roundtrip_exact_64kb() {
    let data: Vec<u8> = (0..65536u32).map(|i| (i % 256) as u8).collect();
    let compressed = gdeflate_compress_tile(&data).expect("compress 64KB");
    let decompressed = gdeflate_decompress_tile(&compressed, data.len()).expect("decompress 64KB");
    assert_eq!(decompressed, data);
}

#[test]
fn roundtrip_english_text() {
    let text = "The quick brown fox jumps over the lazy dog. \
                This sentence contains various English words \
                that should compress well with DEFLATE encoding. \
                Repeated phrases help: the quick brown fox, the lazy dog. ";
    let data: Vec<u8> = text.bytes().cycle().take(32768).collect();
    let compressed = gdeflate_compress_tile(&data).expect("compress english");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress english");
    assert_eq!(decompressed, data);
}

#[test]
fn roundtrip_pseudo_random() {
    let mut data = vec![0u8; 4096];
    let mut state: u32 = 0xDEAD_BEEF;
    for byte in &mut data {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
        *byte = (state >> 16) as u8;
    }
    let compressed = gdeflate_compress_tile(&data).expect("compress random");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress random");
    assert_eq!(decompressed, data);
}

// ---------------------------------------------------------------------------
// T009: Compression ratio tests
// ---------------------------------------------------------------------------

#[test]
fn compression_ratio_english_text() {
    let text = "The quick brown fox jumps over the lazy dog. \
                Pack my box with five dozen liquor jugs. \
                How vexingly quick daft zebras jump. ";
    let data: Vec<u8> = text.bytes().cycle().take(32768).collect();

    let compressed = gdeflate_compress_tile(&data).expect("compress english for ratio");

    let ratio = compressed.len() as f64 / data.len() as f64;
    assert!(
        ratio < 0.5,
        "English text ratio {ratio:.3} should be < 0.5 (got {} -> {} bytes)",
        data.len(),
        compressed.len()
    );
}

#[test]
fn compression_ratio_binary_mixed() {
    let mut data = Vec::with_capacity(32768);
    // First half: repeated pattern (compressible)
    data.extend(std::iter::repeat_n(b"ABCDEFGHIJ", 1638).flatten());
    data.truncate(16384);
    // Second half: sequential bytes (somewhat compressible)
    for i in 0..16384u32 {
        data.push((i % 256) as u8);
    }

    let compressed = gdeflate_compress_tile(&data).expect("compress mixed for ratio");

    let ratio = compressed.len() as f64 / data.len() as f64;
    assert!(
        ratio < 0.8,
        "Mixed data ratio {ratio:.3} should be < 0.8 (got {} -> {} bytes)",
        data.len(),
        compressed.len()
    );
}

// ---------------------------------------------------------------------------
// T010: Edge case tests
// ---------------------------------------------------------------------------

#[test]
fn edge_case_single_byte() {
    let data = [42u8];
    let compressed = gdeflate_compress_tile(&data).expect("compress single byte");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress single byte");
    assert_eq!(decompressed, data.as_slice());
}

#[test]
fn edge_case_all_zeros() {
    let data = vec![0u8; 8192];
    let compressed = gdeflate_compress_tile(&data).expect("compress all zeros");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress all zeros");
    assert_eq!(decompressed, data);

    let ratio = compressed.len() as f64 / data.len() as f64;
    assert!(ratio < 0.1, "All-zeros ratio {ratio:.3} should be < 0.1");
}

#[test]
fn edge_case_all_ones() {
    let data = vec![0xFFu8; 4096];
    let compressed = gdeflate_compress_tile(&data).expect("compress all ones");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress all ones");
    assert_eq!(decompressed, data);
}

#[test]
fn edge_case_exceeds_64kb() {
    let data = vec![0u8; 65537]; // 64KB + 1
    let result = gdeflate_compress_tile(&data);
    assert!(result.is_err(), "should reject data > 64KB");
}

#[test]
fn edge_case_two_bytes() {
    let data = [0xAB, 0xCD];
    let compressed = gdeflate_compress_tile(&data).expect("compress two bytes");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress two bytes");
    assert_eq!(decompressed, data.as_slice());
}

#[test]
fn edge_case_run_lengths() {
    let mut data = Vec::with_capacity(16384);
    for ch in b'A'..=b'Z' {
        data.extend(std::iter::repeat_n(ch, 630));
    }
    data.truncate(16384);

    let compressed = gdeflate_compress_tile(&data).expect("compress run lengths");
    let decompressed =
        gdeflate_decompress_tile(&compressed, data.len()).expect("decompress run lengths");
    assert_eq!(decompressed, data);
}

#[test]
fn roundtrip_various_sizes() {
    for size in [
        1, 2, 3, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 512, 1024, 4096,
    ] {
        let data: Vec<u8> = (0..size).map(|i: usize| (i % 251) as u8).collect();
        let compressed =
            gdeflate_compress_tile(&data).unwrap_or_else(|_| panic!("compress size {size}"));
        let decompressed = gdeflate_decompress_tile(&compressed, data.len())
            .unwrap_or_else(|_| panic!("decompress size {size}"));
        assert_eq!(decompressed, data, "roundtrip failed for size {size}");
    }
}
