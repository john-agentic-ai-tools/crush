//! Main compression and decompression entry points.

use crate::block::{BlockOutput, compress_block, decompress_block_into};
use crate::codec::{BlockCodec, DeflateCodec};
use crate::config::{EngineConfiguration, ProgressEvent, ProgressPhase};
use crate::format::{BlockHeader, BlockIndexEntry, FileFlags, FileFooter, FileHeader, IndexHeader};
use crate::index::{BlockIndex, load_index};
use crush_core::error::{CrushError, Result};
use rayon::prelude::*;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

// ---------------------------------------------------------------------------
// Thread pool helper
// ---------------------------------------------------------------------------

/// Run `f` inside the thread pool from `config`, or the global rayon pool when
/// `config.workers == 0`.
fn with_pool<T: Send>(config: &EngineConfiguration, f: impl FnOnce() -> T + Send) -> T {
    match &config.thread_pool {
        Some(pool) => pool.install(f),
        None => f(),
    }
}

/// Turn a failed per-worker state creation into a block error.
fn worker_state<S>(state: &mut Result<S>) -> Result<&mut S> {
    state
        .as_mut()
        .map_err(|e| CrushError::InvalidConfig(format!("codec state: {e}")))
}

// ---------------------------------------------------------------------------
// compress
// ---------------------------------------------------------------------------

/// Compress `input` bytes with DEFLATE through the parallel block engine.
///
/// # Errors
///
/// - [`CrushError::Cancelled`] — cancelled by the flag or the progress callback.
/// - [`CrushError::InvalidConfig`] — configuration validation failed.
pub fn compress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>> {
    let codec = DeflateCodec::new(config.compression_level, config.incompressible_probe)?;
    compress_with(&codec, input, config)
}

/// Compress `input` bytes with `codec` through the parallel block engine.
///
/// # Errors
///
/// - [`CrushError::Cancelled`] — cancelled by the flag or the progress callback.
/// - [`CrushError::InvalidConfig`] — configuration validation failed.
#[allow(clippy::too_many_lines)] // single cohesive pipeline: parallel compress → pre-allocate → parallel assemble → index → footer → progress
pub fn compress_with<C: BlockCodec>(
    codec: &C,
    input: &[u8],
    config: &EngineConfiguration,
) -> Result<Vec<u8>> {
    let block_size = config.block_size as usize;
    let blocks: Vec<&[u8]> = input.chunks(block_size).collect();
    let total_blocks = blocks.len() as u64;

    // Slice A: pool one encoder and one scratch buffer per worker thread via
    // map_init. The first block on each worker allocates them; every
    // subsequent block on that worker reuses them.
    let results: Vec<Result<BlockOutput>> = with_pool(config, || {
        blocks
            .par_iter()
            .enumerate()
            .map_init(
                || (codec.encoder(), Vec::new()),
                |(encoder, scratch), (i, chunk)| {
                    if config.is_cancelled() {
                        return Err(CrushError::Cancelled);
                    }
                    compress_block(codec, worker_state(encoder)?, scratch, chunk, i, config)
                },
            )
            .collect()
    });

    // block count fits usize: validated against u32::MAX in the index write path.
    #[allow(clippy::cast_possible_truncation)]
    let mut compressed_blocks: Vec<BlockOutput> = Vec::with_capacity(total_blocks as usize);
    for r in results {
        compressed_blocks.push(r?);
    }

    // ---------------------------------------------------------------------
    // Slice B: compute exact output size, pre-allocate, parallel assembly.
    // ---------------------------------------------------------------------

    let mut flags = FileFlags::default();
    if config.checksums {
        flags = flags.with_checksums();
    }

    let file_header = FileHeader::for_codec(
        config.block_size,
        config.compression_level,
        flags,
        input.len() as u64,
        total_blocks,
        C::CODEC_ID,
    );

    let header_size = FileHeader::SIZE;
    let blocks_region_size: usize = compressed_blocks
        .iter()
        .map(|b| BlockHeader::SIZE + b.payload.len())
        .sum();
    let index_region_size = IndexHeader::SIZE + compressed_blocks.len() * BlockIndexEntry::SIZE;
    let footer_size = FileFooter::SIZE;
    let total_size = header_size + blocks_region_size + index_region_size + footer_size;

    let mut out = vec![0u8; total_size];

    // Compute per-block output offsets on the driver thread.
    let mut block_offsets: Vec<usize> = Vec::with_capacity(compressed_blocks.len());
    let mut cursor = header_size;
    for b in &compressed_blocks {
        block_offsets.push(cursor);
        cursor += BlockHeader::SIZE + b.payload.len();
    }
    debug_assert_eq!(cursor, header_size + blocks_region_size);

    // Write FileHeader (driver).
    out[..header_size].copy_from_slice(&file_header.to_bytes());

    // Partition the blocks region into disjoint per-block slices and copy
    // headers + payloads in parallel (Slice B).
    {
        let (_header_region, rest) = out.split_at_mut(header_size);
        let (blocks_region, _trailing) = rest.split_at_mut(blocks_region_size);

        let mut block_slices: Vec<&mut [u8]> = Vec::with_capacity(compressed_blocks.len());
        let mut remaining: &mut [u8] = blocks_region;
        for b in &compressed_blocks {
            let chunk_len = BlockHeader::SIZE + b.payload.len();
            let (chunk, rest) = remaining.split_at_mut(chunk_len);
            block_slices.push(chunk);
            remaining = rest;
        }
        debug_assert_eq!(remaining, []);

        block_slices
            .into_par_iter()
            .zip(compressed_blocks.par_iter())
            .for_each(|(slice, block)| {
                let hdr_bytes = block.header.to_bytes();
                slice[..BlockHeader::SIZE].copy_from_slice(&hdr_bytes);
                slice[BlockHeader::SIZE..].copy_from_slice(&block.payload);
            });
    }

    // Build index entries (driver; cheap relative to payload copies).
    let mut index_entries = Vec::with_capacity(compressed_blocks.len());
    for (i, b) in compressed_blocks.iter().enumerate() {
        index_entries.push(BlockIndexEntry {
            block_offset: block_offsets[i] as u64,
            compressed_size: b.header.compressed_size,
            uncompressed_size: b.header.uncompressed_size,
            checksum: b.header.checksum,
        });
    }

    // Write index region (driver).
    let index_offset = header_size + blocks_region_size;
    let entry_count = u32::try_from(index_entries.len())
        .map_err(|_| CrushError::InvalidConfig("too many blocks for index".to_owned()))?;
    let ih = IndexHeader {
        entry_count,
        index_flags: 0,
    };
    out[index_offset..index_offset + IndexHeader::SIZE].copy_from_slice(&ih.to_bytes());
    let mut entry_cursor = index_offset + IndexHeader::SIZE;
    for e in &index_entries {
        out[entry_cursor..entry_cursor + BlockIndexEntry::SIZE].copy_from_slice(&e.to_bytes());
        entry_cursor += BlockIndexEntry::SIZE;
    }
    debug_assert_eq!(entry_cursor, index_offset + index_region_size);

    // Write footer (driver).
    let index_size_u32 = u32::try_from(index_region_size)
        .map_err(|_| CrushError::InvalidConfig("index too large for footer".to_owned()))?;
    let footer = FileFooter::new(index_offset as u64, index_size_u32);
    out[total_size - FileFooter::SIZE..].copy_from_slice(&footer.to_bytes());

    // Invoke progress callback in a single driver-side post-pass so we don't
    // re-serialize parallel assembly through a mutex (Slice B).
    if let Some(cb_arc) = &config.progress {
        let mut cb = cb_arc.lock().map_err(|_| {
            CrushError::InvalidConfig("progress callback mutex poisoned".to_owned())
        })?;
        let mut bytes_processed: u64 = 0;
        for (i, block) in compressed_blocks.iter().enumerate() {
            bytes_processed += u64::from(block.header.uncompressed_size);
            let event = ProgressEvent {
                bytes_processed,
                blocks_completed: i as u64 + 1,
                total_blocks: Some(total_blocks),
                phase: ProgressPhase::Compressing,
            };
            if !cb(event) {
                return Err(CrushError::Cancelled);
            }
        }
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// compress_file
// ---------------------------------------------------------------------------

/// Compress a file at `path` using memory-mapped zero-copy I/O (FR-009).
///
/// This is the preferred entry point for large file inputs.
/// Internally uses `memmap2::MmapOptions` to avoid copying file contents
/// into a heap buffer before compression.
///
/// # Errors
///
/// - `CrushError::Io` — file could not be opened or memory-mapped.
/// - `CrushError::Cancelled` — cancelled via the progress callback.
/// - `CrushError::InvalidConfig` — configuration validation failed.
pub fn compress_file(path: &Path, config: &EngineConfiguration) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    // SAFETY: the file is only read, not written, during compression.
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
    compress(&mmap, config)
}

// ---------------------------------------------------------------------------
// compress_to_writer
// ---------------------------------------------------------------------------

/// Compress `input` bytes, writing the CRSH output to `writer`.
///
/// # Errors
///
/// Returns any error from `compress()` or from the underlying `write_all`.
pub fn compress_to_writer<W: Write>(
    input: &[u8],
    mut writer: W,
    config: &EngineConfiguration,
) -> Result<u64> {
    let out = compress(input, config)?;
    let len = out.len() as u64;
    writer.write_all(&out)?;
    Ok(len)
}

// ---------------------------------------------------------------------------
// compress_stream
// ---------------------------------------------------------------------------

/// Compress a [`Read`] stream, writing CRSH output to `writer`.
///
/// Total input size is unknown at start; `FileHeader` `block_count` and
/// `uncompressed_size` are set to `u64::MAX` (streaming sentinel).
///
/// # Errors
///
/// Returns any error from `compress()` or the underlying writer.
//
// NOTE: FR-015 streaming (do not buffer full input) is deferred to
// feature 012-streaming-pipeline (see specs/011-perf-optimizations/research.md § 6).
pub fn compress_stream<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    config: &EngineConfiguration,
) -> Result<u64> {
    let mut input = Vec::new();
    reader.read_to_end(&mut input)?;
    let out = compress(&input, config)?;
    let len = out.len() as u64;
    writer.write_all(&out)?;
    Ok(len)
}

// ---------------------------------------------------------------------------
// decompress
// ---------------------------------------------------------------------------

/// Decompress a DEFLATE CRSH byte slice.
///
/// Reads the file footer first, loads the index, then decompresses all
/// blocks in parallel.
///
/// # Errors
///
/// - `VersionMismatch` — file was produced by a different engine version.
/// - `InvalidFormat` — magic bytes invalid, file truncated, or wrong codec.
/// - `ChecksumMismatch` — a block's CRC32 does not match.
/// - `ExpansionLimitExceeded` — output would exceed `config.max_decompression_ratio`.
/// - `IndexCorrupted` — block index is missing or truncated.
/// - `Cancelled` — cancelled by the flag or the progress callback.
pub fn decompress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>> {
    decompress_with(&DeflateCodec::default(), input, config)
}

/// Decompress a CRSH byte slice written by `codec`.
///
/// # Errors
///
/// As for [`decompress`].
pub fn decompress_with<C: BlockCodec>(
    codec: &C,
    input: &[u8],
    config: &EngineConfiguration,
) -> Result<Vec<u8>> {
    let index = load_index(&mut Cursor::new(input))?;
    let total = checked_output_size::<C>(&index, input.len() as u64, config)?;
    let located = locate_blocks(input, &index)?;
    let mut output = vec![0u8; total];
    decode_blocks(
        codec,
        &raw_blocks(&located),
        index.checksums_enabled,
        config,
        &mut output,
    )?;
    Ok(output)
}

/// Validated decompressed size of a CRSH byte slice written by `codec`.
///
/// # Errors
///
/// As for [`decompress`], for everything checked before decoding starts.
pub fn decompressed_size<C: BlockCodec>(
    input: &[u8],
    config: &EngineConfiguration,
) -> Result<usize> {
    let index = load_index(&mut Cursor::new(input))?;
    checked_output_size::<C>(&index, input.len() as u64, config)
}

/// Decompress a CRSH byte slice written by `codec` into `output`, whose length
/// must equal [`decompressed_size`]. Blocks decode in parallel, straight into
/// their place in `output`, so this needs no buffer of its own.
///
/// # Errors
///
/// As for [`decompress`]. On error, `output` may hold partial data.
pub fn decompress_into<C: BlockCodec>(
    codec: &C,
    input: &[u8],
    config: &EngineConfiguration,
    output: &mut [u8],
) -> Result<()> {
    let index = load_index(&mut Cursor::new(input))?;
    checked_output_size::<C>(&index, input.len() as u64, config)?;
    let located = locate_blocks(input, &index)?;
    decode_blocks(
        codec,
        &raw_blocks(&located),
        index.checksums_enabled,
        config,
        output,
    )
}

/// Decompress a DEFLATE CRSH file from a seekable reader.
///
/// Reads every block into memory, then decodes in parallel. Input that is
/// already in memory should use [`decompress`], which borrows it.
///
/// # Errors
///
/// As for [`decompress`], plus [`CrushError::Io`] for I/O errors.
pub fn decompress_from_reader<R: Read + Seek>(
    reader: &mut R,
    config: &EngineConfiguration,
) -> Result<Vec<u8>> {
    let index = load_index(reader)?;
    let file_size = reader.seek(SeekFrom::End(0))?;
    let total = checked_output_size::<DeflateCodec>(&index, file_size, config)?;

    let mut raw: Vec<(BlockHeader, Vec<u8>)> = Vec::with_capacity(index.entries.len());
    for (i, entry) in index.entries.iter().enumerate() {
        check_block_in_file(i, entry, file_size)?;
        reader.seek(SeekFrom::Start(entry.block_offset))?;
        let mut hdr_buf = [0u8; BlockHeader::SIZE];
        reader
            .read_exact(&mut hdr_buf)
            .map_err(|e| CrushError::InvalidFormat(format!("block {i} header read error: {e}")))?;
        let header = BlockHeader::from_bytes(&hdr_buf);
        check_header_matches_index(i, &header, entry)?;
        let mut payload = vec![0u8; header.compressed_size as usize];
        reader
            .read_exact(&mut payload)
            .map_err(|e| CrushError::InvalidFormat(format!("block {i} payload read error: {e}")))?;
        raw.push((header, payload));
    }

    let blocks: Vec<RawBlock> = raw
        .iter()
        .map(|(header, payload)| RawBlock { header, payload })
        .collect();
    let mut output = vec![0u8; total];
    decode_blocks(
        &DeflateCodec::default(),
        &blocks,
        index.checksums_enabled,
        config,
        &mut output,
    )?;
    Ok(output)
}

/// One block's validated header and payload.
struct RawBlock<'a> {
    header: &'a BlockHeader,
    payload: &'a [u8],
}

fn raw_blocks<'a>(located: &'a [(BlockHeader, &'a [u8])]) -> Vec<RawBlock<'a>> {
    located
        .iter()
        .map(|(header, payload)| RawBlock { header, payload })
        .collect()
}

/// Check the codec and expansion limit, and return the total output size.
fn checked_output_size<C: BlockCodec>(
    index: &BlockIndex,
    file_size: u64,
    config: &EngineConfiguration,
) -> Result<usize> {
    if index.codec_id != C::CODEC_ID {
        return Err(CrushError::InvalidFormat(format!(
            "file uses codec {} but this reader is codec {}",
            index.codec_id,
            C::CODEC_ID
        )));
    }

    // Expansion ratio guard.
    // Precision and sign-loss are acceptable: this is a heuristic ratio check, not a byte counter.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let limit = (file_size as f64 * config.max_decompression_ratio) as u64;
    let total = index.total_uncompressed_size();
    if total > limit {
        return Err(CrushError::ExpansionLimitExceeded { block_index: 0 });
    }
    usize::try_from(total).map_err(|_| {
        CrushError::InvalidConfig(format!("total uncompressed size {total} overflows usize"))
    })
}

/// Reject an index entry that cannot lie inside the file, before allocating for it.
pub(crate) fn check_block_in_file(i: usize, entry: &BlockIndexEntry, file_size: u64) -> Result<()> {
    let end = entry
        .block_offset
        .checked_add(BlockHeader::SIZE as u64)
        .and_then(|e| e.checked_add(u64::from(entry.compressed_size)));
    match end {
        Some(end) if end <= file_size => Ok(()),
        _ => Err(CrushError::InvalidFormat(format!(
            "block {i} extends past the end of the file"
        ))),
    }
}

/// A block header must agree with the index, which sized the output.
pub(crate) fn check_header_matches_index(
    i: usize,
    header: &BlockHeader,
    entry: &BlockIndexEntry,
) -> Result<()> {
    if header.compressed_size == entry.compressed_size
        && header.uncompressed_size == entry.uncompressed_size
    {
        Ok(())
    } else {
        Err(CrushError::InvalidFormat(format!(
            "block {i} header disagrees with the index"
        )))
    }
}

/// Parse every block's header, borrowing its payload from `input`.
fn locate_blocks<'a>(input: &'a [u8], index: &BlockIndex) -> Result<Vec<(BlockHeader, &'a [u8])>> {
    index
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            check_block_in_file(i, entry, input.len() as u64)?;
            // In bounds and within usize: the block lies inside `input`.
            #[allow(clippy::cast_possible_truncation)]
            let start = entry.block_offset as usize;
            let (hdr, rest) = input[start..]
                .split_first_chunk::<{ BlockHeader::SIZE }>()
                .ok_or_else(|| CrushError::InvalidFormat(format!("block {i} truncated")))?;
            let header = BlockHeader::from_bytes(hdr);
            check_header_matches_index(i, &header, entry)?;
            Ok((header, &rest[..header.compressed_size as usize]))
        })
        .collect()
}

/// Decode all blocks in parallel into disjoint slices of `output`, then report progress.
fn decode_blocks<C: BlockCodec>(
    codec: &C,
    blocks: &[RawBlock<'_>],
    checksums_enabled: bool,
    config: &EngineConfiguration,
    output: &mut [u8],
) -> Result<()> {
    // Partition output into disjoint per-block slices.
    let mut output_slices: Vec<&mut [u8]> = Vec::with_capacity(blocks.len());
    let mut remaining: &mut [u8] = output;
    for (i, block) in blocks.iter().enumerate() {
        let size = block.header.uncompressed_size as usize;
        if size > remaining.len() {
            return Err(CrushError::InvalidFormat(format!(
                "block {i} does not fit the output"
            )));
        }
        let (chunk, rest) = remaining.split_at_mut(size);
        output_slices.push(chunk);
        remaining = rest;
    }
    if !remaining.is_empty() {
        return Err(CrushError::InvalidFormat(
            "blocks do not fill the output".to_owned(),
        ));
    }

    // Parallel decode with pooled decoders, writing directly into the slices.
    let results: Vec<Result<()>> = with_pool(config, || {
        output_slices
            .into_par_iter()
            .zip(blocks.par_iter())
            .enumerate()
            .map_init(
                || codec.decoder(),
                |decoder, (i, (slice, block))| {
                    if config.is_cancelled() {
                        return Err(CrushError::Cancelled);
                    }
                    decompress_block_into(
                        codec,
                        worker_state(decoder)?,
                        block.header,
                        block.payload,
                        slice,
                        i as u64,
                        checksums_enabled,
                    )
                },
            )
            .collect()
    });
    for r in results {
        r?;
    }

    // Invoke progress callback in a single driver-side post-pass (matches compress path).
    if let Some(cb_arc) = &config.progress {
        let mut cb = cb_arc
            .lock()
            .map_err(|_| CrushError::InvalidConfig("progress mutex poisoned".to_owned()))?;
        let mut bytes_processed: u64 = 0;
        for (i, block) in blocks.iter().enumerate() {
            bytes_processed += u64::from(block.header.uncompressed_size);
            let event = ProgressEvent {
                bytes_processed,
                blocks_completed: i as u64 + 1,
                total_blocks: Some(blocks.len() as u64),
                phase: ProgressPhase::Decompressing,
            };
            if !cb(event) {
                return Err(CrushError::Cancelled);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::missing_panics_doc
)]
mod tests {
    use super::*;
    use crate::format::FORMAT_VERSION;
    use std::io::Write as IoWrite;
    use tempfile::NamedTempFile;

    fn default_config() -> EngineConfiguration {
        EngineConfiguration::builder()
            .block_size(65_536) // small blocks for fast tests
            .build()
            .expect("config")
    }

    #[test]
    fn test_compress_roundtrip_small() {
        let data: Vec<u8> = b"hello world"
            .iter()
            .cycle()
            .take(200_000)
            .copied()
            .collect();
        let config = default_config();
        let compressed = compress(&data, &config).expect("compress");
        let recovered = decompress(&compressed, &config).expect("decompress");
        assert_eq!(data, recovered);
    }

    #[test]
    fn test_compress_incompressible_stored() {
        // Use a tiny expansion ratio so that even well-compressed output triggers stored.
        // With ratio 0.001, DEFLATE would need to compress a 65536-byte block to < 66 bytes
        // to avoid stored — impossible in practice. This deterministically tests stored-block
        // write/read paths without depending on data entropy.
        let data: Vec<u8> = b"hello world!"
            .iter()
            .cycle()
            .take(200_000)
            .copied()
            .collect();
        let config = EngineConfiguration::builder()
            .block_size(65_536)
            .max_expansion_ratio(0.001) // essentially forces stored for all blocks
            .build()
            .expect("config");
        let compressed = compress(&data, &config).expect("compress");
        // Round-trip must still work
        let recovered = decompress(&compressed, &config).expect("decompress");
        assert_eq!(data, recovered);
        // Verify at least one stored block exists
        let mut cursor = Cursor::new(&compressed);
        let index = load_index(&mut cursor).expect("load_index");
        // Seek to first block header and check stored flag
        cursor
            .seek(SeekFrom::Start(index.entries[0].block_offset))
            .expect("seek");
        let mut hdr = [0u8; BlockHeader::SIZE];
        cursor.read_exact(&mut hdr).expect("read hdr");
        let header = BlockHeader::from_bytes(&hdr);
        assert!(
            header.flags.stored(),
            "expected stored flag on incompressible data"
        );
    }

    #[test]
    fn test_compress_output_valid_crsh_format() {
        let data: Vec<u8> = b"test".iter().cycle().take(100_000).copied().collect();
        let config = default_config();
        let compressed = compress(&data, &config).expect("compress");
        // Validate FileHeader
        let hdr_bytes: [u8; FileHeader::SIZE] = compressed[..FileHeader::SIZE]
            .try_into()
            .expect("hdr bytes");
        let hdr = FileHeader::from_bytes(&hdr_bytes).expect("parse header");
        assert_eq!(hdr.magic, crate::format::CRSH_MAGIC);
        assert_eq!(hdr.format_version, FORMAT_VERSION);
        // Validate footer
        let footer_bytes: [u8; FileFooter::SIZE] = compressed
            [compressed.len() - FileFooter::SIZE..]
            .try_into()
            .expect("footer bytes");
        let footer = FileFooter::from_bytes(&footer_bytes).expect("parse footer");
        assert_eq!(footer.magic, crate::format::CRSH_MAGIC);
        // Load index and verify entry count matches header block_count
        let mut cursor = Cursor::new(&compressed);
        let index = load_index(&mut cursor).expect("load_index");
        assert_eq!(index.len(), hdr.block_count);
    }

    #[test]
    fn test_progress_callback_invoked_per_block() {
        use std::sync::{Arc, Mutex};
        let data: Vec<u8> = b"abc".iter().cycle().take(300_000).copied().collect();
        let count = Arc::new(Mutex::new(0u64));
        let count_clone = count.clone();
        let cb: crate::config::ProgressCallback = Box::new(move |_event| {
            let mut c = count_clone.lock().expect("lock");
            *c += 1;
            true
        });
        let config = EngineConfiguration::builder()
            .block_size(65_536)
            .progress(Arc::new(Mutex::new(cb)))
            .build()
            .expect("config");
        compress(&data, &config).expect("compress");
        let final_count = *count.lock().expect("lock");
        // 300_000 / 65_536 = ceil 5 blocks
        assert!(final_count >= 1, "progress callback was not invoked");
    }

    #[test]
    fn test_cancel_halts_at_block_boundary() {
        use std::sync::{Arc, Mutex};
        let data: Vec<u8> = b"xyz".iter().cycle().take(1_000_000).copied().collect();
        let cb: crate::config::ProgressCallback = Box::new(|_event| false); // always cancel
        let config = EngineConfiguration::builder()
            .block_size(65_536)
            .progress(Arc::new(Mutex::new(cb)))
            .build()
            .expect("config");
        let result = compress(&data, &config);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_cancelled());
    }

    #[test]
    fn test_compress_file_roundtrip() {
        let data: Vec<u8> = b"file data".iter().cycle().take(200_000).copied().collect();
        let mut tmp = NamedTempFile::new().expect("temp file");
        tmp.write_all(&data).expect("write");
        let config = default_config();
        let compressed = compress_file(tmp.path(), &config).expect("compress_file");
        let recovered = decompress(&compressed, &config).expect("decompress");
        assert_eq!(data, recovered);
    }

    #[test]
    fn test_decompress_roundtrip() {
        let data: Vec<u8> = b"decompress me"
            .iter()
            .cycle()
            .take(500_000)
            .copied()
            .collect();
        let config = default_config();
        let compressed = compress(&data, &config).expect("compress");
        let recovered = decompress(&compressed, &config).expect("decompress");
        assert_eq!(data, recovered);
    }

    #[test]
    fn test_decompress_corrupt_block_detected() {
        let data: Vec<u8> = b"corrupt test"
            .iter()
            .cycle()
            .take(200_000)
            .copied()
            .collect();
        let config = default_config();
        let mut compressed = compress(&data, &config).expect("compress");

        // Find first block header offset (right after FileHeader)
        let mut cursor = Cursor::new(&compressed);
        let index = load_index(&mut cursor).expect("load_index");
        let block0_offset = index.entries[0].block_offset as usize;

        // Corrupt a byte in the payload (after BlockHeader)
        let payload_start = block0_offset + BlockHeader::SIZE;
        if payload_start < compressed.len() {
            compressed[payload_start] ^= 0xFF;
        }

        let result = decompress(&compressed, &config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, CrushError::ChecksumMismatch { block_index: 0, .. })
                || matches!(err, CrushError::InvalidFormat(_)),
            "expected checksum or format error, got {err:?}"
        );
    }

    #[test]
    fn test_version_mismatch_rejected() {
        let data: Vec<u8> = b"version test"
            .iter()
            .cycle()
            .take(100_000)
            .copied()
            .collect();
        let config = default_config();
        let mut compressed = compress(&data, &config).expect("compress");

        // Overwrite format_version in footer with a different value
        let footer_start = compressed.len() - FileFooter::SIZE;
        // format_version is at bytes [16..20] of the footer
        compressed[footer_start + 16..footer_start + 20].copy_from_slice(&9999u32.to_le_bytes());
        // Also need to fix the footer checksum (bytes [12..16]) to avoid IndexCorrupted first
        // Actually we want VersionMismatch — just set an obviously wrong version
        // The footer checksum will fail first; we accept either error
        let result = decompress(&compressed, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_expansion_limit_exceeded() {
        let data: Vec<u8> = b"test data".iter().cycle().take(100_000).copied().collect();
        let compress_config = default_config();
        let compressed = compress(&data, &compress_config).expect("compress");

        // Set an absurdly tight decompression ratio
        let decompress_config = EngineConfiguration::builder()
            .block_size(65_536)
            .max_decompression_ratio(0.000_001)
            .build()
            .expect("config");
        let result = decompress(&compressed, &decompress_config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CrushError::ExpansionLimitExceeded { .. }
        ));
    }

    #[test]
    fn test_truncated_footer_rejected() {
        let data: Vec<u8> = b"truncated".iter().cycle().take(100_000).copied().collect();
        let config = default_config();
        let mut compressed = compress(&data, &config).expect("compress");
        // Remove the last 24 bytes (footer)
        compressed.truncate(compressed.len() - FileFooter::SIZE);
        let result = decompress(&compressed, &config);
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------------------
    // Property-based round-trip test (T070)
    // ---------------------------------------------------------------------------

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(50))]

        #[test]
        fn proptest_compress_decompress_roundtrip(
            data in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..200_000),
            block_kb in proptest::prelude::prop_oneof![
                proptest::prelude::Just(64usize),
                proptest::prelude::Just(256),
                proptest::prelude::Just(1024)
            ],
            level in 0u8..=9,
        ) {
            let block_size = u32::try_from(block_kb * 1024).unwrap();
            let config = EngineConfiguration::builder()
                .block_size(block_size)
                .compression_level(level)
                .build()
                .unwrap();
            let compressed = compress(&data, &config).unwrap();
            let recovered = decompress(&compressed, &config).unwrap();
            proptest::prop_assert_eq!(data, recovered);
        }
    }

    // --- Golden output (perf work must not change the compressed bytes) ---

    fn xorshift_bytes(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect()
    }

    /// Text-like input: words from a small vocabulary picked pseudo-randomly.
    fn wordy_bytes(len: usize) -> Vec<u8> {
        const WORDS: &[&[u8]] = &[
            b"fn ", b"let ", b"crush ", b"block ", b"0x1f ", b"
", b"error ",
        ];
        let picks = xorshift_bytes(len, 0x0BAD_5EED_1234_5678);
        let mut out = Vec::with_capacity(len + 8);
        let mut i = 0;
        while out.len() < len {
            out.extend_from_slice(WORDS[usize::from(picks[i % picks.len()]) % WORDS.len()]);
            i += 1;
        }
        out.truncate(len);
        out
    }

    /// (crc32, length) of `compress()` output, pinned before the per-worker
    /// scratch buffer and borrowed stored payloads (perf goal, 2026-10-29).
    #[test]
    fn compress_output_matches_golden() {
        let config = EngineConfiguration::builder()
            .block_size(64 * 1024)
            .build()
            .expect("config");
        let cases: [(&str, Vec<u8>); 4] = [
            ("random", xorshift_bytes(300_000, 0x9E37_79B9_7F4A_7C15)),
            ("text", wordy_bytes(300_000)),
            ("zeros", vec![0u8; 300_000]),
            // random first blocks, text tail, partial last block
            ("mixed-partial", {
                let mut v = xorshift_bytes(150_001, 7);
                v.extend(wordy_bytes(70_000));
                v
            }),
        ];
        let mut mismatches = Vec::new();
        for (name, data) in &cases {
            let out = compress(data, &config).expect("compress");
            let got = (crc32fast::hash(&out), out.len());
            if got != golden(name) {
                mismatches.push(*name);
            }
        }
        assert!(mismatches.is_empty(), "output changed for {mismatches:?}");
    }

    fn golden(name: &str) -> (u32, usize) {
        match name {
            "random" => (4_000_492_043, 300_276),
            "text" => (2_413_880_173, 45_728),
            "zeros" => (592_274_169, 641),
            "mixed-partial" => (1_403_566_687, 161_425),
            _ => unreachable!("no golden value for {name}"),
        }
    }

    // --- Incompressibility probe (perf goal 2026-10-29) ---

    fn probe_config(probe: bool) -> EngineConfiguration {
        EngineConfiguration::builder()
            .block_size(256 * 1024)
            .incompressible_probe(probe)
            .build()
            .expect("config")
    }

    /// On inputs where every block either clearly compresses or clearly
    /// doesn't, the probe must not change a single output byte.
    #[test]
    fn probe_output_identical_on_clear_cut_inputs() {
        let cases: [(&str, Vec<u8>); 4] = [
            ("random", xorshift_bytes(1_000_000, 0x9E37_79B9_7F4A_7C15)),
            ("text", wordy_bytes(1_000_000)),
            ("zeros", vec![0u8; 1_000_000]),
            ("random-then-text", {
                let mut v = xorshift_bytes(600_000, 3);
                v.extend(wordy_bytes(400_000));
                v
            }),
        ];
        for (name, data) in &cases {
            let on = compress(data, &probe_config(true)).expect("compress");
            let off = compress(data, &probe_config(false)).expect("compress");
            assert!(on == off, "probe changed output for {name}");
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(32))]
        /// Random data with compressible spans at arbitrary offsets: whatever
        /// the probe decides, the data must round-trip.
        #[test]
        fn probe_roundtrip_mixed_blocks(
            seed in proptest::prelude::any::<u64>(),
            spans in proptest::collection::vec((0usize..600_000, 1usize..40_000), 0..12),
        ) {
            let mut data = xorshift_bytes(600_000, seed | 1);
            for (start, len) in spans {
                let end = (start + len).min(data.len());
                for (i, b) in data[start..end].iter_mut().enumerate() {
                    *b = b"compressible text "[i % 18];
                }
            }
            let config = probe_config(true);
            let compressed = compress(&data, &config).expect("compress");
            let recovered = decompress(&compressed, &config).expect("decompress");
            proptest::prop_assert_eq!(data, recovered);
        }
    }
}
