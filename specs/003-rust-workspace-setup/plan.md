# Implementation Plan: Rust Workspace Project Structure

**Branch**: `003-rust-workspace-setup` | **Date**: 2026-01-18 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/003-rust-workspace-setup/spec.md`

## Summary

Establish the foundational Rust workspace structure for the Crush high-performance compression library, following a virtual workspace model with two crates: `crush-core` (library) and `crush-cli` (binary wrapper). The workspace will include strict quality tooling (rustfmt, clippy), reproducible builds via pinned toolchain, and minimal compilable code with passing tests. This infrastructure validates the CI/CD pipeline from feature 002 and enables all future compression feature development.

## Technical Context

**Language/Version**: Rust 1.84.0 (stable) - pinned via rust-toolchain.toml
**Primary Dependencies**: None initially (rayon, flate2, etc. deferred to compression features)
**Storage**: N/A (filesystem I/O deferred to compression features)
**Testing**: cargo test (unit), cargo nextest (CI profile from feature 002), cargo llvm-cov (coverage)
**Target Platform**: Linux, Windows, macOS (tier-1 platforms)
**Project Type**: Workspace (library + CLI binary)
**Performance Goals**: Compile time < 30 seconds (clean build, excluding deps)
**Constraints**: Zero warnings (compiler, clippy, rustfmt, rustdoc), >80% code coverage
**Scale/Scope**: 2 crates initially, ~100 LOC placeholder code, foundation for ~10k+ LOC future project

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Performance First ✅ PASS

**Requirement**: Every decision MUST consider performance impact.

**Assessment**: This feature creates minimal code with no hot paths. Performance-critical decisions (memory pooling, SIMD, zero-copy) are deferred to compression algorithm features where benchmarks will drive optimization.

**Evidence**:
- Workspace resolver "2" chosen for faster dependency resolution
- No premature optimization introduced
- Placeholder code intentionally trivial (no performance impact)

### Principle II: Correctness & Safety ✅ PASS

**Requirement**: 100% memory safe, no `.unwrap()` in production, comprehensive error handling.

**Assessment**: This feature enforces safety through tooling and establishes patterns for future code.

**Evidence**:
- Clippy configuration includes `deny = ["unwrap_used", "expect_used", "panic"]`
- No unsafe code in placeholder implementation
- Error handling pattern defined (thiserror-based Result types)
- Constitution mandates fuzz testing (deferred to compression features with complex input)

### Principle III: Modularity & Extensibility ✅ PASS

**Requirement**: Clean separation, trait-based interfaces, plugin architecture from day one.

**Assessment**: Workspace structure enforces modularity via crate boundaries.

**Evidence**:
- Core library (`crush-core`) separated from CLI (`crush-cli`)
- CLI depends on core, not vice versa (enforces layering)
- Future traits (`CompressionAlgorithm`, `FormatHandler`) will live in `crush-core`
- Zero-cost abstractions enabled (no runtime overhead for modularity)

### Principle IV: Test-First Development ✅ PASS

**Requirement**: TDD mandatory, tests written before implementation, Red-Green-Refactor.

**Assessment**: This feature establishes test infrastructure and includes passing tests per spec.

**Evidence**:
- Each crate includes at least one unit test (FR-010)
- Integration with nextest from feature 002 (US3)
- Coverage measurement enabled via llvm-cov (SC-009)
- Doc-tests enabled for example validation

### Dependency Management ✅ PASS

**Requirement**: Minimalism principle, justify every dependency.

**Assessment**: Zero runtime dependencies initially. Dev dependencies justified.

**Evidence**:
- No crate dependencies in initial implementation (rayon, flate2 deferred)
- No prohibited dependencies (async runtimes, heavy frameworks)
- Future dependencies follow allowed list (thiserror, memmap2, etc.)

### Quality Gates ✅ PASS

**Requirement**: All quality gates must pass before merge.

**Assessment**: This feature creates the infrastructure to enforce quality gates.

**Evidence**:
- rustfmt.toml configured (SC-005)
- clippy.toml configured with pedantic lints (SC-004)
- Tests present and passing (SC-006)
- Documentation builds cleanly (SC-007)
- Coverage >80% achievable with minimal test code (SC-009)
- CI from feature 002 will validate (SC-008)

### Branching & Merge Governance ✅ PASS

**Requirement**: Git Flow model, PRs required, CI enforcement.

**Assessment**: Feature created on branch from develop, will merge via PR.

**Evidence**:
- Branch `003-rust-workspace-setup` created from `develop`
- PR will target `develop` (not `main`)
- CI enforces quality gates (feature 002)
- Branch protection active on `develop` and `main` (feature 002, T057-T058)

### Rust Style & Standards ✅ PASS

**Requirement**: rustfmt, clippy pedantic, document public APIs.

**Assessment**: Tooling configured and enforced.

**Evidence**:
- rustfmt.toml defines formatting rules
- clippy.toml enables pedantic lints
- All public items will have doc comments (FR-014)
- Error handling pattern defined (thiserror Result types)

**FINAL VERDICT**: ✅ ALL GATES PASS - No constitution violations. Proceed to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/003-rust-workspace-setup/
├── spec.md              # Feature specification
├── plan.md              # This file (implementation plan)
├── research.md          # Workspace & tooling research (Phase 0)
├── data-model.md        # Configuration entities (Phase 1)
├── quickstart.md        # Local testing guide (Phase 1)
├── contracts/           # File structure contracts (Phase 1)
│   └── workspace.md     # Cargo.toml schema contract
├── checklists/
│   └── requirements.md  # Spec quality checklist (complete)
└── tasks.md             # Generated by /speckit.tasks (Phase 2)
```

### Source Code (repository root)

```text
crush/                           # Repository root
├── Cargo.toml                   # Workspace manifest (virtual workspace)
├── rust-toolchain.toml          # Pinned Rust 1.84.0 stable
├── rustfmt.toml                 # Formatting configuration
├── clippy.toml                  # Linting configuration (pedantic)
├── .gitignore                   # Rust project exclusions
│
├── crush-core/                  # Core compression library
│   ├── Cargo.toml               # Library crate manifest
│   ├── README.md                # Crate documentation
│   └── src/
│       └── lib.rs               # Library entry point with placeholder code
│
├── crush-cli/                   # CLI binary wrapper
│   ├── Cargo.toml               # Binary crate manifest
│   ├── README.md                # Crate documentation
│   └── src/
│       └── main.rs              # Binary entry point with placeholder main()
│
├── .github/                     # CI/CD from feature 002 (existing)
│   └── workflows/
│       ├── ci.yml               # Quality gates workflow
│       ├── security-audit.yml   # Security scanning
│       └── release.yml          # Release automation
│
├── .config/                     # Tool configs from feature 002 (existing)
│   └── nextest.toml             # Test runner configuration
│
├── .cargo/                      # Cargo configs from feature 002 (existing)
│   └── deny.toml                # Dependency policy
│
├── specs/                       # SpecKit feature specs
│   ├── 002-github-actions-ci/
│   └── 003-rust-workspace-setup/  # This feature
│
└── target/                      # Build artifacts (gitignored)
```

**Structure Decision**: Virtual workspace with flat crate layout. Core library and CLI wrapper are sibling directories at repository root (not nested). This follows Rust community best practices for workspaces and maximizes discoverability. The workspace Cargo.toml at root defines both crates as members and manages shared dependencies (none initially). Future crates (benches/, fuzz/) will be added as additional workspace members.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No violations detected. Table not required.

## Phase 0: Research ✅ COMPLETE

**Status**: Research complete - see [research.md](research.md)

### Research Findings Summary

1. **Workspace Configuration**: Virtual workspace with resolver "2", shared dependencies via `[workspace.dependencies]` pattern, unified profiles
2. **Version Control**: Standard .gitignore excluding `/target/`, build artifacts, backup files
3. **Toolchain Pinning**: `rust-toolchain.toml` with channel = "stable", components = ["rustfmt", "clippy"]
4. **Code Quality**: Strict rustfmt (100 char width), pedantic clippy with unwrap/panic denial
5. **Project Layout**: Flat workspace (crates as siblings), library + binary separation
6. **Documentation**: Mandatory doc comments, separate README.md files, doc-test examples
7. **Testing**: Inline unit tests, integration tests in `tests/`, doc-tests in comments

All research decisions align with constitution principles and support the success criteria.

## Phase 1: Design & Contracts

### Data Model

See [data-model.md](data-model.md) for complete entity specifications.

**Key Entities**:

1. **Workspace Manifest** (`Cargo.toml` at root)
   - Defines workspace members: `["crush-core", "crush-cli"]`
   - Resolver version: "2"
   - Shared workspace metadata (authors, edition, license)
   - Shared dependency specifications (for future use)

2. **Library Crate Manifest** (`crush-core/Cargo.toml`)
   - Package metadata: name, version, description
   - Library target configuration
   - Dependencies: none initially
   - Dev dependencies: none initially (test framework built-in)

3. **Binary Crate Manifest** (`crush-cli/Cargo.toml`)
   - Package metadata
   - Binary target configuration (`crush` executable)
   - Dependencies: crush-core (workspace path dependency)

4. **Toolchain Configuration** (`rust-toolchain.toml`)
   - Channel: stable (Rust 1.84.0)
   - Components: rustfmt, clippy
   - Targets: none (uses default host triple)

5. **Quality Tool Configurations**
   - `rustfmt.toml`: Formatting rules (100 char line width, imports grouping)
   - `clippy.toml`: Lint configuration (pedantic enabled, unwrap denial)

### API Contracts

See [contracts/](contracts/) directory for complete specifications.

**contracts/workspace.md**: Cargo.toml schema contract

Defines the expected structure of workspace and crate manifests including:
- Workspace member declarations
- Package metadata requirements
- Dependency specification patterns (none initially, but schema defined)
- Profile configurations (dev, release, test)

**contracts/library-api.md**: crush-core public API contract

Initial minimal API:
```rust
/// Placeholder function demonstrating public API structure.
///
/// This function will be replaced with actual compression functionality
/// in future features. It exists to validate:
/// - Documentation builds correctly
/// - Public APIs are exported
/// - Tests can call public functions
///
/// # Examples
///
/// ```
/// use crush_core::hello;
/// assert_eq!(hello(), "Hello from crush-core!");
/// ```
pub fn hello() -> &'static str {
    "Hello from crush-core!"
}
```

**contracts/cli-interface.md**: crush-cli executable contract

Initial CLI behavior:
- Executable name: `crush`
- Invocation: `cargo run --bin crush` or direct `./target/debug/crush`
- Output: Basic usage message demonstrating successful compilation
- Exit code: 0 (success)

Future iterations will define argument parsing contracts (clap integration).

### Quickstart Testing Guide

See [quickstart.md](quickstart.md) for complete local testing procedures.

**Testing Workflow**:

1. **Clean Build Test** (validates SC-001, SC-002):
   ```bash
   cargo clean
   time cargo build
   # Expected: Success in < 30 seconds
   ```

2. **Quality Gates Test** (validates SC-003, SC-004, SC-005):
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo build
   # Expected: Zero warnings/errors
   ```

3. **Test Execution** (validates SC-006):
   ```bash
   cargo test
   cargo nextest run --profile ci  # Uses nextest from feature 002
   # Expected: All tests pass
   ```

4. **Coverage Measurement** (validates SC-009):
   ```bash
   cargo llvm-cov --html
   # Expected: >80% coverage, report in target/llvm-cov/html/
   ```

5. **Documentation Build** (validates SC-007):
   ```bash
   cargo doc --no-deps
   # Expected: Clean build, docs in target/doc/
   ```

6. **CLI Execution** (validates US1 acceptance scenario 2):
   ```bash
   cargo run --bin crush
   # Expected: Program runs without panic, shows basic message
   ```

7. **CI Validation** (validates SC-008):
   ```bash
   # Trigger CI by pushing branch
   git push origin 003-rust-workspace-setup
   # Observe CI results on PR
   # Expected: All jobs pass (format_check, lint, build_matrix, test, coverage)
   ```

### Agent Context Update

Technology stack detected for this feature:
- Language: Rust 1.84.0
- Build tool: Cargo (workspace)
- Testing: cargo test, cargo nextest
- Quality: rustfmt, clippy (pedantic)
- Coverage: cargo llvm-cov

This information will be added to `.specify/memory/CLAUDE.md` via the agent context update script.

## Phase 2: Tasks (Deferred to /speckit.tasks)

Task breakdown will be generated via `/speckit.tasks` command after plan approval.

**Expected Task Structure**:

- **Phase 1 (Setup)**: Create root configuration files (Cargo.toml, rust-toolchain.toml, .gitignore, rustfmt.toml, clippy.toml)
- **Phase 2 (US1 - MVP)**: Create crush-core crate structure with minimal lib.rs and tests
- **Phase 3 (US1 - MVP)**: Create crush-cli crate structure with minimal main.rs
- **Phase 4 (US2)**: Validate quality tooling (rustfmt, clippy)
- **Phase 5 (US3)**: Verify test infrastructure (cargo test, nextest, coverage)
- **Phase 6 (US4)**: Create crate documentation (READMEs, doc comments)
- **Phase 7 (Polish)**: Local testing validation, CI validation, resume feature 002 tasks

## Implementation Notes

### Minimal Viable Code Philosophy

This feature intentionally creates placeholder code - just enough to:
1. Compile successfully (satisfy cargo build)
2. Pass quality gates (fmt, clippy, tests)
3. Demonstrate API structure (doc comments, examples)
4. Enable CI validation (feature 002 integration)

**No actual compression functionality** is implemented. Future features will replace placeholder functions with real algorithms following TDD (write tests first, then implement).

### CI Integration

This feature serves as the validation point for feature 002 (GitHub Actions CI/CD):
- Remaining tasks T049-T061 from feature 002 can be completed after this feature merges
- CI workflows will run against real Rust code for the first time
- Quality gates will enforce constitution principles automatically

### Feature 002 Resume Points

After this feature is complete, resume feature 002 validation:
- **T049**: Test format_check failure (intentional formatting violation)
- **T050**: Test lint failure (intentional clippy warning)
- **T051**: Test test failure (failing unit test)
- **T052**: Test coverage failure (remove tests to drop below 90%)
- **T053**: Test multi-platform builds (introduce platform-specific code)
- **T054**: Test security audit (add vulnerable dependency)

See `specs/002-github-actions-ci/REMINDER.md` for complete resumption guide.

### Dependency Management Strategy

**Phase 1 (This Feature)**: Zero runtime dependencies
- Validates workspace structure works
- Minimizes initial complexity
- Fast compilation for testing

**Future Phases**: Add dependencies incrementally as needed
- `thiserror`: Error handling (when implementing real algorithms)
- `rayon`: Parallel processing (when implementing multi-threading)
- `flate2`: DEFLATE (Phase 1 compression feature)
- `memmap2`: Memory-mapped I/O (when implementing file handling)
- `clap`: CLI argument parsing (when expanding CLI functionality)

Each dependency addition will be justified against constitution's minimalism principle.

### Error Handling Pattern

While minimal code doesn't require complex error handling, the pattern is established:

```rust
// Future error type structure (defined but not yet used)
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CrushError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Compression failed: {0}")]
    Compression(String),
}

pub type Result<T> = std::result::Result<T, CrushError>;
```

This pattern will be applied when implementing real compression features.

### Documentation Standards

**Crate-level documentation** (in lib.rs or main.rs):
```rust
//! Crush Core Library
//!
//! High-performance parallel compression library.
//!
//! # Examples
//!
//! ```
//! use crush_core::hello;
//! assert_eq!(hello(), "Hello from crush-core!");
//! ```
```

**Function-level documentation** (for all public items):
```rust
/// Brief one-line description.
///
/// Longer explanation if needed (2-3 sentences).
///
/// # Examples
///
/// ```
/// use crush_core::hello;
/// let result = hello();
/// assert!(!result.is_empty());
/// ```
///
/// # Errors (when applicable)
///
/// Returns `Err` if...
///
/// # Panics (avoid in production code)
///
/// This function panics if... (document but avoid)
pub fn hello() -> &'static str { ... }
```

### Testing Organization

**Unit tests** (inline with code):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        assert_eq!(hello(), "Hello from crush-core!");
    }
}
```

**Integration tests** (future, in `tests/` directory):
```rust
// tests/integration_test.rs
use crush_core;

#[test]
fn test_public_api() {
    assert!(crush_core::hello().len() > 0);
}
```

**Doc tests** (in documentation comments):
```rust
/// # Examples
///
/// ```
/// use crush_core::hello;
/// assert_eq!(hello(), "Hello from crush-core!");
/// ```
```

## Success Criteria Mapping

| Criterion | How Validated | Evidence Location |
|-----------|---------------|-------------------|
| SC-001 | Fresh clone → cargo build succeeds | quickstart.md section 1 |
| SC-002 | Build time < 30s | quickstart.md section 1 timing output |
| SC-003 | Zero compiler warnings | cargo build output |
| SC-004 | Zero clippy warnings | cargo clippy output |
| SC-005 | Formatting passes | cargo fmt --check output |
| SC-006 | Tests pass | cargo test output |
| SC-007 | Docs build cleanly | cargo doc output |
| SC-008 | CI passes | GitHub Actions results on PR |
| SC-009 | Coverage >80% | cargo llvm-cov report |
| SC-010 | Structure matches CLAUDE.md | Directory tree comparison |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| CI fails on first real code | Medium | High | Follow quickstart.md locally before pushing; CI configuration already tested with act (feature 002 T047-T048) |
| Coverage falls below 80% with minimal code | Low | Medium | Minimal code means high test-to-code ratio; add trivial helper functions if needed to demonstrate coverage |
| Toolchain pinning causes issues on different platforms | Low | Medium | Rust 1.84.0 stable is tier-1 on Linux/Windows/macOS; CI will validate all platforms |
| Placeholder code too trivial for meaningful validation | Low | Low | Goal is infrastructure validation, not functionality; actual compression features follow TDD in future |

## Approval Checklist

Before proceeding to `/speckit.tasks`:

- [X] Constitution check passes (all principles satisfied)
- [X] Research complete (research.md with all decisions documented)
- [X] Data model defined (data-model.md with all entities specified)
- [X] Contracts specified (contracts/ directory with workspace and API schemas)
- [X] Testing guide created (quickstart.md with step-by-step validation)
- [X] Project structure finalized (directory tree documented)
- [X] Success criteria mapped to validation steps
- [X] No NEEDS CLARIFICATION markers remain
- [ ] User approval obtained (awaiting review)

**Next Step**: Await user approval, then run `/speckit.tasks` to generate executable task breakdown.
