//! Byte identity of compressed output across the single-core pipeline work
//! of feature 023 (US3, T037): the library output for each plugin matches
//! golden hashes captured before that work, and the CLI's file output equals
//! the library output for the same plugin and file metadata.
#![allow(clippy::expect_used, clippy::cast_possible_truncation)]

mod common;

use common::*;
use crush_core::plugin::FileMetadata;
use crush_core::{CompressionOptions, compress_with_options, init_plugins};
use filetime::FileTime;

// Link the plugins the CLI registers.
use crush_gpu as _;
use crush_lz as _;
use crush_parallel as _;

const PLUGINS: [&str; 3] = ["fast-lz", "deflate", "parallel-deflate"];
/// One byte, one below the CLI's parallel threshold (25 MB), and 30 MB.
const SIZES: [usize; 3] = [1, 25 * 1024 * 1024 - 1, 30 * 1024 * 1024];
const MTIME: i64 = 1_790_000_000;

/// Deterministic mixed data: text-like runs and pseudo-random stretches.
fn data(len: usize) -> Vec<u8> {
    let words: [&[u8]; 6] = [
        b"block ",
        b"offset ",
        b"match\n",
        b"crush ",
        b"0x1f ",
        b"literal ",
    ];
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        if s.is_multiple_of(5) {
            out.extend_from_slice(&s.to_le_bytes());
        } else {
            out.extend_from_slice(words[(s % 6) as usize]);
        }
    }
    out.truncate(len);
    out
}

/// FNV-1a, 64 bits.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn library(plugin: &str, input: &[u8], meta: Option<FileMetadata>) -> Vec<u8> {
    init_plugins().expect("plugins");
    let mut options = CompressionOptions::default().with_plugin(plugin);
    if let Some(m) = meta {
        options = options.with_file_metadata(m);
    }
    compress_with_options(input, &options).expect("compress")
}

/// (plugin, size) -> (hash, length) of the library output without metadata,
/// captured from the code before the US3 pipeline changes.
const GOLDEN: [(&str, usize, u64, usize); 9] = [
    ("fast-lz", SIZES[0], 0x0fb2_a439_9053_ff51, 41),
    ("fast-lz", SIZES[1], 0x1e19_e770_6fdb_7a6a, 14_036_356),
    ("fast-lz", SIZES[2], 0x69d5_5c1e_b1e4_68fe, 16_842_704),
    ("deflate", SIZES[0], 0x26e1_e6c4_e0d4_b03e, 23),
    ("deflate", SIZES[1], 0xd40e_c9e7_6b5d_4210, 9_467_794),
    ("deflate", SIZES[2], 0x54b2_7469_b9b8_4d06, 11_360_820),
    ("parallel-deflate", SIZES[0], 0xfd2b_4c6c_b591_4ece, 153),
    (
        "parallel-deflate",
        SIZES[1],
        0x79f5_efc0_702f_9ab6,
        9_689_288,
    ),
    (
        "parallel-deflate",
        SIZES[2],
        0x0aad_2149_14a2_2ee4,
        11_626_299,
    ),
];

#[test]
fn library_output_matches_golden() {
    for (plugin, size, hash, len) in GOLDEN {
        let out = library(plugin, &data(size), None);
        assert_eq!(
            (fnv(&out), out.len()),
            (hash, len),
            "{plugin}, {size} bytes"
        );
    }
}

/// Prints the table for `GOLDEN` (run once, before changing the pipeline).
#[test]
#[ignore = "capture: prints the GOLDEN table"]
fn capture_golden() {
    for plugin in PLUGINS {
        for (k, size) in SIZES.into_iter().enumerate() {
            let out = library(plugin, &data(size), None);
            println!(
                "    (\"{plugin}\", SIZES[{k}], 0x{:016x}, {}),",
                fnv(&out),
                out.len()
            );
        }
    }
}

#[test]
fn cli_file_output_equals_library_output() {
    let dir = test_dir();
    for size in SIZES {
        let content = data(size);
        let input = create_test_file(dir.path(), "input.bin", &content);
        filetime::set_file_mtime(&input, FileTime::from_unix_time(MTIME, 0)).expect("mtime");
        let meta = FileMetadata {
            mtime: Some(MTIME),
            #[cfg(unix)]
            permissions: {
                use std::os::unix::fs::PermissionsExt;
                Some(
                    std::fs::metadata(&input)
                        .expect("stat")
                        .permissions()
                        .mode(),
                )
            },
        };
        for plugin in PLUGINS {
            let packed = dir.path().join("input.bin.crush");
            crush_cmd()
                .args(["compress", "--force", "--plugin", plugin])
                .arg(&input)
                .assert()
                .success();
            let cli = read_file(&packed);
            let lib = library(plugin, &content, Some(meta.clone()));
            assert!(cli == lib, "{plugin}, {size} bytes: CLI output differs");
        }
    }
}
