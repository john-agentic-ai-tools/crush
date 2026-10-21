//! `crush-parallel` — Parallel DEFLATE compression engine
//!
//! Implements a pigz-inspired multi-threaded compression engine using a custom
//! binary format (CRSH) optimised for parallel decompression and random block access.
//!
//! # Quick Start
//!
//! ```no_run
//! use crush_parallel::{compress, decompress, EngineConfiguration};
//!
//! let config = EngineConfiguration::default();
//! let data = b"hello world".repeat(10000);
//! let compressed = compress(&data, &config).expect("compression failed");
//! let recovered = decompress(&compressed, &config).expect("decompression failed");
//! assert_eq!(data.as_slice(), recovered.as_slice());
//! ```

pub mod block;
pub mod config;
pub mod engine;
pub mod format;
pub mod index;

// Public API re-exports
pub use config::{
    EngineConfiguration, EngineConfigurationBuilder, ProgressCallback, ProgressEvent, ProgressPhase,
};
pub use engine::{
    compress, compress_file, compress_stream, compress_to_writer, decompress,
    decompress_from_reader,
};
pub use format::BlockIndexEntry;
pub use index::{BlockIndex, decompress_block, load_index};

use crush_core::error::Result;
use crush_core::plugin::{COMPRESSION_ALGORITHMS, CompressionAlgorithm, PluginMetadata};
use linkme::distributed_slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Magic number for the parallel-deflate plugin in the crush-core outer format.
///
/// Format: `[0x43, 0x52, 0x01, plugin_id]` = `"CR"` + version 1 + plugin ID.
/// Plugin ID 0x00 is reserved for the native deflate plugin; 0x02 identifies
/// the parallel-deflate engine so `CrushHeader::has_valid_version()` passes.
pub const PLUGIN_MAGIC: [u8; 4] = [0x43, 0x52, 0x01, 0x02];

/// Crush-parallel plugin implementation registered into the crush-core plugin registry.
struct ParallelDeflatePlugin;

impl CompressionAlgorithm for ParallelDeflatePlugin {
    fn name(&self) -> &'static str {
        "parallel-deflate"
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "parallel-deflate",
            version: env!("CARGO_PKG_VERSION"),
            magic_number: PLUGIN_MAGIC,
            throughput: 500.0,
            compression_ratio: 0.65,
            description: "Multi-threaded DEFLATE with CRSH block format; parallel decompress and random access",
        }
    }

    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        use crate::config::ProgressCallback;
        use std::sync::Mutex;
        use std::sync::atomic::Ordering;

        // Bridge the crush-core AtomicBool cancel flag into our ProgressCallback.
        let cb: ProgressCallback = Box::new(move |_event| !cancel_flag.load(Ordering::Acquire));
        let config = EngineConfiguration::builder()
            .progress(Arc::new(Mutex::new(cb)))
            .build()?;
        compress(input, &config)
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        use crate::config::ProgressCallback;
        use std::sync::Mutex;
        use std::sync::atomic::Ordering;

        let cb: ProgressCallback = Box::new(move |_event| !cancel_flag.load(Ordering::Acquire));
        let config = EngineConfiguration::builder()
            .progress(Arc::new(Mutex::new(cb)))
            .build()?;
        decompress(input, &config)
    }

    fn detect(&self, file_header: &[u8]) -> bool {
        // CRSH files start with the 4-byte magic [0x43, 0x52, 0x53, 0x48] ("CRSH")
        file_header.len() >= 4 && file_header[0..4] == crate::format::CRSH_MAGIC
    }
}

/// Compile-time plugin registration via `linkme` distributed slice.
#[distributed_slice(COMPRESSION_ALGORITHMS)]
static PARALLEL_DEFLATE_PLUGIN: &dyn CompressionAlgorithm = &ParallelDeflatePlugin;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod plugin_tests {
    use super::*;
    use crush_core::error::CrushError;
    use std::sync::atomic::Ordering;

    /// A cancel flag that is not set — the normal case.
    fn live() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn plugin_reports_stable_identity() {
        let plugin = ParallelDeflatePlugin;
        assert_eq!(plugin.name(), "parallel-deflate");

        let meta = plugin.metadata();
        assert_eq!(meta.name, "parallel-deflate");
        assert_eq!(meta.magic_number, PLUGIN_MAGIC);
        // The registry keys plugins by magic number, so the leading bytes must
        // stay in the crush-core envelope shape: "CR" + version 1 + plugin id.
        assert_eq!(&meta.magic_number[0..3], &[0x43, 0x52, 0x01]);
        // Version is wired to the crate version so archives record their producer.
        assert_eq!(meta.version, env!("CARGO_PKG_VERSION"));
        assert!(meta.throughput > 0.0);
        assert!(meta.compression_ratio > 0.0 && meta.compression_ratio < 1.0);
    }

    #[test]
    fn plugin_roundtrips_through_the_trait() {
        let plugin = ParallelDeflatePlugin;
        // Repetitive enough to actually compress, and larger than one block.
        let data = b"fn main() { println!(\"hello\"); }\n".repeat(4096);

        let compressed = plugin.compress(&data, live()).expect("compress");
        assert!(compressed.len() < data.len(), "expected some compression");

        let recovered = plugin.decompress(&compressed, live()).expect("decompress");
        assert_eq!(recovered, data);
    }

    #[test]
    fn plugin_roundtrips_empty_input() {
        let plugin = ParallelDeflatePlugin;
        let compressed = plugin.compress(b"", live()).expect("compress empty");
        let recovered = plugin.decompress(&compressed, live()).expect("decompress");
        assert!(recovered.is_empty());
    }

    #[test]
    fn compress_honours_a_pre_set_cancel_flag() {
        let plugin = ParallelDeflatePlugin;
        let data = b"cancel me".repeat(100_000);

        let flag = Arc::new(AtomicBool::new(true));
        let result = plugin.compress(&data, Arc::clone(&flag));

        assert!(
            matches!(result, Err(CrushError::Cancelled)),
            "expected Cancelled, got {result:?}"
        );
        assert!(flag.load(Ordering::Acquire));
    }

    #[test]
    fn decompress_honours_a_pre_set_cancel_flag() {
        let plugin = ParallelDeflatePlugin;
        let data = b"cancel me".repeat(100_000);
        let compressed = plugin.compress(&data, live()).expect("compress");

        let result = plugin.decompress(&compressed, Arc::new(AtomicBool::new(true)));

        assert!(
            matches!(result, Err(CrushError::Cancelled)),
            "expected Cancelled, got {result:?}"
        );
    }

    #[test]
    fn detect_matches_only_the_crsh_magic() {
        let plugin = ParallelDeflatePlugin;

        assert!(plugin.detect(&crate::format::CRSH_MAGIC));
        // Trailing bytes are irrelevant; only the 4-byte prefix is inspected.
        let mut with_tail = crate::format::CRSH_MAGIC.to_vec();
        with_tail.extend_from_slice(&[0xFF; 32]);
        assert!(plugin.detect(&with_tail));

        // Wrong magic, including the plugin's own envelope magic.
        assert!(!plugin.detect(b"GZIP"));
        assert!(!plugin.detect(&PLUGIN_MAGIC));

        // Too short to contain a magic at all — must not panic.
        assert!(!plugin.detect(b""));
        assert!(!plugin.detect(&crate::format::CRSH_MAGIC[..3]));
    }

    #[test]
    fn detect_accepts_real_compressed_output() {
        let plugin = ParallelDeflatePlugin;
        let compressed = plugin.compress(b"detect me", live()).expect("compress");
        assert!(
            plugin.detect(&compressed),
            "compress() output should be recognised by detect()"
        );
    }

    #[test]
    fn plugin_is_registered_in_the_distributed_slice() {
        let found = COMPRESSION_ALGORITHMS
            .iter()
            .any(|p| p.metadata().magic_number == PLUGIN_MAGIC);
        assert!(found, "parallel-deflate should be linkme-registered");
    }
}
