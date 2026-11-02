//! Sprint: a hybrid high-speed codec for Crush.
//!
//! The format contract is in `specs/026-hybrid-speed-codec/contracts/sprint-format.md`.

#![forbid(unsafe_code)]
// Hot paths are inlined by hand, and every length here is bounded by a block.
#![allow(
    clippy::inline_always,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

pub mod bits;
pub mod codec;
pub mod decompress;
pub mod format;
pub mod histogram;
pub mod huffman;
pub mod matcher;
pub mod sequences;
pub mod tans;

pub use codec::{CODEC_ID, SprintCodec};

use crush_core::error::Result;
use crush_core::plugin::{COMPRESSION_ALGORITHMS, CompressionAlgorithm, PluginMetadata};
use crush_parallel::engine::{self, decompressed_size};
use crush_parallel::format::CRSH_MAGIC;
use crush_parallel::{EngineConfiguration, decompress_block_with, load_index};
use linkme::distributed_slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Plugin name used for explicit selection (`--plugin sprint`).
pub const PLUGIN_NAME: &str = "sprint";

/// Crush outer-format magic for sprint files (`"CR"`, version 1, plugin 0x05).
pub const PLUGIN_MAGIC: [u8; 4] = [0x43, 0x52, 0x01, 0x05];

/// Offset of the codec id in the CRSH file header.
const CODEC_ID_BYTE: usize = 22;
/// An RLE segment turns 4 bytes into 128 `KiB`, so the engine's limit must allow that.
const MAX_DECOMPRESSION_RATIO: f64 = 40_000.0;

fn config(cancel_flag: Option<Arc<AtomicBool>>) -> Result<EngineConfiguration> {
    let builder = EngineConfiguration::builder()
        .block_size(format::BLOCK_SIZE as u32)
        .max_decompression_ratio(MAX_DECOMPRESSION_RATIO);
    match cancel_flag {
        Some(flag) => builder.cancel_flag(flag).build(),
        None => builder.build(),
    }
}

/// The sprint plugin registered with crush-core.
///
/// Linking this crate registers it. It is chosen only by name until the
/// speed gates pass.
///
/// # Examples
///
/// ```
/// use crush_core::{CompressionOptions, compress_with_options, decompress, init_plugins};
/// use crush_sprint as _; // registers sprint
///
/// init_plugins()?;
/// let data = b"one line of a log file\n".repeat(1000);
/// let options = CompressionOptions::default().with_plugin("sprint");
/// let packed = compress_with_options(&data, &options)?;
/// assert_eq!(decompress(&packed)?.data, data);
/// # Ok::<(), crush_core::CrushError>(())
/// ```
pub struct SprintPlugin;

impl CompressionAlgorithm for SprintPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: PLUGIN_NAME,
            version: env!("CARGO_PKG_VERSION"),
            magic_number: PLUGIN_MAGIC,
            throughput: 300.0,
            compression_ratio: 0.27,
            description: "Hybrid speed codec: separated streams, Huffman literals, tANS sequences, parallel blocks; opt-in",
        }
    }

    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        engine::compress_with(&SprintCodec, input, &config(Some(cancel_flag))?)
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        engine::decompress_with(&SprintCodec, input, &config(Some(cancel_flag))?)
    }

    fn decompressed_len(&self, input: &[u8]) -> Result<Option<usize>> {
        decompressed_size::<SprintCodec>(input, &config(None)?).map(Some)
    }

    fn decompress_into(
        &self,
        input: &[u8],
        cancel_flag: Arc<AtomicBool>,
        output: &mut [u8],
    ) -> Result<()> {
        engine::decompress_into(&SprintCodec, input, &config(Some(cancel_flag))?, output)
    }

    fn decompress_block(&self, input: &[u8], block_n: u64) -> Result<Option<Vec<u8>>> {
        let mut reader = std::io::Cursor::new(input);
        let index = load_index(&mut reader)?;
        decompress_block_with(&SprintCodec, &mut reader, &index, block_n, &config(None)?).map(Some)
    }

    fn block_count(&self, input: &[u8]) -> Result<Option<u64>> {
        load_index(&mut std::io::Cursor::new(input)).map(|index| Some(index.len()))
    }

    fn detect(&self, file_header: &[u8]) -> bool {
        file_header.get(..4) == Some(&CRSH_MAGIC[..])
            && file_header.get(CODEC_ID_BYTE) == Some(&CODEC_ID)
    }

    fn auto_selectable(&self) -> bool {
        false
    }
}

/// Compile-time plugin registration via `linkme`.
#[distributed_slice(COMPRESSION_ALGORITHMS)]
static SPRINT_PLUGIN: &dyn CompressionAlgorithm = &SprintPlugin;
