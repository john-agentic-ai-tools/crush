//! GPU tile format: header, tile, index, and footer structures
//!
//! # File Layout
//!
//! ```text
//! Offset 0:           GpuFileHeader (64 bytes)
//! Offset 64:          Tile 0 — TileHeader (32 bytes) + payload (padded to 128-byte boundary)
//! ...                 Tile N-1
//! Offset X:           TileIndexHeader (8 bytes)
//! Offset X+8:         TileIndexEntry[0..N] (24 bytes each)
//! Offset X+8+24N:     GpuFileFooter (24 bytes)  ← last 24 bytes of file
//! ```

use crc32fast::Hasher;
use crush_core::error::{CrushError, Result};

/// CGPU magic bytes: ASCII "CGPU"
pub const CGPU_MAGIC: [u8; 4] = [0x43, 0x47, 0x50, 0x55];

/// Current format version. Files with a different version are rejected.
/// Version 2: `GDeflate` encoding (replaces v1 LZ77).
pub const FORMAT_VERSION: u32 = 2;

/// Default tile size: 64KB (65536 bytes)
pub const DEFAULT_TILE_SIZE: u32 = 65536;

/// Default sub-stream count: 32 (matches GPU warp width)
pub const DEFAULT_SUB_STREAM_COUNT: u8 = 32;

/// Alignment boundary for tile payloads (128 bytes for GPU memory coalescing)
pub const TILE_ALIGNMENT: usize = 128;

/// Current engine semantic version (set at build time).
pub const ENGINE_VERSION_STR: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// EngineVersion (8 bytes)
// ---------------------------------------------------------------------------

/// Packed 8-byte semantic version of the producing engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub pre: u8,
    pub build: u8,
}

impl EngineVersion {
    /// Parse the crate's `CARGO_PKG_VERSION` into an [`EngineVersion`].
    #[must_use]
    pub fn current() -> Self {
        let v = ENGINE_VERSION_STR;
        let mut parts = v.split('.');
        let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        Self {
            major,
            minor,
            patch,
            pre: 0,
            build: 0,
        }
    }

    /// Serialize to 8 bytes (little-endian).
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..2].copy_from_slice(&self.major.to_le_bytes());
        b[2..4].copy_from_slice(&self.minor.to_le_bytes());
        b[4..6].copy_from_slice(&self.patch.to_le_bytes());
        b[6] = self.pre;
        b[7] = self.build;
        b
    }

    /// Deserialize from 8 bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8; 8]) -> Self {
        Self {
            major: u16::from_le_bytes([b[0], b[1]]),
            minor: u16::from_le_bytes([b[2], b[3]]),
            patch: u16::from_le_bytes([b[4], b[5]]),
            pre: b[6],
            build: b[7],
        }
    }
}

// ---------------------------------------------------------------------------
// GpuFileFlags
// ---------------------------------------------------------------------------

/// Bitfield flags stored in the GPU file header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuFileFlags(pub u8);

impl GpuFileFlags {
    /// Bit 0: per-tile CRC32 checksums are present.
    pub const CHECKSUMS_ENABLED: u8 = 0b0000_0001;
    /// Bit 1: vectorized string matching was applied.
    pub const VECTORIZE_USED: u8 = 0b0000_0010;
    /// Bit 2: entropy was validated before compression.
    pub const ENTROPY_CHECKED: u8 = 0b0000_0100;

    #[must_use]
    pub fn checksums_enabled(self) -> bool {
        self.0 & Self::CHECKSUMS_ENABLED != 0
    }

    #[must_use]
    pub fn vectorize_used(self) -> bool {
        self.0 & Self::VECTORIZE_USED != 0
    }

    #[must_use]
    pub fn entropy_checked(self) -> bool {
        self.0 & Self::ENTROPY_CHECKED != 0
    }

    #[must_use]
    pub fn with_checksums(mut self) -> Self {
        self.0 |= Self::CHECKSUMS_ENABLED;
        self
    }

    #[must_use]
    pub fn with_vectorize(mut self) -> Self {
        self.0 |= Self::VECTORIZE_USED;
        self
    }

    #[must_use]
    pub fn with_entropy_checked(mut self) -> Self {
        self.0 |= Self::ENTROPY_CHECKED;
        self
    }
}

// ---------------------------------------------------------------------------
// GpuFileHeader (64 bytes)
// ---------------------------------------------------------------------------

/// Fixed 64-byte header at the start of every GPU-compressed archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuFileHeader {
    pub magic: [u8; 4],
    pub format_version: u32,
    pub engine_version: EngineVersion,
    pub tile_size: u32,
    pub sub_stream_count: u8,
    pub flags: GpuFileFlags,
    pub uncompressed_size: u64,
    pub tile_count: u64,
}

impl GpuFileHeader {
    pub const SIZE: usize = 64;

    /// Build a header with default tile size (64KB) and sub-stream count (32).
    #[must_use]
    pub fn new(tile_count: u64, uncompressed_size: u64) -> Self {
        Self {
            magic: CGPU_MAGIC,
            format_version: FORMAT_VERSION,
            engine_version: EngineVersion::current(),
            tile_size: DEFAULT_TILE_SIZE,
            sub_stream_count: DEFAULT_SUB_STREAM_COUNT,
            flags: GpuFileFlags::default()
                .with_checksums()
                .with_entropy_checked(),
            uncompressed_size,
            tile_count,
        }
    }

    /// Serialize to exactly 64 bytes (little-endian).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.magic);
        b[4..8].copy_from_slice(&self.format_version.to_le_bytes());
        b[8..16].copy_from_slice(&self.engine_version.to_bytes());
        b[16..20].copy_from_slice(&self.tile_size.to_le_bytes());
        b[20] = self.sub_stream_count;
        b[21] = self.flags.0;
        // b[22..24] reserved
        b[24..32].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        b[32..40].copy_from_slice(&self.tile_count.to_le_bytes());
        // b[40..64] reserved
        b
    }

    /// Deserialize and validate a 64-byte slice.
    ///
    /// # Errors
    ///
    /// - [`CrushError::InvalidFormat`] if magic bytes are wrong.
    /// - [`CrushError::VersionMismatch`] if `format_version` differs.
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Result<Self> {
        let magic = [b[0], b[1], b[2], b[3]];
        if magic != CGPU_MAGIC {
            return Err(CrushError::InvalidFormat(format!(
                "expected CGPU magic {CGPU_MAGIC:?}, got {magic:?}"
            )));
        }
        let format_version = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        if format_version != FORMAT_VERSION {
            let ev = EngineVersion::from_bytes(
                b[8..16]
                    .try_into()
                    .map_err(|_| CrushError::InvalidFormat("header too short".to_owned()))?,
            );
            return Err(CrushError::VersionMismatch {
                file_version: format!(
                    "format v{format_version} (engine {}.{}.{})",
                    ev.major, ev.minor, ev.patch
                ),
                current_version: format!("format v{FORMAT_VERSION} (engine {ENGINE_VERSION_STR})"),
            });
        }
        let engine_version = EngineVersion::from_bytes(
            b[8..16]
                .try_into()
                .map_err(|_| CrushError::InvalidFormat("header too short".to_owned()))?,
        );
        Ok(Self {
            magic,
            format_version,
            engine_version,
            tile_size: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
            sub_stream_count: b[20],
            flags: GpuFileFlags(b[21]),
            uncompressed_size: u64::from_le_bytes(
                b[24..32]
                    .try_into()
                    .map_err(|_| CrushError::InvalidFormat("header truncated".to_owned()))?,
            ),
            tile_count: u64::from_le_bytes(
                b[32..40]
                    .try_into()
                    .map_err(|_| CrushError::InvalidFormat("header truncated".to_owned()))?,
            ),
        })
    }
}

// ---------------------------------------------------------------------------
// TileFlags
// ---------------------------------------------------------------------------

/// Per-tile bitfield flags stored in the tile header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TileFlags(pub u8);

impl TileFlags {
    /// Bit 0: tile is stored raw (uncompressed).
    pub const STORED: u8 = 0b0000_0001;
    /// Bit 1: this is the final tile (may be <64KB).
    pub const LAST_TILE: u8 = 0b0000_0010;

    #[must_use]
    pub fn stored(self) -> bool {
        self.0 & Self::STORED != 0
    }

    #[must_use]
    pub fn last_tile(self) -> bool {
        self.0 & Self::LAST_TILE != 0
    }

    #[must_use]
    pub fn with_stored(mut self) -> Self {
        self.0 |= Self::STORED;
        self
    }

    #[must_use]
    pub fn with_last_tile(mut self) -> Self {
        self.0 |= Self::LAST_TILE;
        self
    }
}

// ---------------------------------------------------------------------------
// TileHeader (32 bytes)
// ---------------------------------------------------------------------------

/// Fixed 32-byte header preceding each compressed tile's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileHeader {
    /// Tile format version (initially 1). Decompressor rejects unknown versions.
    pub version: u8,
    pub flags: TileFlags,
    pub sub_stream_count: u8,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    /// CRC32 of the uncompressed tile data. 0 if checksums disabled.
    pub checksum: u32,
    /// Size of the sub-stream offset table in bytes.
    pub sub_stream_offsets_size: u32,
}

impl TileHeader {
    pub const SIZE: usize = 32;

    /// Serialize to 32 bytes (little-endian).
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = self.version;
        b[1] = self.flags.0;
        b[2] = self.sub_stream_count;
        // b[3] reserved
        b[4..8].copy_from_slice(&self.compressed_size.to_le_bytes());
        b[8..12].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        b[12..16].copy_from_slice(&self.checksum.to_le_bytes());
        b[16..20].copy_from_slice(&self.sub_stream_offsets_size.to_le_bytes());
        // b[20..32] reserved
        b
    }

    /// Deserialize from 32 bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Self {
        Self {
            version: b[0],
            flags: TileFlags(b[1]),
            sub_stream_count: b[2],
            compressed_size: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            uncompressed_size: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            checksum: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            sub_stream_offsets_size: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
        }
    }
}

// ---------------------------------------------------------------------------
// TileIndexEntry (24 bytes)
// ---------------------------------------------------------------------------

/// One entry per tile in the trailing index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileIndexEntry {
    /// Absolute byte offset of the [`TileHeader`] from start of file.
    pub tile_offset: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub checksum: u32,
    pub flags: u32,
}

impl TileIndexEntry {
    pub const SIZE: usize = 24;

    /// Serialize to 24 bytes (little-endian).
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..8].copy_from_slice(&self.tile_offset.to_le_bytes());
        b[8..12].copy_from_slice(&self.compressed_size.to_le_bytes());
        b[12..16].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        b[16..20].copy_from_slice(&self.checksum.to_le_bytes());
        b[20..24].copy_from_slice(&self.flags.to_le_bytes());
        b
    }

    /// Deserialize from 24 bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Self {
        Self {
            tile_offset: u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            compressed_size: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            uncompressed_size: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            checksum: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
            flags: u32::from_le_bytes([b[20], b[21], b[22], b[23]]),
        }
    }
}

// ---------------------------------------------------------------------------
// TileIndexHeader (8 bytes)
// ---------------------------------------------------------------------------

/// 8-byte header immediately before the tile index entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileIndexHeader {
    pub entry_count: u32,
    /// Reserved — must be 0.
    pub index_flags: u32,
}

impl TileIndexHeader {
    pub const SIZE: usize = 8;

    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.entry_count.to_le_bytes());
        b[4..8].copy_from_slice(&self.index_flags.to_le_bytes());
        b
    }

    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Self {
        Self {
            entry_count: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            index_flags: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        }
    }
}

// ---------------------------------------------------------------------------
// GpuFileFooter (24 bytes)
// ---------------------------------------------------------------------------

/// Fixed 24-byte record at the very end of every GPU archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuFileFooter {
    /// Absolute byte offset of the [`TileIndexHeader`] from start of file.
    pub index_offset: u64,
    /// Byte length of the index region.
    pub index_size: u32,
    /// CRC32 of bytes [0..12] of this footer.
    pub footer_checksum: u32,
    /// Redundant copy of format version.
    pub format_version: u32,
    pub magic: [u8; 4],
}

impl GpuFileFooter {
    pub const SIZE: usize = 24;

    /// Build and compute the footer checksum.
    #[must_use]
    pub fn new(index_offset: u64, index_size: u32) -> Self {
        let mut f = Self {
            index_offset,
            index_size,
            footer_checksum: 0,
            format_version: FORMAT_VERSION,
            magic: CGPU_MAGIC,
        };
        f.footer_checksum = f.compute_checksum();
        f
    }

    fn compute_checksum(self) -> u32 {
        let b = self.to_bytes_unchecked();
        let mut h = Hasher::new();
        h.update(&b[0..12]);
        h.finalize()
    }

    fn to_bytes_unchecked(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..8].copy_from_slice(&self.index_offset.to_le_bytes());
        b[8..12].copy_from_slice(&self.index_size.to_le_bytes());
        b[12..16].copy_from_slice(&self.footer_checksum.to_le_bytes());
        b[16..20].copy_from_slice(&self.format_version.to_le_bytes());
        b[20..24].copy_from_slice(&self.magic);
        b
    }

    /// Serialize to 24 bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        self.to_bytes_unchecked()
    }

    /// Deserialize and validate a 24-byte footer.
    ///
    /// # Errors
    ///
    /// - [`CrushError::InvalidFormat`] if magic is wrong or checksum fails.
    /// - [`CrushError::VersionMismatch`] if format version differs.
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Result<Self> {
        let magic = [b[20], b[21], b[22], b[23]];
        if magic != CGPU_MAGIC {
            return Err(CrushError::InvalidFormat(format!(
                "footer magic {magic:?} does not match CGPU"
            )));
        }
        let format_version = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
        if format_version != FORMAT_VERSION {
            return Err(CrushError::VersionMismatch {
                file_version: format!("format v{format_version}"),
                current_version: format!("format v{FORMAT_VERSION} (engine {ENGINE_VERSION_STR})"),
            });
        }
        let footer = Self {
            index_offset: u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            index_size: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            footer_checksum: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            format_version,
            magic,
        };
        let expected = {
            let mut h = Hasher::new();
            h.update(&b[0..12]);
            h.finalize()
        };
        if footer.footer_checksum != expected {
            return Err(CrushError::IndexCorrupted(format!(
                "GPU footer checksum mismatch: expected {expected:#010x}, got {:#010x}",
                footer.footer_checksum
            )));
        }
        Ok(footer)
    }
}

/// Compute padding needed to align a size to the tile alignment boundary.
#[must_use]
pub fn padding_to_alignment(size: usize) -> usize {
    let remainder = size % TILE_ALIGNMENT;
    if remainder == 0 {
        0
    } else {
        TILE_ALIGNMENT - remainder
    }
}
