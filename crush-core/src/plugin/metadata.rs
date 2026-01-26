//! Plugin metadata and file format structures

use crate::error::{Result, ValidationError};
use std::io::{Read, Write};

/// Metadata describing a compression plugin's capabilities and performance
#[derive(Debug, Clone, Copy)]
pub struct PluginMetadata {
    /// Plugin name (e.g., "deflate", "zstd", "lz4")
    pub name: &'static str,

    /// Plugin version (semantic versioning)
    pub version: &'static str,

    /// Unique 4-byte magic number for this plugin's compressed format
    /// Format: [0x43, 0x52, version, `plugin_id`]
    /// Where: 0x43='C', 0x52='R' (Crush identifier)
    pub magic_number: [u8; 4],

    /// Expected throughput in MB/s (measured under standard conditions)
    pub throughput: f64,

    /// Expected compression ratio (`compressed_size` / `original_size`)
    /// Range: (0.0, 1.0] where lower is better compression
    pub compression_ratio: f64,

    /// Human-readable description
    pub description: &'static str,
}

/// Crush compressed file header (16 bytes, little-endian)
///
/// Format:
/// ```text
/// Offset | Size | Field
/// -------|------|-------
/// 0      | 4    | magic_number ([u8; 4])
/// 4      | 8    | original_size (u64, little-endian)
/// 12     | 1    | flags (u8)
/// 13     | 3    | reserved (padding to 16 bytes)
/// ```
///
/// Flags byte (bit fields):
/// - Bit 0: Has CRC32 (if set, CRC32 follows header)
/// - Bit 1: Has metadata (if set, variable-length metadata follows header)
/// - Bits 2-7: Reserved for future use
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct CrushHeader {
    /// Magic number identifying the compression plugin
    pub magic: [u8; 4],

    /// Original uncompressed size in bytes
    pub original_size: u64,

    /// Feature flags (see struct documentation)
    pub flags: u8,

    /// Reserved bytes for future extensions (must be zero)
    pub reserved: [u8; 3],
}

/// Header feature flags
pub mod flags {
    /// CRC32 checksum present after header
    pub const HAS_CRC32: u8 = 0x01;

    /// Variable-length metadata section present
    pub const HAS_METADATA: u8 = 0x02;
}

impl CrushHeader {
    /// Size of the header in bytes (fixed at 16 bytes)
    pub const SIZE: usize = 16;

    /// Crush file format identifier prefix ("CR" = 0x43 0x52)
    pub const MAGIC_PREFIX: [u8; 2] = [0x43, 0x52];

    /// Crush format version (V1 = 0x01)
    pub const VERSION: u8 = 0x01;

    /// Create a new header with the given plugin magic number and original size
    #[must_use]
    pub fn new(magic: [u8; 4], original_size: u64) -> Self {
        Self {
            magic,
            flags: 0,
            original_size,
            reserved: [0; 3],
        }
    }

    /// Create a header with CRC32 flag set
    #[must_use]
    pub fn with_crc32(mut self) -> Self {
        self.flags |= flags::HAS_CRC32;
        self
    }

    /// Create a header with metadata flag set
    #[must_use]
    pub fn with_metadata(mut self) -> Self {
        self.flags |= flags::HAS_METADATA;
        self
    }

    /// Check if this header has a valid Crush magic number prefix
    #[must_use]
    pub fn has_valid_prefix(&self) -> bool {
        self.magic[0] == Self::MAGIC_PREFIX[0] && self.magic[1] == Self::MAGIC_PREFIX[1]
    }

    /// Check if this header has a valid Crush format version
    #[must_use]
    pub fn has_valid_version(&self) -> bool {
        self.magic[2] == Self::VERSION
    }

    /// Get the plugin ID from the magic number (4th byte)
    #[must_use]
    pub fn plugin_id(&self) -> u8 {
        self.magic[3]
    }

    /// Check if CRC32 flag is set
    #[must_use]
    pub fn has_crc32(&self) -> bool {
        (self.flags & flags::HAS_CRC32) != 0
    }

    /// Check if metadata flag is set
    #[must_use]
    pub fn has_metadata(&self) -> bool {
        (self.flags & flags::HAS_METADATA) != 0
    }

    /// Serialize header to bytes (little-endian)
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];

        // Magic number (4 bytes)
        bytes[0..4].copy_from_slice(&self.magic);

        // Original size (8 bytes, little-endian)
        bytes[4..12].copy_from_slice(&self.original_size.to_le_bytes());

        // Flags (1 byte)
        bytes[12] = self.flags;

        // Reserved (3 bytes, must be zero)
        bytes[13..16].copy_from_slice(&self.reserved);

        bytes
    }

    /// Deserialize header from bytes
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The magic number prefix is not valid (not "CR")
    /// - The version byte is unsupported
    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Result<Self> {
        let header = Self {
            magic: [bytes[0], bytes[1], bytes[2], bytes[3]],
            original_size: u64::from_le_bytes([
                bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
            ]),
            flags: bytes[12],
            reserved: [bytes[13], bytes[14], bytes[15]],
        };

        // Validate Crush format
        if !header.has_valid_prefix() {
            return Err(ValidationError::InvalidMagic(header.magic).into());
        }

        if !header.has_valid_version() {
            return Err(ValidationError::InvalidHeader(format!(
                "Unsupported version: 0x{:02x}",
                header.magic[2]
            ))
            .into());
        }

        Ok(header)
    }

    /// Write header to a writer
    ///
    /// # Errors
    ///
    /// Returns an error if the write operation fails
    pub fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.to_bytes())
    }

    /// Read header from a reader
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The read operation fails
    /// - The header validation fails (invalid magic or version)
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut bytes = [0u8; Self::SIZE];
        reader.read_exact(&mut bytes)?;
        Self::from_bytes(&bytes)
    }
}

use serde::Serialize; // This one should stay

/// Optional file metadata that can be stored in the compressed file
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct FileMetadata {
    /// Modification time (seconds since Unix epoch)
    pub mtime: Option<i64>,

    /// Unix file permissions (mode bits)
    /// Only stored and restored on Unix platforms
    #[cfg(unix)]
    pub permissions: Option<u32>,
}

impl FileMetadata {
    /// Serialize metadata to a byte vector using TLV format
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        if let Some(mtime) = self.mtime {
            // Type: 0x01 for mtime
            bytes.push(0x01);
            // Length: 8 bytes for i64
            bytes.push(8);
            // Value: mtime as i64
            bytes.extend_from_slice(&mtime.to_le_bytes());
        }
        #[cfg(unix)]
        if let Some(permissions) = self.permissions {
            // Type: 0x02 for Unix permissions
            bytes.push(0x02);
            // Length: 4 bytes for u32
            bytes.push(4);
            // Value: permissions as u32
            bytes.extend_from_slice(&permissions.to_le_bytes());
        }
        bytes
    }

    /// Deserialize metadata from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut metadata = Self::default();
        let mut i = 0;
        while i < bytes.len() {
            if i + 2 > bytes.len() {
                return Err(ValidationError::InvalidMetadata("Incomplete TLV record".into()).into());
            }
            let type_ = bytes[i];
            let length = bytes[i + 1] as usize;
            i += 2;

            if i + length > bytes.len() {
                return Err(ValidationError::InvalidMetadata("Incomplete TLV value".into()).into());
            }

            let value = &bytes[i..i + length];
            i += length;

            match type_ {
                0x01 => { // mtime
                    if length == 8 {
                        let mut mtime_bytes = [0u8; 8];
                        mtime_bytes.copy_from_slice(value);
                        metadata.mtime = Some(i64::from_le_bytes(mtime_bytes));
                    } else {
                        return Err(ValidationError::InvalidMetadata("Invalid mtime length".into()).into());
                    }
                }
                #[cfg(unix)]
                0x02 => { // Unix permissions
                    if length == 4 {
                        let mut perm_bytes = [0u8; 4];
                        perm_bytes.copy_from_slice(value);
                        metadata.permissions = Some(u32::from_le_bytes(perm_bytes));
                    } else {
                        return Err(ValidationError::InvalidMetadata("Invalid permissions length".into()).into());
                    }
                }
                _ => { /* Ignore unknown types for forward compatibility */ }
            }
        }
        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_size() {
        // The serialized header is always 16 bytes
        assert_eq!(CrushHeader::SIZE, 16);

        // Note: The in-memory struct size may be larger due to alignment padding
        // (typically 24 bytes with repr(C)), but serialization produces exactly 16 bytes
        let header = CrushHeader::new([0x43, 0x52, 0x01, 0x00], 12345);
        assert_eq!(header.to_bytes().len(), 16);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_header_roundtrip() {
        let original = CrushHeader::new([0x43, 0x52, 0x01, 0x00], 12345);
        let bytes = original.to_bytes();
        let deserialized = CrushHeader::from_bytes(&bytes).unwrap();

        assert_eq!(original, deserialized);
        assert_eq!(deserialized.original_size, 12345);
    }

    #[test]
    fn test_invalid_magic_prefix() {
        let mut bytes = [0u8; CrushHeader::SIZE];
        bytes[0] = 0xFF; // Invalid prefix
        bytes[1] = 0xFF;

        let result = CrushHeader::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_crc32_flag() {
        let header = CrushHeader::new([0x43, 0x52, 0x01, 0x00], 100).with_crc32();
        assert!(header.has_crc32());

        let bytes = header.to_bytes();
        assert_eq!(bytes[12] & flags::HAS_CRC32, flags::HAS_CRC32);
    }
}
