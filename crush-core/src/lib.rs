//! Crush Core Library
//!
//! High-performance parallel compression library with pluggable compression algorithms.
//!
//! # Overview
//!
//! Crush provides a flexible plugin-based compression system with:
//! - **Pluggable algorithms**: Add custom compression algorithms via the [`CompressionAlgorithm`] trait
//! - **Intelligent selection**: Automatic plugin selection based on scoring weights
//! - **Timeout protection**: Configurable timeouts prevent runaway compression operations
//! - **Zero-copy design**: Minimal allocations and efficient memory usage
//!
//! # Quick Start
//!
//! ```
//! use crush_core::{init_plugins, compress, decompress};
//!
//! // Initialize plugin registry (call once at startup)
//! init_plugins().expect("Plugin initialization failed");
//!
//! // Compress data
//! let data = b"Hello, Crush!";
//! let compressed = compress(data).expect("Compression failed");
//!
//! // Decompress data
//! let decompressed = decompress(&compressed).expect("Decompression failed");
//! assert_eq!(data.as_slice(), decompressed.data.as_slice());
//! ```
//!
//! # Advanced Usage
//!
//! ## Custom Plugin Selection
//!
//! ```
//! use crush_core::{init_plugins, compress_with_options, CompressionOptions, ScoringWeights};
//! use std::time::Duration;
//!
//! init_plugins().expect("Plugin initialization failed");
//!
//! // Manual plugin override
//! let options = CompressionOptions::default()
//!     .with_plugin("deflate")
//!     .with_timeout(Duration::from_secs(10));
//! let compressed = compress_with_options(b"data", &options).expect("Compression failed");
//!
//! // Automatic selection with custom weights
//! let weights = ScoringWeights {
//!     throughput: 0.8,        // Prioritize speed
//!     compression_ratio: 0.2, // Less emphasis on compression ratio
//! };
//! let options = CompressionOptions::default().with_weights(weights);
//! let compressed = compress_with_options(b"data", &options).expect("Compression failed");
//! ```
//!
//! ## Plugin Discovery
//!
//! ```
//! use crush_core::{init_plugins, list_plugins};
//!
//! init_plugins().expect("Plugin initialization failed");
//!
//! // List all registered plugins
//! for plugin in list_plugins() {
//!     println!("Plugin: {} ({})", plugin.name, plugin.version);
//!     println!("  Throughput: {} MB/s", plugin.throughput);
//!     println!("  Compression ratio: {:.2}", plugin.compression_ratio);
//! }
//! ```

pub mod cancel;
pub mod compression;
pub mod decompression;
pub mod error;
pub mod inspection;
pub mod plugin;

pub use cancel::{AtomicCancellationToken, CancellationToken, ResourceTracker};
pub use compression::{compress, compress_with_options, CompressionOptions};
pub use decompression::decompress;
pub use error::{CrushError, PluginError, Result, TimeoutError, ValidationError};
pub use inspection::{inspect, InspectResult};
pub use plugin::{
    calculate_plugin_score, init_plugins, list_plugins, CompressionAlgorithm, CrushHeader,
    PluginMetadata, PluginSelector, ScoringWeights, COMPRESSION_ALGORITHMS,
};
