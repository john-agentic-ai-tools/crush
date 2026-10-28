//! CUDA backend integration tests
//!
//! These tests exercise the CUDA backend directly (bypassing `discover_gpu()`)
//! to validate CUDA-specific correctness independent of wgpu.
//!
//! Run with: `cargo test -p crush-gpu --features cuda -- cuda`

#![cfg(feature = "cuda")]
#![allow(
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use std::sync::atomic::{AtomicBool, Ordering};

use crush_gpu::backend::cuda::CudaBackend;
use crush_gpu::backend::{CompressedTile, ComputeBackend};

/// Helper: try to create a CUDA backend, skipping the test if no NVIDIA GPU.
fn require_cuda() -> CudaBackend {
    if let Some(backend) = CudaBackend::try_new().expect("CudaBackend::try_new should not error") {
        let info = backend.gpu_info();
        eprintln!(
            "  CUDA backend: {} ({} GB VRAM)",
            info.name,
            info.vram_bytes / (1024 * 1024 * 1024)
        );
        backend
    } else {
        eprintln!("  No NVIDIA GPU — skipping CUDA test");
        std::process::exit(0);
    }
}

// =========================================================================
// T009: CUDA unit-level tests — LZ77 roundtrip
// =========================================================================

#[test]
fn cuda_lz77_roundtrip_small() {
    let backend = require_cuda();
    let cancel = AtomicBool::new(false);

    // 256 bytes of deterministic data, 32 sub-streams.
    let n: u8 = 32;
    let original: Vec<u8> = (0..256u16).map(|i| (i % 251) as u8).collect();

    let tile = build_lz77_tile(&original, n);

    let results = backend
        .decompress_tiles(&[tile], &cancel)
        .expect("CUDA LZ77 decompress should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].len(),
        original.len(),
        "size mismatch: got {} expected {}",
        results[0].len(),
        original.len()
    );
    assert_eq!(results[0], original, "CUDA LZ77 data mismatch");
    eprintln!("  CUDA LZ77 roundtrip (256B) PASSED");
}

#[test]
fn cuda_lz77_roundtrip_64kb() {
    let backend = require_cuda();
    let cancel = AtomicBool::new(false);

    // Full 64KB tile.
    let original: Vec<u8> = (0..65536u32).map(|i| (i % 251) as u8).collect();
    let tile = build_lz77_tile(&original, 32);

    let results = backend
        .decompress_tiles(&[tile], &cancel)
        .expect("CUDA LZ77 decompress 64KB should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].len(), original.len());
    assert_eq!(results[0], original, "CUDA LZ77 64KB data mismatch");
    eprintln!("  CUDA LZ77 roundtrip (64KB) PASSED");
}

#[test]
fn cuda_lz77_roundtrip_multiple_tiles() {
    let backend = require_cuda();
    let cancel = AtomicBool::new(false);

    let tiles_data: Vec<Vec<u8>> = (0..4u32)
        .map(|t| (0..8192u32).map(|i| ((i + t * 31) % 251) as u8).collect())
        .collect();

    let tiles: Vec<CompressedTile> = tiles_data.iter().map(|d| build_lz77_tile(d, 32)).collect();

    let results = backend
        .decompress_tiles(&tiles, &cancel)
        .expect("CUDA LZ77 multi-tile decompress should succeed");

    assert_eq!(results.len(), 4);
    for (i, (got, expected)) in results.iter().zip(tiles_data.iter()).enumerate() {
        assert_eq!(got.len(), expected.len(), "tile {i} size mismatch");
        assert_eq!(got, expected, "tile {i} data mismatch");
    }
    eprintln!("  CUDA LZ77 multi-tile (4 × 8KB) PASSED");
}

// =========================================================================
// T009: CUDA unit-level tests — GDeflate roundtrip
// =========================================================================

#[test]
fn cuda_gdeflate_roundtrip_sizes() {
    use crush_gpu::gdeflate::gdeflate_compress_tile;

    let backend = require_cuda();
    let cancel = AtomicBool::new(false);

    for &size in &[1024u32, 4096, 32768, 65536] {
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
            .expect("CUDA GDeflate decompress should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].len(),
            original.len(),
            "GDeflate {size}B size mismatch"
        );
        assert_eq!(results[0], original, "CUDA GDeflate {size}B data mismatch");
        eprintln!("  CUDA GDeflate roundtrip ({size}B) PASSED");
    }
}

#[test]
fn cuda_gdeflate_batch() {
    use crush_gpu::gdeflate::gdeflate_compress_tile;

    let backend = require_cuda();
    let cancel = AtomicBool::new(false);

    let tile_size = 65536u32;
    let num_tiles = 8usize;

    let originals: Vec<Vec<u8>> = (0..num_tiles)
        .map(|t| {
            (0..tile_size)
                .map(|i| ((i + t as u32 * 31) % 256) as u8)
                .collect()
        })
        .collect();

    let tiles: Vec<CompressedTile> = originals
        .iter()
        .map(|data| {
            let compressed = gdeflate_compress_tile(data).expect("compress");
            CompressedTile {
                data: compressed,
                uncompressed_size: tile_size,
                sub_stream_count: 32,
                checksum: 0,
            }
        })
        .collect();

    let results = backend
        .decompress_tiles_gdeflate(&tiles, &cancel)
        .expect("CUDA GDeflate batch should succeed");

    assert_eq!(results.len(), num_tiles);
    for (i, (got, expected)) in results.iter().zip(originals.iter()).enumerate() {
        assert_eq!(got, expected, "CUDA GDeflate batch tile {i} mismatch");
    }
    eprintln!("  CUDA GDeflate batch ({num_tiles} × 64KB) PASSED");
}

// =========================================================================
// T009: CUDA cancellation test
// =========================================================================

#[test]
fn cuda_lz77_cancellation() {
    let backend = require_cuda();

    // Pre-set the cancel flag before dispatch.
    let cancel = AtomicBool::new(true);

    let original: Vec<u8> = (0..4096u16).map(|i| (i % 251) as u8).collect();
    let tiles: Vec<CompressedTile> = (0..4).map(|_| build_lz77_tile(&original, 32)).collect();

    let result = backend.decompress_tiles(&tiles, &cancel);

    // Should return Cancelled error (the loop checks cancel before each tile).
    assert!(
        result.is_err(),
        "should return error when cancelled, got {} tiles",
        result.expect("unreachable").len()
    );
    eprintln!("  CUDA LZ77 cancellation PASSED");
}

#[test]
fn cuda_gdeflate_cancellation() {
    use crush_gpu::gdeflate::gdeflate_compress_tile;

    let backend = require_cuda();
    let cancel = AtomicBool::new(true);

    let data: Vec<u8> = (0..4096u32).map(|i| (i % 256) as u8).collect();
    let compressed = gdeflate_compress_tile(&data).expect("compress");

    let tiles: Vec<CompressedTile> = (0..4)
        .map(|_| CompressedTile {
            data: compressed.clone(),
            uncompressed_size: 4096,
            sub_stream_count: 32,
            checksum: 0,
        })
        .collect();

    let result = backend.decompress_tiles_gdeflate(&tiles, &cancel);
    assert!(
        result.is_err(),
        "should return error when cancelled, got {} tiles",
        result.expect("unreachable").len()
    );
    eprintln!("  CUDA GDeflate cancellation PASSED");
}

// =========================================================================
// T009: CUDA cancellation mid-flight
// =========================================================================

#[test]
fn cuda_lz77_cancel_mid_batch() {
    let backend = require_cuda();

    // Start with cancel=false, set it after a moment.
    let cancel = AtomicBool::new(false);

    let original: Vec<u8> = (0..65536u32).map(|i| (i % 251) as u8).collect();
    let tiles: Vec<CompressedTile> = (0..64).map(|_| build_lz77_tile(&original, 32)).collect();

    // Set cancel immediately — the loop checks before each tile,
    // so at least some tiles may complete before cancellation kicks in.
    cancel.store(true, Ordering::Relaxed);

    let result = backend.decompress_tiles(&tiles, &cancel);
    // Either cancelled or completed (race condition is OK — we just verify no panic).
    match result {
        Ok(results) => eprintln!(
            "  CUDA cancel race: completed {} tiles before check",
            results.len()
        ),
        Err(e) => eprintln!("  CUDA cancel race: cancelled as expected ({e})"),
    }
}

// =========================================================================
// T010: CUDA vs CPU parity — engine-level roundtrip
// =========================================================================

#[test]
fn cuda_vs_cpu_parity() {
    use crush_gpu::engine::{EngineConfig, compress, decompress};

    // Skip if no CUDA GPU — require_cuda() exits the process.
    let _backend = require_cuda();

    let cancel = AtomicBool::new(false);

    let pattern = b"CUDA parity test data with repeated patterns! ";
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

    // Compress (always CPU).
    let config = EngineConfig::default();
    let compressed = compress(&data, &config, &cancel).expect("compress should succeed");

    // Decompress with GPU (which will pick up CUDA via discover_gpu auto-select).
    let gpu_config = EngineConfig {
        force_cpu: false,
        ..EngineConfig::default()
    };
    let gpu_result =
        decompress(&compressed, &gpu_config, &cancel).expect("GPU decompress should succeed");

    // Decompress with CPU fallback.
    let cpu_config = EngineConfig {
        force_cpu: true,
        ..EngineConfig::default()
    };
    let cpu_result =
        decompress(&compressed, &cpu_config, &cancel).expect("CPU decompress should succeed");

    assert_eq!(
        gpu_result.len(),
        cpu_result.len(),
        "GPU vs CPU size mismatch"
    );
    assert_eq!(gpu_result, cpu_result, "GPU vs CPU data mismatch");
    assert_eq!(gpu_result, data, "decompressed data should match original");
    eprintln!("  CUDA vs CPU parity PASSED (200KB)");
}

// =========================================================================
// T009: CUDA backend info validation
// =========================================================================

#[test]
fn cuda_backend_info() {
    let backend = require_cuda();

    assert_eq!(backend.name(), "CUDA");

    let info = backend.gpu_info();
    assert!(!info.name.is_empty(), "GPU name should not be empty");
    assert_eq!(info.vendor, crush_gpu::backend::GpuVendor::Nvidia);
    assert!(
        info.vram_bytes >= 2 * 1024 * 1024 * 1024,
        "GPU should have >= 2GB VRAM"
    );
    assert_eq!(info.api_backend, "CUDA");
    eprintln!(
        "  CUDA backend info: {} ({} GB VRAM)",
        info.name,
        info.vram_bytes / (1024 * 1024 * 1024)
    );
}

// =========================================================================
// Helpers
// =========================================================================

/// Build an LZ77-compressed tile from raw data, matching the GPU payload format.
fn build_lz77_tile(original: &[u8], n: u8) -> CompressedTile {
    use crush_gpu::lz77;

    let ns = usize::from(n);

    // Interleave bytes into sub-streams (byte i → sub-stream i%n).
    let mut sub_streams: Vec<Vec<u8>> = vec![Vec::new(); ns];
    for (i, &b) in original.iter().enumerate() {
        sub_streams[i % ns].push(b);
    }

    // LZ77-encode each sub-stream.
    let compressed_subs: Vec<Vec<u8>> = sub_streams
        .iter()
        .map(|ss| lz77::lz77_encode(ss, &lz77::STANDARD_CONFIG))
        .collect();

    // Build tile payload: [N × u32 LE offset table] [compressed sub-stream data...]
    let mut payload = Vec::new();
    let mut running_offset: u32 = 0;
    for cs in &compressed_subs {
        payload.extend_from_slice(&running_offset.to_le_bytes());
        running_offset += cs.len() as u32;
    }
    for cs in &compressed_subs {
        payload.extend_from_slice(cs);
    }

    CompressedTile {
        data: payload,
        uncompressed_size: original.len() as u32,
        sub_stream_count: n,
        checksum: 0,
    }
}
