<!--
Sync Impact Report:
- Version change: 1.2.0 → 1.3.0
- Modified principles: AI Agent Behavior Guidance - added Specification Adherence
- Added sections:
  * Specification Adherence (CRITICAL) - agents must implement only what is in spec.md
- Removed sections: N/A
- Templates requiring updates:
  ✅ constitution.md - updated
  ⚠ plan-template.md - should validate implementation against spec.md
  ⚠ tasks-template.md - should reference spec.md for feature scope
- Follow-up TODOs: None
- Rationale: Prevents scope creep by requiring agents to implement exactly what's specified, not what they assume based on conventions from other tools
-->

# Crush Constitution

## Core Principles

### I. Performance First (NON-NEGOTIABLE)

Every implementation decision MUST consider performance impact. Default to parallel execution patterns that match or exceed pigz performance benchmarks.

**Requirements**:
- Use zero-copy operations wherever possible
- Implement SIMD-friendly data structures when beneficial
- Apply memory pooling to reduce allocations in hot paths
- Drive optimization through benchmarks, not speculation
- Require benchmark comparisons for performance-critical changes

**Rationale**: Crush exists to provide high-performance compression. Performance is not a feature - it is the product's fundamental value proposition.

### II. Correctness & Safety (NON-NEGOTIABLE)

Memory safety and correctness are absolute requirements. No compromise on safety for performance gains.

**Requirements**:
- 100% memory safe Rust code (no `unsafe` unless absolutely necessary and extensively documented)
- Comprehensive error handling - production code MUST NOT use `.unwrap()` or `.expect()` on fallible operations
- Input validation at all system boundaries
- Fuzz testing for all compression/decompression code paths
- Property-based testing for round-trip guarantees (compress → decompress → verify)

**Rationale**: Compression libraries handle untrusted input and must never compromise system security. Memory safety is Rust's superpower - we enforce it rigorously.

### III. Modularity & Extensibility (NON-NEGOTIABLE)

Plugin architecture from day one. Clean separation between core engine, format handlers, and compression algorithms.

**Requirements**:
- Clear trait-based interfaces: `CompressionAlgorithm`, `FormatHandler`, `ChecksumAlgorithm`
- Core engine (`crush-core`) is a library, CLI (`crush-cli`) is a thin wrapper
- Zero-cost abstractions - unused plugins compile away
- Configuration via builders, not constructors
- Each plugin must be independently testable

**Rationale**: Begin with gzip but design for extensibility. ZSTD, LZ4, Brotli, and custom algorithms should be straightforward to add without core engine changes.

### IV. Test-First Development (NON-NEGOTIABLE)

TDD is mandatory for all production code. Tests written → User approved → Tests fail → Implementation begins.

**Requirements**:
- Unit tests for all algorithms and data structures
- Integration tests for CLI interface
- Roundtrip tests for all compression formats
- Fuzz testing with cargo-fuzz (minimum 100k iterations per release)
- Benchmark suite using criterion
- Red-Green-Refactor cycle strictly enforced

**Rationale**: Compression correctness is binary - data loss is unacceptable. TDD ensures contracts are clear before implementation and provides regression protection.

## Dependency Management

### Minimalism Principle

Every dependency MUST be justified. Prefer standard library solutions when performance/correctness equivalent.

### Allowed Core Dependencies

- `rayon` - Parallel processing (justified: core parallelization strategy)
- `flate2` - DEFLATE implementation (Phase 1 only, replace in Phase 2)
- `crc32fast` - CRC32 checksums (justified: performance-critical checksum)
- `memmap2` - Memory-mapped files (justified: zero-copy large file handling)
- `thiserror` - Error handling (justified: ergonomic error types)
- `clap` - CLI parsing (CLI crate only, justified: robust argument parsing)
- `criterion` - Benchmarking (dev dependency, justified: performance measurement)
- `cargo-husky` - Pre-commit hooks (dev dependency, justified: enforce formatting and linting)

### Prohibited Dependencies

- Async runtimes (`tokio`, `async-std`) in core library - creates unnecessary complexity
- Heavy framework dependencies - keep core lean
- Deprecated or unmaintained crates
- Dependencies with known security vulnerabilities

## Quality Gates

All of the following MUST pass before merge:

- [ ] All tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- [ ] Code coverage > 80% (`cargo tarpaulin` or similar)
- [ ] Benchmarks show no regression (< 5% slowdown on reference workloads)
- [ ] Documentation builds without warnings (`cargo doc --no-deps`)
- [ ] Fuzz testing runs clean (100k iterations minimum, no panics or errors)
- [ ] No memory leaks (miri or valgrind clean for unsafe code)
- [ ] SpecKit task checklist complete

## Rust Style & Standards

### Code Style

- Follow official Rust style guide (enforced via `rustfmt`)
- Use `clippy` with pedantic lints enabled
- Prefer iterators over manual loops
- Use `?` operator for error propagation, never `.unwrap()` in production
- Document all public APIs with examples

### Error Handling Pattern

```rust
// Rich error types with thiserror
#[derive(Debug, thiserror::Error)]
pub enum CrushError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Compression failed: {0}")]
    Compression(String),

    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    #[error("Checksum mismatch: expected {expected:x}, got {actual:x}")]
    ChecksumMismatch { expected: u32, actual: u32 },
}

pub type Result<T> = std::result::Result<T, CrushError>;
```

## Development Toolchain

### Rust Toolchain Pinning (MANDATORY)

All developers and CI environments MUST use a pinned Rust toolchain defined in `rust-toolchain.toml` at the repository root.

**Requirements**:
- Fixed channel specification (e.g., `stable`, `1.75.0`, or `nightly-2024-01-01`)
- Include `rustfmt` component
- Include `clippy` component

**Example `rust-toolchain.toml`**:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

**Rationale**: Consistent toolchain across all environments prevents "works on my machine" issues and ensures reproducible builds and linting results.

### Pre-Commit Hooks (MANDATORY)

All commits MUST pass automated pre-commit checks managed via `cargo-husky`.

**Requirements**:
- Install `cargo-husky` as a dev dependency in workspace root `Cargo.toml`
- Configure `.cargo-husky/hooks/pre-commit` to run fast checks only
- Hooks MUST complete in under 10 seconds on a modern development machine

**Allowed Pre-Commit Checks**:
- `cargo fmt --all -- --check` (formatting verification)
- `cargo clippy --quiet` (linting without verbose output)

**Prohibited Pre-Commit Checks**:
- `cargo test` or any test execution - MUST run in CI only
- `cargo bench` or benchmark execution - MUST run in CI only
- `cargo build --release` or release builds - MUST run in CI only
- Full feature matrix builds - MUST run in CI only
- Any check taking longer than 10 seconds

**Example `Cargo.toml` (workspace root)**:

```toml
[workspace]
members = ["crush-core", "crush-cli"]

[workspace.dependencies]
# ... other dependencies ...

[dev-dependencies]
cargo-husky = { version = "1", default-features = false, features = ["user-hooks"] }
```

**Example `.cargo-husky/hooks/pre-commit`**:

```bash
#!/bin/sh
cargo fmt --all -- --check
cargo clippy --quiet
```

**Rationale**: Fast pre-commit hooks catch trivial errors (formatting, common lints) before they enter the commit history, reducing CI failures and review burden. Heavy operations in pre-commit destroy developer flow and MUST be avoided.

## Performance Standards

### Phase 1 Targets (Pigz Port)

- **Single-threaded**: Within 5% of gzip performance
- **Multi-threaded**: Match or exceed pigz on 4+ core systems
- **Memory usage**: < 32MB per thread for default block size
- **Throughput**: > 500 MB/s compression on modern 8-core CPU

### Benchmark-Driven Development

- All performance claims MUST be backed by criterion benchmarks
- Regression tests MUST include performance benchmarks
- Optimization PRs MUST include before/after benchmark comparisons
- Benchmark fixtures MUST represent realistic workloads

## Branching & Merge Governance

### Git Flow Model (MANDATORY)

This repository enforces a Git Flow branching model with strict merge requirements.

**Branch Roles**:
- `main` - Release-only branch. Contains only stable, released versions. Direct commits are prohibited.
- `develop` - Default integration branch. All feature work merges here first. Direct commits are prohibited.
- Feature branches - Short-lived branches for active development. Created from `develop`, merged back to `develop`.

**Branching Requirements**:
- Feature branches MUST be created from `develop`
- Feature branches MUST be short-lived (delete after merge)
- Feature branches are NOT stability boundaries and MAY introduce breaking changes
- Feature branches MUST NOT be treated as compatibility boundaries

**Merge Requirements**:
- All merges into `develop` MUST occur via pull requests
- All merges into `main` MUST occur via pull requests
- Direct commits to `develop` are prohibited
- Direct commits to `main` are prohibited
- Pull requests MUST pass all CI checks before merge
- Pull requests MUST receive code review approval before merge

**Rationale**: Git Flow separates unstable development work from stable releases, enabling rapid iteration while maintaining release quality. Pull requests ensure review and automated quality enforcement.

## CI Enforcement

### Mandatory CI Gates (MANDATORY)

Continuous Integration (CI) is the enforcement mechanism for all quality standards defined in this constitution.

**CI Requirements**:
- CI MUST run on all pull requests targeting `develop` or `main`
- CI MUST block merges on failure - no manual override permitted
- CI MUST enforce all Rust best practices defined in this constitution
- CI MUST report results visibly on pull requests

**Required CI Checks**:
- Formatting validation across all code
- Comprehensive linting across all targets, features, and workspace members
- Full test suite execution (unit, integration, roundtrip)
- Security scanning for dependencies and known vulnerabilities
- Supply chain security verification
- Test coverage measurement and enforcement of coverage thresholds
- Documentation build verification
- Benchmark execution and regression detection

**Prohibited CI Specifications**:
- This constitution MUST NOT specify CI vendors, tools, or platforms
- This constitution MUST NOT define job configurations or pipeline syntax
- Implementation details are delegated to CI configuration files

**Rationale**: CI acts as the automated enforcement layer for constitution compliance. By blocking merges on failure, CI ensures no code enters integration branches without meeting quality standards. CI complements fast pre-commit hooks by running comprehensive checks that would be too slow locally.

## Release & Compatibility Policy

### Release Workflow (MANDATORY)

Releases are produced exclusively from the `main` branch following a defined promotion workflow.

**Release Requirements**:
- Releases MUST be produced only from `main`
- `main` MUST be updated exclusively via pull requests from `develop`
- Release tags MUST be applied only to commits on `main`
- Release artifacts MUST be built from tagged commits on `main`

**Compatibility Boundaries**:
- Backward compatibility guarantees apply only at release boundaries (between versions on `main`)
- Feature branches MAY introduce breaking changes without compatibility layers
- `develop` branch MAY introduce breaking changes without compatibility layers
- Breaking changes between feature branches are expected and require no migration code

**Prohibited Compatibility Practices**:
- Compatibility layers, shims, or transitional code MUST NOT be introduced unless required for a release
- Backward compatibility code MUST NOT be added speculatively "just in case"
- Feature branches MUST NOT preserve compatibility with other feature branches
- Deprecation warnings and migration helpers are only required for released public APIs

**Rationale**: Treating every branch as a compatibility boundary creates technical debt and slows development. By restricting compatibility guarantees to releases, we enable rapid iteration and refactoring during development while honoring commitments to users at release time.

## AI Agent Behavior Guidance

### Automated Agent Constraints (MANDATORY)

Automated agents, including Claude Code and similar AI-assisted development tools, MUST adhere to specific behavioral constraints when working in this repository.

**Stability Assumptions**:
- Agents MUST assume feature branches are unstable by design
- Agents MUST assume `develop` is unstable and may contain breaking changes
- Agents MUST NOT treat feature branches as compatibility boundaries
- Agents MUST NOT preserve backward compatibility between feature branches

**Compatibility Behavior**:
- Agents MUST NOT introduce compatibility layers unless targeting a release
- Agents MUST NOT add shims, adapters, or transitional code speculatively
- Agents MUST NOT create defensive code "just in case" something breaks
- Agents MUST NOT preserve deprecated interfaces during feature development

**Quality Enforcement**:
- Agents MUST rely on CI gates for correctness enforcement, not local safeguards
- Agents MUST rely on CI gates for security enforcement, not preemptive checks
- Agents MUST rely on CI gates for coverage enforcement, not manual coverage code
- Agents MUST trust the constitution-defined CI pipeline as the source of truth

**Refactoring Freedom**:
- Agents MAY freely refactor code on feature branches without compatibility concerns
- Agents MAY rename, restructure, or remove interfaces on feature branches
- Agents MAY introduce breaking changes during feature development
- Agents MUST NOT hesitate to improve code due to compatibility fears

**Specification Adherence** (CRITICAL):
- Agents MUST implement ONLY features explicitly specified in `specs/*/spec.md`
- Agents MUST NOT add features, flags, or behavior not documented in specifications
- Agents MUST NOT make assumptions about "expected" features from other tools
- Agents MUST ask for clarification if specifications are ambiguous or incomplete
- When in doubt, implement less rather than more - ship the minimal viable implementation
- Agents MUST NOT introduce "standard" or "common" features without specification approval

**Rationale**: AI agents often err on the side of caution, introducing defensive code and compatibility layers unnecessarily. By explicitly instructing agents that feature branches are unstable and CI is the enforcement mechanism, we prevent bloat and enable confident refactoring. Specification adherence prevents scope creep - agents must build exactly what was designed, not what they assume users want based on conventions from other tools.

## Governance

### Constitution Authority

This constitution supersedes all other development practices. When in conflict, constitution principles win.

### Amendment Process

1. **Proposal**: Document proposed change with rationale
2. **Discussion**: Allow minimum 7 days for feedback
3. **Approval**: Requires project maintainer approval
4. **Migration**: Create migration plan for affected code
5. **Version Bump**: Follow semantic versioning (MAJOR.MINOR.PATCH)

### Version Semantics

- **MAJOR**: Backward incompatible governance changes, principle removals or redefinitions
- **MINOR**: New principles added, materially expanded guidance
- **PATCH**: Clarifications, wording improvements, non-semantic refinements

### Compliance Verification

- All pull requests MUST verify compliance with constitution principles
- Code reviews MUST check Quality Gates completion
- Complexity that violates principles MUST be explicitly justified or rejected
- Use CLAUDE.md for runtime development guidance and patterns

### Violation Handling

- Minor violations (style, documentation): Fix in follow-up PR
- Major violations (safety, correctness, TDD bypass): Block merge, require rework
- Repeated violations: Review contributor access and training needs

**Version**: 1.2.0 | **Ratified**: 2026-01-17 | **Last Amended**: 2026-01-17
