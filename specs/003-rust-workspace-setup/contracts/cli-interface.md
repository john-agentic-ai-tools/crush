# Contract: CLI Executable Interface (crush-cli)

**Feature**: 003-rust-workspace-setup | **Created**: 2026-01-18
**Contract Type**: Executable Behavior
**Crate**: `crush-cli`
**Binary Name**: `crush`
**Stability**: Unstable (0.1.0 - placeholder implementation)

## Purpose

This contract defines the minimal executable behavior for the `crush` CLI binary. The initial implementation contains placeholder code sufficient to validate the build system, executable linking, and integration with the crush-core library. Future features will extend this with argument parsing, file processing, and compression operations.

## Executable Specification

### Binary Metadata

- **Executable Name**: `crush` (defined in `[[bin]]` section of Cargo.toml)
- **Entry Point**: `crush-cli/src/main.rs`
- **Crate Type**: Binary (`[[bin]]`)
- **Dependencies**: `crush-core` (workspace path dependency)

### Invocation Methods

```bash
# Method 1: Via cargo run (development)
cargo run --bin crush

# Method 2: Direct execution (after build)
./target/debug/crush          # Debug build
./target/release/crush        # Release build

# Method 3: Installed binary (future, after cargo install)
crush                         # From $PATH
```

---

## Behavioral Contract (Version 0.1.0)

### Command: `crush` (no arguments)

**Input**: No command-line arguments

**Output**: Informational message to stdout

**Exit Code**: `0` (success)

**Expected Behavior**:
```
$ cargo run --bin crush
Crush CLI - Version 0.1.0
High-performance parallel compression tool.

This is a placeholder implementation.
Future versions will support compression operations.

Library says: Hello from crush-core!
```

**Constraints**:
- Must not panic or crash
- Must print to stdout (not stderr)
- Must terminate cleanly with exit code 0
- Must demonstrate integration with crush-core library

---

## Implementation

### Complete main.rs

```rust
//! Crush CLI - Command-line interface for Crush compression library.
//!
//! This binary provides a user-facing command-line tool for compression
//! and decompression operations using the crush-core library.
//!
//! # Current Status
//!
//! **Version 0.1.0**: Placeholder implementation. This binary contains
//! minimal code to validate project structure and demonstrate library
//! integration. Actual CLI functionality (argument parsing, file I/O)
//! will be implemented in future features.
//!
//! # Usage
//!
//! ```bash
//! # Run via cargo
//! cargo run --bin crush
//!
//! # Run compiled binary
//! ./target/debug/crush
//! ```
//!
//! # Future Features
//!
//! Planned functionality:
//! - File compression: `crush compress input.txt output.gz`
//! - File decompression: `crush decompress input.gz output.txt`
//! - Streaming I/O: `cat file.txt | crush compress > file.gz`
//! - Multi-threading control: `crush compress -j 8 file.txt`
//! - Algorithm selection: `crush compress --algo deflate file.txt`
//! - Progress reporting: `crush compress --progress file.txt`

use crush_core;

fn main() {
    println!("Crush CLI - Version 0.1.0");
    println!("High-performance parallel compression tool.");
    println!();
    println!("This is a placeholder implementation.");
    println!("Future versions will support compression operations.");
    println!();
    println!("Library says: {}", crush_core::hello());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_does_not_panic() {
        // This test verifies that main() can be called without panicking.
        // We can't easily test stdout in unit tests, but we can ensure
        // the function executes successfully.
        main();
    }
}
```

---

## Validation Requirements

### Compilation

```bash
# Binary must compile successfully
cargo build -p crush-cli

# Expected: Binary at target/debug/crush
# Verify: ls -lh target/debug/crush (or dir on Windows)
```

### Execution

```bash
# Binary must execute without errors
cargo run --bin crush

# Expected: Output printed to console, exit code 0
# Verify: echo $? (should show 0)
```

### Integration Test

```bash
# Verify library integration
cargo run --bin crush | grep "Hello from crush-core"

# Expected: Match found (demonstrates crush-core dependency working)
```

### Release Build

```bash
# Binary must build in release mode
cargo build --bin crush --release

# Expected: Optimized binary at target/release/crush
# Verify size: ls -lh target/release/crush
```

### Cross-Platform Build

```bash
# Binary must build on all tier-1 platforms
cargo build --bin crush --target x86_64-unknown-linux-gnu
cargo build --bin crush --target x86_64-pc-windows-msvc
cargo build --bin crush --target x86_64-apple-darwin

# Expected: Successful compilation on CI (GitHub Actions matrix)
```

---

## Future Interface Design (Out of Scope)

This section outlines the planned CLI interface for reference. **NOT IMPLEMENTED** in this feature.

### Command Structure (Future)

```
crush <COMMAND> [OPTIONS] [FILES]

Commands:
  compress    Compress files
  decompress  Decompress files
  list        List archive contents
  test        Test archive integrity
  help        Print help information

Options:
  -j, --threads <NUM>    Number of threads (default: auto)
  -l, --level <LEVEL>    Compression level 1-9 (default: 6)
  -a, --algo <ALGO>      Algorithm: deflate, brotli, zstd
  -v, --verbose          Verbose output
  -q, --quiet            Suppress output
  -f, --force            Overwrite existing files
  --progress             Show progress bar
```

### Example Usage (Future)

```bash
# Compress a file
crush compress -l 9 file.txt -o file.txt.gz

# Decompress a file
crush decompress file.txt.gz -o file.txt

# Compress with specific algorithm
crush compress --algo brotli file.txt

# Multi-threaded compression
crush compress -j 8 large-file.bin

# Stream compression
cat file.txt | crush compress > file.txt.gz

# Batch compression
crush compress *.txt
```

### Argument Parsing (Future)

Implementation will use `clap` (constitution-approved):

```rust
use clap::Parser;

#[derive(Parser)]
#[command(name = "crush")]
#[command(about = "High-performance parallel compression tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Compress {
        #[arg(short = 'j', long, default_value = "0")]
        threads: usize,

        #[arg(short = 'l', long, default_value = "6")]
        level: u8,

        files: Vec<PathBuf>,
    },
    Decompress {
        files: Vec<PathBuf>,
    },
}
```

### Error Handling (Future)

```rust
use crush_core::{CrushError, Result};
use std::process;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    // Application logic
    Ok(())
}
```

### Exit Codes (Future)

- `0` - Success
- `1` - Generic error
- `2` - File not found
- `3` - Compression failed
- `4` - Decompression failed
- `5` - Invalid arguments

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_does_not_panic() {
        main();  // Should not panic
    }
}
```

**Limitation**: Unit tests cannot easily verify stdout output. Integration tests required for full validation.

### Integration Tests (Future)

```rust
// tests/cli_tests.rs
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_cli_runs_successfully() {
    let mut cmd = Command::cargo_bin("crush").unwrap();
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Crush CLI"));
}

#[test]
fn test_cli_shows_version() {
    let mut cmd = Command::cargo_bin("crush").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}
```

**Dependencies** (added when implementing):
```toml
[dev-dependencies]
assert_cmd = "2.0"
predicates = "3.0"
```

---

## Performance Expectations

### Startup Time (Placeholder Version)

- **Target**: < 10ms cold start (minimal initialization)
- **Measurement**: `time cargo run --bin crush`
- **Baseline**: Empty main() function on standard hardware

### Binary Size (Placeholder Version)

- **Debug Build**: < 5 MB (includes debug symbols)
- **Release Build**: < 2 MB (stripped, optimized)
- **Future Target**: < 10 MB (with compression functionality)

### Memory Usage (Placeholder Version)

- **Baseline**: < 1 MB RSS (resident set size)
- **Future Target**: < 32 MB per thread during compression

---

## CI/CD Integration

GitHub Actions workflows (feature 002) will validate:

### Build Job

```yaml
- name: Build CLI
  run: cargo build --bin crush --release
```

### Test Job

```yaml
- name: Test CLI
  run: cargo test -p crush-cli
```

### Execution Smoke Test

```yaml
- name: Run CLI
  run: |
    cargo run --bin crush
    ./target/debug/crush
```

### Multi-Platform Matrix

```yaml
strategy:
  matrix:
    os: [ubuntu-latest, windows-latest, macos-latest]
steps:
  - name: Build CLI
    run: cargo build --bin crush
```

---

## Constitution Compliance

### Principle II: Correctness & Safety

- ✅ No unsafe code in placeholder implementation
- ✅ No `.unwrap()` or `.expect()` calls
- ✅ main() cannot panic (no error conditions exist)
- ✅ Clean exit with code 0

### Principle III: Modularity & Extensibility

- ✅ CLI separated from core library (thin wrapper pattern)
- ✅ All logic delegated to crush-core (no business logic in CLI)
- ✅ Future extensibility through command pattern

### Principle IV: Test-First Development

- ✅ Test present (verifies main() doesn't panic)
- ✅ Future TDD approach documented for CLI features

### Rust Style & Standards

- ✅ Module-level documentation (`//!` comments)
- ✅ Future usage examples documented
- ✅ Planned architecture outlined

---

## Dependencies

### Current Dependencies (Version 0.1.0)

```toml
[dependencies]
crush-core = { path = "../crush-core" }
```

### Future Dependencies (Post-0.1.0)

```toml
[dependencies]
crush-core = { path = "../crush-core" }
clap = { workspace = true, features = ["derive"] }

[dev-dependencies]
assert_cmd = "2.0"
predicates = "3.0"
```

All dependencies must be justified against constitution's allowed list.

---

## User Experience

### Developer Experience

```bash
# Simple build and run workflow
git clone https://github.com/your-org/crush
cd crush
cargo build
cargo run --bin crush

# Expected: Immediate success, clear output
```

### End-User Experience (Future)

```bash
# Install from crates.io
cargo install crush

# Use like standard Unix tool
crush compress file.txt
gzip -d file.txt.gz  # Interoperable output
```

---

## Success Criteria

A valid CLI implementation must:

1. ✅ Compile successfully (`cargo build -p crush-cli`)
2. ✅ Produce executable binary (`target/debug/crush`)
3. ✅ Execute without panic (`cargo run --bin crush`)
4. ✅ Exit with code 0 (success)
5. ✅ Print informational output to stdout
6. ✅ Demonstrate crush-core integration (call library function)
7. ✅ Pass all unit tests
8. ✅ Build on all tier-1 platforms (Linux, Windows, macOS)
9. ✅ Pass clippy lints (no warnings)
10. ✅ Follow rustfmt formatting

---

## Breaking Changes Policy

### Version 0.1.0 (This Feature)

- Placeholder implementation
- No CLI arguments supported
- Output format not stable
- Exit codes not defined

### Version 0.2.0+ (Future)

- Introduce clap-based argument parsing
- Define stable command structure
- Establish exit code conventions
- Follow semver for CLI interface changes

### Compatibility Strategy

- CLI interface follows semantic versioning
- Output format versioned separately (machine-readable output)
- Deprecation warnings for removed commands
- Migration guide for major version bumps

---

## References

- Cargo Book - Binaries: https://doc.rust-lang.org/cargo/reference/cargo-targets.html#binaries
- clap Documentation: https://docs.rs/clap/latest/clap/
- Command Line Interface Guidelines: https://clig.dev/
- Rust CLI Working Group: https://www.rust-lang.org/governance/wgs/cli
- Constitution: `.specify/memory/constitution.md` (dependency policy)
