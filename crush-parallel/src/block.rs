//! Per-block compression and decompression helpers.

use crate::config::EngineConfiguration;
use crate::format::{BlockFlags, BlockHeader};
use crush_core::error::{CrushError, Result};
use libdeflater::{CompressionLvl, Compressor, Decompressor};

/// Result of compressing a single block.
pub struct CompressedBlock {
    pub header: BlockHeader,
    pub payload: Vec<u8>,
}

/// Compress one block of input data.
///
/// If the compressed output would exceed `config.max_expansion_ratio * input.len()`,
/// the block is stored uncompressed (raw) with the `stored` flag set.
///
/// # Errors
///
/// Returns an error if DEFLATE encoding fails internally.
pub fn compress_block(
    input: &[u8],
    block_index: usize,
    config: &EngineConfiguration,
) -> Result<CompressedBlock> {
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

    // Map 0-9 compression level to libdeflater's CompressionLvl (valid range 0-12).
    let lvl = CompressionLvl::new(i32::from(config.compression_level)).map_err(|_| {
        CrushError::InvalidConfig(format!(
            "invalid compression level {}",
            config.compression_level
        ))
    })?;

    let mut compressor = Compressor::new(lvl);
    // deflate_compress_bound gives the exact worst-case output size — no over-allocation.
    let buf_size = compressor.deflate_compress_bound(in_len);
    let mut compressed = vec![0u8; buf_size];

    let bytes_written = compressor
        .deflate_compress(input, &mut compressed)
        .map_err(|e| {
            CrushError::InvalidFormat(format!(
                "DEFLATE encode error at block {block_index}: {e:?}"
            ))
        })?;
    compressed.truncate(bytes_written);

    // Fall back to raw storage if compressed output is not smaller enough.
    // Precision loss from usize→f64 is acceptable for this ratio heuristic.
    #[allow(clippy::cast_precision_loss)]
    let use_stored =
        in_len > 0 && (compressed.len() as f64 / in_len as f64) > config.max_expansion_ratio;

    let (payload, flags, cs) = if use_stored {
        (
            input.to_vec(),
            BlockFlags::default().with_stored(),
            uncompressed_size,
        )
    } else {
        let cs = u32::try_from(compressed.len()).map_err(|_| {
            CrushError::InvalidConfig(format!("compressed block {block_index} exceeds u32::MAX"))
        })?;
        (compressed, BlockFlags::default(), cs)
    };

    Ok(CompressedBlock {
        header: BlockHeader {
            compressed_size: cs,
            uncompressed_size,
            checksum,
            flags,
        },
        payload,
    })
}

/// Decompress one block payload.
///
/// If the block has the `stored` flag, the payload is returned as-is after
/// checksum verification.
///
/// # Errors
///
/// - [`CrushError::ChecksumMismatch`] if checksums are enabled and CRC32 fails.
/// - [`CrushError::InvalidFormat`] if DEFLATE decoding fails.
pub fn decompress_block_payload(
    header: &BlockHeader,
    payload: &[u8],
    block_index: u64,
    checksums_enabled: bool,
) -> Result<Vec<u8>> {
    let decompressed: Vec<u8> = if header.flags.stored() {
        payload.to_vec()
    } else {
        // Cast is safe: uncompressed_size is validated to fit in u32 at compress time.
        #[allow(clippy::cast_possible_truncation)]
        let expected_size = header.uncompressed_size as usize;
        let mut out = vec![0u8; expected_size];
        let mut decompressor = Decompressor::new();

        let bytes_out = decompressor
            .deflate_decompress(payload, &mut out)
            .map_err(|e| {
                CrushError::InvalidFormat(format!(
                    "DEFLATE decode error at block {block_index}: {e:?}"
                ))
            })?;
        out.truncate(bytes_out);
        out
    };

    if checksums_enabled && header.checksum != 0 {
        let actual = crc32fast::hash(&decompressed);
        if actual != header.checksum {
            return Err(CrushError::ChecksumMismatch {
                block_index,
                expected: header.checksum,
                actual,
            });
        }
    }

    Ok(decompressed)
}
