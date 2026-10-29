//! `fast-lz` through the CLI (feature 023, User Story 1).
mod common;

use common::*;
use predicates::prelude::*;

fn text(len: usize) -> Vec<u8> {
    b"2026-10-29 12:00:01 INFO request served path=/api/v1/items status=200 ms=12\n"
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

/// FR-001: the plugin is listed.
#[test]
fn plugins_list_shows_fast_lz() {
    crush_cmd()
        .args(["plugins", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fast-lz"));
}

/// FR-002/FR-003/FR-011: explicit compression, decompression without naming
/// the plugin, and inspection.
#[test]
fn explicit_fast_lz_round_trips_and_inspects() {
    let dir = test_dir();
    let data = text(3 << 20);
    let input = create_test_file(dir.path(), "log.txt", &data);
    let packed = dir.path().join("log.txt.crush");
    let restored = dir.path().join("restored.txt");

    crush_cmd()
        .args(["compress", "--plugin", "fast-lz"])
        .arg(&input)
        .assert()
        .success();
    crush_cmd()
        .arg("inspect")
        .arg(&packed)
        .assert()
        .success()
        .stdout(predicate::str::contains("Plugin: fast-lz"));
    crush_cmd()
        .arg("decompress")
        .arg(&packed)
        .arg("-o")
        .arg(&restored)
        .assert()
        .success();
    assert_eq!(std::fs::read(&restored).expect("read restored"), data);
}

/// The CLI rejects empty input files for every plugin; `fast-lz` is no
/// exception (the library itself round-trips empty input, see crush-lz tests).
#[test]
fn empty_file_rejected_like_other_plugins() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "empty.bin", b"");
    for plugin in ["fast-lz", "deflate"] {
        crush_cmd()
            .args(["compress", "--force", "--plugin", plugin])
            .arg(&input)
            .assert()
            .failure()
            .stderr(predicate::str::contains("empty"));
    }
}

/// FR-005: corrupted fast-lz data is an error, not a crash or wrong output.
#[test]
fn corrupted_fast_lz_file_fails_cleanly() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "log.txt", &text(200_000));
    let packed = dir.path().join("log.txt.crush");
    crush_cmd()
        .args(["compress", "--plugin", "fast-lz"])
        .arg(&input)
        .assert()
        .success();

    let mut bytes = std::fs::read(&packed).expect("read packed");
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    std::fs::write(&packed, &bytes).expect("write corrupted");

    crush_cmd()
        .arg("decompress")
        .arg(&packed)
        .arg("-o")
        .arg(dir.path().join("out.txt"))
        .assert()
        .failure();
}
