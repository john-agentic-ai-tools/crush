# Testing Guide

This document describes the testing infrastructure for the Crush compression library.

## Running Tests

### Quick Test

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p crush-core
cargo test -p crush-cli

# Run a specific test
cargo test test_config_set_and_get
```

### With Coverage

```bash
# Install cargo-llvm-cov if not already installed
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --html
```

## Test Isolation

### Config Tests

The config tests in `crush-cli/tests/config.rs` use **isolated temporary config files** to prevent race conditions when running tests in parallel.

**How it works:**
- Each test creates a unique temporary config file via `setup_test_config()`
- The config file path is passed to the CLI via the `CRUSH_TEST_CONFIG_FILE` environment variable
- Tests can run in parallel without interfering with each other
- No cleanup needed - temporary directories are automatically cleaned up

**Implementation:**
```rust
let (_temp_dir, config_path) = setup_test_config();
crush_cmd_with_config(&config_path)
    .arg("config")
    .arg("set")
    .arg("compression.level")
    .arg("fast")
    .assert()
    .success();
```

### Environment Variables for Testing

The following environment variables can be used to customize test behavior:

- `CRUSH_TEST_CONFIG_FILE`: Override config file path for testing (used internally by config tests)
- `CARGO_TERM_COLOR`: Control color output (`always`, `never`, `auto`)
- `RUST_BACKTRACE`: Enable backtraces on test failures (`1` or `full`)

## CI Pipeline

The CI pipeline (`.github/workflows/ci.yml`) runs tests on multiple platforms:

- **Ubuntu, Windows, macOS** on stable and beta Rust
- **Parallel execution enabled** - all tests are safe to run concurrently
- **Coverage threshold**: 80% minimum
- **Fuzz testing**: 100k iterations minimum for compress/decompress

### CI Configuration

The CI uses `cargo-nextest` for faster test execution:

```bash
cargo nextest run --all-features --profile ci
```

No special configuration needed for parallel execution - tests are designed to be isolation-safe.

## Test Structure

```
crush/
├── crush-core/
│   ├── tests/          # Integration tests
│   └── fuzz/           # Fuzz targets
└── crush-cli/
    └── tests/
        ├── common/     # Shared test utilities
        ├── compress.rs # Compress command tests
        ├── decompress.rs # Decompress command tests
        ├── config.rs   # Config command tests (isolated)
        ├── inspect.rs  # Inspect command tests
        ├── plugins.rs  # Plugin management tests
        ├── help.rs     # Help system tests
        ├── logging.rs  # Logging tests
        ├── pipeline.rs # Pipeline integration tests
        └── roundtrip.rs # End-to-end tests
```

## Test Categories

### Unit Tests
Located within `src/` modules using `#[cfg(test)]` blocks. Test individual functions and modules in isolation.

### Integration Tests
Located in `tests/` directories. Test complete workflows and command-line operations.

### Fuzz Tests
Located in `crush-core/fuzz/`. Test robustness against malformed inputs:
- `compress` - Test compression with random input
- `decompress` - Test decompression with corrupted data

## Writing New Tests

### CLI Integration Tests

Use the common test utilities:

```rust
mod common;
use common::*;

#[test]
fn test_my_feature() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"data");

    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    let output = dir.path().join("test.txt.crush");
    assert_file_exists(&output);
}
```

### Config Tests (Isolated)

For tests that modify configuration:

```rust
#[test]
fn test_my_config_feature() {
    let (_temp_dir, config_path) = setup_test_config();

    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("set")
        .arg("key")
        .arg("value")
        .assert()
        .success();
}
```

## Troubleshooting

### Race Conditions

If you see intermittent test failures:
1. Check if tests are modifying shared state (config files, environment variables)
2. Use isolated resources (temporary files, unique config paths)
3. Consider using test fixtures that cleanup properly

### Windows Path Issues

On Windows, use forward slashes or proper path joining:
```rust
// Good
let path = dir.join("file.txt");

// Bad
let path = format!("{}/file.txt", dir.display());
```

### CI Failures

If tests pass locally but fail in CI:
1. Check for platform-specific issues (line endings, path separators)
2. Verify tests don't depend on specific directory structures
3. Ensure tests don't require network access or external dependencies

## Performance Testing

For performance-sensitive changes, run benchmarks:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench compress_throughput
```

Benchmarks are located in `benches/` directories.
