//! `crush-gpu` — GPU-accelerated tile-based compression engine
//!
//! Implements a GDeflate-inspired GPU compression engine using 64KB independent
//! tiles with 32-way sub-stream parallelism for massively parallel decompression.

pub mod backend;
pub mod engine;
pub mod entropy;
pub mod error;
pub mod format;
pub mod gdeflate;
pub mod lz77;
pub mod scorer;
pub mod vectorize;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use crush_core::error::Result;
use crush_core::plugin::{CompressionAlgorithm, PluginMetadata, COMPRESSION_ALGORITHMS};
use linkme::distributed_slice;

// Re-export GPU device discovery types for CLI `plugins info` usage.
pub use backend::{discover_gpu, GpuInfo, GpuVendor};

/// Magic number for the gpu-deflate plugin in the crush-core outer format.
///
/// Format: `[0x43, 0x52, 0x01, plugin_id]` = `"CR"` + version 1 + plugin ID 0x03.
pub const PLUGIN_MAGIC: [u8; 4] = [0x43, 0x52, 0x01, 0x03];

// ============================================================================
// Process-global GPU plugin configuration
// ============================================================================

/// Process-global GPU plugin configuration.
///
/// Set once at CLI startup via [`configure()`]. The GPU plugin reads these
/// settings when constructing [`engine::EngineConfig`] for compression and
/// decompression.
#[derive(Debug, Clone, Default)]
pub struct GpuPluginConfig {
    /// If `true`, never attempt GPU decompression — always use CPU fallback.
    pub force_cpu: bool,
    /// Specific GPU device to use. `None` means auto-select best available.
    pub device_index: Option<u32>,
}

/// Cached process-global GPU plugin configuration.
static GPU_PLUGIN_CONFIG: OnceLock<GpuPluginConfig> = OnceLock::new();

/// Configure the GPU plugin with CLI/config-derived settings.
///
/// Must be called before any compression/decompression operations.
/// Can only be called once per process (uses `OnceLock` internally).
/// Subsequent calls are silently ignored.
pub fn configure(config: GpuPluginConfig) {
    let _ = GPU_PLUGIN_CONFIG.set(config);
}

/// Get the current GPU plugin configuration.
///
/// Returns a reference to the default config if [`configure()`] was never called.
pub fn get_config() -> &'static GpuPluginConfig {
    static DEFAULT_CONFIG: GpuPluginConfig = GpuPluginConfig {
        force_cpu: false,
        device_index: None,
    };
    GPU_PLUGIN_CONFIG.get().unwrap_or(&DEFAULT_CONFIG)
}

// ============================================================================
// Plugin implementation
// ============================================================================

/// Crush-gpu plugin implementation registered into the crush-core plugin registry.
struct GpuDeflatePlugin;

impl CompressionAlgorithm for GpuDeflatePlugin {
    fn name(&self) -> &'static str {
        "gpu-deflate"
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "gpu-deflate",
            version: env!("CARGO_PKG_VERSION"),
            magic_number: PLUGIN_MAGIC,
            throughput: 2000.0,
            compression_ratio: 0.65,
            description:
                "GPU-accelerated tile-based compression with 32-way parallel decompression",
        }
    }

    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        let config = engine::EngineConfig::default();
        engine::compress(input, &config, &cancel_flag)
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        let plugin_cfg = get_config();
        let config = engine::EngineConfig {
            force_cpu: plugin_cfg.force_cpu,
            ..engine::EngineConfig::default()
        };
        engine::decompress(input, &config, &cancel_flag)
    }

    fn detect(&self, file_header: &[u8]) -> bool {
        // CGPU files start with the 4-byte magic [0x43, 0x47, 0x50, 0x55]
        file_header.len() >= 4 && file_header[0..4] == format::CGPU_MAGIC
    }
}

/// Compile-time plugin registration via `linkme` distributed slice.
#[distributed_slice(COMPRESSION_ALGORITHMS)]
static GPU_DEFLATE_PLUGIN: &dyn CompressionAlgorithm = &GpuDeflatePlugin;
