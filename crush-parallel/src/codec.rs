//! The interface between the parallel engine and a block codec.
//!
//! The contract is in `specs/026-hybrid-speed-codec/contracts/block-codec.md`.

use crush_core::error::{CrushError, Result};
use libdeflater::{CompressionLvl, Compressor, Decompressor};

/// A codec the engine can run on independent blocks.
pub trait BlockCodec: Sync {
    /// Value stored in the container header. 0 is reserved for DEFLATE.
    const CODEC_ID: u8;
    /// Per-worker compression state, reused across blocks.
    type Encoder: Send;
    /// Per-worker decompression state, reused across blocks.
    type Decoder: Send;

    /// Create one worker's compression state.
    ///
    /// # Errors
    ///
    /// Returns an error if the state cannot be created.
    fn encoder(&self) -> Result<Self::Encoder>;

    /// Create one worker's decompression state.
    ///
    /// # Errors
    ///
    /// Returns an error if the state cannot be created.
    fn decoder(&self) -> Result<Self::Decoder>;

    /// Compress one block into `out`, replacing its contents. `out` may be a
    /// buffer of any length left over from an earlier block.
    ///
    /// Returns `Ok(false)` when the block should be stored raw, in which case
    /// `out` is ignored.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    fn compress_block(
        &self,
        encoder: &mut Self::Encoder,
        input: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<bool>;

    /// Decode one block into `output`, whose length is the block's validated
    /// uncompressed size. `output` must be filled exactly.
    ///
    /// # Errors
    ///
    /// Returns an error on any malformed payload. Implementations must never
    /// panic on a payload.
    fn decompress_block(
        &self,
        decoder: &mut Self::Decoder,
        payload: &[u8],
        output: &mut [u8],
    ) -> Result<()>;
}

/// DEFLATE through libdeflate. This is the codec of CRSH format version 1.
#[derive(Debug, Clone, Copy)]
pub struct DeflateCodec {
    level: CompressionLvl,
    probe: bool,
}

impl DeflateCodec {
    /// Create a codec. With `probe`, a block that looks incompressible is
    /// stored without a full DEFLATE attempt.
    ///
    /// # Errors
    ///
    /// Returns [`CrushError::InvalidConfig`] if `level` is outside libdeflate's
    /// 0-12 range.
    pub fn new(level: u8, probe: bool) -> Result<Self> {
        let level = CompressionLvl::new(i32::from(level))
            .map_err(|_| CrushError::InvalidConfig(format!("invalid compression level {level}")))?;
        Ok(Self { level, probe })
    }
}

impl Default for DeflateCodec {
    fn default() -> Self {
        Self {
            level: CompressionLvl::default(),
            probe: true,
        }
    }
}

impl BlockCodec for DeflateCodec {
    const CODEC_ID: u8 = 0;
    type Encoder = Compressor;
    type Decoder = Decompressor;

    fn encoder(&self) -> Result<Compressor> {
        Ok(Compressor::new(self.level))
    }

    fn decoder(&self) -> Result<Decompressor> {
        Ok(Decompressor::new())
    }

    fn compress_block(
        &self,
        encoder: &mut Compressor,
        input: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<bool> {
        // deflate_compress_bound gives the exact worst-case output size. A
        // fresh zeroed Vec (calloc) is cheaper than resizing, which memsets.
        let bound = encoder.deflate_compress_bound(input.len());
        if out.len() < bound {
            *out = vec![0u8; bound];
        }
        if self.probe && looks_incompressible(encoder, out, input)? {
            return Ok(false);
        }
        let written = encoder
            .deflate_compress(input, &mut out[..bound])
            .map_err(|e| CrushError::InvalidFormat(format!("DEFLATE encode error: {e:?}")))?;
        out.truncate(written);
        Ok(true)
    }

    fn decompress_block(
        &self,
        decoder: &mut Decompressor,
        payload: &[u8],
        output: &mut [u8],
    ) -> Result<()> {
        let written = decoder
            .deflate_decompress(payload, output)
            .map_err(|e| CrushError::InvalidFormat(format!("DEFLATE decode error: {e:?}")))?;
        if written != output.len() {
            return Err(CrushError::InvalidFormat(format!(
                "uncompressed size mismatch: expected {} but decoded {written}",
                output.len()
            )));
        }
        Ok(())
    }
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
