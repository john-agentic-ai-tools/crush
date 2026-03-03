//! Compression and decompression orchestration
//!
//! Implements tile-based compression using 64KB tiles with `GDeflate` encoding.
//! Compression runs on the CPU ([`crate::gdeflate::gdeflate_compress_tile`]).
//! Decompression can run on GPU (via [`crate::backend::ComputeBackend`]) or CPU fallback.

use std::sync::atomic::{AtomicBool, Ordering};

use crc32fast::Hasher;
use crush_core::error::{CrushError, PluginError, Result};

use crate::format::{
    padding_to_alignment, GpuFileFooter, GpuFileHeader, TileFlags, TileHeader, TileIndexEntry,
    TileIndexHeader, DEFAULT_SUB_STREAM_COUNT, DEFAULT_TILE_SIZE,
};
use crate::gdeflate;

// ============================================================================
// EngineConfig
// ============================================================================

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
    let mut index_entries: Vec<TileIndexEntry> = Vec::with_capacity(tile_count);

    for (i, tile_data) in tiles.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CrushError::Cancelled);
        }

        let is_last = i + 1 == tile_count;

        let tile_offset =
            u64::try_from(output.len()).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

        let checksum = if config.enable_checksums {
            let mut h = Hasher::new();
            h.update(tile_data);
            h.finalize()
        } else {
            0
        };

        // Compress tile with GDeflate (handles 32-way sub-stream interleaving internally).
        let compressed_payload = gdeflate::gdeflate_compress_tile(tile_data)?;

        let compressed_size = u32::try_from(compressed_payload.len())
            .map_err(|e| PluginError::OperationFailed(e.to_string()))?;
        let uncompressed_size = u32::try_from(tile_data.len())
            .map_err(|e| PluginError::OperationFailed(e.to_string()))?;

        // Build tile flags.
        let mut flags = TileFlags::default();
        if is_last {
            flags = flags.with_last_tile();
        }
        // If "compression" expanded the data, store raw.
        let (final_payload, final_compressed_size, final_flags) =
            if compressed_payload.len() >= tile_data.len() {
                // Stored mode – write raw tile
                let stored_size = u32::try_from(tile_data.len())
                    .map_err(|e| PluginError::OperationFailed(e.to_string()))?;
                (tile_data.to_vec(), stored_size, flags.with_stored())
            } else {
                (compressed_payload, compressed_size, flags)
            };

        let tile_header = TileHeader {
            version: 2,
            flags: final_flags,
            sub_stream_count: config.sub_stream_count,
            compressed_size: final_compressed_size,
            uncompressed_size,
            checksum,
            sub_stream_offsets_size: 0, // GDeflate handles sub-streams internally
        };

        output.extend_from_slice(&tile_header.to_bytes());
        output.extend_from_slice(&final_payload);

        // Pad to 128-byte alignment.
        let written = TileHeader::SIZE + final_payload.len();
        let pad = padding_to_alignment(written);
        output.resize(output.len() + pad, 0);

        let entry = TileIndexEntry {
            tile_offset,
            compressed_size: final_compressed_size,
            uncompressed_size,
            checksum,
            flags: u32::from(final_flags.0),
        };
        index_entries.push(entry);
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

/// Decompress a GPU-format archive back to the original bytes.
///
/// # Errors
///
/// * [`CrushError::InvalidFormat`] if the archive header/footer is invalid.
/// * [`CrushError::Cancelled`] if `cancel` is set during processing.
pub fn decompress(input: &[u8], config: &EngineConfig, cancel: &AtomicBool) -> Result<Vec<u8>> {
    if input.len() < GpuFileHeader::SIZE + GpuFileFooter::SIZE {
        return try_decompress_empty_archive(input);
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
        return Ok(Vec::new());
    }

    let entries = read_tile_index(input, &footer, footer_start)?;

    // ── Try GPU decompression first ──────────────────────────────────
    if !config.force_cpu {
        if let Ok(Some(backend)) = crate::backend::discover_gpu() {
            match decompress_tiles_gpu(input, &header, &entries, config, cancel, &*backend) {
                Ok(output) => {
                    return Ok(output);
                }
                Err(e) => {
                    eprintln!("crush-gpu: GPU decompression failed, falling back to CPU: {e}");
                }
            }
        }
    }

    decompress_tiles_cpu(input, &header, &entries, config, cancel)
}

/// Attempt to parse an undersized archive as an empty archive.
fn try_decompress_empty_archive(input: &[u8]) -> Result<Vec<u8>> {
    if input.len() == GpuFileHeader::SIZE + TileIndexHeader::SIZE + GpuFileFooter::SIZE {
        let hdr_bytes: &[u8; GpuFileHeader::SIZE] = input[..GpuFileHeader::SIZE]
            .try_into()
            .map_err(|_| CrushError::InvalidFormat("header truncated".to_owned()))?;
        let header = GpuFileHeader::from_bytes(hdr_bytes)?;
        if header.tile_count == 0 {
            return Ok(Vec::new());
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
    header: &GpuFileHeader,
    entries: &[TileIndexEntry],
    config: &EngineConfig,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let uncompressed_total = usize::try_from(header.uncompressed_size).map_err(|_| {
        CrushError::InvalidFormat("uncompressed size too large for platform".to_owned())
    })?;
    let mut output = Vec::with_capacity(uncompressed_total);

    for (i, entry) in entries.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(CrushError::Cancelled);
        }

        let tile_data = read_and_decompress_tile(input, entry, i, config.sub_stream_count)?;

        if config.enable_checksums && entry.checksum != 0 {
            let mut h = Hasher::new();
            h.update(&tile_data);
            let actual = h.finalize();
            if actual != entry.checksum {
                return Err(CrushError::ChecksumMismatch {
                    block_index: u64::try_from(i)
                        .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
                    expected: entry.checksum,
                    actual,
                });
            }
        }

        output.extend_from_slice(&tile_data);
    }

    Ok(output)
}

/// GPU decompression of all tiles via a [`ComputeBackend`].
fn decompress_tiles_gpu(
    input: &[u8],
    header: &GpuFileHeader,
    entries: &[TileIndexEntry],
    config: &EngineConfig,
    cancel: &AtomicBool,
    backend: &dyn crate::backend::ComputeBackend,
) -> Result<Vec<u8>> {
    // Build CompressedTile structs from the archive entries.
    let mut tiles = Vec::with_capacity(entries.len());
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

        tiles.push(crate::backend::CompressedTile {
            data: input[payload_start..payload_end].to_vec(),
            uncompressed_size: tile_hdr.uncompressed_size,
            sub_stream_count: config.sub_stream_count,
            checksum: entry.checksum,
        });
    }

    // Dispatch to GPU backend (GDeflate path).
    let decompressed = backend.decompress_tiles_gdeflate(&tiles, cancel)?;

    // Validate checksums and assemble output.
    let uncompressed_total = usize::try_from(header.uncompressed_size).map_err(|_| {
        CrushError::InvalidFormat("uncompressed size too large for platform".to_owned())
    })?;
    let mut output = Vec::with_capacity(uncompressed_total);
    for (i, (tile_data, entry)) in decompressed.iter().zip(entries.iter()).enumerate() {
        if config.enable_checksums && entry.checksum != 0 {
            let mut h = Hasher::new();
            h.update(tile_data);
            let actual = h.finalize();
            if actual != entry.checksum {
                return Err(CrushError::ChecksumMismatch {
                    block_index: u64::try_from(i)
                        .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
                    expected: entry.checksum,
                    actual,
                });
            }
        }
        output.extend_from_slice(tile_data);
    }

    Ok(output)
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
