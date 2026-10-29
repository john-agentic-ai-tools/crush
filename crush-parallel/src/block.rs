//! Per-block compression and decompression helpers.

use crate::config::EngineConfiguration;
use crate::format::{BlockFlags, BlockHeader};
use crush_core::error::{CrushError, Result};
use libdeflater::{CompressionLvl, Compressor, Decompressor};
use std::borrow::Cow;

/// Result of compressing a single block.
pub struct CompressedBlock {
    pub header: BlockHeader,
    pub payload: Vec<u8>,
}

/// Result of compressing a single block for the engine: a stored block's
/// payload borrows the input instead of copying it.
pub(crate) struct BlockOutput<'a> {
    pub header: BlockHeader,
    pub payload: Cow<'a, [u8]>,
}

/// Resolve a config-level `compression_level` (0-9) to libdeflater's `CompressionLvl`.
///
/// Exposed to `engine.rs` so the resolution happens once on the driver thread,
/// not once per block.
///
/// # Errors
///
/// Returns [`CrushError::InvalidConfig`] if `level` is outside libdeflater's 0-12 range.
pub(crate) fn resolve_compression_level(level: u8) -> Result<CompressionLvl> {
    CompressionLvl::new(i32::from(level))
        .map_err(|_| CrushError::InvalidConfig(format!("invalid compression level {level}")))
}

/// Compress one block of input data using a caller-provided, worker-pooled `Compressor`.
///
/// If the compressed output would exceed `config.max_expansion_ratio * input.len()`,
/// the block is stored uncompressed (raw) with the `stored` flag set.
///
/// # Errors
///
/// Returns an error if DEFLATE encoding fails internally.
pub fn compress_block(
    compressor: &mut Compressor,
    input: &[u8],
    block_index: usize,
    config: &EngineConfiguration,
) -> Result<CompressedBlock> {
    let mut scratch = Vec::new();
    let out = compress_block_with_scratch(compressor, &mut scratch, input, block_index, config)?;
    Ok(CompressedBlock {
        header: out.header,
        payload: out.payload.into_owned(),
    })
}

/// [`compress_block`] for the engine's workers. DEFLATE writes into the
/// worker's `scratch` buffer. A stored block borrows `input` and leaves
/// `scratch` for the next block (no allocation, no copy); a compressed block
/// takes `scratch` as its payload.
///
/// # Errors
///
/// Returns an error if DEFLATE encoding fails internally.
pub(crate) fn compress_block_with_scratch<'a>(
    compressor: &mut Compressor,
    scratch: &mut Vec<u8>,
    input: &'a [u8],
    block_index: usize,
    config: &EngineConfiguration,
) -> Result<BlockOutput<'a>> {
    let checksum = if config.checksums {
        crc32fast::hash(input)
    } else {
        0
    };

    let in_len = input.len();
    let uncompressed_size = u32::try_from(in_len).map_err(|_| {
        CrushError::InvalidConfig(format!(
            "block {block_index} input length {in_len} exceeds u32::MAX"
        ))
    })?;

    // deflate_compress_bound gives the exact worst-case output size. A fresh
    // zeroed Vec (calloc) is cheaper than resizing, which memsets.
    let buf_size = compressor.deflate_compress_bound(in_len);
    if scratch.len() < buf_size {
        *scratch = vec![0u8; buf_size];
    }

    let bytes_written = compressor
        .deflate_compress(input, &mut scratch[..buf_size])
        .map_err(|e| {
            CrushError::InvalidFormat(format!(
                "DEFLATE encode error at block {block_index}: {e:?}"
            ))
        })?;
    debug_assert!(
        bytes_written <= buf_size,
        "libdeflater returned {bytes_written} > capacity {buf_size}"
    );

    // Fall back to raw storage if compressed output is not smaller enough.
    // Precision loss from usize→f64 is acceptable for this ratio heuristic.
    #[allow(clippy::cast_precision_loss)]
    let use_stored =
        in_len > 0 && (bytes_written as f64 / in_len as f64) > config.max_expansion_ratio;

    let (payload, flags, cs) = if use_stored {
        (
            Cow::Borrowed(input),
            BlockFlags::default().with_stored(),
            uncompressed_size,
        )
    } else {
        let cs = u32::try_from(bytes_written).map_err(|_| {
            CrushError::InvalidConfig(format!("compressed block {block_index} exceeds u32::MAX"))
        })?;
        // Hand the buffer itself over instead of copying the output out of it;
        // the worker allocates a new one for its next block.
        let mut payload = std::mem::take(scratch);
        payload.truncate(bytes_written);
        (Cow::Owned(payload), BlockFlags::default(), cs)
    };

    Ok(BlockOutput {
        header: BlockHeader {
            compressed_size: cs,
            uncompressed_size,
            checksum,
            flags,
        },
        payload,
    })
}

/// Decompress one block payload directly into a caller-provided output slice.
///
/// `output.len()` MUST equal `header.uncompressed_size`.
///
/// This is the hot-path variant: worker threads call it via `par_iter_mut` over
/// disjoint slices of the final output buffer, so there is no per-block
/// `Vec<u8>` allocation and no final copy into place.
///
/// # Errors
///
/// - [`CrushError::InvalidFormat`] if `output.len()` disagrees with `header.uncompressed_size`,
///   if the decoder reports a length mismatch, or if DEFLATE decoding fails.
/// - [`CrushError::ChecksumMismatch`] if checksums are enabled and CRC32 fails.
pub fn decompress_block_into(
    decompressor: &mut Decompressor,
    header: &BlockHeader,
    payload: &[u8],
    output: &mut [u8],
    block_index: u64,
    checksums_enabled: bool,
) -> Result<()> {
    let expected_size = header.uncompressed_size as usize;
    if output.len() != expected_size {
        return Err(CrushError::InvalidFormat(format!(
            "block {block_index} output buffer size {} != header uncompressed_size {expected_size}",
            output.len()
        )));
    }

    if header.flags.stored() {
        if payload.len() != expected_size {
            return Err(CrushError::InvalidFormat(format!(
                "stored block {block_index} payload length {} != uncompressed_size {expected_size}",
                payload.len()
            )));
        }
        output.copy_from_slice(payload);
    } else {
        let bytes_out = decompressor
            .deflate_decompress(payload, output)
            .map_err(|e| {
                CrushError::InvalidFormat(format!(
                    "DEFLATE decode error at block {block_index}: {e:?}"
                ))
            })?;
        if bytes_out != expected_size {
            return Err(CrushError::InvalidFormat(format!(
                "block {block_index} uncompressed size mismatch: header {expected_size} vs decoded {bytes_out}"
            )));
        }
    }

    if checksums_enabled && header.checksum != 0 {
        let actual = crc32fast::hash(output);
        if actual != header.checksum {
            return Err(CrushError::ChecksumMismatch {
                block_index,
                expected: header.checksum,
                actual,
            });
        }
    }

    Ok(())
}

/// Allocating wrapper around [`decompress_block_into`] — convenience for callers
/// that want an owned `Vec<u8>` (e.g. random-access `decompress_block`).
///
/// The hot path in `engine::decompress_from_reader` does NOT use this; it calls
/// `decompress_block_into` against a pre-allocated slice.
///
/// # Errors
///
/// Same as [`decompress_block_into`].
pub fn decompress_block_payload(
    decompressor: &mut Decompressor,
    header: &BlockHeader,
    payload: &[u8],
    block_index: u64,
    checksums_enabled: bool,
) -> Result<Vec<u8>> {
    let expected_size = header.uncompressed_size as usize;
    let mut out = vec![0u8; expected_size];
    decompress_block_into(
        decompressor,
        header,
        payload,
        &mut out,
        block_index,
        checksums_enabled,
    )?;
    Ok(out)
}
