//! `sprint` through the CLI, per `specs/026-hybrid-speed-codec/contracts/cli.md`.
mod common;

use common::*;
use predicates::prelude::*;

const BLOCK: usize = 4 << 20;

fn data(len: usize) -> Vec<u8> {
    let mut x = 0x0123_4567_89AB_CDEF_u64;
    let words: [&[u8]; 6] = [
        b"crush ",
        b"sprint ",
        b"block ",
        b"segment ",
        b"tile ",
        b"\n",
    ];
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(words[(x % 6) as usize]);
    }
    out.truncate(len);
    out
}

fn compressed(dir: &std::path::Path, input: &[u8]) -> std::path::PathBuf {
    let path = create_test_file(dir, "input.bin", input);
    let packed = dir.join("input.bin.crush");
    crush_cmd()
        .args(["compress", "--plugin", "sprint"])
        .arg(&path)
        .assert()
        .success();
    packed
}

#[test]
fn plugins_list_shows_sprint() {
    crush_cmd()
        .args(["plugins", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sprint"));
}

#[test]
fn file_round_trip_detects_the_plugin_from_the_file() {
    let dir = test_dir();
    let input = data(BLOCK + 12_345);
    let packed = compressed(dir.path(), &input);
    let restored = dir.path().join("restored.bin");
    crush_cmd()
        .arg("decompress")
        .arg(&packed)
        .arg("-o")
        .arg(&restored)
        .assert()
        .success();
    assert!(read_file(&restored) == input);
}

#[test]
fn inspect_reports_plugin_sizes_and_blocks() {
    let dir = test_dir();
    let input = data(2 * BLOCK + 1);
    let packed = compressed(dir.path(), &input);
    crush_cmd()
        .arg("inspect")
        .arg(&packed)
        .assert()
        .success()
        .stdout(predicate::str::contains("Plugin: sprint"))
        .stdout(predicate::str::contains("Blocks: 3"));
}

#[test]
fn piped_round_trip() {
    let input = data(BLOCK / 2);
    let packed = crush_cmd()
        .args(["compress", "--plugin", "sprint", "--stdout"])
        .write_stdin(input.clone())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(packed.len() < input.len());
    let restored = crush_cmd()
        .args(["decompress", "--stdout"])
        .write_stdin(packed)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(restored == input);
}

#[test]
fn block_flag_returns_one_block() {
    let dir = test_dir();
    let input = data(3 * BLOCK);
    let packed = compressed(dir.path(), &input);
    let output = dir.path().join("block.bin");
    crush_cmd()
        .args(["decompress", "--block", "1"])
        .arg(&packed)
        .arg("-o")
        .arg(&output)
        .assert()
        .success();
    assert!(read_file(&output) == input[BLOCK..2 * BLOCK]);
}

#[test]
fn block_flag_out_of_range_names_the_valid_range() {
    let dir = test_dir();
    let packed = compressed(dir.path(), &data(BLOCK + 1));
    let output = dir.path().join("block.bin");
    crush_cmd()
        .args(["decompress", "--block", "9"])
        .arg(&packed)
        .arg("-o")
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("2 entries"));
    assert!(!output.exists());
}

#[test]
fn corrupt_file_fails_and_leaves_no_output() {
    let dir = test_dir();
    let packed = compressed(dir.path(), &data(BLOCK / 2));
    let mut bytes = read_file(&packed);
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    std::fs::write(&packed, bytes).expect("rewrite");
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

/// The first block payload follows the 64-byte CRSH header and a 16-byte
/// block header. The outer CRC is recomputed, as a real newer file's would be.
#[test]
fn newer_sprint_version_is_an_unsupported_version_error() {
    let dir = test_dir();
    let packed = compressed(dir.path(), &data(BLOCK / 2));
    let mut bytes = read_file(&packed);
    let crsh = bytes
        .windows(4)
        .position(|w| w == b"CRSH")
        .expect("CRSH header");
    bytes[crsh + 64 + 16] = 0xEE;
    let crc = crc32fast::hash(&bytes[crush_core::CrushHeader::SIZE + 4..]);
    let header = crush_core::CrushHeader::SIZE;
    bytes[header..header + 4].copy_from_slice(&crc.to_le_bytes());
    std::fs::write(&packed, bytes).expect("rewrite");
    let output = dir.path().join("out.bin");
    crush_cmd()
        .arg("decompress")
        .arg(&packed)
        .arg("-o")
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("ersion"));
    assert!(!output.exists());
}

#[test]
fn no_plugin_named_does_not_pick_sprint() {
    let dir = test_dir();
    let path = create_test_file(dir.path(), "plain.bin", &data(BLOCK));
    crush_cmd().arg("compress").arg(&path).assert().success();
    let bytes = read_file(&dir.path().join("plain.bin.crush"));
    assert_ne!(bytes[..4], [0x43, 0x52, 0x01, 0x05]);
}

/// Ctrl-C leaves no partial output. The run may finish before the signal
/// lands on a fast machine, and then its output must be whole.
#[cfg(unix)]
#[test]
fn interrupt_leaves_no_partial_output() {
    use std::process::{Command, Stdio};

    let dir = test_dir();
    let input = data(256 << 20);
    let path = create_test_file(dir.path(), "large.bin", &input);
    let output = dir.path().join("large.crush");

    let mut child = Command::new(assert_cmd::cargo::cargo_bin!("crush"))
        .args(["compress", "--plugin", "sprint"])
        .arg(&path)
        .arg("-o")
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    std::thread::sleep(std::time::Duration::from_millis(100));
    Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill");
    let status = child.wait().expect("wait");

    if status.success() {
        let restored = dir.path().join("restored.bin");
        crush_cmd()
            .arg("decompress")
            .arg(&output)
            .arg("-o")
            .arg(&restored)
            .assert()
            .success();
        assert!(read_file(&restored) == input);
    } else {
        assert!(!output.exists(), "partial output left behind");
    }
}
