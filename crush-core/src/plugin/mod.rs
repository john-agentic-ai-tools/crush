//! Plugin system for Crush compression library
//!
//! This module provides the plugin infrastructure for extending Crush with
//! custom compression algorithms. Plugins are registered at compile-time using
//! the `linkme` crate for zero runtime overhead.

pub mod contract;
pub mod default;
pub mod metadata;

pub use contract::CompressionAlgorithm;
pub use metadata::{CrushHeader, PluginMetadata};

use linkme::distributed_slice;

/// Global registry of all compile-time registered compression plugins
///
/// Plugins register themselves by adding to this distributed slice using:
/// ```
/// use crush_core::plugin::{CompressionAlgorithm, COMPRESSION_ALGORITHMS};
/// use linkme::distributed_slice;
///
/// // Plugin registration example (requires implementing CompressionAlgorithm)
/// // #[distributed_slice(COMPRESSION_ALGORITHMS)]
/// // static MY_PLUGIN: &dyn CompressionAlgorithm = &MyPluginImpl;
/// ```
#[distributed_slice]
pub static COMPRESSION_ALGORITHMS: [&'static dyn CompressionAlgorithm] = [..];
