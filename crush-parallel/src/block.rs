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

    if config.incompressible_probe && looks_incompressible(compressor, scratch, input)? {
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

/// Bytes per probe sample.
const PROBE_SAMPLE_LEN: usize = 16 * 1024;
/// Samples per block, at evenly spaced offsets starting at 0.
const PROBE_SAMPLES: usize = 8;

/// True if no evenly spaced sample of `input` shrinks under DEFLATE and every
/// 4 KB window looks random byte-wise, so the block can be stored without a
/// full DEFLATE attempt.
///
/// Samples are tried from offset 0 and the first one that shrinks ends the
/// probe, so a compressible block usually costs one 16 KB sample (about 1.5%
/// of a 1 MB block's DEFLATE). Blocks shorter than all samples together are
/// not probed. `scratch` must hold at least `deflate_compress_bound(input.len())`
/// bytes.
fn looks_incompressible(
    compressor: &mut Compressor,
    scratch: &mut [u8],
    input: &[u8],
) -> Result<bool> {
    if input.len() < PROBE_SAMPLES * PROBE_SAMPLE_LEN {
        return Ok(false);
    }
    let sample_shrinks = |compressor: &mut Compressor, scratch: &mut [u8], k: usize| {
        let start = k * input.len() / PROBE_SAMPLES;
        let sample = &input[start..start + PROBE_SAMPLE_LEN];
        compressor
            .deflate_compress(sample, scratch)
            .map(|written| written < sample.len())
            .map_err(|e| CrushError::InvalidFormat(format!("DEFLATE encode error in probe: {e:?}")))
    };
    // Cheapest decisive checks first: one sample, then the byte-entropy scan
    // (stops at the first text-like window), then the remaining samples.
    // The samples miss whatever lies between them, which the scan covers.
    if sample_shrinks(compressor, scratch, 0)? || !windows_look_random(input) {
        return Ok(false);
    }
    for k in 1..PROBE_SAMPLES {
        if sample_shrinks(compressor, scratch, k)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Bytes per window for [`windows_look_random`].
const ENTROPY_WINDOW: usize = 4 * 1024;
/// Largest sum of squared byte counts a 4 KB window may have and still count
/// as random. Collision entropy H2 = -log2(sum(c^2) / n^2); the threshold is
/// H2 = 7.8 bits/byte, i.e. sum(c^2) <= 4096^2 * 2^-7.8 ≈ 75,281. Uniform
/// random data averages 69,616 (sd ≈ 360); text is around 4–6 bits/byte,
/// far above the threshold in sum(c^2). H2 <= Shannon entropy, so a window
/// that fails this may still be incompressible: the check only errs toward
/// running full DEFLATE.
const ENTROPY_WINDOW_MAX_COLLISIONS: u64 = 75_281;

/// True if every full 4 KB window of `input` has byte collision entropy of at
/// least 7.8 bits/byte. Integer-only: a histogram and a sum of squares.
fn windows_look_random(input: &[u8]) -> bool {
    let (windows, _tail) = input.as_chunks::<ENTROPY_WINDOW>();
    windows.iter().all(|window| {
        let mut counts = [0u32; 256];
        for &b in window {
            counts[usize::from(b)] += 1;
        }
        let collisions: u64 = counts.iter().map(|&c| u64::from(c) * u64::from(c)).sum();
        collisions <= ENTROPY_WINDOW_MAX_COLLISIONS
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn random_bytes(len: usize) -> Vec<u8> {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect()
    }

    #[test]
    fn entropy_guard_accepts_random_and_rejects_hidden_text() {
        let random = random_bytes(1024 * 1024);
        assert!(windows_look_random(&random));

        // 8 KB of text between probe samples: the samples alone miss it.
        let mut hidden = random.clone();
        for (i, b) in hidden[200_000..208_192].iter_mut().enumerate() {
            *b = b"plain old text "[i % 15];
        }
        assert!(!windows_look_random(&hidden));

        let mut compressor = Compressor::new(CompressionLvl::default());
        let mut scratch = vec![0u8; compressor.deflate_compress_bound(random.len())];
        assert!(looks_incompressible(&mut compressor, &mut scratch, &random).expect("probe"));
        assert!(!looks_incompressible(&mut compressor, &mut scratch, &hidden).expect("probe"));
    }

    #[test]
    fn probe_skips_blocks_smaller_than_its_samples() {
        let small = random_bytes(PROBE_SAMPLES * PROBE_SAMPLE_LEN - 1);
        let mut compressor = Compressor::new(CompressionLvl::default());
        let mut scratch = vec![0u8; compressor.deflate_compress_bound(small.len())];
        assert!(!looks_incompressible(&mut compressor, &mut scratch, &small).expect("probe"));
    }
}
