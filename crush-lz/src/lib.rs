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
mod estimate;

use crate::format::{DEFAULT_BLOCK_SIZE_LOG2, FrameHeader};
use crush_core::error::Result;
use crush_core::plugin::{COMPRESSION_ALGORITHMS, CompressionAlgorithm, PluginMetadata};
use linkme::distributed_slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Codec configuration. Build with [`FastLzConfig::builder`].
///
/// # Examples
///
/// ```
/// use crush_lz::FastLzConfig;
///
/// // 256-kilobyte blocks instead of the default 1 megabyte
/// let config = FastLzConfig::builder().block_size_log2(18).build()?;
/// assert_eq!(config.block_size(), 256 * 1024);
/// assert!(FastLzConfig::builder().block_size_log2(21).build().is_err());
/// # Ok::<(), crush_core::CrushError>(())
/// ```
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
/// `expected_len` bytes. The output is reserved up front from
/// `expected_len`, but never beyond what `payload` could expand to (256
/// bytes per byte), so a forged length cannot force a huge allocation.
///
/// # Examples
///
/// ```
/// use crush_lz::{FastLzConfig, compress, decompress};
///
/// let data = vec![7u8; 100_000];
/// let packed = compress(&data, &FastLzConfig::default())?;
/// assert_eq!(decompress(&packed, data.len())?, data);
/// // the wrong length is an error, not a short or padded result
/// assert!(decompress(&packed, data.len() + 1).is_err());
/// # Ok::<(), crush_core::CrushError>(())
/// ```
pub fn decompress(payload: &[u8], expected_len: usize) -> Result<Vec<u8>> {
    decompress::decode_frame(payload, Some(expected_len), None)
}

/// The `fast-lz` plugin registered with crush-core.
///
/// Linking this crate registers it; select it by name, or let `--level fast`
/// style weights pick it for data its estimate shows compresses well.
///
/// # Examples
///
/// ```
/// use crush_core::{CompressionOptions, compress_with_options, decompress, init_plugins};
/// use crush_lz as _; // registers fast-lz
///
/// init_plugins()?;
/// let data = b"one line of a log file\n".repeat(1000);
/// let options = CompressionOptions::default().with_plugin("fast-lz");
/// let packed = compress_with_options(&data, &options)?;
/// assert_eq!(decompress(&packed)?.data, data);
/// # Ok::<(), crush_core::CrushError>(())
/// ```
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

    fn compress_into(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        compress::compress_frame_into(input, FrameHeader::default(), Some(&cancel_flag), out)
    }

    fn streams_compressed_output(&self) -> bool {
        true
    }

    fn compress_to(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        out: &mut dyn std::io::Write,
    ) -> Result<u64> {
        compress::compress_frame_to(input, FrameHeader::default(), Some(&cancel_flag), out)
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        // crush-core checks the total against the outer header's original size.
        decompress::decode_frame(input, None, Some(&cancel_flag))
    }

    fn decompress_to(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        out: &mut dyn std::io::Write,
    ) -> Result<u64> {
        decompress::decode_frame_to(input, Some(&cancel_flag), out)
    }

    fn detect(&self, file_header: &[u8]) -> bool {
        file_header.get(..4) == Some(&PLUGIN_MAGIC[..])
    }

    fn content_aware(&self) -> bool {
        true
    }

    /// Compresses evenly spaced samples (about 1/64 of `input`) and returns
    /// their combined ratio; `None` for empty input.
    fn estimate_ratio(&self, input: &[u8]) -> Option<f64> {
        estimate::estimate_ratio(input)
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

    /// `compress_into` writes the same payload as `compress`, after
    /// whatever the caller already put in the buffer (feature 023, T039).
    #[test]
    fn compress_into_appends_the_compress_output() {
        let data: Vec<u8> = b"fast-lz writes in place "
            .iter()
            .copied()
            .cycle()
            .take(3 << 20)
            .collect();
        let alone = FastLzPlugin.compress(&data, live()).expect("compress");
        for prefix_len in [0usize, 1, 20, 4096] {
            let mut out = vec![0xA5; prefix_len];
            FastLzPlugin
                .compress_into(&data, live(), &mut out)
                .expect("compress_into");
            assert_eq!(&out[..prefix_len], &vec![0xA5; prefix_len][..]);
            assert_eq!(&out[prefix_len..], &alone[..], "prefix {prefix_len}");
        }
    }

    /// `decompress_to` writes exactly what `decompress` returns, across
    /// compressed and stored blocks (feature 023, T040).
    #[test]
    fn decompress_to_writes_the_decompress_output() {
        let mut data: Vec<u8> = b"stream me block by block "
            .iter()
            .copied()
            .cycle()
            .take(3 << 20)
            .collect();
        // an incompressible megabyte in the middle becomes a stored block
        let mut s = 0x2545_F491_4F6C_DD1Du64;
        for b in &mut data[(1 << 20)..(2 << 20)] {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *b = s.to_le_bytes()[0];
        }
        let packed = FastLzPlugin.compress(&data, live()).expect("compress");
        let whole = FastLzPlugin
            .decompress(&packed, live())
            .expect("decompress");
        let mut sink = Vec::new();
        let n = FastLzPlugin
            .decompress_to(&packed, live(), &mut sink)
            .expect("decompress_to");
        assert_eq!(n, data.len() as u64);
        assert_eq!(sink, whole);
        assert_eq!(sink, data);

        // First block header (after the 8-byte frame header): a compressed
        // length past the end of the payload. (Stored-block contents can't
        // be checked by the codec; crush-core's CRC covers those.)
        let mut bad = packed.clone();
        bad[8..12].copy_from_slice(&0x7FFF_FFF0u32.to_le_bytes());
        let mut sink = Vec::new();
        assert!(FastLzPlugin.decompress_to(&bad, live(), &mut sink).is_err());
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
