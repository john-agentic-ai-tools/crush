//! Plugin contract trait definition
//!
//! This module defines the `CompressionAlgorithm` trait that all plugins must implement.
//! Plugins register themselves at compile-time using the `linkme` distributed slice pattern.

use crate::error::Result;
use crate::plugin::PluginMetadata;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Trait that all compression plugins must implement
///
/// Plugins provide compress/decompress operations with cooperative cancellation support.
/// The `cancel_flag` parameter allows the library to signal timeout or user cancellation,
/// and plugins SHOULD check this flag periodically (e.g., every block) and return early if set.
///
/// # Safety and Error Handling
///
/// - All methods return `Result<T>` to enable proper error propagation
/// - Plugins MUST NOT panic - all errors should be returned via `Result`
/// - Plugins MUST validate input data before processing
/// - If `cancel_flag` is set to `true`, plugins SHOULD return `Err(PluginError::Cancelled)`
///
/// # Example Implementation
///
/// ```no_run
/// use crush_core::plugin::{CompressionAlgorithm, PluginMetadata, COMPRESSION_ALGORITHMS};
/// use crush_core::error::Result;
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicBool, Ordering};
/// use linkme::distributed_slice;
///
/// struct MyPlugin;
///
/// impl CompressionAlgorithm for MyPlugin {
///     fn name(&self) -> &'static str { "my_plugin" }
///
///     fn metadata(&self) -> PluginMetadata {
///         PluginMetadata {
///             name: "my_plugin",
///             version: "1.0.0",
///             magic_number: [0x43, 0x52, 0x01, 0x10],
///             throughput: 500.0,
///             compression_ratio: 0.65,
///             description: "My compression algorithm",
///         }
///     }
///
///     fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
///         let mut output = Vec::new();
///         for chunk in input.chunks(4096) {
///             // Check cancellation flag periodically
///             if cancel_flag.load(Ordering::Acquire) {
///                 return Err(crush_core::error::PluginError::Cancelled.into());
///             }
///             // Compress chunk...
///         }
///         Ok(output)
///     }
///
///     fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
///         // Similar implementation with cancellation checking
///         Ok(vec![])
///     }
///
///     fn detect(&self, file_header: &[u8]) -> bool {
///         file_header.len() >= 4 && file_header[0..4] == self.metadata().magic_number
///     }
/// }
///
/// // Register plugin at compile-time
/// #[distributed_slice(COMPRESSION_ALGORITHMS)]
/// static MY_PLUGIN: &dyn CompressionAlgorithm = &MyPlugin;
/// ```
pub trait CompressionAlgorithm: Send + Sync {
    /// Plugin name (must be unique, used for manual selection)
    fn name(&self) -> &'static str;

    /// Plugin metadata (performance characteristics and identification)
    fn metadata(&self) -> PluginMetadata;

    /// Compress input data
    ///
    /// # Parameters
    ///
    /// - `input`: Raw uncompressed data to compress
    /// - `cancel_flag`: Atomic flag for cooperative cancellation
    ///   Plugins SHOULD check `cancel_flag.load(Ordering::Acquire)` periodically
    ///   and return `Err(PluginError::Cancelled)` if set to `true`
    ///
    /// # Returns
    ///
    /// Compressed data without any header (header is added by the library)
    ///
    /// # Errors
    ///
    /// - `PluginError::Cancelled` if `cancel_flag` is set
    /// - `PluginError::OperationFailed` for compression errors
    /// - `ValidationError::CorruptedData` if input is invalid
    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>>;

    /// Decompress compressed data
    ///
    /// # Parameters
    ///
    /// - `input`: Compressed data (without Crush header, header already removed by library)
    /// - `cancel_flag`: Atomic flag for cooperative cancellation
    ///
    /// # Returns
    ///
    /// Original uncompressed data
    ///
    /// # Errors
    ///
    /// - `PluginError::Cancelled` if `cancel_flag` is set
    /// - `PluginError::OperationFailed` for decompression errors
    /// - `ValidationError::CorruptedData` if compressed data is invalid
    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>>;

    /// Detect if this plugin can handle the given file header
    ///
    /// This method determines if the plugin supports compressing a particular file type.
    /// Implementation is plugin-specific and may check:
    /// - File extension (if passed as metadata)
    /// - Magic bytes at the start of the file
    /// - Content analysis
    ///
    /// For decompression routing, the library uses the magic number in the Crush header,
    /// so this method is primarily for compression-time file type detection.
    ///
    /// # Parameters
    ///
    /// - `file_header`: First bytes of the file (typically 512-4096 bytes)
    ///
    /// # Returns
    ///
    /// `true` if this plugin can compress this file type, `false` otherwise
    ///
    /// # Performance Note
    ///
    /// This method should execute in sub-millisecond time as it may be called
    /// for every plugin during file type detection.
    fn detect(&self, file_header: &[u8]) -> bool;
}
