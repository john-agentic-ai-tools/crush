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
pub use index::{decompress_block, load_index, BlockIndex};

use crush_core::error::Result;
use crush_core::plugin::{CompressionAlgorithm, PluginMetadata, COMPRESSION_ALGORITHMS};
use linkme::distributed_slice;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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
        use std::sync::atomic::Ordering;
        use std::sync::Mutex;

        // Bridge the crush-core AtomicBool cancel flag into our ProgressCallback.
        let cb: ProgressCallback = Box::new(move |_event| !cancel_flag.load(Ordering::Acquire));
        let config = EngineConfiguration::builder()
            .progress(Arc::new(Mutex::new(cb)))
            .build()?;
        compress(input, &config)
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        use crate::config::ProgressCallback;
        use std::sync::atomic::Ordering;
        use std::sync::Mutex;

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
