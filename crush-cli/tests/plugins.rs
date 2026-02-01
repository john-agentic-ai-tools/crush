mod common;

use common::*;
use predicates::prelude::*;

/// T113: Test plugins list shows all registered plugins
#[test]
fn test_plugins_list() -> Result<(), Box<dyn std::error::Error>> {
    let assert = crush_cmd().arg("plugins").arg("list").assert().success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone())?;

    // Should show at least the deflate plugin (default)
    assert!(stdout.contains("deflate"), "Should list deflate plugin");

    // Should show metadata columns
    assert!(
        stdout.contains("Name") || stdout.contains("name"),
        "Should show plugin names"
    );
    assert!(
        stdout.contains("Version") || stdout.contains("version"),
        "Should show version info"
    );
    assert!(
        stdout.contains("Throughput") || stdout.contains("throughput") || stdout.contains("MB/s"),
        "Should show throughput"
    );

    Ok(())
}

/// T114: Test plugins list with JSON format
#[test]
fn test_plugins_list_json() -> Result<(), Box<dyn std::error::Error>> {
    let assert = crush_cmd()
        .arg("plugins")
        .arg("list")
        .arg("--format")
        .arg("json")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone())?;

    // Should be valid JSON
    assert!(
        stdout.contains("{") && stdout.contains("}"),
        "Should output JSON"
    );

    // Should contain plugin data
    assert!(
        stdout.contains("deflate"),
        "Should include deflate plugin in JSON"
    );
    assert!(
        stdout.contains("name") || stdout.contains("\"name\""),
        "Should have name field"
    );

    Ok(())
}

/// T115: Test plugins info shows detailed information
#[test]
fn test_plugins_info() -> Result<(), Box<dyn std::error::Error>> {
    let assert = crush_cmd()
        .arg("plugins")
        .arg("info")
        .arg("deflate")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone())?;

    // Should show detailed information
    assert!(stdout.contains("deflate"), "Should show plugin name");
    assert!(
        stdout.contains("Version") || stdout.contains("version"),
        "Should show version"
    );
    assert!(
        stdout.contains("Throughput") || stdout.contains("throughput"),
        "Should show throughput"
    );
    assert!(
        stdout.contains("Compression") || stdout.contains("ratio"),
        "Should show compression ratio"
    );
    assert!(
        stdout.contains("Description") || stdout.contains("description"),
        "Should show description"
    );

    Ok(())
}

/// T116: Test plugin not found error
#[test]
fn test_plugins_info_not_found() {
    crush_cmd()
        .arg("plugins")
        .arg("info")
        .arg("nonexistent_plugin")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("unknown"))
                .or(predicate::str::contains("Plugin")),
        );
}

/// T117: Test plugin self-test roundtrip validation
#[test]
fn test_plugins_test() -> Result<(), Box<dyn std::error::Error>> {
    let assert = crush_cmd()
        .arg("plugins")
        .arg("test")
        .arg("deflate")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone())?;

    // Should show test results
    assert!(
        stdout.contains("PASS") || stdout.contains("passed") || stdout.contains("success"),
        "Should show test passed"
    );
    assert!(stdout.contains("deflate"), "Should mention plugin name");

    Ok(())
}
