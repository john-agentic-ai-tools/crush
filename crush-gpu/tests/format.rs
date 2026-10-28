//! GPU tile format serialization tests

use crush_gpu::format::{
    CGPU_MAGIC, GpuFileFooter, GpuFileHeader, TileFlags, TileHeader, TileIndexEntry,
    TileIndexHeader,
};

// T009: GpuFileHeader round-trip tests

#[test]
fn test_gpu_file_header_size() {
    assert_eq!(GpuFileHeader::SIZE, 64);
}

#[test]
#[allow(clippy::expect_used)]
fn test_gpu_file_header_roundtrip() {
    let header = GpuFileHeader::new(3200, 500_000_000);
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), GpuFileHeader::SIZE);
    let deserialized = GpuFileHeader::from_bytes(&bytes).expect("valid header should parse");
    assert_eq!(header, deserialized);
    assert_eq!(deserialized.tile_count, 3200);
    assert_eq!(deserialized.uncompressed_size, 500_000_000);
    assert_eq!(deserialized.tile_size, 65536);
    assert_eq!(deserialized.sub_stream_count, 32);
}

#[test]
fn test_gpu_file_header_rejects_invalid_magic() {
    let mut bytes = [0u8; GpuFileHeader::SIZE];
    bytes[0] = 0xFF; // wrong magic
    let result = GpuFileHeader::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_gpu_file_header_rejects_invalid_version() {
    let header = GpuFileHeader::new(10, 1000);
    let mut bytes = header.to_bytes();
    // Overwrite format version with unsupported value
    bytes[4..8].copy_from_slice(&9999u32.to_le_bytes());
    let result = GpuFileHeader::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_gpu_file_header_magic_is_cgpu() {
    let header = GpuFileHeader::new(1, 100);
    let bytes = header.to_bytes();
    assert_eq!(&bytes[0..4], &CGPU_MAGIC);
}

// T010: TileHeader round-trip tests

#[test]
fn test_tile_header_size() {
    assert_eq!(TileHeader::SIZE, 32);
}

#[test]
fn test_tile_header_roundtrip() {
    let header = TileHeader {
        version: 1,
        flags: TileFlags::default(),
        sub_stream_count: 32,
        compressed_size: 50000,
        uncompressed_size: 65536,
        checksum: 0xDEAD_BEEF,
        sub_stream_offsets_size: 128,
    };
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), TileHeader::SIZE);
    let deserialized = TileHeader::from_bytes(&bytes);
    assert_eq!(header, deserialized);
}

#[test]
fn test_tile_header_stored_flag() {
    let header = TileHeader {
        version: 1,
        flags: TileFlags::default().with_stored(),
        sub_stream_count: 32,
        compressed_size: 65536,
        uncompressed_size: 65536,
        checksum: 0,
        sub_stream_offsets_size: 0,
    };
    assert!(header.flags.stored());
    assert!(!header.flags.last_tile());
}

#[test]
fn test_tile_header_last_tile_flag() {
    let header = TileHeader {
        version: 1,
        flags: TileFlags::default().with_last_tile(),
        sub_stream_count: 32,
        compressed_size: 1000,
        uncompressed_size: 2000,
        checksum: 0,
        sub_stream_offsets_size: 128,
    };
    assert!(header.flags.last_tile());
    assert!(!header.flags.stored());
}

// T011: TileIndexEntry and TileIndexHeader round-trip tests

#[test]
fn test_tile_index_entry_size() {
    assert_eq!(TileIndexEntry::SIZE, 24);
}

#[test]
fn test_tile_index_entry_roundtrip() {
    let entry = TileIndexEntry {
        tile_offset: 12_345_678,
        compressed_size: 50000,
        uncompressed_size: 65536,
        checksum: 0xCAFE_BABE,
        flags: 0,
    };
    let bytes = entry.to_bytes();
    assert_eq!(bytes.len(), TileIndexEntry::SIZE);
    let deserialized = TileIndexEntry::from_bytes(&bytes);
    assert_eq!(entry, deserialized);
}

#[test]
fn test_tile_index_header_size() {
    assert_eq!(TileIndexHeader::SIZE, 8);
}

#[test]
fn test_tile_index_header_roundtrip() {
    let header = TileIndexHeader {
        entry_count: 42,
        index_flags: 0,
    };
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), TileIndexHeader::SIZE);
    let deserialized = TileIndexHeader::from_bytes(&bytes);
    assert_eq!(header, deserialized);
}

// T012: GpuFileFooter round-trip tests

#[test]
fn test_gpu_file_footer_size() {
    assert_eq!(GpuFileFooter::SIZE, 24);
}

#[test]
#[allow(clippy::expect_used)]
fn test_gpu_file_footer_roundtrip() {
    let footer = GpuFileFooter::new(99999, 8 + 24 * 10);
    let bytes = footer.to_bytes();
    assert_eq!(bytes.len(), GpuFileFooter::SIZE);
    let deserialized = GpuFileFooter::from_bytes(&bytes).expect("valid footer should parse");
    assert_eq!(footer, deserialized);
}

#[test]
fn test_gpu_file_footer_rejects_invalid_magic() {
    let mut bytes = [0u8; GpuFileFooter::SIZE];
    bytes[20..24].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    let result = GpuFileFooter::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_gpu_file_footer_detects_corrupted_checksum() {
    let footer = GpuFileFooter::new(1000, 248);
    let mut bytes = footer.to_bytes();
    bytes[12] ^= 0xFF; // corrupt checksum
    let result = GpuFileFooter::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_gpu_file_footer_rejects_wrong_version() {
    let footer = GpuFileFooter::new(1000, 248);
    let mut bytes = footer.to_bytes();
    bytes[16..20].copy_from_slice(&9999u32.to_le_bytes());
    let result = GpuFileFooter::from_bytes(&bytes);
    assert!(result.is_err());
}

// T053: 128-byte alignment verification test

#[test]
#[allow(clippy::expect_used)]
fn test_tile_payloads_are_128_byte_aligned() {
    use crush_gpu::engine::{EngineConfig, compress, load_tile_index};
    use std::sync::atomic::AtomicBool;

    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    // Create data spanning 3 tiles
    let data: Vec<u8> = (0u32..65536 * 3).map(|i| (i % 251) as u8).collect();
    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");

    let index = load_tile_index(&compressed).expect("index should load");
    assert_eq!(index.tile_count(), 3);

    // Each tile offset + TileHeader::SIZE should leave the payload start aligned
    // OR the next tile offset should be 128-byte aligned
    for (i, entry) in index.entries.iter().enumerate() {
        let tile_start = usize::try_from(entry.tile_offset).expect("offset fits usize");
        // The tile data (header + payload + padding) should end at a 128-byte
        // aligned position, so the NEXT tile starts aligned.
        // The first tile starts at GpuFileHeader::SIZE (64) which after header
        // serialization is the start of tile data in the archive.
        if i > 0 {
            assert_eq!(
                tile_start % 128,
                0,
                "tile {i} offset {tile_start} is not 128-byte aligned"
            );
        }
    }
}

// T054: Tile index O(1) lookup test

#[test]
#[allow(clippy::expect_used)]
fn test_tile_index_o1_lookup() {
    use crush_gpu::engine::{EngineConfig, compress, load_tile_index};
    use std::sync::atomic::AtomicBool;

    let cancel = AtomicBool::new(false);
    let config = EngineConfig::default();

    let data: Vec<u8> = (0u32..65536 * 5).map(|i| (i % 251) as u8).collect();
    let compressed = compress(&data, &config, &cancel).expect("compression should succeed");

    let index = load_tile_index(&compressed).expect("index should load");
    assert_eq!(index.tile_count(), 5);

    // Verify each entry has valid offset and size
    for i in 0..5 {
        let entry = index.get(i).expect("entry should exist");
        assert!(
            entry.tile_offset > 0,
            "tile {i} should have non-zero offset"
        );
        assert!(
            entry.uncompressed_size > 0,
            "tile {i} should have non-zero uncompressed size"
        );
    }

    // Verify out-of-bounds returns None
    assert!(index.get(5).is_none());
    assert!(index.get(100).is_none());
}
