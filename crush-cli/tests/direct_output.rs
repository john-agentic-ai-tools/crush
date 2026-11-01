//! Whole-file output through the mapped path, and `--block` on real files.

mod common;

use common::*;
use std::fs;

const BLOCK: usize = 1024 * 1024;

fn data() -> Vec<u8> {
    (0..3 * BLOCK + 1234)
        .map(|i| ((i / 97) ^ (i * 31 % 251)) as u8)
        .collect()
}

fn compressed(dir: &std::path::Path, input: &[u8]) -> std::path::PathBuf {
    let source = create_test_file(dir, "in.bin", input);
    let packed = dir.join("in.bin.crush");
    crush_cmd()
        .args(["compress", "--plugin", "parallel-deflate"])
        .arg(&source)
        .arg("-o")
        .arg(&packed)
        .assert()
        .success();
    packed
}

#[test]
fn a_large_file_round_trips() {
    let dir = test_dir();
    let input = data();
    let packed = compressed(dir.path(), &input);
    let output = dir.path().join("out.bin");
    crush_cmd()
        .arg("decompress")
        .arg(&packed)
        .arg("-o")
        .arg(&output)
        .assert()
        .success();
    assert_eq!(read_file(&output), input);
}

#[test]
fn a_corrupt_file_leaves_no_output() {
    let dir = test_dir();
    let packed = compressed(dir.path(), &data());
    let mut bytes = read_file(&packed);
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    fs::write(&packed, bytes).expect("write");
    let output = dir.path().join("out.bin");
    crush_cmd()
        .arg("decompress")
        .arg(&packed)
        .arg("-o")
        .arg(&output)
        .assert()
        .failure();
    assert!(!output.exists());
}

#[test]
fn block_flag_returns_one_block_of_a_crush_file() {
    let dir = test_dir();
    let input = data();
    let packed = compressed(dir.path(), &input);
    let output = dir.path().join("block.bin");
    crush_cmd()
        .args(["decompress", "--block", "2"])
        .arg(&packed)
        .arg("-o")
        .arg(&output)
        .assert()
        .success();
    assert_eq!(read_file(&output), input[2 * BLOCK..3 * BLOCK]);
}

#[test]
fn block_flag_rejects_an_index_past_the_end() {
    let dir = test_dir();
    let packed = compressed(dir.path(), &data());
    crush_cmd()
        .args(["decompress", "--block", "99"])
        .arg(&packed)
        .arg("-o")
        .arg(dir.path().join("block.bin"))
        .assert()
        .failure();
}
