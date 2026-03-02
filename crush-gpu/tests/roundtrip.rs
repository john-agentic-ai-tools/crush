//! End-to-end compress/decompress round-trip tests

#![allow(clippy::expect_used)]

use std::sync::atomic::AtomicBool;

use crush_gpu::engine::{
    compress, decompress, decompress_tile_by_index, load_tile_index, EngineConfig,
};

// T022: Basic round-trip test

#[test]
fn test_roundtrip_1mb_repeating_data() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    // Generate 1 MB of compressible, repeating data
    let pattern = b"The quick brown fox jumps over the lazy dog. ";
    let mut data = Vec::with_capacity(1_048_576);
    while data.len() < 1_048_576 {
        let remaining = 1_048_576 - data.len();
        let chunk = if remaining >= pattern.len() {
            pattern.as_slice()
        } else {
            &pattern[..remaining]
        };
        data.extend_from_slice(chunk);
    }

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");

    assert_eq!(data.len(), decompressed.len(), "decompressed size mismatch");
    assert_eq!(
        data, decompressed,
        "decompressed data differs from original"
    );
}

#[test]
fn test_roundtrip_small_data() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    // Data smaller than one tile (< 64KB)
    let data = b"Hello, GPU compression world!".to_vec();

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");

    assert_eq!(data, decompressed);
}

#[test]
fn test_roundtrip_empty_data() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    let data: Vec<u8> = Vec::new();

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");

    assert_eq!(data, decompressed);
}

// T023: Property-based round-trip test

mod property {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn roundtrip_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..200_000)) {
            let cancel = AtomicBool::new(false);
            let config = EngineConfig::default();

            let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
            let decompressed = decompress(&compressed, &config, &cancel).expect("decompression should succeed");

            prop_assert_eq!(&data, &decompressed, "round-trip mismatch");
        }
    }
}

// T024: CPU fallback decompression test

#[test]
fn test_cpu_fallback_decompression() {
    let cancel = AtomicBool::new(false);

    // Compress with default config
    let config = EngineConfig::default();
    let pattern = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz ";
    let mut data = Vec::with_capacity(200_000);
    while data.len() < 200_000 {
        let remaining = 200_000 - data.len();
        let chunk = if remaining >= pattern.len() {
            pattern.as_slice()
        } else {
            &pattern[..remaining]
        };
        data.extend_from_slice(chunk);
    }

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");

    // Decompress with force_cpu — should always work without GPU
    let cpu_config = EngineConfig {
        force_cpu: true,
        ..EngineConfig::default()
    };
    let decompressed =
        decompress(&compressed, &cpu_config, &cancel).expect("CPU decompression should succeed");

    assert_eq!(data, decompressed, "CPU fallback decompression mismatch");
}

// T025: Tile boundary tests

#[test]
fn test_tile_boundary_exact_multiple() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    // Exactly 2 × 64KB = 131072 bytes
    let data: Vec<u8> = (0..131_072u32).map(|i| (i % 251) as u8).collect();

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");

    assert_eq!(data.len(), decompressed.len());
    assert_eq!(data, decompressed);
}

#[test]
fn test_tile_boundary_plus_one() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    // 2 × 64KB + 1 = 131073 bytes (3 tiles, last tile is 1 byte)
    let data: Vec<u8> = (0..131_073u32).map(|i| (i % 251) as u8).collect();

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");

    assert_eq!(data.len(), decompressed.len());
    assert_eq!(data, decompressed);
}

#[test]
fn test_tile_boundary_minus_one() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    // 2 × 64KB - 1 = 131071 bytes (1 full tile + 1 partial tile of 65535 bytes)
    let data: Vec<u8> = (0..131_071u32).map(|i| (i % 251) as u8).collect();

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");

    assert_eq!(data.len(), decompressed.len());
    assert_eq!(data, decompressed);
}

// T052: Random access decompression tests

#[test]
fn test_random_access_first_tile() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();
    let tile_size = config.tile_size as usize;

    // Create data spanning multiple tiles (4 × 64KB = 256KB)
    let data: Vec<u8> = (0u32..u32::try_from(tile_size * 4).expect("safe"))
        .map(|i| (i % 251) as u8)
        .collect();
    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");

    let index = load_tile_index(&compressed).expect("index should load");
    assert_eq!(index.tile_count(), 4);

    // Decompress tile 0
    let tile0 = decompress_tile_by_index(&compressed, &index, 0, &config)
        .expect("tile 0 should decompress");
    assert_eq!(tile0, &data[..tile_size]);
}

#[test]
fn test_random_access_middle_tile() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();
    let tile_size = config.tile_size as usize;

    let data: Vec<u8> = (0u32..u32::try_from(tile_size * 4).expect("safe"))
        .map(|i| (i % 251) as u8)
        .collect();
    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let index = load_tile_index(&compressed).expect("index should load");

    // Decompress tile 2 (middle)
    let tile2 = decompress_tile_by_index(&compressed, &index, 2, &config)
        .expect("tile 2 should decompress");
    assert_eq!(tile2, &data[tile_size * 2..tile_size * 3]);
}

#[test]
fn test_random_access_last_tile() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();
    let tile_size = config.tile_size as usize;

    let data: Vec<u8> = (0u32..u32::try_from(tile_size * 4).expect("safe"))
        .map(|i| (i % 251) as u8)
        .collect();
    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let index = load_tile_index(&compressed).expect("index should load");

    // Decompress last tile
    let last = decompress_tile_by_index(&compressed, &index, 3, &config)
        .expect("last tile should decompress");
    assert_eq!(last, &data[tile_size * 3..]);
}

// GDeflate roundtrip tests for diverse data types

#[test]
fn test_gdeflate_csv_data_roundtrip() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    let line = b"2026-02-23T10:15:30Z,INFO,user_service,\"User login successful\",user_id=12345,ip=192.168.1.100\n";
    let mut data = Vec::with_capacity(200_000);
    while data.len() < 200_000 {
        let remaining = 200_000 - data.len();
        let chunk = if remaining >= line.len() {
            line.as_slice()
        } else {
            &line[..remaining]
        };
        data.extend_from_slice(chunk);
    }

    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("decompression should succeed");
    assert_eq!(data, decompressed);

    // GDeflate should compress repetitive CSV data well (< 50% of original)
    assert!(
        compressed.len() < data.len() / 2,
        "GDeflate compressed size ({}) should be < 50% of original ({}) for CSV",
        compressed.len(),
        data.len()
    );
}

#[test]
fn test_gdeflate_diverse_data_roundtrip() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    let test_cases: Vec<Vec<u8>> = vec![
        // Text-heavy data
        b"The quick brown fox jumps over the lazy dog. ".repeat(1000),
        // Binary-ish data
        (0u32..50_000)
            .flat_map(|i| (i % 251).to_le_bytes().to_vec())
            .collect(),
        // Mixed data
        {
            let mut mixed = b"Header: value\n".repeat(500);
            mixed.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF].repeat(500));
            mixed
        },
    ];

    for (i, data) in test_cases.iter().enumerate() {
        let compressed = compress(data, &config, &cancel).expect("compression should succeed");
        let decompressed =
            decompress(&compressed, &config, &cancel).expect("decompression should succeed");
        assert_eq!(data, &decompressed, "test case {i}: round-trip mismatch");
    }
}

#[test]
fn test_random_access_out_of_bounds() {
    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    let data = vec![42u8; 65536]; // 1 tile
    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");
    let index = load_tile_index(&compressed).expect("index should load");
    assert_eq!(index.tile_count(), 1);

    // Index 1 is out of range
    let result = decompress_tile_by_index(&compressed, &index, 1, &config);
    assert!(result.is_err());
}
