# Research: CLI Implementation

**Feature**: CLI Implementation | **Branch**: `005-cli-implementation` | **Date**: 2026-01-22

## Phase 0: Technical Research

This document captures research findings, decisions, and rationales for technical choices in the CLI implementation.

## Dependency Selection

### Command-Line Argument Parsing: clap v4

**Decision**: Use `clap` v4 with derive API for argument parsing.

**Rationale**:
- Already approved in project constitution
- Industry standard for Rust CLIs (used by cargo, ripgrep, bat, fd)
- Derive API provides type-safe argument definitions
- Built-in help generation with excellent formatting
- Supports subcommands, flags, options, and positional arguments
- Minimal runtime overhead (startup time <5ms)
- Strong validation and error messages out of the box

**Alternatives Considered**:
- `structopt`: Deprecated, merged into clap v3+
- `argh`: Lighter weight but less feature-complete (no "did you mean" suggestions)
- Manual parsing: Error-prone, no help generation, violates DRY principle

**References**:
- clap v4 docs: https://docs.rs/clap/latest/clap/
- Performance benchmarks: clap startup overhead <5ms for typical CLIs

---

### Progress Bar Rendering: indicatif

**Decision**: Use `indicatif` v0.17+ for progress bars and spinners.

**Rationale**:
- De facto standard for Rust CLI progress bars
- Supports multi-bar layouts (useful for batch operations)
- Automatic terminal width detection and resizing
- Graceful degradation in non-TTY environments
- Template-based customization for styling
- Built-in spinner styles for indeterminate operations
- Minimal performance overhead (renders at configurable Hz)

**Alternatives Considered**:
- `pbr`: Less actively maintained, fewer features
- Custom implementation: Reinventing the wheel, terminal compatibility issues
- No progress bars: Poor UX for large file operations

**Implementation Notes**:
- Use `ProgressBar::hidden()` for files <1MB to avoid flicker
- Update frequency: 10Hz (every 100ms) balances responsiveness and overhead
- Clear progress bar on completion to avoid terminal clutter

---

### Terminal Color Output: termcolor

**Decision**: Use `termcolor` for cross-platform ANSI color support.

**Rationale**:
- Cross-platform: handles Windows console API differences
- Respects `NO_COLOR` environment variable
- Automatic TTY detection (no colors when piped)
- Lightweight: minimal dependency tree
- Used by cargo and other rust-lang projects

**Alternatives Considered**:
- `colored`: Simpler API but less robust Windows support
- `ansi_term`: Deprecated, unmaintained
- `owo-colors`: Compile-time colors, but less mature

**Color Usage Strategy**:
- Errors: Red
- Warnings: Yellow
- Success: Green
- Info/Headers: Cyan
- File paths: Bold
- Metrics/Numbers: White (default)

---

### Configuration Management: serde + toml

**Decision**: Use `serde` + `toml` for configuration file handling.

**Rationale**:
- TOML is human-readable and easy to edit manually
- `serde` is the standard serialization framework in Rust
- Type-safe configuration with derive macros
- Minimal dependencies (both are widely used)
- Supports comments in config files

**Configuration File Location**:
- Linux/macOS: `~/.config/crush/config.toml`
- Windows: `%APPDATA%\Crush\config.toml`
- Use `dirs` crate for cross-platform path resolution

**Default Configuration**:
```toml
[compression]
default-plugin = "auto"  # Auto-select based on weights
level = "balanced"       # fast | balanced | best

[output]
progress-bars = true     # Show progress for long operations
color = "auto"           # auto | always | never
quiet = false

[logging]
format = "human"         # human | json
level = "info"           # error | warn | info | debug | trace
file = ""                # Empty = stderr only
```

**Alternatives Considered**:
- JSON: Less human-friendly, no comment support
- YAML: More complex, security concerns with anchors/aliases
- INI: Less structured, no nested sections

---

### Structured Logging: tracing

**Decision**: Use `tracing` + `tracing-subscriber` for structured logging.

**Rationale**:
- Designed for async-aware structured logging (future-proof)
- Supports multiple output formats (human-readable, JSON)
- Zero-cost when disabled (compile-time filtering)
- Spans provide hierarchical context for operations
- Industry adoption: used by tokio, hyper, axum ecosystems

**Logging Levels**:
- ERROR: Operation failures, invalid input
- WARN: Deprecated flags, potential issues
- INFO: Operation start/completion, summary statistics
- DEBUG: Detailed operation flow, decisions made
- TRACE: Low-level details (buffer sizes, plugin scoring)

**Verbose Mode Mapping**:
- Normal mode: INFO and above
- `--verbose` (`-v`): DEBUG and above
- `--verbose --verbose` (`-vv`): TRACE (all events)

**Production JSON Format**:
```json
{
  "timestamp": "2026-01-22T10:30:45.123Z",
  "level": "INFO",
  "target": "crush_cli::commands::compress",
  "fields": {
    "message": "Compression completed",
    "plugin": "deflate",
    "input_size": 104857600,
    "output_size": 52428800,
    "ratio": 50.0,
    "throughput_mbps": 523.5,
    "duration_ms": 200
  }
}
```

**Alternatives Considered**:
- `env_logger`: Simpler but less structured, no spans
- `slog`: Powerful but more complex API, less ecosystem adoption
- `log` + custom formatter: Reinventing the wheel

---

### Terminal Detection: atty (is-terminal)

**Decision**: Use `is-terminal` crate (modern replacement for `atty`).

**Rationale**:
- `atty` is deprecated, `is-terminal` is the successor
- Detects if stdout/stderr are connected to a terminal
- Enables graceful degradation (no colors/progress in pipes)
- Tiny crate, single purpose, no dependencies
- Follows Rust API guidelines

**Usage**:
```rust
use is_terminal::IsTerminal;

let use_colors = std::io::stdout().is_terminal();
let show_progress = std::io::stderr().is_terminal();
let is_pipe_input = !std::io::stdin().is_terminal();
```

---

## CLI Architecture Decisions

### Command Structure

**Decision**: Use clap subcommands with a hierarchy:

```
crush
├── compress [FILES]...
├── decompress [FILES]...
├── inspect [FILES]...
├── config
│   ├── set <KEY> <VALUE>
│   ├── get <KEY>
│   ├── list
│   └── reset
└── plugins
    ├── list
    ├── info <PLUGIN>
    └── test <PLUGIN>
```

**Rationale**:
- Follows Git/Cargo convention (familiar to developers)
- Subcommands group related functionality
- Scalable: easy to add new commands without flag conflicts
- Self-documenting: `crush help compress` provides targeted help

---

### Error Handling Strategy

**Decision**: Wrap crush-core errors with CLI-specific context and user-friendly messages.

**Error Translation**:
- `CrushError::PluginNotFound` → "Compression plugin 'xyz' not found. Run 'crush plugins list' to see available plugins."
- `CrushError::ValidationError` → "File corrupted: CRC32 checksum mismatch. The compressed file may be damaged."
- `CrushError::IoError` → "Failed to read file: {path}. Reason: {error}"

**Exit Codes** (following Unix conventions):
- 0: Success
- 1: General error (validation, corruption)
- 2: Invalid usage (bad arguments, missing files)
- 130: Interrupted by Ctrl+C (128 + SIGINT)

**Implementation**:
```rust
pub enum CliError {
    Core(CrushError),           // Wrap crush-core errors
    Config(String),             // Configuration errors
    Io(std::io::Error),         // I/O errors
    Interrupted,                // Ctrl+C signal
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CliError::Core(e) => write!(f, "{}", user_friendly_message(e)),
            CliError::Config(msg) => write!(f, "Configuration error: {}", msg),
            // ... other variants
        }
    }
}
```

---

### Signal Handling (Ctrl+C)

**Decision**: Use `ctrlc` crate for graceful shutdown.

**Rationale**:
- Cross-platform signal handling (Windows + Unix)
- Simple API: just register a handler
- Allows cleanup of partial output files before exit

**Implementation Strategy**:
1. Register Ctrl+C handler that sets atomic flag
2. Check flag periodically during compression (every block)
3. On interrupt: close file handles, delete partial outputs, exit 130

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

let interrupted = Arc::new(AtomicBool::new(false));
let r = interrupted.clone();

ctrlc::set_handler(move || {
    r.store(true, Ordering::SeqCst);
})?;

// In compression loop:
if interrupted.load(Ordering::SeqCst) {
    cleanup_partial_files()?;
    return Err(CliError::Interrupted);
}
```

---

### Batch Operation Strategy

**Decision**: Process files sequentially with consolidated error reporting.

**Rationale**:
- Sequential processing is simpler and matches user expectations
- Parallel file processing requires complex state management
- Most bottleneck is I/O, not CPU (parallel wouldn't help much)
- Allows single progress bar tracking total progress across files
- Easier error recovery and cleanup

**Error Handling**:
- Continue processing remaining files after individual failures
- Report all errors at the end with file names
- Exit code 1 if any failures occurred

**Future Enhancement** (out of scope):
- Parallel batch processing with `rayon` for CPU-bound workloads
- Requires: thread-safe progress tracking, atomic error collection

---

## Performance Optimization

### Startup Time Target: <50ms

**Strategy**:
- Lazy plugin initialization: only call `init_plugins()` when needed
- Avoid heavy allocations in main() before command dispatch
- Use `clap`'s derive API (faster than builder at runtime)
- Minimize dependency count to reduce link time

**Measurement**:
```bash
hyperfine --warmup 3 'crush --version'
```

---

### Progress Bar Update Frequency

**Decision**: Update progress every 100ms (10Hz).

**Rationale**:
- Human perception: updates faster than 10Hz appear smooth
- Terminal rendering overhead: ~1ms per update
- Total overhead: ~1% (1ms / 100ms)
- Balances responsiveness with performance

**Implementation**:
```rust
let progress = ProgressBar::new(total_bytes);
progress.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
```

---

### Memory Usage

**Constraint**: <100MB base + streaming buffers

**Strategy**:
- Use crush-core's streaming API (no full-file buffering)
- Progress bars use minimal memory (<1KB per bar)
- Configuration files are small (<10KB)
- No caching of compressed data in memory

**Monitoring** (in verbose mode):
```rust
// Report peak memory usage at completion
println!("Peak memory: {} MB", peak_rss_mb);
```

---

## Testing Strategy

### Integration Tests with assert_cmd

**Decision**: Use `assert_cmd` + `predicates` for CLI integration tests.

**Rationale**:
- Industry standard for Rust CLI testing
- Spawns actual binary as subprocess (realistic testing)
- Fluent assertion API for exit codes, stdout, stderr
- Works with temporary files via `tempfile` crate

**Example Test**:
```rust
#[test]
fn compress_basic_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let input = temp.path().join("input.txt");
    std::fs::write(&input, b"test data").unwrap();

    Command::cargo_bin("crush")
        .unwrap()
        .arg("compress")
        .arg(&input)
        .assert()
        .success()
        .stdout(predicate::str::contains("Compressed"));

    assert!(temp.path().join("input.txt.crush").exists());
}
```

---

### Roundtrip Testing

**Decision**: Test compress → decompress cycles verify data integrity.

**Implementation**:
```rust
#[test]
fn roundtrip_preserves_data() {
    let original = b"test data with special chars: \x00\xFF";
    let compressed = compress(original).unwrap();
    let decompressed = decompress(&compressed).unwrap();
    assert_eq!(original.as_slice(), decompressed.as_slice());
}
```

---

## Open Questions & Decisions

### Q: Should config support environment variables?

**Decision**: Yes, for production deployments.

**Precedence** (highest to lowest):
1. Command-line flags (e.g., `--plugin deflate`)
2. Environment variables (e.g., `CRUSH_PLUGIN=deflate`)
3. Config file (`~/.config/crush/config.toml`)
4. Built-in defaults

**Environment Variable Naming**:
- Prefix: `CRUSH_`
- Format: `CRUSH_<SECTION>_<KEY>` (e.g., `CRUSH_COMPRESSION_LEVEL`)

---

### Q: Should CLI support reading from stdin?

**Decision**: Yes, for pipeline integration.

**Usage**:
```bash
cat data.txt | crush compress -o data.txt.crush
crush decompress data.txt.crush -o - | less
```

**Implementation**:
- Detect stdin with `std::io::stdin().is_terminal()`
- Disable progress bars when reading from stdin (no total size)
- Use spinner instead: "Compressing... (bytes processed: 1.2 GB)"

---

### Q: How to handle file permissions/timestamps?

**Decision**: Preserve where possible, warn on failure.

**Strategy**:
- Use `filetime` crate to preserve mtime
- Store original permissions in crush header metadata
- Restore permissions on decompress using `std::fs::set_permissions`
- Warn (don't fail) if restoration fails (e.g., cross-platform issues)

**Out of Scope**:
- Extended attributes (xattrs)
- ACLs (Windows)
- SELinux contexts

---

## Risk Assessment

### Risk 1: Cross-Platform Terminal Compatibility

**Impact**: Medium | **Likelihood**: Low

**Mitigation**:
- Use well-tested crates (`termcolor`, `is-terminal`)
- Graceful degradation: disable features in unsupported environments
- CI testing on Windows, Linux, macOS

---

### Risk 2: Large File Handling (>10GB)

**Impact**: High | **Likelihood**: Low

**Mitigation**:
- Use crush-core's streaming API (already handles large files)
- Progress bar uses u64 for byte counts (supports files up to 16 EB)
- Test with 50GB+ files in CI (if feasible) or document limits

---

### Risk 3: Plugin Discovery Performance

**Impact**: Low | **Likelihood**: Low

**Mitigation**:
- Plugin discovery happens once at CLI startup
- Target: <10ms for plugin registry initialization
- Benchmark in CI to detect regressions

---

## References

- clap v4 documentation: https://docs.rs/clap/latest/clap/
- indicatif examples: https://github.com/console-rs/indicatif/tree/main/examples
- tracing best practices: https://tokio.rs/tokio/topics/tracing
- Rust CLI Book: https://rust-cli.github.io/book/
- Command Line Interface Guidelines: https://clig.dev/

---

## Research Summary

All technical unknowns resolved. No NEEDS CLARIFICATION markers required in plan.md.

**Key Decisions**:
1. Dependencies selected and justified (all MIT/Apache-2.0 licensed)
2. CLI architecture follows Git/Cargo subcommand conventions
3. Error handling wraps crush-core with user-friendly messages
4. Progress bars use indicatif with 10Hz updates
5. Configuration uses TOML with environment variable overrides
6. Testing strategy uses assert_cmd for integration tests
7. Signal handling with ctrlc for graceful Ctrl+C shutdown

**No Blockers**: All dependencies available, no unresolved technical risks.
