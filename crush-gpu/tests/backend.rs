//! Backend detection and selection tests

#![allow(
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use crush_gpu::backend::{discover_gpu, GpuVendor};

// T039: Backend discovery tests

#[test]
fn test_discover_backends_returns_result() {
    // discover_gpu should succeed even if no GPU is found (returns None)
    let result = discover_gpu();
    assert!(
        result.is_ok(),
        "discover_gpu should not error: {}",
        result.err().map_or_else(String::new, |e| e.to_string())
    );

    // Print GPU info when present (visible with --nocapture).
    if let Ok(Some(backend)) = &result {
        let info = backend.gpu_info();
        eprintln!("  GPU detected: {} ({})", info.name, info.vendor);
        eprintln!(
            "  VRAM (max_buffer_size): {} bytes ({} GB)",
            info.vram_bytes,
            info.vram_bytes / (1024 * 1024 * 1024)
        );
        eprintln!("  API backend: {}", info.api_backend);
        eprintln!("  Backend name: {}", backend.name());
    } else {
        eprintln!("  No GPU detected — CPU fallback will be used");
    }
}

#[test]
fn test_gpu_detected_on_capable_machine() {
    // On a machine with a supported GPU, discover_gpu should return Some.
    // This test will be skipped (pass vacuously) on CI / headless / no GPU.
    let result = discover_gpu().expect("discover_gpu should not error");
    if let Some(backend) = &result {
        let info = backend.gpu_info();
        eprintln!(
            "  Found GPU: {} ({}) via {}",
            info.name, info.vendor, info.api_backend
        );
        // Verify the info is populated
        assert!(!info.name.is_empty(), "GPU name should not be empty");
        assert!(
            info.vram_bytes >= 2 * 1024 * 1024 * 1024,
            "GPU should have >= 2GB VRAM"
        );
    }
    // If no GPU found, this test still passes — it's informational.
}

#[test]
fn test_gpu_vendor_display() {
    assert_eq!(GpuVendor::Nvidia.to_string(), "NVIDIA");
    assert_eq!(GpuVendor::Amd.to_string(), "AMD");
    assert_eq!(GpuVendor::Intel.to_string(), "Intel");
    assert_eq!(GpuVendor::Apple.to_string(), "Apple");
    assert_eq!(GpuVendor::Other.to_string(), "Other");
}

// T032-T033: GPU decompression via WGSL shader

#[test]
fn test_gpu_decompression_roundtrip() {
    use crush_gpu::backend::CompressedTile;
    use crush_gpu::lz77;
    use std::sync::atomic::AtomicBool;

    let Some(backend) = discover_gpu().expect("discover_gpu should not error") else {
        eprintln!("  No GPU — skipping GPU decompression test");
        return;
    };

    let info = backend.gpu_info();
    eprintln!(
        "  GPU decompression test on: {} ({})",
        info.name, info.vendor
    );

    let cancel = AtomicBool::new(false);

    // Build a small tile: 256 bytes of known data, 32 sub-streams.
    let n: u8 = 32;
    #[allow(clippy::cast_possible_truncation)]
    let original = (0..256u16).map(|i| (i % 251) as u8).collect::<Vec<_>>();

    // Interleave bytes into sub-streams (same as engine compress_tile).
    let ns = usize::from(n);
    let mut sub_streams: Vec<Vec<u8>> = vec![Vec::new(); ns];
    for (i, &b) in original.iter().enumerate() {
        sub_streams[i % ns].push(b);
    }

    // LZ77-encode each sub-stream.
    let compressed_subs: Vec<Vec<u8>> = sub_streams
        .iter()
        .map(|ss| lz77::lz77_encode(ss, &lz77::STANDARD_CONFIG))
        .collect();

    // Build tile payload: [N × u32 LE offset table] [compressed data...]
    let mut payload = Vec::new();
    let mut running_offset: u32 = 0;
    for cs in &compressed_subs {
        payload.extend_from_slice(&running_offset.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        let len = cs.len() as u32;
        running_offset += len;
    }
    for cs in &compressed_subs {
        payload.extend_from_slice(cs);
    }

    let tile = CompressedTile {
        data: payload,
        #[allow(clippy::cast_possible_truncation)]
        uncompressed_size: original.len() as u32,
        sub_stream_count: n,
        checksum: 0,
    };

    let results = backend
        .decompress_tiles(&[tile], &cancel)
        .expect("GPU decompress_tiles should succeed");

    assert_eq!(results.len(), 1, "should have one tile result");
    assert_eq!(
        results[0].len(),
        original.len(),
        "decompressed size mismatch: got {} expected {}",
        results[0].len(),
        original.len()
    );
    assert_eq!(
        results[0], original,
        "GPU decompressed data should match original"
    );
    eprintln!(
        "  GPU decompression roundtrip PASSED ({} bytes)",
        original.len()
    );
}

// T020: GDeflate GPU decompression roundtrip

#[test]
fn test_gdeflate_gpu_roundtrip() {
    use crush_gpu::backend::CompressedTile;
    use crush_gpu::gdeflate::gdeflate_compress_tile;
    use std::sync::atomic::AtomicBool;

    let Some(backend) = discover_gpu().expect("discover_gpu should not error") else {
        eprintln!("  No GPU — skipping GDeflate GPU test");
        return;
    };

    let info = backend.gpu_info();
    eprintln!("  GDeflate GPU test on: {} ({})", info.name, info.vendor);

    let cancel = AtomicBool::new(false);

    // Test with multiple tile sizes
    for &size in &[1024u32, 32768, 65536] {
        #[allow(clippy::cast_possible_truncation)]
        let original: Vec<u8> = (0..size).map(|i| ((i * 7 + 13) % 256) as u8).collect();

        let compressed = gdeflate_compress_tile(&original).expect("GDeflate compress");

        let tile = CompressedTile {
            data: compressed,
            uncompressed_size: size,
            sub_stream_count: 32,
            checksum: 0,
        };

        let results = backend
            .decompress_tiles_gdeflate(&[tile], &cancel)
            .expect("GDeflate GPU decompress should succeed");

        assert_eq!(results.len(), 1, "should have one tile result");
        assert_eq!(
            results[0].len(),
            original.len(),
            "GDeflate decompressed size mismatch for {size}B tile: got {} expected {}",
            results[0].len(),
            original.len()
        );
        assert_eq!(
            results[0], original,
            "GDeflate GPU data mismatch for {size}B tile"
        );
        eprintln!("  GDeflate GPU roundtrip PASSED ({size} bytes)");
    }
}

// T021: GDeflate GPU throughput smoke test

#[test]
fn test_gdeflate_gpu_throughput() {
    use crush_gpu::backend::CompressedTile;
    use crush_gpu::gdeflate::gdeflate_compress_tile;
    use std::sync::atomic::AtomicBool;

    let Some(backend) = discover_gpu().expect("discover_gpu should not error") else {
        eprintln!("  No GPU — skipping GDeflate throughput test");
        return;
    };

    let cancel = AtomicBool::new(false);

    // Compress 1MB of data as 16 × 64KB tiles
    let tile_size: usize = 65536;
    let num_tiles: usize = 16;
    let total_bytes = tile_size * num_tiles;

    let mut tiles = Vec::with_capacity(num_tiles);
    for t in 0..num_tiles {
        #[allow(clippy::cast_possible_truncation)]
        let data: Vec<u8> = (0..tile_size).map(|i| ((i + t * 31) % 256) as u8).collect();
        let compressed = gdeflate_compress_tile(&data).expect("compress tile");
        tiles.push(CompressedTile {
            data: compressed,
            uncompressed_size: tile_size as u32,
            sub_stream_count: 32,
            checksum: 0,
        });
    }

    let start = std::time::Instant::now();
    let results = backend
        .decompress_tiles_gdeflate(&tiles, &cancel)
        .expect("GDeflate GPU batch decompress");
    let elapsed = start.elapsed();

    assert_eq!(results.len(), num_tiles);

    let throughput_mib = total_bytes as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64();
    eprintln!(
        "  GDeflate GPU throughput: {throughput_mib:.1} MiB/s ({total_bytes} bytes in {elapsed:.2?})"
    );

    // Smoke test: require > 3 MiB/s or < 500ms total.  The lower bound
    // accommodates CI runners with weak integrated GPUs (e.g. macOS Apple
    // Silicon iGPU ≈ 4-5 MiB/s).  On discrete GPUs with batched dispatch,
    // throughput is typically 100+ MiB/s.
    assert!(
        throughput_mib > 3.0 || elapsed.as_millis() < 500,
        "GDeflate GPU throughput {throughput_mib:.1} MiB/s too low"
    );
}

// T040: CPU fallback when no GPU

#[test]
fn test_cpu_fallback_when_no_gpu() {
    use crush_gpu::engine::{compress, decompress, EngineConfig};
    use std::sync::atomic::AtomicBool;

    let cancel = AtomicBool::new(false);

    // Force CPU mode — should always work regardless of GPU presence.
    let config = EngineConfig {
        force_cpu: true,
        ..EngineConfig::default()
    };

    let data = b"CPU fallback test data with repeated patterns! ".repeat(2000);
    let compressed = compress(&data, &config, &cancel).expect("CPU compress should succeed");
    let decompressed =
        decompress(&compressed, &config, &cancel).expect("CPU decompress should succeed");
    assert_eq!(data.as_slice(), decompressed.as_slice());
}
