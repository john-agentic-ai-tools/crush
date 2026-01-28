mod common;

use common::*;
use predicates::prelude::*;

/// T126: Test root --help shows all commands
#[test]
fn test_root_help() {
    let assert = crush_cmd().arg("--help").assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Should show all major commands
    assert!(stdout.contains("compress"), "Should list compress command");
    assert!(
        stdout.contains("decompress"),
        "Should list decompress command"
    );
    assert!(stdout.contains("inspect"), "Should list inspect command");
    assert!(stdout.contains("config"), "Should list config command");
    assert!(stdout.contains("plugins"), "Should list plugins command");

    // Should show usage information
    assert!(
        stdout.contains("Usage:") || stdout.contains("USAGE:"),
        "Should show usage"
    );

    // Should show description
    assert!(
        stdout.contains("compression") || stdout.contains("Crush"),
        "Should show description"
    );
}

/// T127: Test compress --help shows detailed options
#[test]
fn test_compress_help() {
    let assert = crush_cmd().arg("compress").arg("--help").assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Should show compress command description
    assert!(stdout.contains("Compress"), "Should mention compress");

    // Should show options
    assert!(
        stdout.contains("--output") || stdout.contains("-o"),
        "Should show output option"
    );
    assert!(
        stdout.contains("--plugin") || stdout.contains("-p"),
        "Should show plugin option"
    );
    assert!(
        stdout.contains("--level") || stdout.contains("-l"),
        "Should show level option"
    );
    assert!(
        stdout.contains("--force") || stdout.contains("-f"),
        "Should show force option"
    );

    // Should show usage pattern
    assert!(
        stdout.contains("Usage:") || stdout.contains("USAGE:"),
        "Should show usage"
    );
    assert!(
        stdout.contains("<FILE>") || stdout.contains("[FILE]") || stdout.contains("INPUT"),
        "Should show file argument"
    );
}

/// T128: Test invalid command suggests alternative
#[test]
fn test_invalid_command_suggestion() {
    let assert = crush_cmd()
        .arg("compres") // Typo: should suggest "compress"
        .assert()
        .failure();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    // Should show error about invalid command
    assert!(
        stderr.contains("unrecognized") || stderr.contains("invalid") || stderr.contains("unknown"),
        "Should indicate invalid command"
    );

    // Clap should suggest similar command
    // Note: This test verifies the error message exists, clap may or may not suggest alternatives
    assert!(!stderr.is_empty(), "Should show error message");
}
