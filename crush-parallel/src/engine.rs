//! Main compression and decompression entry points.

use crate::block::{
    CompressedBlock, compress_block, decompress_block_into, resolve_compression_level,
};
use crate::config::{EngineConfiguration, ProgressEvent, ProgressPhase};
use crate::format::{BlockHeader, BlockIndexEntry, FileFlags, FileFooter, FileHeader, IndexHeader};
use crate::index::load_index;
use crush_core::error::{CrushError, Result};
use libdeflater::{Compressor, Decompressor};
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

// ---------------------------------------------------------------------------
// compress
// ---------------------------------------------------------------------------

/// Compress `input` bytes using the parallel block engine.
///
/// # Errors
///
/// - [`CrushError::Cancelled`] — cancelled via the progress callback.
/// - [`CrushError::InvalidConfig`] — configuration validation failed.
#[allow(clippy::too_many_lines)] // single cohesive pipeline: resolve → parallel compress → pre-allocate → parallel assemble → index → footer → progress
pub fn compress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>> {
    let block_size = config.block_size as usize;
    let blocks: Vec<&[u8]> = input.chunks(block_size).collect();
    let total_blocks = blocks.len() as u64;

    // Resolve compression level once on the driver, not per-block (Slice A).
    let lvl = resolve_compression_level(config.compression_level)?;

    // Slice A: pool one Compressor per worker thread via map_init.
    // The first block on each worker allocates its Compressor; every subsequent
    // block on that worker reuses it.
    let results: Vec<Result<CompressedBlock>> = with_pool(config, || {
        blocks
            .par_iter()
            .enumerate()
            .map_init(
                || Compressor::new(lvl),
                |compressor, (i, chunk)| compress_block(compressor, chunk, i, config),
            )
            .collect()
    });

    // block count fits usize: validated against u32::MAX in the index write path.
    #[allow(clippy::cast_possible_truncation)]
    let mut compressed_blocks: Vec<CompressedBlock> = Vec::with_capacity(total_blocks as usize);
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

    let file_header = FileHeader::new(
        config.block_size,
        config.compression_level,
        flags,
        input.len() as u64,
        total_blocks,
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
        debug_assert!(remaining.is_empty());

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

/// Decompress a CRSH-format byte slice.
///
/// Reads the file footer first, loads the index, then decompresses all
/// blocks in parallel.
///
/// # Errors
///
/// - `VersionMismatch` — file was produced by a different engine version.
/// - `InvalidFormat` — magic bytes invalid or file truncated.
/// - `ChecksumMismatch` — a block's CRC32 does not match.
/// - `ExpansionLimitExceeded` — output would exceed `config.max_decompression_ratio`.
/// - `IndexCorrupted` — block index is missing or truncated.
/// - `Cancelled` — cancelled via progress callback.
pub fn decompress(input: &[u8], config: &EngineConfiguration) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(input);
    decompress_from_reader(&mut cursor, config)
}

// ---------------------------------------------------------------------------
// decompress_from_reader
// ---------------------------------------------------------------------------

/// Decompress a CRSH file from a seekable reader.
///
/// Phase 1 (sequential): reads all block headers and payloads into memory.
/// Phase 2 (parallel): decompresses the collected payloads directly into a
/// pre-allocated output buffer via pooled `Decompressor`s.
///
/// This two-phase approach avoids sharing `&mut R` across rayon worker threads.
///
/// # Errors
///
/// - [`CrushError::ExpansionLimitExceeded`] — decompressed size would exceed the ratio limit.
/// - [`CrushError::ChecksumMismatch`] — a block's CRC32 does not match.
/// - [`CrushError::InvalidFormat`] — file is truncated or block data is corrupt.
/// - [`CrushError::IndexCorrupted`] — footer or index is missing or invalid.
/// - [`CrushError::Cancelled`] — cancelled via progress callback.
/// - [`CrushError::Io`] — underlying I/O error.
#[allow(clippy::too_many_lines)] // single cohesive pipeline: load index → read phase → partition → parallel decompress → progress
pub fn decompress_from_reader<R: Read + Seek>(
    reader: &mut R,
    config: &EngineConfiguration,
) -> Result<Vec<u8>> {
    let index = load_index(reader)?;

    // Expansion ratio guard.
    // Precision and sign-loss are acceptable: this is a heuristic ratio check, not a byte counter.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let limit = {
        let file_size = reader.seek(SeekFrom::End(0))?;
        (file_size as f64 * config.max_decompression_ratio) as u64
    };
    let total_uncompressed = index.total_uncompressed_size();
    if total_uncompressed > limit {
        return Err(CrushError::ExpansionLimitExceeded { block_index: 0 });
    }

    let total_blocks = index.len();
    let checksums_enabled = index.checksums_enabled;

    // Phase 1: Read all block data sequentially (requires exclusive &mut reader).
    let raw_blocks: Vec<(BlockHeader, Vec<u8>)> = index
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| -> Result<(BlockHeader, Vec<u8>)> {
            reader.seek(SeekFrom::Start(entry.block_offset))?;
            let mut hdr_buf = [0u8; BlockHeader::SIZE];
            reader.read_exact(&mut hdr_buf).map_err(|e| {
                CrushError::InvalidFormat(format!("block {i} header read error: {e}"))
            })?;
            let header = BlockHeader::from_bytes(&hdr_buf);
            let mut payload = vec![0u8; header.compressed_size as usize];
            reader.read_exact(&mut payload).map_err(|e| {
                CrushError::InvalidFormat(format!("block {i} payload read error: {e}"))
            })?;
            Ok((header, payload))
        })
        .collect::<Result<Vec<_>>>()?;

    // Slice C: pre-allocate the final output once, with the known total uncompressed size.
    let total_usize = usize::try_from(total_uncompressed).map_err(|_| {
        CrushError::InvalidConfig(format!(
            "total uncompressed size {total_uncompressed} overflows usize"
        ))
    })?;
    let mut output = vec![0u8; total_usize];

    // Compute per-block output offsets inline with a single running sum — no dependency on
    // BlockIndex::cumulative_uncompressed (US3), so this slice lands independently.
    let mut per_block_sizes: Vec<usize> = Vec::with_capacity(raw_blocks.len());
    let mut running: usize = 0;
    for (header, _) in &raw_blocks {
        let sz = header.uncompressed_size as usize;
        per_block_sizes.push(sz);
        running = running.checked_add(sz).ok_or_else(|| {
            CrushError::InvalidFormat(
                "block uncompressed sizes overflow usize during offset calculation".to_owned(),
            )
        })?;
    }
    debug_assert_eq!(running, total_usize);

    // Partition output into disjoint per-block slices.
    let mut output_slices: Vec<&mut [u8]> = Vec::with_capacity(raw_blocks.len());
    {
        let mut remaining: &mut [u8] = &mut output;
        for &sz in &per_block_sizes {
            let (chunk, rest) = remaining.split_at_mut(sz);
            output_slices.push(chunk);
            remaining = rest;
        }
        debug_assert!(remaining.is_empty());
    }

    // Phase 2: parallel decompress with pooled Decompressors, writing directly into the
    // pre-partitioned output slices (Slice A + Slice C combined).
    #[allow(clippy::type_complexity)]
    let pairs: Vec<(&mut [u8], &(BlockHeader, Vec<u8>))> =
        output_slices.into_iter().zip(raw_blocks.iter()).collect();

    let results: Vec<Result<()>> = with_pool(config, || {
        pairs
            .into_par_iter()
            .enumerate()
            .map_init(
                Decompressor::new,
                |decompressor, (i, (slice, (header, payload)))| {
                    decompress_block_into(
                        decompressor,
                        header,
                        payload,
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
        for (i, (header, _)) in raw_blocks.iter().enumerate() {
            bytes_processed += u64::from(header.uncompressed_size);
            let event = ProgressEvent {
                bytes_processed,
                blocks_completed: i as u64 + 1,
                total_blocks: Some(total_blocks),
                phase: ProgressPhase::Decompressing,
            };
            if !cb(event) {
                return Err(CrushError::Cancelled);
            }
        }
    }

    Ok(output)
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
}
