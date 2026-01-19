# Feature Specification: Rust Workspace Project Structure

**Feature Branch**: `003-rust-workspace-setup`
**Created**: 2026-01-18
**Status**: Draft
**Input**: User description: "Create the structure for the Rust project as described in the Constitution. This spec is focused on ensuring the correct project structure is in place and that project can compile. Once the structure is in place we can verify that the local build tool chain functions, precommit hooks are working as expected, and that the remaining tasks from last feature branch that defined the CI pipeline are fully functional."

## User Scenarios & Testing

### User Story 1 - Compilable Workspace Foundation (Priority: P1) 🎯 MVP

Developers need a working Rust workspace structure that compiles successfully and follows the constitution's modular architecture (core library + CLI wrapper).

**Why this priority**: This is the absolute foundation - without a compilable workspace, no other development can proceed. This enables all subsequent features and validates that the basic project structure is correct.

**Independent Test**: Run `cargo build` from the repository root. The command completes successfully without errors, producing binaries for both crush-core library and crush-cli executable. Test deliverability: A developer can clone the repository and build the project immediately.

**Acceptance Scenarios**:

1. **Given** a fresh clone of the repository, **When** running `cargo build`, **Then** the workspace compiles successfully with both crush-core and crush-cli crates
2. **Given** the compiled workspace, **When** running `cargo run --bin crush`, **Then** the CLI binary executes without panic and displays basic usage information
3. **Given** the workspace manifest, **When** inspecting Cargo.toml, **Then** both crates are defined as workspace members with correct paths

---

### User Story 2 - Code Quality Tooling (Priority: P2)

Developers need automatic code formatting and linting to maintain consistent code quality and catch issues early, as mandated by the constitution's quality gates.

**Why this priority**: Essential for maintaining code quality and passing CI checks, but the project can technically compile without it. This must be in place before any significant code is written.

**Independent Test**: Run `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`. Both commands pass without errors. Create intentional formatting violation, verify `cargo fmt --all` auto-fixes it.

**Acceptance Scenarios**:

1. **Given** properly formatted code, **When** running `cargo fmt --all -- --check`, **Then** no formatting changes are suggested
2. **Given** unformatted code, **When** running `cargo fmt --all`, **Then** all code is automatically formatted according to rustfmt rules
3. **Given** the codebase, **When** running `cargo clippy --all-targets -- -D warnings`, **Then** no warnings are emitted
4. **Given** a pinned Rust version in rust-toolchain.toml, **When** running any cargo command, **Then** the specified toolchain version is used automatically

---

### User Story 3 - Test Infrastructure (Priority: P3)

Developers need a working test framework to practice TDD (constitution requirement) and verify code correctness before implementation of actual features.

**Why this priority**: Required by constitution's Test-First Development principle, but can be added after basic structure compiles. Needed before implementing any real compression logic.

**Independent Test**: Run `cargo test` or `cargo nextest run` (using the nextest configuration from feature 002). At least one passing test exists in each crate. Code coverage can be measured with `cargo llvm-cov`.

**Acceptance Scenarios**:

1. **Given** the workspace, **When** running `cargo test`, **Then** all tests pass including at least one test in crush-core and one in crush-cli
2. **Given** the nextest configuration from feature 002, **When** running `cargo nextest run`, **Then** tests execute successfully using the CI profile
3. **Given** the test suite, **When** running `cargo llvm-cov`, **Then** code coverage report is generated successfully
4. **Given** a failing test, **When** running cargo test, **Then** clear error messages indicate which test failed and why

---

### User Story 4 - Documentation Foundation (Priority: P4)

Developers and users need basic documentation to understand project structure, usage, and API contracts from the beginning.

**Why this priority**: Important for onboarding and long-term maintainability, but not required for initial development. Can be enhanced incrementally as features are added.

**Independent Test**: Run `cargo doc --no-deps` and verify HTML documentation builds without warnings. Each crate has a README.md explaining its purpose. Public functions have doc comments with examples.

**Acceptance Scenarios**:

1. **Given** the workspace, **When** running `cargo doc --no-deps`, **Then** documentation builds successfully with zero warnings
2. **Given** the crush-core library, **When** viewing its documentation, **Then** all public types and functions have doc comments with descriptions
3. **Given** each crate directory, **When** inspecting the contents, **Then** a README.md file exists explaining the crate's purpose and basic usage
4. **Given** doc comments with code examples, **When** running `cargo test --doc`, **Then** all documentation examples compile and run successfully

---

### Edge Cases

- What happens when running cargo commands outside the workspace root directory?
- How does the workspace behave when only one crate is selected for building (e.g., `cargo build -p crush-core`)?
- What error messages appear if Rust toolchain version is incompatible or missing?
- How does the project handle when cargo-nextest is not installed but tests are run?
- What happens if .gitignore is missing and build artifacts are committed?

## Requirements

### Functional Requirements

- **FR-001**: Workspace MUST define crush-core and crush-cli as members in workspace Cargo.toml
- **FR-002**: crush-core MUST be a library crate (`[lib]`) with src/lib.rs as entry point
- **FR-003**: crush-cli MUST be a binary crate (`[[bin]]`) with src/main.rs as entry point
- **FR-004**: crush-cli MUST depend on crush-core as a workspace dependency
- **FR-005**: Workspace MUST compile successfully with `cargo build` producing zero errors
- **FR-006**: Project MUST specify Rust toolchain version in rust-toolchain.toml file
- **FR-007**: Project MUST include .gitignore file configured for Rust development (target/ directory, Cargo.lock for libraries, etc.)
- **FR-008**: crush-core lib.rs MUST export at least one public function or type
- **FR-009**: crush-cli main.rs MUST contain a functional main() function that compiles
- **FR-010**: Each crate MUST include at least one unit test marked with #[cfg(test)]
- **FR-011**: rustfmt configuration MUST be defined (rustfmt.toml or .rustfmt.toml)
- **FR-012**: clippy configuration MUST enforce pedantic lints as per constitution quality gates
- **FR-013**: Workspace MUST build successfully on Linux, Windows, and macOS (will be verified by CI from feature 002)
- **FR-014**: All public APIs in crush-core MUST have documentation comments (///)
- **FR-015**: Documentation MUST build successfully with cargo doc --no-deps

### Key Entities

- **Workspace Manifest (Cargo.toml)**: Defines the workspace structure, shared dependencies, workspace-level configuration
- **crush-core Crate**: Core compression library containing algorithms, traits, and engine logic (future implementation)
- **crush-cli Crate**: Command-line interface wrapper providing user-facing commands and argument parsing (future implementation)
- **rust-toolchain.toml**: Specifies the pinned Rust toolchain version ensuring consistent builds across environments
- **Configuration Files**: rustfmt.toml for formatting rules, clippy.toml for linting rules, .gitignore for VCS exclusions

## Success Criteria

### Measurable Outcomes

- **SC-001**: Developers can clone the repository and run `cargo build` successfully on first attempt without manual intervention
- **SC-002**: Workspace compiles in under 30 seconds on standard development hardware (first build, excluding dependency downloads)
- **SC-003**: Zero compiler warnings when building with `cargo build`
- **SC-004**: Zero clippy warnings when running `cargo clippy --all-targets -- -D warnings`
- **SC-005**: All formatting checks pass with `cargo fmt --all -- --check`
- **SC-006**: At least one test passes in each crate when running `cargo test`
- **SC-007**: Documentation builds successfully with zero warnings using `cargo doc --no-deps`
- **SC-008**: CI pipeline from feature 002 runs successfully against this workspace (format_check, lint, build_matrix, test, coverage jobs all pass)
- **SC-009**: Code coverage measurement succeeds with `cargo llvm-cov` showing >80% coverage for existing code
- **SC-010**: Project structure matches the Expected Workspace Structure defined in CLAUDE.md constitution

## Out of Scope

- Implementation of actual compression algorithms (deferred to future features)
- Benchmark infrastructure setup (criterion) - will be added when performance-critical code exists
- Fuzz testing setup (cargo-fuzz) - will be added when complex parsing/compression logic exists
- Plugin system architecture - will be designed in separate feature
- CLI argument parsing implementation - will be detailed in separate feature
- Performance optimization - no code to optimize yet
- Cross-compilation targets beyond standard tier-1 platforms
- Pre-commit hooks installation (cargo-husky) - mentioned in CONTRIBUTING.md but not blocking compilation

## Assumptions

- Rust toolchain (stable) is available via rustup on developer machines
- Developers have basic familiarity with Cargo workspace concepts
- The rust-toolchain.toml will pin to latest stable Rust unless specific version required
- Standard rustfmt and clippy configurations are acceptable (custom rules can be added incrementally)
- The .gitignore will follow standard Rust project patterns from github/gitignore repository
- Initial crates will contain minimal placeholder code (empty functions, basic main()) sufficient to compile
- Documentation can be sparse initially - detailed docs will grow with feature implementation
- The workspace will use default Cargo resolver (resolver = "2" in workspace manifest)
- Dependencies from the constitution's allowed list (rayon, flate2, etc.) will be added in future features when needed
- Build artifacts (target/ directory) are acceptable to be large initially - optimization later

## Dependencies

### Upstream Dependencies

- Feature 002 (GitHub Actions CI/CD Pipeline) MUST be complete - this feature validates that CI works with real code
- Rust toolchain installation documented in CONTRIBUTING.md

### Downstream Consumers

- All future features depend on this workspace structure being in place
- Feature 002 remaining validation tasks (T049-T061) can be completed after this feature
- Future compression algorithm features will add code to crush-core
- Future CLI enhancement features will add code to crush-cli

## Related Work

- Constitution: Defines core principles, quality gates, and allowed dependencies
- CLAUDE.md: Specifies expected workspace structure in "Project Structure" section
- Feature 002: CI/CD pipeline that will validate this workspace structure
- CONTRIBUTING.md: Documents development setup and workflow

## Notes

- This feature intentionally creates minimal code - just enough to compile and test the infrastructure
- Actual functionality (compression, CLI features) will be implemented in subsequent features following TDD
- The workspace structure follows constitution Principle III: Modularity & Extensibility (core library + thin CLI wrapper)
- All code must pass quality gates defined in constitution before merge
- This feature enables resuming Feature 002 validation tasks T049-T061 which require actual Rust code
