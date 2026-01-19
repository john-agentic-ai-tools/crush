# Data Model: Rust Workspace Configuration Entities

**Feature**: 003-rust-workspace-setup | **Created**: 2026-01-18

This document defines the configuration entities that comprise the Rust workspace structure for the Crush high-performance compression library.

## Overview

The data model consists of configuration files (TOML manifests) and directory structures that define the workspace build system, toolchain requirements, and quality enforcement. These entities are declarative specifications consumed by Cargo, rustup, rustfmt, and clippy tooling.

## Entity Specifications

### 1. Workspace Manifest

**File**: `Cargo.toml` (repository root)
**Format**: TOML
**Consumer**: Cargo build system
**Purpose**: Defines workspace members, shared dependencies, and unified build profiles

#### Schema

```toml
[workspace]
resolver = "<string>"              # Dependency resolver version ("2")
members = [<string>, ...]          # Array of member crate paths

[workspace.package]                # Shared package metadata (optional)
edition = "<string>"               # Rust edition ("2021")
authors = [<string>, ...]          # Author list
license = "<string>"               # SPDX license identifier
repository = "<string>"            # Git repository URL
homepage = "<string>"              # Project homepage URL

[workspace.dependencies]           # Shared dependency specifications (optional)
<crate-name> = { version = "<semver>", ... }

[profile.dev]                      # Development profile
opt-level = <integer>              # Optimization level (0-3)
debug = <boolean>                  # Debug symbols

[profile.release]                  # Release profile
opt-level = <integer>              # Optimization level (0-3, s, z)
lto = <boolean|string>             # Link-time optimization
codegen-units = <integer>          # Parallel code generation units
strip = <boolean|string>           # Symbol stripping
```

#### Field Constraints

- **resolver**: Must be `"2"` (latest resolver, faster dependency resolution)
- **members**: Must include `["crush-core", "crush-cli"]` (relative paths from workspace root)
- **edition**: Must be `"2021"` (current stable Rust edition)
- **profile.dev.opt-level**: `0` (fastest compilation for development)
- **profile.release.opt-level**: `3` (maximum optimization for performance)
- **profile.release.lto**: `"thin"` (balanced link-time optimization)
- **profile.release.codegen-units**: `1` (maximize runtime performance)

#### Initial Values

```toml
[workspace]
resolver = "2"
members = ["crush-core", "crush-cli"]

[workspace.package]
edition = "2021"
authors = ["Crush Contributors"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/your-org/crush"

[profile.dev]
opt-level = 0
debug = true

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true

[profile.test]
inherits = "dev"
```

#### Relationships

- **Contains**: References to member crates (crush-core, crush-cli)
- **Inherited by**: Member crate manifests inherit workspace.package fields
- **Used by**: Cargo resolver, build system, test runner

---

### 2. Library Crate Manifest

**File**: `crush-core/Cargo.toml`
**Format**: TOML
**Consumer**: Cargo build system
**Purpose**: Defines the core compression library crate metadata and dependencies

#### Schema

```toml
[package]
name = "<string>"                  # Crate name (must be "crush-core")
version = "<semver>"               # Crate version
description = "<string>"           # One-line description
edition.workspace = true           # Inherit from workspace
authors.workspace = true           # Inherit from workspace
license.workspace = true           # Inherit from workspace
repository.workspace = true        # Inherit from workspace

[lib]
name = "<string>"                  # Library name (defaults to package name with underscores)
path = "<string>"                  # Library entry point (defaults to "src/lib.rs")

[dependencies]                     # Runtime dependencies (initially empty)

[dev-dependencies]                 # Test/bench dependencies (initially empty)
```

#### Field Constraints

- **package.name**: Must be `"crush-core"` (Cargo convention: kebab-case)
- **package.version**: Must follow SemVer (`0.1.0` initially)
- **lib.name**: Defaults to `"crush_core"` (Cargo converts hyphens to underscores)
- **lib.path**: Defaults to `"src/lib.rs"` (standard convention)
- **dependencies**: Empty initially (rayon, flate2, etc. added in future features)

#### Initial Values

```toml
[package]
name = "crush-core"
version = "0.1.0"
description = "High-performance parallel compression library"
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[lib]
# name and path use defaults (crush_core, src/lib.rs)

[dependencies]
# No dependencies in initial implementation

[dev-dependencies]
# Built-in test framework, no external dependencies needed
```

#### Relationships

- **Member of**: Workspace (referenced in workspace.members)
- **Depended on by**: crush-cli crate (workspace path dependency)
- **Provides**: Public library API for compression functionality

---

### 3. Binary Crate Manifest

**File**: `crush-cli/Cargo.toml`
**Format**: TOML
**Consumer**: Cargo build system
**Purpose**: Defines the CLI wrapper binary crate metadata and dependencies

#### Schema

```toml
[package]
name = "<string>"                  # Crate name (must be "crush-cli")
version = "<semver>"               # Crate version
description = "<string>"           # One-line description
edition.workspace = true           # Inherit from workspace
authors.workspace = true           # Inherit from workspace
license.workspace = true           # Inherit from workspace
repository.workspace = true        # Inherit from workspace

[[bin]]
name = "<string>"                  # Executable name ("crush")
path = "<string>"                  # Binary entry point (defaults to "src/main.rs")

[dependencies]
<crate-name> = { path = "<path>" } # Workspace member dependencies
```

#### Field Constraints

- **package.name**: Must be `"crush-cli"` (kebab-case)
- **bin.name**: Must be `"crush"` (final executable name)
- **bin.path**: Defaults to `"src/main.rs"` (standard convention)
- **dependencies.crush-core**: Must use `{ path = "../crush-core" }` (workspace path dependency)

#### Initial Values

```toml
[package]
name = "crush-cli"
version = "0.1.0"
description = "Command-line interface for Crush compression library"
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "crush"
# path defaults to src/main.rs

[dependencies]
crush-core = { path = "../crush-core" }
```

#### Relationships

- **Member of**: Workspace (referenced in workspace.members)
- **Depends on**: crush-core (workspace path dependency)
- **Produces**: Executable binary named `crush`

---

### 4. Toolchain Configuration

**File**: `rust-toolchain.toml`
**Format**: TOML
**Consumer**: rustup toolchain manager
**Purpose**: Pins the Rust toolchain version for reproducible builds

#### Schema

```toml
[toolchain]
channel = "<string>"               # Toolchain channel ("stable", "nightly", "1.84.0")
components = [<string>, ...]       # Required components (rustfmt, clippy)
targets = [<string>, ...]          # Additional compilation targets (optional)
profile = "<string>"               # Component profile ("minimal", "default")
```

#### Field Constraints

- **channel**: Must be `"stable"` or specific version like `"1.84.0"` (ensures consistency)
- **components**: Must include `["rustfmt", "clippy"]` (quality gates requirement)
- **targets**: Empty initially (uses default host triple)
- **profile**: `"default"` (includes rust-std, rust-docs)

#### Initial Values

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
profile = "default"
```

#### Behavioral Notes

- Rustup automatically installs the specified toolchain when entering the repository directory
- Overrides system-wide default toolchain for this project
- CI/CD environments respect this configuration for consistent builds
- Version pinning (e.g., `"1.84.0"`) can be used to freeze toolchain if needed

#### Relationships

- **Used by**: rustup, cargo, rustfmt, clippy
- **Enforces**: Consistent Rust version across all developer environments and CI

---

### 5. Formatting Configuration

**File**: `rustfmt.toml` (repository root)
**Format**: TOML
**Consumer**: rustfmt code formatter
**Purpose**: Defines code formatting rules for consistent style

#### Schema

```toml
edition = "<string>"               # Rust edition for formatting rules
max_width = <integer>              # Maximum line width in characters
hard_tabs = <boolean>              # Use tabs instead of spaces
tab_spaces = <integer>             # Number of spaces per tab (if hard_tabs=false)
newline_style = "<string>"         # Line ending style ("Auto", "Unix", "Windows")
use_small_heuristics = "<string>"  # Heuristics for formatting ("Default", "Off", "Max")
reorder_imports = <boolean>        # Alphabetically sort use statements
reorder_modules = <boolean>        # Alphabetically sort module declarations
remove_nested_parens = <boolean>   # Simplify unnecessary nested parentheses
```

#### Field Constraints

- **edition**: Must match workspace edition (`"2021"`)
- **max_width**: `100` (balances readability and screen space)
- **hard_tabs**: `false` (spaces preferred in Rust community)
- **tab_spaces**: `4` (Rust standard)
- **newline_style**: `"Auto"` (detects platform-appropriate line endings)

#### Initial Values

```toml
edition = "2021"
max_width = 100
hard_tabs = false
tab_spaces = 4
newline_style = "Auto"
use_small_heuristics = "Default"
reorder_imports = true
reorder_modules = true
remove_nested_parens = true
```

#### Enforcement

- Checked in CI via `cargo fmt --all -- --check` (fails if unformatted)
- Auto-fixable via `cargo fmt --all` (developers run before commit)
- Pre-commit hooks can automate (optional, not required)

#### Relationships

- **Applied to**: All Rust source files (*.rs) in workspace
- **Enforced by**: CI format_check job (feature 002)

---

### 6. Linting Configuration

**File**: `clippy.toml` (repository root)
**Format**: TOML
**Consumer**: clippy linter
**Purpose**: Defines lint rules for code quality enforcement

#### Schema

```toml
# Pedantic lint group (enabled via CLI flag --all-targets -D warnings)
# Configuration options for specific lints:

cognitive-complexity-threshold = <integer>  # Max function complexity
type-complexity-threshold = <integer>       # Max type nesting depth
too-many-arguments-threshold = <integer>    # Max function parameters
```

#### Field Constraints

- **cognitive-complexity-threshold**: `25` (allows moderate complexity)
- **type-complexity-threshold**: `100` (default, rarely needs tuning)
- **too-many-arguments-threshold**: `7` (encourages builder patterns)

#### Initial Values

```toml
# Clippy configuration for Crush project
# Pedantic lints enabled via: cargo clippy --all-targets -- -D warnings

cognitive-complexity-threshold = 25
type-complexity-threshold = 100
too-many-arguments-threshold = 7
```

#### Lint Groups Enabled (via CLI)

- **pedantic**: Opinionated lints for best practices
- **deny**: Unwrap/panic lints (configured separately, not in clippy.toml)

#### Enforcement

- Checked in CI via `cargo clippy --all-targets -- -D warnings -W clippy::pedantic -D clippy::unwrap_used -D clippy::expect_used`
- Zero tolerance for warnings (all lints treated as errors in CI)

#### Relationships

- **Applied to**: All Rust source files in workspace
- **Enforced by**: CI lint job (feature 002)

---

### 7. Version Control Exclusions

**File**: `.gitignore` (repository root)
**Format**: Plain text (glob patterns)
**Consumer**: Git version control
**Purpose**: Excludes build artifacts and temporary files from version control

#### Schema

```
# Pattern format (one per line):
<path>          # Exact path or directory
*.ext           # Wildcard file extension
**/pattern      # Recursive pattern
!exception      # Negation (include despite previous exclude)
```

#### Standard Rust Patterns

```
# Build artifacts
/target/
Cargo.lock      # Lock file excluded for libraries (included for binaries)

# IDE files
.vscode/
.idea/
*.swp
*.swo
*~

# OS files
.DS_Store
Thumbs.db

# Test/coverage artifacts
*.profraw
*.profdata
/target/llvm-cov/

# Backup files
*.bak
*.tmp
```

#### Cargo.lock Policy

- **Libraries (crush-core)**: Excluded (downstream crates generate their own lock)
- **Binaries (crush-cli)**: **Included** (reproducible builds, CI uses exact versions)
- **Workspace root**: Included (workspace is effectively a binary project due to crush-cli)

**Decision**: Include `Cargo.lock` in repository root to ensure reproducible CI builds.

#### Relationships

- **Applied to**: Git add/commit operations
- **Ensures**: Clean repository without build artifacts
- **Coordinates with**: CI workflows (expect clean working directory)

---

## Entity Relationships Diagram

```
rust-toolchain.toml
    ↓ (specifies toolchain for)
Workspace Manifest (Cargo.toml)
    ↓ (contains members)
    ├─→ Library Crate Manifest (crush-core/Cargo.toml)
    │       ↓ (defines)
    │   src/lib.rs (public API)
    │       ↑ (tested by)
    │   tests in #[cfg(test)] modules
    │
    └─→ Binary Crate Manifest (crush-cli/Cargo.toml)
            ↓ (depends on crush-core)
        src/main.rs (CLI entry point)

rustfmt.toml ───→ (formats) ───→ All *.rs files
clippy.toml  ───→ (lints)   ───→ All *.rs files
.gitignore   ───→ (excludes)───→ /target/, artifacts
```

## Validation Rules

### Cross-Entity Consistency

1. **Edition Alignment**: `rust-toolchain.toml` channel must support the edition specified in `workspace.package.edition`
2. **Dependency Coherence**: crush-cli must declare crush-core dependency with correct relative path
3. **Version Synchronization**: All workspace members should share the same version number (0.1.0 initially)
4. **Formatting Consistency**: `rustfmt.toml` edition must match workspace edition

### Compilation Prerequisites

1. All manifests must be valid TOML (syntax check)
2. Workspace members must exist as directories with src/ subdirectories
3. Library crate must have src/lib.rs, binary crate must have src/main.rs
4. Toolchain specified in rust-toolchain.toml must be available via rustup

### Quality Gate Prerequisites

1. All source files must pass `cargo fmt --check` (formatting)
2. All source files must pass `cargo clippy` (linting)
3. All tests must pass `cargo test` (correctness)
4. Documentation must build cleanly `cargo doc` (documentation)

## Future Extensions

### Phase 2+ Enhancements (Out of Scope for This Feature)

1. **Workspace Dependencies**: Shared dependency versions added to `[workspace.dependencies]` when implementing compression algorithms
2. **Benchmark Crate**: New workspace member `benches/` with Criterion integration
3. **Fuzz Crate**: New workspace member `fuzz/` with cargo-fuzz targets
4. **Feature Flags**: Conditional compilation features in crush-core (e.g., `default = ["gzip"]`)
5. **Build Scripts**: `build.rs` for platform-specific compilation (SIMD detection)
6. **Custom Profiles**: Additional profiles like `[profile.bench]` for performance testing

These enhancements will be specified in future feature specifications following the SpecKit workflow.

## References

- Cargo Book: https://doc.rust-lang.org/cargo/reference/
- rustfmt Configuration: https://rust-lang.github.io/rustfmt/
- clippy Lints: https://rust-lang.github.io/rust-clippy/
- Rust Edition Guide: https://doc.rust-lang.org/edition-guide/
- Constitution: `.specify/memory/constitution.md` (dependency policy, quality gates)
