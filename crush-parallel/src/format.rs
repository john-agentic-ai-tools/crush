//! CRSH binary file format serialization/deserialization.
//!
//! # File Layout
//!
//! ```text
//! Offset 0:        FileHeader (64 bytes)
//! Offset 64:       Block 0 — BlockHeader (16 bytes) + payload (compressed_size bytes)
//! ...              Block N-1
//! Offset X:        IndexHeader (8 bytes)
//! Offset X+8:      BlockIndexEntry[0..N] (20 bytes each)
//! Offset X+8+20N:  FileFooter (24 bytes)  ← last 24 bytes of file
//! ```

use crc32fast::Hasher;
use crush_core::error::{CrushError, Result};

/// CRSH magic bytes: ASCII "CRSH"
pub const CRSH_MAGIC: [u8; 4] = [0x43, 0x52, 0x53, 0x48];

/// Current format version. Files with a different version are rejected.
pub const FORMAT_VERSION: u32 = 1;

/// Current engine semantic version (set at build time).
pub const ENGINE_VERSION_STR: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// EngineVersion
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

    /// Format as a human-readable version string.
    #[must_use]
    pub fn to_string_repr(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ---------------------------------------------------------------------------
// FileFlags
// ---------------------------------------------------------------------------

/// Bitfield flags stored in the file header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileFlags(pub u8);

impl FileFlags {
    /// Bit 0: per-block CRC32 checksums are present and validated.
    pub const CHECKSUMS_ENABLED: u8 = 0b0000_0001;
    /// Bit 1: file was produced from a streaming input (sizes may be `u64::MAX`).
    pub const STREAMING: u8 = 0b0000_0010;

    #[must_use]
    pub fn checksums_enabled(self) -> bool {
        self.0 & Self::CHECKSUMS_ENABLED != 0
    }
    #[must_use]
    pub fn streaming(self) -> bool {
        self.0 & Self::STREAMING != 0
    }

    #[must_use]
    pub fn with_checksums(mut self) -> Self {
        self.0 |= Self::CHECKSUMS_ENABLED;
        self
    }
    #[must_use]
    pub fn with_streaming(mut self) -> Self {
        self.0 |= Self::STREAMING;
        self
    }
}

// ---------------------------------------------------------------------------
// BlockFlags
// ---------------------------------------------------------------------------

/// Per-block bitfield flags stored in the block header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlockFlags(pub u8);

impl BlockFlags {
    /// Bit 0: block is stored raw (uncompressed).
    pub const STORED: u8 = 0b0000_0001;

    #[must_use]
    pub fn stored(self) -> bool {
        self.0 & Self::STORED != 0
    }

    #[must_use]
    pub fn with_stored(mut self) -> Self {
        self.0 |= Self::STORED;
        self
    }
}

// ---------------------------------------------------------------------------
// FileHeader (64 bytes)
// ---------------------------------------------------------------------------

/// Fixed 64-byte header at the start of every CRSH file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHeader {
    pub magic: [u8; 4],
    pub format_version: u32,
    pub engine_version: EngineVersion,
    pub block_size: u32,
    pub compression_level: u8,
    pub flags: FileFlags,
    /// `u64::MAX` = unknown (streaming input).
    pub uncompressed_size: u64,
    /// `u64::MAX` = unknown (streaming input).
    pub block_count: u64,
}

impl FileHeader {
    pub const SIZE: usize = 64;

    /// Build a header from engine configuration values.
    #[must_use]
    pub fn new(
        block_size: u32,
        compression_level: u8,
        flags: FileFlags,
        uncompressed_size: u64,
        block_count: u64,
    ) -> Self {
        Self {
            magic: CRSH_MAGIC,
            format_version: FORMAT_VERSION,
            engine_version: EngineVersion::current(),
            block_size,
            compression_level,
            flags,
            uncompressed_size,
            block_count,
        }
    }

    /// Serialize to exactly 64 bytes (little-endian).
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.magic);
        b[4..8].copy_from_slice(&self.format_version.to_le_bytes());
        b[8..16].copy_from_slice(&self.engine_version.to_bytes());
        b[16..20].copy_from_slice(&self.block_size.to_le_bytes());
        b[20] = self.compression_level;
        b[21] = self.flags.0;
        // b[22..24] reserved — zero
        b[24..32].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        b[32..40].copy_from_slice(&self.block_count.to_le_bytes());
        // b[40..64] reserved — zero
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
        if magic != CRSH_MAGIC {
            return Err(CrushError::InvalidFormat(format!(
                "expected magic {CRSH_MAGIC:?}, got {magic:?}"
            )));
        }
        let format_version = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        if format_version != FORMAT_VERSION {
            let ev = EngineVersion::from_bytes(b[8..16].try_into().map_err(|_| {
                CrushError::InvalidFormat("header too short for engine version".to_owned())
            })?);
            return Err(CrushError::VersionMismatch {
                file_version: format!("format v{format_version} (engine {})", ev.to_string_repr()),
                current_version: format!("format v{FORMAT_VERSION} (engine {ENGINE_VERSION_STR})"),
            });
        }
        let engine_version = EngineVersion::from_bytes(
            b[8..16]
                .try_into()
                .map_err(|_| CrushError::InvalidFormat("header too short".to_owned()))?,
        );
        let block_size = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);
        let compression_level = b[20];
        let flags = FileFlags(b[21]);
        let uncompressed_size = u64::from_le_bytes(b[24..32].try_into().map_err(|_| {
            CrushError::InvalidFormat("header truncated at uncompressed_size".to_owned())
        })?);
        let block_count = u64::from_le_bytes(b[32..40].try_into().map_err(|_| {
            CrushError::InvalidFormat("header truncated at block_count".to_owned())
        })?);
        Ok(Self {
            magic,
            format_version,
            engine_version,
            block_size,
            compression_level,
            flags,
            uncompressed_size,
            block_count,
        })
    }
}

// ---------------------------------------------------------------------------
// BlockHeader (16 bytes)
// ---------------------------------------------------------------------------

/// Fixed 16-byte header preceding each compressed block's payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHeader {
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    /// CRC32 of the **uncompressed** block data. `0` if checksums disabled.
    pub checksum: u32,
    pub flags: BlockFlags,
}

impl BlockHeader {
    pub const SIZE: usize = 16;

    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..4].copy_from_slice(&self.compressed_size.to_le_bytes());
        b[4..8].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        b[8..12].copy_from_slice(&self.checksum.to_le_bytes());
        b[12] = self.flags.0;
        // b[13..16] reserved — zero
        b
    }

    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Self {
        Self {
            compressed_size: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            uncompressed_size: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            checksum: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            flags: BlockFlags(b[12]),
        }
    }
}

// ---------------------------------------------------------------------------
// BlockIndexEntry (20 bytes)
// ---------------------------------------------------------------------------

/// One entry in the trailing block index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockIndexEntry {
    /// Absolute byte offset of the [`BlockHeader`] from start of file.
    pub block_offset: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub checksum: u32,
}

impl BlockIndexEntry {
    pub const SIZE: usize = 20;

    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0..8].copy_from_slice(&self.block_offset.to_le_bytes());
        b[8..12].copy_from_slice(&self.compressed_size.to_le_bytes());
        b[12..16].copy_from_slice(&self.uncompressed_size.to_le_bytes());
        b[16..20].copy_from_slice(&self.checksum.to_le_bytes());
        b
    }

    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Self {
        Self {
            block_offset: u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            compressed_size: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            uncompressed_size: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
            checksum: u32::from_le_bytes([b[16], b[17], b[18], b[19]]),
        }
    }
}

// ---------------------------------------------------------------------------
// IndexHeader (8 bytes)
// ---------------------------------------------------------------------------

/// 8-byte header immediately before the block index entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexHeader {
    pub entry_count: u32,
    /// Reserved — must be 0.
    pub index_flags: u32,
}

impl IndexHeader {
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
// FileFooter (24 bytes)
// ---------------------------------------------------------------------------

/// Fixed 24-byte record at the very end of every CRSH file.
///
/// Reading algorithm:
/// 1. Seek to `file_size - 24`.
/// 2. Read and deserialize this struct.
/// 3. Validate magic, `format_version`, and `footer_checksum`.
/// 4. Seek to `index_offset` to read the index region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFooter {
    /// Absolute byte offset of the [`IndexHeader`] from start of file.
    pub index_offset: u64,
    /// Byte length of the index region: `8 + 20 * block_count`.
    pub index_size: u32,
    /// CRC32 of bytes `[0..12]` of this footer (protects `index_offset` + `index_size`).
    pub footer_checksum: u32,
    /// Redundant copy of `FileHeader::format_version`.
    pub format_version: u32,
    pub magic: [u8; 4],
}

impl FileFooter {
    pub const SIZE: usize = 24;

    /// Build and compute the footer checksum.
    #[must_use]
    pub fn new(index_offset: u64, index_size: u32) -> Self {
        let mut f = Self {
            index_offset,
            index_size,
            footer_checksum: 0,
            format_version: FORMAT_VERSION,
            magic: CRSH_MAGIC,
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

    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::SIZE] {
        self.to_bytes_unchecked()
    }

    /// Deserialize and validate a 24-byte footer.
    ///
    /// # Errors
    ///
    /// - [`CrushError::InvalidFormat`] if magic is wrong or checksum fails.
    /// - [`CrushError::VersionMismatch`] if `format_version` differs.
    pub fn from_bytes(b: &[u8; Self::SIZE]) -> Result<Self> {
        let magic = [b[20], b[21], b[22], b[23]];
        if magic != CRSH_MAGIC {
            return Err(CrushError::InvalidFormat(format!(
                "footer magic {magic:?} does not match CRSH"
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
                "footer checksum mismatch: expected {expected:#010x}, got {:#010x}",
                footer.footer_checksum
            )));
        }
        Ok(footer)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_file_header_roundtrip() {
        let h = FileHeader::new(
            1_048_576,
            6,
            FileFlags::default().with_checksums(),
            10_000_000,
            10,
        );
        let bytes = h.to_bytes();
        assert_eq!(bytes.len(), FileHeader::SIZE);
        let h2 = FileHeader::from_bytes(&bytes).expect("roundtrip");
        assert_eq!(h, h2);
    }

    #[test]
    fn test_file_header_magic_rejection() {
        let mut bytes = [0u8; FileHeader::SIZE];
        bytes[0] = 0xFF; // wrong magic
        let result = FileHeader::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_header_version_mismatch() {
        let mut h = FileHeader::new(1_048_576, 6, FileFlags::default(), 0, 0);
        h.magic = CRSH_MAGIC; // ensure correct magic
        let mut bytes = h.to_bytes();
        // Overwrite format_version with an unsupported value
        bytes[4..8].copy_from_slice(&9999u32.to_le_bytes());
        let result = FileHeader::from_bytes(&bytes);
        assert!(matches!(result, Err(CrushError::VersionMismatch { .. })));
    }

    #[test]
    fn test_block_header_roundtrip() {
        let bh = BlockHeader {
            compressed_size: 512,
            uncompressed_size: 1024,
            checksum: 0xDEAD_BEEF,
            flags: BlockFlags::default(),
        };
        let bytes = bh.to_bytes();
        assert_eq!(bytes.len(), BlockHeader::SIZE);
        let bh2 = BlockHeader::from_bytes(&bytes);
        assert_eq!(bh, bh2);
    }

    #[test]
    fn test_block_index_entry_roundtrip() {
        let e = BlockIndexEntry {
            block_offset: 12345,
            compressed_size: 888,
            uncompressed_size: 1024,
            checksum: 0xCAFE_BABE,
        };
        let bytes = e.to_bytes();
        assert_eq!(bytes.len(), BlockIndexEntry::SIZE);
        let e2 = BlockIndexEntry::from_bytes(&bytes);
        assert_eq!(e, e2);
    }

    #[test]
    fn test_index_header_roundtrip() {
        let ih = IndexHeader {
            entry_count: 42,
            index_flags: 0,
        };
        let bytes = ih.to_bytes();
        assert_eq!(bytes.len(), IndexHeader::SIZE);
        let ih2 = IndexHeader::from_bytes(&bytes);
        assert_eq!(ih, ih2);
    }

    #[test]
    fn test_file_footer_roundtrip() {
        let ff = FileFooter::new(99999, 8 + 20 * 10);
        let bytes = ff.to_bytes();
        assert_eq!(bytes.len(), FileFooter::SIZE);
        let ff2 = FileFooter::from_bytes(&bytes).expect("roundtrip");
        assert_eq!(ff, ff2);
    }

    #[test]
    fn test_file_footer_magic_rejection() {
        let mut bytes = [0u8; FileFooter::SIZE];
        bytes[20..24].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        let result = FileFooter::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_footer_truncated_detection() {
        // A correct footer but with a mangled checksum byte
        let ff = FileFooter::new(1000, 208);
        let mut bytes = ff.to_bytes();
        bytes[12] ^= 0xFF; // corrupt checksum
        let result = FileFooter::from_bytes(&bytes);
        assert!(matches!(result, Err(CrushError::IndexCorrupted(_))));
    }
}
