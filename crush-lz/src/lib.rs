//! `crush-lz` — the `fast-lz` compression plugin for Crush.
//!
//! An LZ4-class codec: byte-oriented `LZ77` sequences with a 64-kilobyte
//! window and no entropy coding, compressed in independent blocks. It trades
//! ratio for single-core speed. The format is described in
//! `specs/023-lz4-fast-plugin/contracts/fast-lz-format.md`.
pub mod format;

/// Plugin name used for explicit selection (`--plugin fast-lz`).
pub const PLUGIN_NAME: &str = "fast-lz";

/// Crush outer-format magic for `fast-lz` payloads (`"CR"`, version 1, plugin 0x04).
pub const PLUGIN_MAGIC: [u8; 4] = [0x43, 0x52, 0x01, 0x04];

mod compress;
mod decompress;

use crate::format::{DEFAULT_BLOCK_SIZE_LOG2, FrameHeader};
use crush_core::error::Result;
use crush_core::plugin::{COMPRESSION_ALGORITHMS, CompressionAlgorithm, PluginMetadata};
use linkme::distributed_slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Codec configuration. Build with [`FastLzConfig::builder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FastLzConfig {
    frame: FrameHeader,
}

impl FastLzConfig {
    /// Start a builder with the defaults (1 megabyte blocks).
    #[must_use]
    pub fn builder() -> FastLzConfigBuilder {
        FastLzConfigBuilder {
            block_size_log2: DEFAULT_BLOCK_SIZE_LOG2,
        }
    }

    /// Block size in bytes.
    #[must_use]
    pub fn block_size(&self) -> usize {
        self.frame.block_size()
    }
}

/// Builder for [`FastLzConfig`].
#[derive(Debug, Clone, Copy)]
pub struct FastLzConfigBuilder {
    block_size_log2: u8,
}

impl FastLzConfigBuilder {
    /// Block size as a power of two, 16..=20 (64 kilobytes to 1 megabyte).
    #[must_use]
    pub fn block_size_log2(mut self, log2: u8) -> Self {
        self.block_size_log2 = log2;
        self
    }

    /// Validate and build.
    ///
    /// # Errors
    ///
    /// `CorruptedData` if the block size is outside 16..=20.
    pub fn build(self) -> Result<FastLzConfig> {
        Ok(FastLzConfig {
            frame: FrameHeader::new(self.block_size_log2)?,
        })
    }
}

/// Compress `input` into a `fast-lz` payload.
///
/// # Errors
///
/// Only if the configuration is invalid; compression itself cannot fail.
///
/// # Examples
///
/// ```
/// use crush_lz::{FastLzConfig, compress, decompress};
///
/// let data = b"to be or not to be, that is the question. ".repeat(100);
/// let packed = compress(&data, &FastLzConfig::default())?;
/// assert!(packed.len() < data.len());
/// assert_eq!(decompress(&packed, data.len())?, data);
/// # Ok::<(), crush_core::CrushError>(())
/// ```
pub fn compress(input: &[u8], config: &FastLzConfig) -> Result<Vec<u8>> {
    compress::compress_frame(input, config.frame, None)
}

/// Decompress a `fast-lz` payload that decodes to exactly `expected_len` bytes.
///
/// # Errors
///
/// `CorruptedData` if the payload is malformed or doesn't decode to
/// `expected_len` bytes. Memory grows block by block, so a payload claiming a
/// huge size fails without allocating it.
pub fn decompress(payload: &[u8], expected_len: usize) -> Result<Vec<u8>> {
    decompress::decode_frame(payload, Some(expected_len), None)
}

/// The `fast-lz` plugin registered with crush-core.
pub struct FastLzPlugin;

impl CompressionAlgorithm for FastLzPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: PLUGIN_NAME,
            version: env!("CARGO_PKG_VERSION"),
            magic_number: PLUGIN_MAGIC,
            throughput: 900.0,
            compression_ratio: 0.44,
            description: "LZ4-class single-core codec: fastest, lower ratio; picked automatically only for compressible data at --level fast",
        }
    }

    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        compress::compress_frame(input, FrameHeader::default(), Some(&cancel_flag))
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        // crush-core checks the total against the outer header's original size.
        decompress::decode_frame(input, None, Some(&cancel_flag))
    }

    fn detect(&self, file_header: &[u8]) -> bool {
        file_header.get(..4) == Some(&PLUGIN_MAGIC[..])
    }

    fn content_aware(&self) -> bool {
        true
    }
}

#[distributed_slice(COMPRESSION_ALGORITHMS)]
static FAST_LZ_PLUGIN: &dyn CompressionAlgorithm = &FastLzPlugin;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod plugin_tests {
    use super::*;
    use crush_core::plugin::CompressionAlgorithm;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn live() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn identity_and_detection() {
        let p = FastLzPlugin;
        assert_eq!(p.name(), "fast-lz");
        let m = p.metadata();
        assert_eq!(m.name, "fast-lz");
        assert_eq!(m.magic_number, [0x43, 0x52, 0x01, 0x04]);
        assert!(p.detect(&[0x43, 0x52, 0x01, 0x04, 0, 0]));
        assert!(!p.detect(&[0x43, 0x52, 0x01, 0x02]));
        assert!(!p.detect(&[0x43, 0x52]));
    }

    #[test]
    fn registered_and_round_trips_through_crush_core() {
        crush_core::init_plugins().expect("init");
        assert!(
            crush_core::list_plugins()
                .iter()
                .any(|p| p.name == "fast-lz")
        );
        let data: Vec<u8> = b"the quick brown fox jumps over the lazy dog. "
            .iter()
            .copied()
            .cycle()
            .take(300_000)
            .collect();
        let options = crush_core::CompressionOptions::default().with_plugin("fast-lz");
        let packed = crush_core::compress_with_options(&data, &options).expect("compress");
        assert_eq!(packed[..4], PLUGIN_MAGIC);
        // No plugin named: the outer magic routes it (FR-003).
        let out = crush_core::decompress(&packed).expect("decompress");
        assert_eq!(out.data, data);
    }

    #[test]
    fn cancellation_is_honoured() {
        let data = vec![7u8; 3 << 20];
        let cancel = Arc::new(AtomicBool::new(true));
        let err = FastLzPlugin.compress(&data, cancel).expect_err("cancelled");
        assert!(matches!(
            err,
            crush_core::CrushError::Plugin(crush_core::PluginError::Cancelled)
        ));
        let packed = FastLzPlugin.compress(&data, live()).expect("compress");
        assert_eq!(
            FastLzPlugin
                .decompress(&packed, live())
                .expect("decompress"),
            data
        );
    }
}
