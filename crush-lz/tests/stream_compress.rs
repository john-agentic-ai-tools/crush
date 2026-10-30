//! Streaming compression through crush-core (feature 023, US3): the bytes
//! `compress_to_writer` writes equal `compress_with_options`'s result, with
//! and without file metadata, and they decompress back.
#![allow(clippy::expect_used)]
// Plain `assert!(a == b)`: `assert_eq!` would print megabytes on failure.
#![allow(clippy::manual_assert_eq)]

use crush_core::plugin::FileMetadata;
use crush_core::{
    CompressionOptions, compress_to_writer, compress_with_options, decompress, init_plugins,
};
use std::io::{Cursor, Seek, SeekFrom};

// Link the plugin so it registers with crush-core.
use crush_lz as _;

#[path = "../../crush-parallel/benches/common/variants.rs"]
mod variants;

fn options(meta: bool) -> CompressionOptions {
    let o = CompressionOptions::default().with_plugin("fast-lz");
    if meta {
        o.with_file_metadata(FileMetadata {
            mtime: Some(1_790_000_000),
            #[cfg(unix)]
            permissions: Some(0o600),
        })
    } else {
        o
    }
}

#[test]
fn streamed_output_equals_in_memory_output() {
    init_plugins().expect("plugins");
    for len in [1usize, 100_000, (3 << 20) + 17] {
        let data = variants::generate_corpus(len, 7);
        for meta in [false, true] {
            let opts = options(meta);
            let whole = compress_with_options(&data, &opts).expect("compress");
            let mut sink = Cursor::new(Vec::new());
            let n = compress_to_writer(&data, &opts, &mut sink).expect("stream");
            assert_eq!(n, whole.len() as u64, "{len} bytes, metadata {meta}");
            assert!(sink.get_ref() == &whole, "{len} bytes, metadata {meta}");
            assert_eq!(decompress(sink.get_ref()).expect("decompress").data, data);
        }
    }
}

/// The CRC is patched relative to where the writer started, and the writer
/// is left positioned after the output.
#[test]
fn streams_after_existing_bytes() {
    init_plugins().expect("plugins");
    let data = variants::generate_corpus(2 << 20, 9);
    let whole = compress_with_options(&data, &options(true)).expect("compress");
    let mut sink = Cursor::new(b"existing".to_vec());
    sink.seek(SeekFrom::End(0)).expect("seek");
    compress_to_writer(&data, &options(true), &mut sink).expect("stream");
    assert_eq!(sink.position(), (8 + whole.len()) as u64);
    assert_eq!(&sink.get_ref()[..8], b"existing");
    assert!(sink.get_ref()[8..] == whole[..]);
}
