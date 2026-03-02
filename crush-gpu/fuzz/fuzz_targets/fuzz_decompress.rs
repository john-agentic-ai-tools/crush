//! Fuzz target for GPU decompression
//!
//! Tests that decompression never panics on arbitrary input.
//! Run with: `cargo +nightly fuzz run fuzz_decompress`

#![no_main]

use std::sync::atomic::AtomicBool;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    use crush_gpu::engine::{decompress, EngineConfig};

    let cancel = AtomicBool::new(false);
    let config = EngineConfig {
        force_cpu: true,
        ..EngineConfig::default()
    };

    // Attempt decompression — should never panic, only return Ok or Err.
    let _ = decompress(data, &config, &cancel);
});
