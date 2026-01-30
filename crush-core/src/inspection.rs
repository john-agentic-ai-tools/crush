use crate::error::{PluginError, Result, ValidationError};
use crate::plugin::registry::get_plugin_by_magic;
use crate::plugin::{CrushHeader, FileMetadata};
use crc32fast::Hasher;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct InspectResult {
    pub original_size: u64,
    pub compressed_size: u64,
    pub plugin_name: String,
    pub crc_valid: bool,
    pub metadata: FileMetadata,
}

/// Inspects a compressed file and returns metadata about its contents.
///
/// # Errors
///
/// Returns an error if:
/// - The input is too short to contain a valid header
/// - The header is invalid or corrupted
/// - The CRC checksum validation fails
/// - The plugin specified in the header is not found
pub fn inspect(input: &[u8]) -> Result<InspectResult> {
    if input.len() < CrushHeader::SIZE {
        return Err(ValidationError::InvalidHeader(format!(
            "Input too short: {} bytes, expected at least {}",
            input.len(),
            CrushHeader::SIZE
        ))
        .into());
    }

    let header_bytes: [u8; CrushHeader::SIZE] = input[0..CrushHeader::SIZE]
        .try_into()
        .map_err(|_| ValidationError::InvalidHeader("Failed to read header".to_string()))?;
    let header = CrushHeader::from_bytes(&header_bytes)?;

    let mut payload_start = CrushHeader::SIZE;
    let mut crc_valid = false;

    if header.has_crc32() {
        if input.len() < payload_start + 4 {
            return Err(ValidationError::InvalidHeader(
                "Truncated: CRC32 flag set but no CRC32 data".to_string(),
            )
            .into());
        }
        let stored_crc = u32::from_le_bytes([
            input[payload_start],
            input[payload_start + 1],
            input[payload_start + 2],
            input[payload_start + 3],
        ]);
        payload_start += 4;

        let payload_for_crc = &input[payload_start..];
        let mut hasher = Hasher::new();
        hasher.update(payload_for_crc);
        let computed_crc = hasher.finalize();

        crc_valid = stored_crc == computed_crc;
    }

    let metadata = if header.has_metadata() {
        if input.len() < payload_start + 2 {
            return Err(ValidationError::InvalidHeader(
                "Truncated: metadata flag set but no metadata length".to_string(),
            )
            .into());
        }
        let metadata_len =
            u16::from_le_bytes([input[payload_start], input[payload_start + 1]]) as usize;
        payload_start += 2;

        if input.len() < payload_start + metadata_len {
            return Err(ValidationError::InvalidHeader(
                "Truncated: metadata length exceeds payload size".to_string(),
            )
            .into());
        }
        let metadata_bytes = &input[payload_start..payload_start + metadata_len];

        FileMetadata::from_bytes(metadata_bytes)?
    } else {
        FileMetadata::default()
    };

    let plugin = get_plugin_by_magic(header.magic).ok_or_else(|| {
        PluginError::NotFound(format!(
            "No plugin found for magic number {:02X?}",
            header.magic
        ))
    })?;

    Ok(InspectResult {
        original_size: header.original_size,
        compressed_size: input.len() as u64,
        plugin_name: plugin.name().to_string(),
        crc_valid,
        metadata,
    })
}
