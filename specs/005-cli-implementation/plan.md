# Implementation Plan: CLI Implementation

**Branch**: `005-cli-implementation` | **Date**: 2026-01-22 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/005-cli-implementation/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Implement a comprehensive command-line interface that exposes crush-core library functionality through intuitive commands. The CLI provides file compression/decompression, inspection, configuration management, and plugin discovery. Key features include progress bars for long operations, verbose diagnostic mode, structured logging for production, and a robust help system. The implementation wraps existing crush-core capabilities with user-friendly error messages and follows Unix CLI conventions.

## Technical Context

**Language/Version**: Rust 1.84+ (stable, aligned with existing crush-core)
**Primary Dependencies**:
- `clap` v4 - Command-line argument parsing (already approved in constitution)
- `indicatif` - Progress bar rendering
- `termcolor` - ANSI terminal colors (cross-platform support)
- `serde` + `toml` - Configuration file handling
- `tracing` + `tracing-subscriber` - Structured logging
- `is-terminal` - Terminal detection (modern replacement for deprecated atty)

**Storage**:
- Configuration files in OS-standard locations (`~/.config/crush/config.toml` on Linux/macOS, `%APPDATA%\Crush\config.toml` on Windows)
- No database required

**Testing**:
- `cargo test` - Unit and integration tests
- `assert_cmd` + `predicates` - CLI integration testing
- Criterion benchmarks for startup time and help command performance

**Target Platform**:
- Cross-platform: Linux, macOS, Windows
- Terminal/TTY environments
- Non-TTY environments (CI/CD pipelines)

**Project Type**: Single workspace with thin CLI wrapper over crush-core library

**Performance Goals**:
- CLI startup time <50ms (command parsing to operation start)
- Help command <100ms execution time
- Progress bar updates ≥1/second
- Verbose mode overhead <5% performance impact
- Binary size <10MB

**Constraints**:
- Memory usage <100MB base + streaming buffers
- Must compile on stable Rust (no nightly features)
- Cross-platform compatibility (Windows, Linux, macOS)
- Terminal compatibility (graceful degradation in non-TTY)
- No .unwrap()/.expect() in production code

**Scale/Scope**:
- 8 user stories (P1-P8 priority)
- ~12-15 subcommands (compress, decompress, inspect, config, plugins, help)
- Batch processing up to 1000+ files
- Support for files >10GB via streaming

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### I. Performance First
- ✅ **PASS**: CLI startup time target <50ms aligns with performance-first principle
- ✅ **PASS**: Progress bar rendering will not block compression operations (parallel updates)
- ✅ **PASS**: Verbose mode constrained to <5% overhead maintains core performance
- ✅ **PASS**: Zero-copy principle preserved - CLI streams data, doesn't buffer entire files
- ✅ **PASS**: Performance benchmarks required for CLI startup and help commands

### II. Correctness & Safety
- ✅ **PASS**: No .unwrap()/.expect() in CLI production code (same as crush-core)
- ✅ **PASS**: Input validation at CLI boundaries (file path validation, argument checking)
- ✅ **PASS**: Error handling wraps crush-core errors with user-friendly messages
- ✅ **PASS**: Graceful handling of Ctrl+C with cleanup of partial files
- ✅ **PASS**: CRC32 validation errors displayed to users during decompression

### III. Modularity & Extensibility
- ✅ **PASS**: CLI is thin wrapper over crush-core library (separation maintained)
- ✅ **PASS**: Configuration uses builder pattern via clap's derive API
- ✅ **PASS**: CLI delegates all compression logic to crush-core (no algorithm duplication)
- ✅ **PASS**: Plugin discovery via crush-core registry (no CLI-specific plugin code)
- ✅ **PASS**: Subcommand structure allows independent testing

### IV. Test-First Development
- ✅ **PASS**: TDD enforced - CLI integration tests written before implementation
- ✅ **PASS**: Unit tests for argument parsing, config management, error formatting
- ✅ **PASS**: Integration tests using assert_cmd for full CLI workflows
- ✅ **PASS**: Benchmark suite for startup time and help performance
- ✅ **PASS**: Roundtrip tests verify CLI compress/decompress matches crush-core behavior

### Dependency Management
- ✅ **PASS**: clap - Already approved in constitution for CLI parsing
- ✅ **PASS**: indicatif - Justified for progress bars (core UX feature)
- ✅ **PASS**: termcolor - Justified for terminal output formatting (cross-platform)
- ✅ **PASS**: serde + toml - Justified for configuration (minimal, standard)
- ✅ **PASS**: tracing - Justified for structured logging (spans, structured fields)
- ✅ **PASS**: is-terminal - Justified for terminal detection (modern, maintained)
- ✅ **PASS**: All dependencies use MIT/Apache-2.0/BSD licenses

### Quality Gates
- ✅ **PASS**: All tests must pass (cargo test on CLI crate)
- ✅ **PASS**: Zero clippy warnings (same pedantic mode as crush-core)
- ✅ **PASS**: Code coverage >80% for CLI code
- ✅ **PASS**: Benchmarks verify startup time <50ms, help <100ms
- ✅ **PASS**: Documentation for all public CLI interfaces
- ✅ **PASS**: SpecKit task checklist completion required

**Constitution Compliance**: ✅ ALL GATES PASSED

No violations detected. CLI implementation aligns with all constitution principles.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
crush/                           # Workspace root
├── Cargo.toml                   # Workspace manifest
├── crush-core/                  # Core compression library (existing)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── compression.rs
│   │   ├── decompression.rs
│   │   ├── error.rs
│   │   └── plugin.rs
│   ├── tests/
│   ├── benches/
│   └── fuzz/
│
├── crush-cli/                   # CLI wrapper crate (THIS FEATURE)
│   ├── Cargo.toml               # CLI dependencies (clap, indicatif, etc.)
│   ├── src/
│   │   ├── main.rs              # Entry point, clap setup
│   │   ├── commands/            # Command implementations
│   │   │   ├── mod.rs
│   │   │   ├── compress.rs      # Compress subcommand
│   │   │   ├── decompress.rs    # Decompress subcommand
│   │   │   ├── inspect.rs       # Inspect subcommand
│   │   │   ├── config.rs        # Config subcommands
│   │   │   └── plugins.rs       # Plugin subcommands
│   │   ├── cli.rs               # Clap argument definitions
│   │   ├── config.rs            # Configuration file handling
│   │   ├── error.rs             # CLI-specific error handling
│   │   ├── output.rs            # Output formatting (progress, colors)
│   │   ├── logging.rs           # Structured logging setup
│   │   └── signal.rs            # Ctrl+C handling
│   └── tests/
│       ├── integration/         # Full CLI integration tests
│       │   ├── compress_tests.rs
│       │   ├── decompress_tests.rs
│       │   ├── inspect_tests.rs
│       │   ├── config_tests.rs
│       │   └── plugins_tests.rs
│       └── fixtures/            # Test data files
│
└── tests/                       # Workspace-level integration tests
    └── cli_roundtrip_tests.rs   # Cross-crate roundtrip validation
```

**Structure Decision**: Rust workspace with two crates following the constitution's modularity principle. The `crush-cli` crate is a thin wrapper that delegates all compression logic to `crush-core`. This separation ensures:
- Core library remains CLI-agnostic for embedded use cases
- CLI code is isolated and independently testable
- Dependencies like `indicatif` and `colored` don't pollute core library
- Clear separation of concerns: core = WHAT to compress, CLI = HOW users interact

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

**No violations detected** - All constitution gates passed. No complexity exceptions required.
