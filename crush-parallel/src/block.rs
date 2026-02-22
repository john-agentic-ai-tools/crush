//! Per-block compression and decompression helpers.

use crate::config::EngineConfiguration;
use crate::format::{BlockFlags, BlockHeader};
use crush_core::error::{CrushError, Result};
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};

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

    // Attempt DEFLATE compression
    let level = Compression::new(u32::from(config.compression_level));
    let mut deflater = Compress::new(level, false);
    // Conservative DEFLATE upper-bound: zlib's compressBound formula adds ~0.03%
    // plus 13 bytes, but dynamic Huffman tables can add up to a few hundred bytes
    // across multiple internal deflate blocks.  12.5% + 1 KiB headroom covers all
    // compression levels safely without looping/retrying.
    let buf_size = in_len.saturating_add(in_len >> 3).saturating_add(1024);
    let mut compressed = vec![0u8; buf_size];

    let status = deflater
        .compress(input, &mut compressed, FlushCompress::Finish)
        .map_err(|e| {
            CrushError::InvalidFormat(format!("DEFLATE encode error at block {block_index}: {e}"))
        })?;

    if status != Status::StreamEnd {
        return Err(CrushError::InvalidFormat(format!(
            "DEFLATE encode did not reach StreamEnd at block {block_index}"
        )));
    }

    let bytes_written = usize::try_from(deflater.total_out()).map_err(|_| {
        CrushError::InvalidFormat(format!(
            "DEFLATE total_out overflows usize at block {block_index}"
        ))
    })?;
    compressed.truncate(bytes_written);

    // Fall back to raw storage if compressed is not smaller enough.
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
        let mut decompress = Decompress::new(false);

        let status = decompress
            .decompress(payload, &mut out, FlushDecompress::Finish)
            .map_err(|e| {
                CrushError::InvalidFormat(format!(
                    "DEFLATE decode error at block {block_index}: {e}"
                ))
            })?;

        if status != Status::StreamEnd {
            return Err(CrushError::InvalidFormat(format!(
                "DEFLATE decode did not reach StreamEnd at block {block_index}"
            )));
        }

        let bytes_out = usize::try_from(decompress.total_out()).map_err(|_| {
            CrushError::InvalidFormat(format!(
                "DEFLATE total_out overflows usize at block {block_index}"
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
