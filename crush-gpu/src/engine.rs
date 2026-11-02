//! Compression and decompression orchestration
//!
//! Implements tile-based compression using 64KB tiles with `GDeflate` encoding.
//! Compression runs on the CPU ([`crate::gdeflate::gdeflate_compress_tile`]).
//! Decompression can run on GPU (via [`crate::backend::ComputeBackend`]) or CPU fallback.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use crc32fast::Hasher;
use crush_core::error::{CrushError, PluginError, Result};
use rayon::prelude::*;
use tracing::{debug, info, warn};

use crate::format::{
    DEFAULT_SUB_STREAM_COUNT, DEFAULT_TILE_SIZE, GpuFileFooter, GpuFileHeader, TileFlags,
    TileHeader, TileIndexEntry, TileIndexHeader, padding_to_alignment,
};
use crate::gdeflate;

// ============================================================================
// EngineConfig
// ============================================================================

/// Most a DEFLATE stream can expand, rounded up: a 258-byte match costs at least
/// 2 bits, about 1032 to 1. Rejects archives that claim more before allocating.
const MAX_EXPANSION: u64 = 2048;

/// Runtime configuration for the GPU compression engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Tile size in bytes (default 65536 = 64 KB).
    pub tile_size: u32,
    /// Number of parallel sub-streams per tile (default 32).
    pub sub_stream_count: u8,
    /// Whether to store per-tile CRC32 checksums.
    pub enable_checksums: bool,
    /// If `true`, never attempt GPU decompression.
    pub force_cpu: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            tile_size: DEFAULT_TILE_SIZE,
            sub_stream_count: DEFAULT_SUB_STREAM_COUNT,
            enable_checksums: true,
            force_cpu: false,
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Compress `input` into a GPU-format archive.
///
/// # Errors
///
/// * [`CrushError::Cancelled`] if `cancel` is set during processing.
/// * [`PluginError::OperationFailed`] on internal encoding failures.
pub fn compress(input: &[u8], config: &EngineConfig, cancel: &AtomicBool) -> Result<Vec<u8>> {
    if input.is_empty() {
        return write_empty_archive(config);
    }

    let tile_size = config.tile_size as usize;
    let tiles: Vec<&[u8]> = input.chunks(tile_size).collect();
    let tile_count = tiles.len();

    // Pre-allocate output buffer (header + estimated body).
    let mut output = Vec::with_capacity(GpuFileHeader::SIZE + input.len());

    // ── File header ──────────────────────────────────────────────────
    let header = GpuFileHeader::new(
        u64::try_from(tile_count).map_err(|e| PluginError::OperationFailed(e.to_string()))?,
        u64::try_from(input.len()).map_err(|e| PluginError::OperationFailed(e.to_string()))?,
    );
    output.extend_from_slice(&header.to_bytes());

    // Pad file header to 128-byte alignment so tiles start aligned.
    let hdr_pad = padding_to_alignment(output.len());
    output.resize(output.len() + hdr_pad, 0);

    // ── Compress each tile ───────────────────────────────────────────
    // Tiles are independent. Their offsets depend on earlier tiles' sizes,
    // so layout follows as a serial pass and keeps the bytes thread-independent.
    let encoded: Vec<EncodedTile<'_>> = tiles
        .par_iter()
        .enumerate()
        .map(|(i, tile_data)| {
            if cancel.load(Ordering::Relaxed) {
                return Err(CrushError::Cancelled);
            }
            encode_tile(tile_data, i + 1 == tile_count, config)
        })
        .collect::<Result<_>>()?;

    let mut index_entries: Vec<TileIndexEntry> = Vec::with_capacity(tile_count);

    for tile in &encoded {
        let tile_offset =
            u64::try_from(output.len()).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

        output.extend_from_slice(&tile.header.to_bytes());
        output.extend_from_slice(&tile.payload);

        // Pad to 128-byte alignment.
        let written = TileHeader::SIZE + tile.payload.len();
        let pad = padding_to_alignment(written);
        output.resize(output.len() + pad, 0);

        index_entries.push(TileIndexEntry {
            tile_offset,
            compressed_size: tile.header.compressed_size,
            uncompressed_size: tile.header.uncompressed_size,
            checksum: tile.header.checksum,
            flags: u32::from(tile.header.flags.0),
        });
    }

    // ── Tile index ───────────────────────────────────────────────────
    let index_offset =
        u64::try_from(output.len()).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

    let index_header = TileIndexHeader {
        entry_count: u32::try_from(index_entries.len())
            .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
        index_flags: 0,
    };
    output.extend_from_slice(&index_header.to_bytes());

    for entry in &index_entries {
        output.extend_from_slice(&entry.to_bytes());
    }

    let idx_off_usize =
        usize::try_from(index_offset).map_err(|e| PluginError::OperationFailed(e.to_string()))?;
    let index_size = u32::try_from(output.len() - idx_off_usize)
        .map_err(|e| PluginError::OperationFailed(e.to_string()))?;

    // ── Footer ───────────────────────────────────────────────────────
    let footer = GpuFileFooter::new(index_offset, index_size);
    output.extend_from_slice(&footer.to_bytes());

    Ok(output)
}

/// One compressed tile, ready for layout.
struct EncodedTile<'a> {
    header: TileHeader,
    /// Compressed bytes, or the tile itself when it is stored.
    payload: Cow<'a, [u8]>,
}

/// Compress one tile, storing it raw if compression does not shrink it.
fn encode_tile<'a>(
    tile_data: &'a [u8],
    is_last: bool,
    config: &EngineConfig,
) -> Result<EncodedTile<'a>> {
    let checksum = if config.enable_checksums {
        let mut h = Hasher::new();
        h.update(tile_data);
        h.finalize()
    } else {
        0
    };

    // Compress tile with GDeflate (handles 32-way sub-stream interleaving internally).
    let compressed_payload = gdeflate::gdeflate_compress_tile(tile_data)?;

    let mut flags = TileFlags::default();
    if is_last {
        flags = flags.with_last_tile();
    }
    let (payload, flags) = if compressed_payload.len() >= tile_data.len() {
        (Cow::Borrowed(tile_data), flags.with_stored())
    } else {
        (Cow::Owned(compressed_payload), flags)
    };

    let header = TileHeader {
        version: 2,
        flags,
        sub_stream_count: config.sub_stream_count,
        compressed_size: u32::try_from(payload.len())
            .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
        uncompressed_size: u32::try_from(tile_data.len())
            .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
        checksum,
        sub_stream_offsets_size: 0, // GDeflate handles sub-streams internally
    };
    Ok(EncodedTile { header, payload })
}

/// Decompress a GPU-format archive back to the original bytes.
///
/// # Errors
///
/// * [`CrushError::InvalidFormat`] if the archive header/footer is invalid.
/// * [`CrushError::Cancelled`] if `cancel` is set during processing.
pub fn decompress(input: &[u8], config: &EngineConfig, cancel: &AtomicBool) -> Result<Vec<u8>> {
    let mut output = vec![0; decompressed_len(input)?];
    decompress_into(input, config, cancel, &mut output)?;
    Ok(output)
}

/// Exact decompressed length of a GPU-format archive.
///
/// # Errors
///
/// * [`CrushError::InvalidFormat`] if the archive header, footer or tile sizes
///   are invalid.
pub fn decompressed_len(input: &[u8]) -> Result<usize> {
    let entries = parse_archive(input)?.unwrap_or_default();
    let total: u64 = entries.iter().map(|e| u64::from(e.uncompressed_size)).sum();
    usize::try_from(total).map_err(|_| {
        CrushError::InvalidFormat("uncompressed size too large for platform".to_owned())
    })
}

/// Decompress a GPU-format archive into `output`, whose length must be
/// [`decompressed_len`].
///
/// Attempts GPU decompression first. If no GPU is available or GPU decoding
/// fails, every tile is decoded again on the CPU.
///
/// # Errors
///
/// * [`CrushError::InvalidFormat`] if the archive is invalid, or `output` has
///   the wrong length. `output` may then hold partial data.
/// * [`CrushError::Cancelled`] if `cancel` is set during processing.
pub fn decompress_into(
    input: &[u8],
    config: &EngineConfig,
    cancel: &AtomicBool,
    output: &mut [u8],
) -> Result<()> {
    let Some(entries) = parse_archive(input)? else {
        return check_output_len(0, output);
    };
    check_output_len(
        entries.iter().map(|e| u64::from(e.uncompressed_size)).sum(),
        output,
    )?;

    // ── Try GPU decompression first ──────────────────────────────────
    if config.force_cpu {
        debug!("force_cpu=true, skipping GPU discovery");
    } else {
        debug!(
            tile_count = entries.len(),
            force_cpu = config.force_cpu,
            "Attempting GPU decompression for {} tiles",
            entries.len()
        );
        match crate::backend::discover_gpu() {
            Ok(Some(backend)) => {
                info!(
                    backend = backend.name(),
                    tiles = entries.len(),
                    "Using GPU backend '{}' for {} tiles",
                    backend.name(),
                    entries.len()
                );
                return decode_with_fallback(input, &entries, config, cancel, &*backend, output);
            }
            Ok(None) => {
                info!("No GPU backend available, using CPU");
            }
            Err(e) => {
                // GPU discovery returned an explicit error (e.g. user requested
                // --gpu-backend cuda but CUDA is unavailable). Propagate it.
                return Err(e);
            }
        }
    }

    info!(
        tiles = entries.len(),
        "Starting CPU decompression for {} tiles",
        entries.len()
    );
    decompress_tiles_cpu(input, &entries, config, cancel, output)
}

/// Decode on `backend`, and on any failure decode every tile again on the CPU,
/// which overwrites whatever the GPU attempt left in `output`.
fn decode_with_fallback(
    input: &[u8],
    entries: &[TileIndexEntry],
    config: &EngineConfig,
    cancel: &AtomicBool,
    backend: &dyn crate::backend::ComputeBackend,
    output: &mut [u8],
) -> Result<()> {
    match decompress_tiles_gpu(input, entries, config, cancel, backend, output) {
        Ok(()) => {
            info!(
                output_bytes = output.len(),
                "GPU decompression succeeded ({} bytes)",
                output.len()
            );
            Ok(())
        }
        Err(e) => {
            warn!("GPU decompression failed, falling back to CPU: {e}");
            decompress_tiles_cpu(input, entries, config, cancel, output)
        }
    }
}

fn check_output_len(expected: u64, output: &[u8]) -> Result<()> {
    if output.len() as u64 == expected {
        Ok(())
    } else {
        Err(CrushError::InvalidFormat(format!(
            "output holds {} bytes, archive decodes to {expected}",
            output.len()
        )))
    }
}

/// Parse and validate an archive. `None` means it has no tiles.
fn parse_archive(input: &[u8]) -> Result<Option<Vec<TileIndexEntry>>> {
    if input.len() < GpuFileHeader::SIZE + GpuFileFooter::SIZE {
        return try_decompress_empty_archive(input).map(|()| None);
    }

    // ── Footer ───────────────────────────────────────────────────────
    let footer_start = input.len() - GpuFileFooter::SIZE;
    let footer_bytes: &[u8; GpuFileFooter::SIZE] = input[footer_start..]
        .try_into()
        .map_err(|_| CrushError::InvalidFormat("footer truncated".to_owned()))?;
    let footer = GpuFileFooter::from_bytes(footer_bytes)?;

    // ── File header ──────────────────────────────────────────────────
    let hdr_bytes: &[u8; GpuFileHeader::SIZE] = input[..GpuFileHeader::SIZE]
        .try_into()
        .map_err(|_| CrushError::InvalidFormat("header truncated".to_owned()))?;
    let header = GpuFileHeader::from_bytes(hdr_bytes)?;

    if header.tile_count == 0 {
        return Ok(None);
    }

    let entries = read_tile_index(input, &footer, footer_start)?;
    validate_sizes(&header, &entries, input.len())?;
    Ok(Some(entries))
}

/// Every size is checked here, because the caller allocates from them.
fn validate_sizes(
    header: &GpuFileHeader,
    entries: &[TileIndexEntry],
    archive_len: usize,
) -> Result<()> {
    let mut total = 0u64;
    for (i, entry) in entries.iter().enumerate() {
        if entry.uncompressed_size > header.tile_size {
            return Err(CrushError::InvalidFormat(format!(
                "tile {i} holds {} bytes, more than the tile size {}",
                entry.uncompressed_size, header.tile_size
            )));
        }
        total += u64::from(entry.uncompressed_size);
    }
    if total != header.uncompressed_size {
        return Err(CrushError::InvalidFormat(format!(
            "tiles hold {total} bytes, header says {}",
            header.uncompressed_size
        )));
    }
    if total > (archive_len as u64).saturating_mul(MAX_EXPANSION) {
        return Err(CrushError::InvalidFormat(format!(
            "{total} bytes cannot come from a {archive_len}-byte archive"
        )));
    }
    Ok(())
}

/// Attempt to parse an undersized archive as an empty archive.
fn try_decompress_empty_archive(input: &[u8]) -> Result<()> {
    if input.len() == GpuFileHeader::SIZE + TileIndexHeader::SIZE + GpuFileFooter::SIZE {
        let hdr_bytes: &[u8; GpuFileHeader::SIZE] = input[..GpuFileHeader::SIZE]
            .try_into()
            .map_err(|_| CrushError::InvalidFormat("header truncated".to_owned()))?;
        let header = GpuFileHeader::from_bytes(hdr_bytes)?;
        if header.tile_count == 0 {
            return Ok(());
        }
    }
    Err(CrushError::InvalidFormat(
        "archive too small for GPU format".to_owned(),
    ))
}

/// Read the tile index from the archive.
fn read_tile_index(
    input: &[u8],
    footer: &GpuFileFooter,
    footer_start: usize,
) -> Result<Vec<TileIndexEntry>> {
    let idx_off = usize::try_from(footer.index_offset)
        .map_err(|_| CrushError::InvalidFormat("index offset too large for platform".to_owned()))?;
    if idx_off + TileIndexHeader::SIZE > footer_start {
        return Err(CrushError::IndexCorrupted(
            "tile index header beyond archive".to_owned(),
        ));
    }
    let idx_hdr_bytes: &[u8; TileIndexHeader::SIZE] = input
        [idx_off..idx_off + TileIndexHeader::SIZE]
        .try_into()
        .map_err(|_| CrushError::IndexCorrupted("index header truncated".to_owned()))?;
    let idx_hdr = TileIndexHeader::from_bytes(idx_hdr_bytes);

    let entry_count = idx_hdr.entry_count as usize;
    let entries_start = idx_off + TileIndexHeader::SIZE;
    let mut entries = Vec::with_capacity(entry_count);
    for i in 0..entry_count {
        let e_off = entries_start + i * TileIndexEntry::SIZE;
        if e_off + TileIndexEntry::SIZE > footer_start {
            return Err(CrushError::IndexCorrupted(
                "tile index entry beyond archive".to_owned(),
            ));
        }
        let e_bytes: &[u8; TileIndexEntry::SIZE] = input[e_off..e_off + TileIndexEntry::SIZE]
            .try_into()
            .map_err(|_| CrushError::IndexCorrupted("entry truncated".to_owned()))?;
        entries.push(TileIndexEntry::from_bytes(e_bytes));
    }
    Ok(entries)
}

/// CPU-fallback decompression of all tiles.
fn decompress_tiles_cpu(
    input: &[u8],
    entries: &[TileIndexEntry],
    config: &EngineConfig,
    cancel: &AtomicBool,
    output: &mut [u8],
) -> Result<()> {
    let slots = tile_slots(entries, output);
    for (i, (entry, slot)) in entries.iter().zip(slots).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CrushError::Cancelled);
        }

        let tile_data = read_and_decompress_tile(input, entry, i, config.sub_stream_count)?;
        place_tile(&tile_data, slot, entry, i, config)?;
    }

    Ok(())
}

/// Split `output` into one slice per tile. Its length has been checked against
/// `entries`.
fn tile_slots<'a>(entries: &[TileIndexEntry], mut output: &'a mut [u8]) -> Vec<&'a mut [u8]> {
    entries
        .iter()
        .map(|entry| {
            let (slot, rest) =
                std::mem::take(&mut output).split_at_mut(entry.uncompressed_size as usize);
            output = rest;
            slot
        })
        .collect()
}

/// Check one decoded tile and copy it into place.
fn place_tile(
    tile: &[u8],
    slot: &mut [u8],
    entry: &TileIndexEntry,
    index: usize,
    config: &EngineConfig,
) -> Result<()> {
    if tile.len() != slot.len() {
        return Err(CrushError::InvalidFormat(format!(
            "tile {index} decoded to {} bytes, expected {}",
            tile.len(),
            slot.len()
        )));
    }
    if config.enable_checksums && entry.checksum != 0 {
        let mut h = Hasher::new();
        h.update(tile);
        let actual = h.finalize();
        if actual != entry.checksum {
            return Err(CrushError::ChecksumMismatch {
                block_index: u64::try_from(index)
                    .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
                expected: entry.checksum,
                actual,
            });
        }
    }
    slot.copy_from_slice(tile);
    Ok(())
}

/// Decompress tiles on the GPU. Stored tiles need no decoding, so they stay on
/// the CPU, and an archive with nothing else never reaches the backend.
fn decompress_tiles_gpu(
    input: &[u8],
    entries: &[TileIndexEntry],
    config: &EngineConfig,
    cancel: &AtomicBool,
    backend: &dyn crate::backend::ComputeBackend,
    output: &mut [u8],
) -> Result<()> {
    // Build CompressedTile structs from the archive entries.
    debug!(
        tile_count = entries.len(),
        backend = backend.name(),
        "Building compressed tile structs for GPU dispatch"
    );
    let build_start = std::time::Instant::now();
    let mut tiles = Vec::with_capacity(entries.len());
    // One slot per tile: its bytes if stored, or `None` if the GPU decodes it.
    let mut stored = Vec::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CrushError::Cancelled);
        }
        let tile_off = usize::try_from(entry.tile_offset).map_err(|_| {
            CrushError::InvalidFormat("tile offset too large for platform".to_owned())
        })?;
        if tile_off + TileHeader::SIZE > input.len() {
            return Err(CrushError::InvalidFormat(format!(
                "tile {i} header at offset {tile_off} is beyond archive"
            )));
        }
        let th_bytes: &[u8; TileHeader::SIZE] = input[tile_off..tile_off + TileHeader::SIZE]
            .try_into()
            .map_err(|_| CrushError::InvalidFormat("tile header truncated".to_owned()))?;
        let tile_hdr = TileHeader::from_bytes(th_bytes);

        let payload_start = tile_off + TileHeader::SIZE;
        let payload_end = payload_start + tile_hdr.compressed_size as usize;
        if payload_end > input.len() {
            return Err(CrushError::InvalidFormat(format!(
                "tile {i} payload extends beyond archive"
            )));
        }

        let payload = &input[payload_start..payload_end];
        if tile_hdr.flags.stored() {
            stored.push(Some(payload));
        } else {
            stored.push(None);
            tiles.push(crate::backend::CompressedTile {
                data: payload.to_vec(),
                uncompressed_size: tile_hdr.uncompressed_size,
                sub_stream_count: config.sub_stream_count,
                checksum: entry.checksum,
            });
        }
    }
    let build_elapsed = build_start.elapsed();
    debug!(
        tile_count = tiles.len(),
        elapsed_secs = build_elapsed.as_secs_f64(),
        "Built {} tile structs in {:.1}ms",
        tiles.len(),
        build_elapsed.as_secs_f64() * 1000.0
    );

    // Dispatch to GPU backend (GDeflate path).
    debug!(
        "Dispatching {} tiles to {} backend...",
        tiles.len(),
        backend.name()
    );
    let dispatch_start = std::time::Instant::now();
    let decompressed = if tiles.is_empty() {
        Vec::new()
    } else {
        backend.decompress_tiles_gdeflate(&tiles, cancel)?
    };
    let dispatch_elapsed = dispatch_start.elapsed();
    debug!(
        elapsed_secs = dispatch_elapsed.as_secs_f64(),
        "GPU dispatch completed in {:.1}ms",
        dispatch_elapsed.as_secs_f64() * 1000.0
    );

    if decompressed.len() != tiles.len() {
        return Err(PluginError::OperationFailed(format!(
            "GPU returned {} tiles for {}",
            decompressed.len(),
            tiles.len()
        ))
        .into());
    }

    // Validate checksums and place each tile.
    let mut from_gpu = decompressed.iter();
    let slots = tile_slots(entries, output);
    for (i, ((raw, entry), slot)) in stored.iter().zip(entries).zip(slots).enumerate() {
        let tile_data: &[u8] = match raw {
            Some(raw) => raw,
            None => from_gpu.next().ok_or_else(|| {
                PluginError::OperationFailed(format!("GPU returned no output for tile {i}"))
            })?,
        };
        place_tile(tile_data, slot, entry, i, config)?;
    }

    Ok(())
}

/// Read a single tile from the archive and decompress it.
fn read_and_decompress_tile(
    input: &[u8],
    entry: &TileIndexEntry,
    tile_idx: usize,
    sub_stream_count: u8,
) -> Result<Vec<u8>> {
    let tile_off = usize::try_from(entry.tile_offset)
        .map_err(|_| CrushError::InvalidFormat("tile offset too large for platform".to_owned()))?;
    if tile_off + TileHeader::SIZE > input.len() {
        return Err(CrushError::InvalidFormat(format!(
            "tile {tile_idx} header at offset {tile_off} is beyond archive"
        )));
    }
    let th_bytes: &[u8; TileHeader::SIZE] = input[tile_off..tile_off + TileHeader::SIZE]
        .try_into()
        .map_err(|_| CrushError::InvalidFormat("tile header truncated".to_owned()))?;
    let tile_hdr = TileHeader::from_bytes(th_bytes);

    let payload_start = tile_off + TileHeader::SIZE;
    let payload_end = payload_start + tile_hdr.compressed_size as usize;
    if payload_end > input.len() {
        return Err(CrushError::InvalidFormat(format!(
            "tile {tile_idx} payload extends beyond archive"
        )));
    }
    let payload = &input[payload_start..payload_end];

    let _ = sub_stream_count; // GDeflate handles sub-streams internally
    if tile_hdr.flags.stored() {
        Ok(payload.to_vec())
    } else {
        gdeflate::gdeflate_decompress_tile(payload, tile_hdr.uncompressed_size as usize)
    }
}

// (Tile compression and decompression are handled by the gdeflate module.)

// ============================================================================
// Internal: empty archive
// ============================================================================

fn write_empty_archive(config: &EngineConfig) -> Result<Vec<u8>> {
    let header = GpuFileHeader::new(0, 0);
    let mut output =
        Vec::with_capacity(GpuFileHeader::SIZE + TileIndexHeader::SIZE + GpuFileFooter::SIZE);
    output.extend_from_slice(&header.to_bytes());

    let index_offset =
        u64::try_from(output.len()).map_err(|e| PluginError::OperationFailed(e.to_string()))?;
    let idx_hdr = TileIndexHeader {
        entry_count: 0,
        index_flags: 0,
    };
    output.extend_from_slice(&idx_hdr.to_bytes());
    let index_size = u32::try_from(TileIndexHeader::SIZE)
        .map_err(|e| PluginError::OperationFailed(e.to_string()))?;
    let footer = GpuFileFooter::new(index_offset, index_size);
    output.extend_from_slice(&footer.to_bytes());

    let _ = config; // config used for consistency with non-empty path
    Ok(output)
}

// ============================================================================
// Public: random access API (US4)
// ============================================================================

/// Loaded tile index for O(1) tile lookup.
#[derive(Debug, Clone)]
pub struct TileIndex {
    /// Header from the archive.
    pub header: GpuFileHeader,
    /// One entry per tile, in order.
    pub entries: Vec<TileIndexEntry>,
}

impl TileIndex {
    /// Number of tiles in the archive.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.entries.len()
    }

    /// Get an entry by tile index.  Returns `None` if out of bounds.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&TileIndexEntry> {
        self.entries.get(index)
    }
}

/// Load the tile index from a GPU archive without decompressing any tiles.
///
/// This enables O(1) tile lookup for random access decompression.
///
/// # Errors
///
/// * [`CrushError::InvalidFormat`] if the archive is malformed.
pub fn load_tile_index(archive: &[u8]) -> Result<TileIndex> {
    let min_size = GpuFileHeader::SIZE + TileIndexHeader::SIZE + GpuFileFooter::SIZE;
    if archive.len() < min_size {
        return Err(CrushError::InvalidFormat(
            "archive too small for tile index".to_owned(),
        ));
    }

    let footer_start = archive.len() - GpuFileFooter::SIZE;
    let footer_bytes: &[u8; GpuFileFooter::SIZE] = archive[footer_start..]
        .try_into()
        .map_err(|_| CrushError::InvalidFormat("footer truncated".to_owned()))?;
    let footer = GpuFileFooter::from_bytes(footer_bytes)?;

    let hdr_bytes: &[u8; GpuFileHeader::SIZE] = archive[..GpuFileHeader::SIZE]
        .try_into()
        .map_err(|_| CrushError::InvalidFormat("header truncated".to_owned()))?;
    let header = GpuFileHeader::from_bytes(hdr_bytes)?;

    let entries = read_tile_index(archive, &footer, footer_start)?;

    Ok(TileIndex { header, entries })
}

/// Decompress a single tile by index from a GPU archive.
///
/// Only reads the target tile's header + payload — no other tiles are touched.
///
/// # Errors
///
/// * [`CrushError::InvalidFormat`] if the tile index is invalid.
/// * [`CrushError::ChecksumMismatch`] if CRC validation fails.
pub fn decompress_tile_by_index(
    archive: &[u8],
    tile_index: &TileIndex,
    index: usize,
    config: &EngineConfig,
) -> Result<Vec<u8>> {
    let entry = tile_index.get(index).ok_or_else(|| {
        CrushError::InvalidFormat(format!(
            "tile index {index} out of range ({})",
            tile_index.tile_count()
        ))
    })?;

    let tile_data = read_and_decompress_tile(archive, entry, index, config.sub_stream_count)?;

    if config.enable_checksums && entry.checksum != 0 {
        let mut h = Hasher::new();
        h.update(&tile_data);
        let actual = h.finalize();
        if actual != entry.checksum {
            return Err(CrushError::ChecksumMismatch {
                block_index: u64::try_from(index)
                    .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
                expected: entry.checksum,
                actual,
            });
        }
    }

    Ok(tile_data)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::backend::{CompressedTile, ComputeBackend, GpuInfo, GpuVendor};

    const TILE: usize = 65536;

    /// Decodes on the CPU with the same `GDeflate` decoder, so a stored tile
    /// handed to it fails just as it does on a GPU.
    struct FakeBackend {
        info: GpuInfo,
        received: Mutex<Vec<usize>>,
        fault: Fault,
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Fault {
        None,
        WrongBytes,
        TooFewTiles,
        ShortTile,
    }

    impl FakeBackend {
        fn new() -> Self {
            Self::faulty(Fault::None)
        }

        fn faulty(fault: Fault) -> Self {
            Self {
                info: GpuInfo {
                    name: "fake".to_owned(),
                    vendor: GpuVendor::Other,
                    vram_bytes: 0,
                    api_backend: "fake".to_owned(),
                },
                received: Mutex::new(Vec::new()),
                fault,
            }
        }

        fn calls(&self) -> Vec<usize> {
            self.received.lock().unwrap().clone()
        }
    }

    impl ComputeBackend for FakeBackend {
        fn name(&self) -> &str {
            &self.info.name
        }

        fn gpu_info(&self) -> &GpuInfo {
            &self.info
        }

        fn decompress_tiles(
            &self,
            tiles: &[CompressedTile],
            cancel: &AtomicBool,
        ) -> Result<Vec<Vec<u8>>> {
            self.decompress_tiles_gdeflate(tiles, cancel)
        }

        fn decompress_tiles_gdeflate(
            &self,
            tiles: &[CompressedTile],
            _cancel: &AtomicBool,
        ) -> Result<Vec<Vec<u8>>> {
            self.received.lock().unwrap().push(tiles.len());
            let mut decoded = tiles
                .iter()
                .map(|t| {
                    gdeflate::gdeflate_decompress_tile(&t.data, t.uncompressed_size as usize)
                        .or_else(|_| Ok(vec![0; t.uncompressed_size as usize]))
                })
                .collect::<Result<Vec<_>>>()?;
            match self.fault {
                Fault::None => {}
                Fault::WrongBytes => decoded[0][0] ^= 0xff,
                Fault::TooFewTiles => {
                    decoded.pop();
                }
                Fault::ShortTile => {
                    decoded[0].pop();
                }
            }
            Ok(decoded)
        }

        fn release(&self) {}
    }

    fn compressible() -> Vec<u8> {
        b"the quick brown fox jumps over the lazy dog. "
            .iter()
            .copied()
            .cycle()
            .take(TILE)
            .collect()
    }

    fn incompressible(mut state: u64) -> Vec<u8> {
        (0..TILE)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    fn mixed() -> Vec<u8> {
        [
            compressible(),
            incompressible(1),
            compressible(),
            incompressible(2),
        ]
        .concat()
    }

    fn through_gpu(
        original: &[u8],
        config: &EngineConfig,
        backend: &FakeBackend,
    ) -> Result<Vec<u8>> {
        let cancel = AtomicBool::new(false);
        let archive = compress(original, config, &cancel)?;
        let entries = parse_archive(&archive)?.expect("tiles");
        let mut output = vec![0xaa; decompressed_len(&archive)?];
        decompress_tiles_gpu(&archive, &entries, config, &cancel, backend, &mut output)?;
        Ok(output)
    }

    fn with_fallback(original: &[u8], backend: &FakeBackend) -> Vec<u8> {
        let config = EngineConfig::default();
        let cancel = AtomicBool::new(false);
        let archive = compress(original, &config, &cancel).unwrap();
        let entries = parse_archive(&archive).unwrap().expect("tiles");
        let mut output = vec![0xaa; decompressed_len(&archive).unwrap()];
        decode_with_fallback(&archive, &entries, &config, &cancel, backend, &mut output).unwrap();
        output
    }

    #[test]
    fn mixed_tiles_round_trip_through_the_gpu_path() {
        let original = mixed();
        let output = through_gpu(&original, &EngineConfig::default(), &FakeBackend::new()).unwrap();
        assert!(output == original, "output differs from the original");
    }

    #[test]
    fn mixed_tiles_round_trip_with_checksums_off() {
        let original = mixed();
        let config = EngineConfig {
            enable_checksums: false,
            ..EngineConfig::default()
        };
        let output = through_gpu(&original, &config, &FakeBackend::new()).unwrap();
        assert!(output == original, "output differs from the original");
    }

    #[test]
    fn decompress_into_matches_decompress() {
        let config = EngineConfig {
            force_cpu: true,
            ..EngineConfig::default()
        };
        let cancel = AtomicBool::new(false);
        for original in [compressible(), incompressible(4), mixed(), Vec::new()] {
            let archive = compress(&original, &config, &cancel).unwrap();
            let mut output = vec![0xaa; decompressed_len(&archive).unwrap()];
            decompress_into(&archive, &config, &cancel, &mut output).unwrap();
            assert!(output == original, "decompress_into differs");
            assert!(
                decompress(&archive, &config, &cancel).unwrap() == original,
                "decompress differs"
            );
        }
    }

    #[test]
    fn a_wrong_length_output_is_rejected() {
        let config = EngineConfig {
            force_cpu: true,
            ..EngineConfig::default()
        };
        let cancel = AtomicBool::new(false);
        let archive = compress(&compressible(), &config, &cancel).unwrap();
        let mut output = vec![0; TILE - 1];
        assert!(decompress_into(&archive, &config, &cancel, &mut output).is_err());
    }

    #[test]
    fn a_bad_gpu_result_falls_back_and_leaves_no_trace() {
        let original = mixed();
        for fault in [Fault::WrongBytes, Fault::TooFewTiles, Fault::ShortTile] {
            let output = with_fallback(&original, &FakeBackend::faulty(fault));
            assert!(output == original, "fallback output differs");
        }
    }

    #[test]
    fn too_few_tiles_is_an_error_not_a_panic() {
        let result = through_gpu(
            &mixed(),
            &EngineConfig::default(),
            &FakeBackend::faulty(Fault::TooFewTiles),
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_short_tile_is_an_error_not_a_panic() {
        let result = through_gpu(
            &mixed(),
            &EngineConfig::default(),
            &FakeBackend::faulty(Fault::ShortTile),
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_header_that_disagrees_with_the_index_is_rejected() {
        let cancel = AtomicBool::new(false);
        let mut archive = compress(&compressible(), &EngineConfig::default(), &cancel).unwrap();
        // The header's uncompressed size, claiming far more than the tiles hold.
        archive[24..32].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert!(decompressed_len(&archive).is_err());
    }

    #[test]
    fn stored_tiles_never_reach_the_backend() {
        let backend = FakeBackend::new();
        through_gpu(&mixed(), &EngineConfig::default(), &backend).unwrap();
        assert_eq!(backend.calls(), vec![2]);
    }

    #[test]
    fn an_all_stored_archive_skips_the_backend() {
        let backend = FakeBackend::new();
        let original = incompressible(3);
        let output = through_gpu(&original, &EngineConfig::default(), &backend).unwrap();
        assert!(output == original, "output differs from the original");
        assert_eq!(backend.calls(), Vec::<usize>::new());
    }
}
