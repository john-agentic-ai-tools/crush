# Contract: Workspace Manifest Schema

**Feature**: 003-rust-workspace-setup | **Created**: 2026-01-18
**Contract Type**: Configuration Schema
**File**: `Cargo.toml` (repository root)

## Purpose

This contract defines the complete schema and requirements for the workspace-level Cargo.toml manifest. This file serves as the single source of truth for workspace configuration, member crate coordination, and shared build settings.

## Schema Specification

### Complete Workspace Manifest

```toml
[workspace]
resolver = "2"
members = [
    "crush-core",
    "crush-cli",
]

[workspace.package]
edition = "2021"
authors = ["Crush Contributors"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/your-org/crush"
homepage = "https://github.com/your-org/crush"

[workspace.dependencies]
# Shared dependency specifications (empty initially)
# Future additions will define versions once:
# rayon = "1.10"
# flate2 = "1.0"
# thiserror = "2.0"

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

## Section Requirements

### [workspace]

**Purpose**: Defines workspace membership and dependency resolution strategy.

**Required Fields**:
- `resolver`: Must be `"2"` (latest resolver, required for consistent feature resolution)
- `members`: Array of workspace member crate paths (relative to workspace root)

**Constraints**:
- Members list must include all crate directories containing Cargo.toml files
- Member paths must be relative (no absolute paths)
- Member paths must exist as directories
- Resolver "2" enables improved feature unification and target-specific dependencies

**Initial Value**:
```toml
[workspace]
resolver = "2"
members = ["crush-core", "crush-cli"]
```

**Future Evolution**: Additional members will be added incrementally:
- `"benches"` - Criterion benchmark suite
- `"fuzz"` - cargo-fuzz targets
- Platform-specific crates if needed

---

### [workspace.package]

**Purpose**: Defines shared package metadata inherited by all workspace members.

**Required Fields**:
- `edition`: Rust edition year (`"2021"` - current stable edition)
- `authors`: Array of author strings
- `license`: SPDX license identifier (dual license preferred for Rust projects)
- `repository`: Git repository URL
- `homepage`: Project website URL (can match repository)

**Constraints**:
- Edition must be supported by toolchain specified in rust-toolchain.toml
- License must be valid SPDX identifier
- URLs must be valid HTTP/HTTPS URLs

**Initial Value**:
```toml
[workspace.package]
edition = "2021"
authors = ["Crush Contributors"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/your-org/crush"
homepage = "https://github.com/your-org/crush"
```

**Inheritance Mechanism**: Member crates inherit these values using:
```toml
# In member Cargo.toml:
[package]
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
```

---

### [workspace.dependencies]

**Purpose**: Centralized dependency version management (DRY principle).

**Format**:
```toml
[workspace.dependencies]
<crate-name> = "<version>"
<crate-name> = { version = "<version>", features = [...] }
```

**Initial State**: Empty (no dependencies in initial implementation).

**Future Population** (when implementing compression features):
```toml
[workspace.dependencies]
# Parallel processing (constitution approved)
rayon = "1.10"

# DEFLATE compression (Phase 1, approved)
flate2 = "1.0"

# Error handling (approved)
thiserror = "2.0"

# Checksums (approved)
crc32fast = "1.4"

# Memory-mapped files (approved)
memmap2 = "0.9"

# CLI parsing (CLI crate only, approved)
clap = { version = "4.5", features = ["derive"] }

# Benchmarking (dev-dependency, approved)
criterion = "0.5"
```

**Inheritance Mechanism**: Member crates reference workspace dependencies:
```toml
# In member Cargo.toml:
[dependencies]
rayon.workspace = true
flate2.workspace = true
```

**Governance**: All dependency additions must be justified against constitution's allowed dependencies list.

---

### [profile.dev]

**Purpose**: Development build profile (optimized for compilation speed).

**Required Fields**:
- `opt-level`: Optimization level (0 = no optimization, fastest compilation)
- `debug`: Debug symbols inclusion (true = full debug info)

**Constraints**:
- Optimization level 0 minimizes incremental build times
- Debug symbols required for debugger integration and panic backtraces

**Initial Value**:
```toml
[profile.dev]
opt-level = 0
debug = true
```

**Rationale**: Developer experience prioritized - fast feedback loop is critical for TDD workflow.

---

### [profile.release]

**Purpose**: Production release profile (optimized for runtime performance).

**Required Fields**:
- `opt-level`: Optimization level (3 = maximum optimization)
- `lto`: Link-time optimization strategy
- `codegen-units`: Parallel code generation units
- `strip`: Symbol stripping for smaller binaries

**Constraints**:
- `opt-level = 3`: Constitution mandates performance-first (Principle I)
- `lto = "thin"`: Balances link time vs. runtime performance (full LTO too slow)
- `codegen-units = 1`: Maximizes optimization opportunities (single monolithic codegen)
- `strip = true`: Removes debug symbols for production (smaller binary size)

**Initial Value**:
```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
strip = true
```

**Performance Target**: Must achieve performance goals specified in constitution (match/exceed pigz).

**Future Tuning**: May add profile-specific settings for advanced optimizations:
- `panic = "abort"` - Smaller binary, no unwinding overhead
- `overflow-checks = false` - Slightly faster (risky, needs benchmarking)

---

### [profile.test]

**Purpose**: Test execution profile (inherits from dev for fast iteration).

**Required Fields**:
- `inherits`: Base profile to copy settings from

**Constraints**:
- Must inherit from dev profile (fast compilation for TDD)
- Can override specific settings if test-specific optimization needed

**Initial Value**:
```toml
[profile.test]
inherits = "dev"
```

**Rationale**: Tests prioritize fast feedback over runtime performance. Constitution requires TDD, so test compilation speed is critical.

**Future Tuning**: May add overrides if test performance becomes bottleneck:
- `opt-level = 1` - Slightly faster test execution without slow compilation

---

## Validation Rules

### Syntax Validation

```bash
# Verify TOML is well-formed
cargo metadata --format-version 1 > /dev/null
```

Expected: Command succeeds, no parse errors.

### Semantic Validation

```bash
# Verify workspace members exist and are buildable
cargo build --workspace
```

Expected: All members compile successfully.

### Consistency Validation

1. **Resolver-Edition Compatibility**: Resolver "2" requires edition 2021 (compatible with Rust 1.51+)
2. **Member Path Existence**: All paths in `members` array must exist as directories with Cargo.toml
3. **Profile Inheritance**: Test profile inherits from dev (verify `inherits` field is valid)

## Examples

### Minimal Valid Workspace

```toml
[workspace]
resolver = "2"
members = ["crush-core", "crush-cli"]
```

This is the absolute minimum required for a functional workspace (though shared metadata is strongly recommended).

### With Shared Dependencies (Future)

```toml
[workspace]
resolver = "2"
members = ["crush-core", "crush-cli", "benches"]

[workspace.package]
edition = "2021"
authors = ["Crush Contributors"]
license = "MIT OR Apache-2.0"

[workspace.dependencies]
rayon = "1.10"
flate2 = { version = "1.0", features = ["zlib"] }
criterion = "0.5"

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
```

### Invalid Examples (Must Reject)

```toml
# INVALID: Resolver version 1 (old, inconsistent features)
[workspace]
resolver = "1"  # ❌ Must use "2"
members = ["crush-core"]

# INVALID: Missing edition field
[workspace.package]
authors = ["Someone"]  # ❌ Edition required for consistency

# INVALID: Absolute path in members
[workspace]
members = ["/home/user/crush-core"]  # ❌ Must be relative

# INVALID: Referencing non-existent profile
[profile.test]
inherits = "nonexistent"  # ❌ Must inherit from valid profile
```

## Change Management

### Adding New Members

When adding workspace members (e.g., benchmark crate):

1. Create crate directory: `benches/`
2. Add crate Cargo.toml: `benches/Cargo.toml`
3. Update workspace members list:
   ```toml
   members = ["crush-core", "crush-cli", "benches"]
   ```
4. Verify with: `cargo build --workspace`

### Adding Shared Dependencies

When adding new shared dependencies:

1. Verify dependency is constitution-approved
2. Add to `[workspace.dependencies]`:
   ```toml
   [workspace.dependencies]
   new-crate = "1.0"
   ```
3. Update member crates to use workspace version:
   ```toml
   # In member Cargo.toml:
   [dependencies]
   new-crate.workspace = true
   ```
4. Verify with: `cargo check --workspace`

### Profile Modifications

When tuning profiles:

1. Benchmark before and after (constitution requires no performance regression)
2. Document rationale in commit message
3. Verify all builds succeed: `cargo build --workspace --release`
4. Run full test suite: `cargo test --workspace --release`

## Integration Points

### CI/CD Integration (Feature 002)

The workspace manifest is consumed by GitHub Actions workflows:

- **Build job**: `cargo build --workspace`
- **Test job**: `cargo test --workspace`
- **Lint job**: `cargo clippy --workspace --all-targets -- -D warnings`
- **Format job**: `cargo fmt --all -- --check`

All jobs respect workspace profile settings and member declarations.

### Tooling Integration

- **cargo-nextest**: Reads workspace manifest to discover test binaries
- **cargo-llvm-cov**: Instruments workspace members for coverage
- **cargo-deny**: Validates dependencies against policy (.cargo/deny.toml)
- **rustfmt**: Applies formatting to all workspace members
- **clippy**: Lints all workspace members

## Success Criteria

A valid workspace manifest must satisfy:

1. ✅ Parses successfully: `cargo metadata` exits 0
2. ✅ All members build: `cargo build --workspace` exits 0
3. ✅ Consistent editions: All members inherit workspace edition 2021
4. ✅ Resolver version 2: Enables consistent feature resolution
5. ✅ Profiles defined: dev, release, test profiles present
6. ✅ Release optimized: opt-level 3, LTO thin, single codegen unit
7. ✅ Metadata complete: authors, license, repository fields populated

## References

- Cargo Book - Workspaces: https://doc.rust-lang.org/cargo/reference/workspaces.html
- Cargo Book - Profiles: https://doc.rust-lang.org/cargo/reference/profiles.html
- Cargo Book - Resolver: https://doc.rust-lang.org/cargo/reference/resolver.html
- Constitution: `.specify/memory/constitution.md` (dependency policy)
