//! Plugin contract trait definition
//!
//! This module defines the `CompressionAlgorithm` trait that all plugins must implement.
//! Plugins register themselves at compile-time using the `linkme` distributed slice pattern.

use crate::error::Result;
use crate::plugin::PluginMetadata;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

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

    /// Compress `input`, appending the payload to `out` after the bytes it
    /// already holds (the library's header prefix).
    ///
    /// The provided implementation calls [`Self::compress`] and copies its
    /// result. Plugins that can write their output in place override it to
    /// save that copy; the bytes appended must equal what `compress` returns.
    ///
    /// # Errors
    ///
    /// As for [`Self::compress`]. On error, `out` may hold a partial payload.
    fn compress_into(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let payload = self.compress(input, cancel_flag)?;
        out.extend_from_slice(&payload);
        Ok(())
    }

    /// True if [`Self::compress_to`] writes its payload piece by piece. Only
    /// then does `crush_core::compress_to_writer` stream; for other plugins
    /// it compresses in memory (with a parallel CRC) and writes once.
    fn streams_compressed_output(&self) -> bool {
        false
    }

    /// Compress `input`, writing the payload to `out`; returns the number of
    /// bytes written. The bytes must equal what [`Self::compress`] returns.
    ///
    /// The provided implementation calls [`Self::compress`] and writes the
    /// result. Plugins that compress in independent pieces override it (and
    /// [`Self::streams_compressed_output`]) so the whole payload is never
    /// held in memory.
    ///
    /// # Errors
    ///
    /// As for [`Self::compress`], plus I/O errors from `out`. On error, `out`
    /// may have received part of the payload.
    fn compress_to(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        out: &mut dyn std::io::Write,
    ) -> Result<u64> {
        let payload = self.compress(input, cancel_flag)?;
        out.write_all(&payload)?;
        Ok(payload.len() as u64)
    }

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

    /// Decompress `input`, writing the result to `out`; returns the number
    /// of bytes written.
    ///
    /// The provided implementation calls [`Self::decompress`] and writes its
    /// result. Plugins that decode in independent pieces override it to
    /// write each piece as it is decoded, so the whole result never has to
    /// be held in memory. The bytes written must equal what `decompress`
    /// returns.
    ///
    /// # Errors
    ///
    /// As for [`Self::decompress`], plus I/O errors from `out`. On error,
    /// `out` may have received part of the result.
    fn decompress_to(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        out: &mut dyn std::io::Write,
    ) -> Result<u64> {
        let data = self.decompress(input, cancel_flag)?;
        out.write_all(&data)?;
        Ok(data.len() as u64)
    }

    /// Exact decompressed length of `input`, when this plugin can decode into
    /// a caller-provided slice with [`Self::decompress_into`]. `None` means it
    /// cannot. All sizes must be validated here, because the caller allocates
    /// from the result.
    ///
    /// # Errors
    ///
    /// Returns an error if `input` is malformed.
    fn decompressed_len(&self, input: &[u8]) -> Result<Option<usize>> {
        let _ = input;
        Ok(None)
    }

    /// Decompress `input` into `output`, whose length is the value returned by
    /// [`Self::decompressed_len`].
    ///
    /// The provided implementation calls [`Self::decompress`] and copies its
    /// result. Plugins that decode in independent pieces override it to decode
    /// each piece straight into place.
    ///
    /// # Errors
    ///
    /// As for [`Self::decompress`]. On error, `output` may hold partial data.
    fn decompress_into(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        output: &mut [u8],
    ) -> Result<()> {
        let data = self.decompress(input, cancel_flag)?;
        if data.len() != output.len() {
            return Err(crate::error::PluginError::OperationFailed(format!(
                "decompressed {} bytes into a {}-byte output",
                data.len(),
                output.len()
            ))
            .into());
        }
        output.copy_from_slice(&data);
        Ok(())
    }

    /// Decompress only block `block_n` of `input`. `None` means this plugin
    /// has no random access.
    ///
    /// # Errors
    ///
    /// Returns an error if `input` is malformed or `block_n` is out of range.
    fn decompress_block(&self, input: &[u8], block_n: u64) -> Result<Option<Vec<u8>>> {
        let _ = (input, block_n);
        Ok(None)
    }

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

    /// True if automatic selection may use this plugin only after
    /// [`estimate_ratio`](Self::estimate_ratio) shows the input compresses
    /// well with it. Content-aware plugins are never picked by weight-based
    /// scoring alone, so registering one doesn't change what other callers get.
    /// Explicit selection by name is unaffected.
    fn content_aware(&self) -> bool {
        false
    }

    /// Cheap estimate of `input.len() / compressed_len` for this plugin, taken
    /// from samples of `input`. `None` means no estimate is available; a
    /// content-aware plugin is then not selected automatically.
    ///
    /// Implementations should cost no more than about 2% of compressing
    /// `input` with this plugin.
    fn estimate_ratio(&self, input: &[u8]) -> Option<f64> {
        let _ = input;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns its input reversed: enough to see where the payload lands.
    struct Reverse;

    impl CompressionAlgorithm for Reverse {
        fn name(&self) -> &'static str {
            "reverse"
        }
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                name: "reverse",
                version: "0",
                magic_number: [0x43, 0x52, 0x01, 0x7F],
                throughput: 1.0,
                compression_ratio: 1.0,
                description: "test",
            }
        }
        fn compress(&self, input: &[u8], _: Arc<AtomicBool>) -> Result<Vec<u8>> {
            Ok(input.iter().rev().copied().collect())
        }
        fn decompress(&self, input: &[u8], _: Arc<AtomicBool>) -> Result<Vec<u8>> {
            Ok(input.iter().rev().copied().collect())
        }
        fn detect(&self, _: &[u8]) -> bool {
            false
        }
    }

    /// The provided `compress_into` keeps the caller's prefix and appends
    /// exactly what `compress` returns.
    #[test]
    #[allow(clippy::expect_used)]
    fn default_compress_into_appends_compress_output() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut out = b"prefix".to_vec();
        Reverse
            .compress_into(b"abc", Arc::clone(&flag), &mut out)
            .expect("compress_into");
        assert_eq!(out, b"prefixcba");
    }
}
