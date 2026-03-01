//! Fuzz target for GPU format parsing
//!
//! Tests that format deserialization never panics on arbitrary input.
//! Run with: `cargo +nightly fuzz run fuzz_format`

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    use crush_gpu::format::{GpuFileFooter, GpuFileHeader, TileHeader};

    // Fuzz GpuFileHeader deserialization
    if data.len() >= GpuFileHeader::SIZE {
        let bytes: &[u8; GpuFileHeader::SIZE] =
            data[..GpuFileHeader::SIZE].try_into().unwrap();
        let _ = GpuFileHeader::from_bytes(bytes);
    }

    // Fuzz TileHeader deserialization
    if data.len() >= TileHeader::SIZE {
        let bytes: &[u8; TileHeader::SIZE] =
            data[..TileHeader::SIZE].try_into().unwrap();
        let _ = TileHeader::from_bytes(bytes);
    }

    // Fuzz GpuFileFooter deserialization
    if data.len() >= GpuFileFooter::SIZE {
        let bytes: &[u8; GpuFileFooter::SIZE] =
            data[..GpuFileFooter::SIZE].try_into().unwrap();
        let _ = GpuFileFooter::from_bytes(bytes);
    }
});
