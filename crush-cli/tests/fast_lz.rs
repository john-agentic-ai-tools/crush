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

fn xorshift(len: usize, mut s: u64) -> Vec<u8> {
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s.to_le_bytes()[0]
        })
        .collect()
}

/// Standard base64 of `data`, 76 columns per line.
fn base64(data: &[u8]) -> Vec<u8> {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    for (n, c) in data.chunks(3).enumerate() {
        let b = [
            c[0],
            c.get(1).copied().unwrap_or(0),
            c.get(2).copied().unwrap_or(0),
        ];
        let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            out.push(A[((v >> (18 - 6 * k)) & 63) as usize]);
        }
        if n % 19 == 18 {
            out.push(b'\n');
        }
    }
    out
}

/// Log-like text whose lines vary (timestamps, ids), 4 megabytes.
fn log_text() -> Vec<u8> {
    let mut out = Vec::with_capacity(4 << 20);
    let mut i = 0u64;
    while out.len() < 4 << 20 {
        out.extend_from_slice(
            format!(
                "2026-10-29 12:{:02}:{:02}.{:03} INFO request id={} path=/api/v1/items/{} status=200 ms={}\n",
                (i / 60) % 60,
                i % 60,
                (i * 7) % 1000,
                i * 2_654_435_761 % 1_000_000,
                i % 977,
                i % 89
            )
            .as_bytes(),
        );
        i += 1;
    }
    out.truncate(4 << 20);
    out
}

/// Compress `data` with `args` and return the plugin `inspect` reports.
fn plugin_used(dir: &std::path::Path, name: &str, data: &[u8], args: &[&str]) -> String {
    let input = create_test_file(dir, name, data);
    let packed = dir.join(format!("{name}.crush"));
    crush_cmd()
        .arg("compress")
        .args(args)
        .args(["--force"])
        .arg(&input)
        .assert()
        .success();
    let out = crush_cmd()
        .arg("inspect")
        .arg(&packed)
        .output()
        .expect("inspect");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Plugin: "))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("no plugin line in: {text}"))
}

/// FR-007, SC-004: `--level fast` uses fast-lz only where it compresses.
#[test]
fn level_fast_routes_by_content() {
    let dir = test_dir();
    let log = log_text();
    let b64 = base64(&xorshift(3 << 20, 11));
    let random = xorshift(4 << 20, 12);
    let fast = ["--level", "fast"];
    assert_eq!(plugin_used(dir.path(), "log.txt", &log, &fast), "fast-lz");
    assert_ne!(plugin_used(dir.path(), "b64.txt", &b64, &fast), "fast-lz");
    assert_ne!(
        plugin_used(dir.path(), "rand.bin", &random, &fast),
        "fast-lz"
    );
}

/// FR-010: balanced and best never use fast-lz and choose independently of
/// the content, as before this feature.
#[test]
fn other_levels_unchanged() {
    let dir = test_dir();
    let log = log_text();
    let random = xorshift(4 << 20, 13);
    for level in ["balanced", "best"] {
        let args = ["--level", level];
        let on_log = plugin_used(dir.path(), "log.txt", &log, &args);
        let on_random = plugin_used(dir.path(), "rand.bin", &random, &args);
        assert_ne!(on_log, "fast-lz", "{level}");
        assert_eq!(
            on_log, on_random,
            "{level}: choice must not depend on content"
        );
    }
}

/// FR-009: naming fast-lz uses it even for data it cannot compress.
#[test]
fn explicit_fast_lz_on_incompressible_data() {
    let dir = test_dir();
    let b64 = base64(&xorshift(1 << 20, 14));
    let args = ["--plugin", "fast-lz"];
    assert_eq!(plugin_used(dir.path(), "b64.txt", &b64, &args), "fast-lz");
}
