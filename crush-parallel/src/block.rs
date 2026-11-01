//! Per-block framing around a [`BlockCodec`]: checksums and stored blocks.

use crate::codec::BlockCodec;
use crate::config::EngineConfiguration;
use crate::format::{BlockFlags, BlockHeader};
use crush_core::error::{CrushError, Result};
use std::borrow::Cow;

/// Result of compressing a single block for the engine: a stored block's
/// payload borrows the input instead of copying it.
pub(crate) struct BlockOutput<'a> {
    pub header: BlockHeader,
    pub payload: Cow<'a, [u8]>,
}

/// Compress one block, using the worker's `encoder` and `scratch` buffer.
///
/// A stored block borrows `input` and leaves `scratch` for the next block (no
/// allocation, no copy); a compressed block takes `scratch` as its payload.
///
/// If the compressed output would exceed `config.max_expansion_ratio * input.len()`,
/// the block is stored uncompressed with the `stored` flag set.
///
/// # Errors
///
/// Returns an error if the codec fails or a size overflows `u32`.
pub(crate) fn compress_block<'a, C: BlockCodec>(
    codec: &C,
    encoder: &mut C::Encoder,
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

    let compressed = codec.compress_block(encoder, input, scratch)?;

    // Precision loss from usize→f64 is acceptable for this ratio heuristic.
    #[allow(clippy::cast_precision_loss)]
    let use_stored = !compressed
        || (in_len > 0 && (scratch.len() as f64 / in_len as f64) > config.max_expansion_ratio);

    if use_stored {
        return Ok(BlockOutput {
            header: BlockHeader {
                compressed_size: uncompressed_size,
                uncompressed_size,
                checksum,
                flags: BlockFlags::default().with_stored(),
            },
            payload: Cow::Borrowed(input),
        });
    }

    let compressed_size = u32::try_from(scratch.len()).map_err(|_| {
        CrushError::InvalidConfig(format!("compressed block {block_index} exceeds u32::MAX"))
    })?;
    // Hand the buffer itself over instead of copying the output out of it;
    // the worker allocates a new one for its next block.
    Ok(BlockOutput {
        header: BlockHeader {
            compressed_size,
            uncompressed_size,
            checksum,
            flags: BlockFlags::default(),
        },
        payload: Cow::Owned(std::mem::take(scratch)),
    })
}

/// Decompress one block payload directly into a caller-provided output slice.
///
/// This is the hot path: worker threads call it over disjoint slices of the
/// final output buffer, so there is no per-block allocation and no final copy.
///
/// # Errors
///
/// - [`CrushError::InvalidFormat`] if `output.len()` disagrees with
///   `header.uncompressed_size`, or the payload is malformed.
/// - [`CrushError::ChecksumMismatch`] if checksums are enabled and CRC32 fails.
pub(crate) fn decompress_block_into<C: BlockCodec>(
    codec: &C,
    decoder: &mut C::Decoder,
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
        codec
            .decompress_block(decoder, payload, output)
            .map_err(|e| match e {
                CrushError::InvalidFormat(m) => {
                    CrushError::InvalidFormat(format!("block {block_index}: {m}"))
                }
                other => other,
            })?;
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
