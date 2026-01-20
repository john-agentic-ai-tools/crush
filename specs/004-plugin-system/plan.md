# Implementation Plan: Plugin System for Crush Library

**Branch**: `004-plugin-system` | **Date**: 2026-01-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/004-plugin-system/spec.md`

## Summary

This feature implements a plugin system for the Crush compression library that enables extensibility through dynamically loadable compression plugins. The core library will provide basic compress/decompress operations using a default algorithm, while plugins can extend functionality with specialized compression for specific file types. The system includes plugin discovery via explicit initialization, intelligent plugin selection based on performance metadata (default 70% throughput, 30% compression ratio weighting), timeout protection, and graceful failure handling with fallback to default compression.

## Technical Context

**Language/Version**: Rust stable (latest, pinned via rust-toolchain.toml)
**Primary Dependencies**:
- `rayon` (parallel processing)
- `flate2` (DEFLATE - default compression, Phase 1)
- `crc32fast` (checksums)
- `thiserror` (error handling)
- `libloading` (dynamic plugin loading) - NEEDS CLARIFICATION: evaluate vs compile-time plugin registration
- `criterion` (benchmarks)

**Storage**: N/A (in-memory plugin registry, file I/O for compression)
**Testing**: `cargo test`, `cargo-fuzz` (100k iterations minimum), property-based testing for roundtrip guarantees
**Target Platform**: Cross-platform (Linux, macOS, Windows)
**Project Type**: Library workspace with plugin trait system
**Performance Goals**:
- Plugin discovery: <500ms for initialization
- Plugin selection: <10ms for scoring and selection
- Default compression: within 5% of gzip single-threaded
- Throughput: >500 MB/s compression (8-core CPU)

**Constraints**:
- No `.unwrap()` in production code
- Memory usage: <32MB per thread
- Plugin timeout: 30s default, configurable
- 100% memory safe (no unsafe unless documented)

**Scale/Scope**:
- Support unlimited plugins (registry limited by memory)
- File sizes: small to multi-GB (memory-mapped I/O)
- Concurrent compression operations (thread-safe plugin registry)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Core Principles Compliance

✅ **I. Performance First**
- Plugin scoring prioritizes throughput (70% weight) by default
- Timeout protection prevents slow plugins from degrading performance
- Zero-copy plugin trait design (passes byte slices, not owned data)
- Performance metadata drives automatic plugin selection

✅ **II. Correctness & Safety**
- Error handling: `Result<T, CrushError>` for all fallible operations
- No `.unwrap()` in production (enforced by clippy)
- Fuzz testing mandatory for compression/decompression paths
- Plugin crash isolation with fallback to default compression
- Input validation at plugin boundaries

✅ **III. Modularity & Extensibility**
- Trait-based plugin contract: `CompressionPlugin` trait
- Clear separation: `crush-core` (library) + `crush-cli` (wrapper)
- Plugins independently testable via trait implementation
- Zero-cost abstractions (plugins compile away if unused)

✅ **IV. Test-First Development**
- TDD workflow: tests → approval → fail → implement
- Unit tests for plugin registry, scoring, timeout mechanisms
- Integration tests for plugin discovery and selection
- Roundtrip tests for compression/decompression
- Fuzz testing for all codec paths

### Dependency Compliance

✅ **Allowed Dependencies Used**:
- `rayon` - parallel processing (justified: core parallelization)
- `flate2` - DEFLATE (justified: default compression algorithm)
- `crc32fast` - checksums (justified: data integrity)
- `thiserror` - error handling (justified: ergonomic error types)
- `criterion` - benchmarks (dev, justified: performance measurement)

⚠️ **New Dependencies to Evaluate** (Phase 0 Research):
- `libloading` - dynamic plugin loading (NEEDS JUSTIFICATION vs compile-time registration)
- Alternative: Inventory-based compile-time registration (zero runtime overhead)

❌ **Prohibited Dependencies**: None planned

### Quality Gates Status

Pre-implementation (all gates pending):
- [ ] All tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- [ ] Code coverage > 80%
- [ ] Benchmarks show no regression (< 5% slowdown)
- [ ] Documentation builds without warnings (`cargo doc --no-deps`)
- [ ] Fuzz testing clean (100k iterations, no panics)
- [ ] No memory leaks (miri clean)
- [ ] SpecKit task checklist complete

### Git Flow & CI Compliance

✅ **Branching Model**:
- Feature branch `004-plugin-system` created from `develop`
- Will merge back to `develop` via PR with CI gates
- No compatibility preservation with other feature branches

✅ **Pre-Commit Hooks**:
- `cargo-husky` installed in workspace root
- Hooks run `cargo fmt --check` + `cargo clippy --quiet`
- Fast checks only (<10s)

✅ **CI Enforcement**:
- CI will block merge on failure
- All quality gates enforced in CI pipeline
- No manual override permitted

### Constitution Verdict

**STATUS**: ✅ **PASS** - No violations. All dependencies justified or pending research. No complexity requiring justification table.

**Post-Design Re-Check Required**: Yes, after Phase 1 (plugin trait design)

## Project Structure

### Documentation (this feature)

```text
specs/004-plugin-system/
├── plan.md              # This file
├── research.md          # Phase 0: Plugin loading mechanisms, scoring algorithms
├── data-model.md        # Phase 1: Plugin metadata, registry, configuration
├── quickstart.md        # Phase 1: Plugin development guide
├── contracts/           # Phase 1: Plugin trait API, error types
│   └── plugin-trait.md  # CompressionPlugin trait specification
└── tasks.md             # Phase 2: NOT created by this command
```

### Source Code (repository root)

```text
crush/
├── crush-core/                    # Core library crate
│   ├── src/
│   │   ├── lib.rs                 # Public API exports
│   │   ├── error.rs               # CrushError types (thiserror)
│   │   ├── engine.rs              # Compression engine (plugin orchestrator)
│   │   ├── plugin/                # Plugin system
│   │   │   ├── mod.rs             # Plugin module exports
│   │   │   ├── contract.rs        # CompressionPlugin trait
│   │   │   ├── registry.rs        # PluginRegistry (thread-safe)
│   │   │   ├── metadata.rs        # PluginMetadata struct
│   │   │   ├── selector.rs        # Plugin selection logic (scoring)
│   │   │   ├── timeout.rs         # Timeout enforcement
│   │   │   └── default.rs         # Default DEFLATE plugin
│   │   ├── compression.rs         # Compression operations API
│   │   └── decompression.rs       # Decompression operations API
│   ├── tests/
│   │   ├── integration/           # Integration tests
│   │   │   ├── plugin_discovery.rs
│   │   │   ├── plugin_selection.rs
│   │   │   └── roundtrip.rs
│   │   └── fixtures/              # Test data
│   └── Cargo.toml
│
├── crush-cli/                     # CLI wrapper crate
│   ├── src/
│   │   ├── main.rs                # CLI entry point
│   │   ├── args.rs                # clap argument parsing
│   │   └── signal.rs              # Signal handling
│   └── Cargo.toml
│
├── benches/                       # Criterion benchmarks
│   ├── plugin_discovery.rs        # Discovery performance
│   ├── plugin_selection.rs        # Selection performance
│   └── compression.rs             # Compression throughput
│
├── fuzz/                          # Fuzz testing (cargo-fuzz)
│   └── fuzz_targets/
│       ├── compress.rs            # Fuzz compression
│       └── decompress.rs          # Fuzz decompression
│
├── Cargo.toml                     # Workspace manifest
├── rust-toolchain.toml            # Toolchain pinning
├── .cargo-husky/                  # Pre-commit hooks
│   └── hooks/
│       └── pre-commit
└── deny.toml                      # cargo-deny configuration
```

**Structure Decision**: Rust workspace with two crates (`crush-core` library, `crush-cli` wrapper) following constitution's modularity principle. Plugin system lives entirely in `crush-core/src/plugin/` module. This structure enables independent testing of plugin components and allows CLI to be a thin wrapper around core functionality.

## Complexity Tracking

> No violations requiring justification.

---

## Phase 0: Research & Clarifications

### Research Tasks

The following questions must be resolved before design begins:

#### R1: Plugin Loading Mechanism

**Question**: Should plugins be loaded dynamically at runtime (`libloading`) or registered at compile-time (`inventory` or similar)?

**Considerations**:
- Dynamic loading: Enables plugin installation without recompilation, but requires `unsafe`, ABI stability concerns, platform differences
- Compile-time registration: Zero runtime overhead, type-safe, but requires recompilation to add plugins
- Hybrid: Default plugins compiled-in, optional dynamic loading for advanced users

**Research Required**:
- Evaluate `libloading` safety guarantees and platform support
- Investigate `inventory` or `linkme` for compile-time registration
- Assess ABI stability requirements for dynamic plugins
- Review pigz and other compression tools for plugin architecture patterns

**Decision Impact**: High - affects plugin distribution model, safety guarantees, and trait design

#### R2: Plugin Scoring Algorithm

**Question**: How should the 70/30 weighted score be calculated? Linear combination, normalized scales, or more sophisticated scoring?

**Considerations**:
- Throughput units: MB/s (varies widely: 50-5000 MB/s)
- Compression ratio units: 0.0-1.0 (typically 0.3-0.9)
- Need normalization to prevent throughput from dominating
- User-configurable weights require flexible formula

**Research Required**:
- Review academic literature on multi-objective optimization
- Investigate normalization techniques (min-max, z-score)
- Benchmark scoring algorithm performance (<10ms requirement)
- Consider edge cases (zero throughput, negative weights)

**Decision Impact**: Medium - affects plugin selection correctness but can be refined post-launch

#### R3: Timeout Implementation

**Question**: How to implement reliable cross-platform timeout enforcement for plugin operations?

**Considerations**:
- Rust timeout patterns: `std::sync::mpsc::recv_timeout`, `tokio::time::timeout` (but no async in core!)
- Thread-based timeout: spawn plugin in thread, join with timeout
- Signal-based timeout: platform-specific (SIGALRM on Unix)
- Cancellation token patterns

**Research Required**:
- Evaluate `crossbeam::channel` with timeout for thread-based approach
- Review Rust ecosystem timeout crates (`timeout-readwrite`, `std::thread::park_timeout`)
- Test reliability on Windows/Linux/macOS
- Measure timeout overhead (<1ms preferred)

**Decision Impact**: High - affects reliability and performance

#### R4: Plugin Metadata Format

**Question**: How should plugins declare metadata (throughput, compression ratio, magic number)?

**Considerations**:
- Trait methods: `fn metadata() -> PluginMetadata`
- Struct attributes: `#[plugin(throughput = "1000", ratio = "0.6")]`
- External file: `plugin.toml` alongside plugin binary
- Hardcoded in trait impl vs runtime configuration

**Research Required**:
- Review Rust plugin frameworks (e.g., `pdk`, `abi_stable`)
- Investigate procedural macro options for declarative metadata
- Assess runtime vs compile-time metadata tradeoffs

**Decision Impact**: Medium - affects plugin developer experience

#### R5: Magic Number Format & Collision Handling

**Question**: What format for magic numbers? How to prevent collisions?

**Considerations**:
- Magic number size: 2 bytes (like gzip), 4 bytes, 8 bytes?
- Format: Raw bytes, string identifier, UUID?
- Collision detection: Registry validates uniqueness on registration
- Reserved ranges: Reserve magic numbers for default/future plugins

**Research Required**:
- Survey existing compression formats (gzip: 1f 8b, zstd: 28 b5 2f fd, etc.)
- Investigate UUID v4 for guaranteed uniqueness (but larger header)
- Benchmark header size impact on small file compression
- Review industry standards for magic number allocation

**Decision Impact**: High - affects file format and decompression routing

#### R6: File Format Structure

**Question**: What should the compressed file header structure look like?

**Considerations**:
- Header components: magic number, version, plugin identifier, metadata (checksums, original size, etc.)
- Endianness: Little-endian (Rust default) vs big-endian (network order)
- Extensibility: Reserve space for future metadata
- Backward compatibility: Version field for format evolution

**Research Required**:
- Review gzip header format (RFC 1952)
- Study zstd frame format (RFC 8878)
- Investigate `bincode` or `postcard` for Rust struct serialization
- Benchmark header serialization/deserialization performance

**Decision Impact**: High - affects file format specification

---

## Phase 1: Design Artifacts

*To be generated after Phase 0 research completion*

### Phase 1 Outputs

1. **data-model.md**: Plugin metadata structure, registry design, configuration options
2. **contracts/plugin-trait.md**: `CompressionPlugin` trait specification, error types
3. **quickstart.md**: Plugin development guide with examples
4. **Agent context update**: Technology stack recorded for future reference

### Phase 1 Re-Check

Constitution check will be repeated after trait design to verify:
- Trait design follows zero-cost abstraction principle
- Error handling uses `Result<T, CrushError>` consistently
- No unsafe code introduced without documentation
- Plugin API is independently testable

---

## Notes

- This plan follows the constitution's TDD mandate: all tests will be written and approved before implementation
- Plugin system design prioritizes performance (70% throughput weight) per clarification session
- Feature branch is unstable per constitution - breaking changes expected during development
- CI gates will enforce quality standards; pre-commit hooks catch trivial errors early
