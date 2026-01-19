# Contract: Library Public API (crush-core)

**Feature**: 003-rust-workspace-setup | **Created**: 2026-01-18
**Contract Type**: API Interface
**Crate**: `crush-core`
**Stability**: Unstable (0.1.0 - placeholder implementation)

## Purpose

This contract defines the minimal public API surface for the `crush-core` library crate. The initial implementation contains placeholder code sufficient to validate the build system, documentation generation, and test infrastructure. Future features will replace these placeholders with actual compression functionality following Test-Driven Development (TDD).

## API Surface

### Crate-Level Documentation

**Location**: `crush-core/src/lib.rs` (file header)

**Required Content**:
```rust
//! Crush Core Library
//!
//! High-performance parallel compression library for Rust.
//!
//! This crate provides the core compression and decompression algorithms
//! for the Crush project. It is designed for modularity and extensibility
//! through trait-based interfaces.
//!
//! # Current Status
//!
//! **Version 0.1.0**: Placeholder implementation. This crate contains
//! minimal code to validate project structure. Actual compression
//! functionality will be implemented in future features following TDD.
//!
//! # Examples
//!
//! ```
//! use crush_core::hello;
//! assert_eq!(hello(), "Hello from crush-core!");
//! ```
//!
//! # Architecture
//!
//! Future versions will expose:
//! - Compression engine traits (`CompressionAlgorithm`, `FormatHandler`)
//! - Streaming I/O interfaces (`Reader`, `Writer`)
//! - Block processing primitives
//! - Memory pool management
//! - Plugin system for extensibility
```

**Constraints**:
- Must include crate description and purpose
- Must document current stability status (version 0.1.0)
- Must include at least one working code example
- Must outline future architecture (guideline for developers)

---

### Public Function: `hello()`

**Signature**:
```rust
pub fn hello() -> &'static str
```

**Documentation**:
```rust
/// Returns a greeting message from the crush-core library.
///
/// This is a placeholder function demonstrating the public API structure.
/// It exists to validate:
/// - Documentation builds correctly
/// - Public APIs are exported and discoverable
/// - Tests can call public functions
/// - Examples in doc comments compile and run
///
/// # Examples
///
/// ```
/// use crush_core::hello;
/// let message = hello();
/// assert_eq!(message, "Hello from crush-core!");
/// assert!(!message.is_empty());
/// ```
///
/// # Future Replacement
///
/// This function will be removed in version 0.2.0 when actual compression
/// APIs are implemented. Do not depend on this function in production code.
pub fn hello() -> &'static str {
    "Hello from crush-core!"
}
```

**Behavior**:
- **Input**: None (no parameters)
- **Output**: Static string slice containing "Hello from crush-core!"
- **Side Effects**: None (pure function)
- **Panics**: Never
- **Errors**: N/A (infallible function)
- **Thread Safety**: Yes (immutable static data)

**Implementation**:
```rust
pub fn hello() -> &'static str {
    "Hello from crush-core!"
}
```

**Test Requirements**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_returns_expected_message() {
        assert_eq!(hello(), "Hello from crush-core!");
    }

    #[test]
    fn test_hello_message_not_empty() {
        assert!(!hello().is_empty());
    }

    #[test]
    fn test_hello_message_contains_crate_name() {
        assert!(hello().contains("crush-core"));
    }
}
```

**Coverage Target**: 100% (trivial function, easy to achieve >80% requirement)

---

## Complete lib.rs Implementation

```rust
//! Crush Core Library
//!
//! High-performance parallel compression library for Rust.
//!
//! This crate provides the core compression and decompression algorithms
//! for the Crush project. It is designed for modularity and extensibility
//! through trait-based interfaces.
//!
//! # Current Status
//!
//! **Version 0.1.0**: Placeholder implementation. This crate contains
//! minimal code to validate project structure. Actual compression
//! functionality will be implemented in future features following TDD.
//!
//! # Examples
//!
//! ```
//! use crush_core::hello;
//! assert_eq!(hello(), "Hello from crush-core!");
//! ```
//!
//! # Architecture
//!
//! Future versions will expose:
//! - Compression engine traits (`CompressionAlgorithm`, `FormatHandler`)
//! - Streaming I/O interfaces (`Reader`, `Writer`)
//! - Block processing primitives
//! - Memory pool management
//! - Plugin system for extensibility

/// Returns a greeting message from the crush-core library.
///
/// This is a placeholder function demonstrating the public API structure.
/// It exists to validate:
/// - Documentation builds correctly
/// - Public APIs are exported and discoverable
/// - Tests can call public functions
/// - Examples in doc comments compile and run
///
/// # Examples
///
/// ```
/// use crush_core::hello;
/// let message = hello();
/// assert_eq!(message, "Hello from crush-core!");
/// assert!(!message.is_empty());
/// ```
///
/// # Future Replacement
///
/// This function will be removed in version 0.2.0 when actual compression
/// APIs are implemented. Do not depend on this function in production code.
pub fn hello() -> &'static str {
    "Hello from crush-core!"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_returns_expected_message() {
        assert_eq!(hello(), "Hello from crush-core!");
    }

    #[test]
    fn test_hello_message_not_empty() {
        assert!(!hello().is_empty());
    }

    #[test]
    fn test_hello_message_contains_crate_name() {
        assert!(hello().contains("crush-core"));
    }
}
```

---

## Future API Evolution (Out of Scope)

This section outlines the planned API surface for reference. **NOT IMPLEMENTED** in this feature.

### Trait: `CompressionAlgorithm`

```rust
/// Trait defining a compression algorithm.
///
/// Implementors provide compression and decompression logic for a specific
/// algorithm (e.g., DEFLATE, Brotli, Zstandard).
pub trait CompressionAlgorithm {
    /// Algorithm identifier (e.g., "deflate", "brotli")
    fn name(&self) -> &str;

    /// Compress data from input buffer to output buffer
    fn compress(&self, input: &[u8], output: &mut Vec<u8>) -> Result<usize>;

    /// Decompress data from input buffer to output buffer
    fn decompress(&self, input: &[u8], output: &mut Vec<u8>) -> Result<usize>;
}
```

### Error Type: `CrushError`

```rust
/// Error types for compression operations.
#[derive(Debug, thiserror::Error)]
pub enum CrushError {
    /// I/O operation failed
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Compression operation failed
    #[error("Compression failed: {0}")]
    Compression(String),

    /// Decompression operation failed
    #[error("Decompression failed: {0}")]
    Decompression(String),

    /// Invalid input format
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

/// Result type alias using CrushError
pub type Result<T> = std::result::Result<T, CrushError>;
```

### Module Structure (Future)

```
crush-core/src/
├── lib.rs              # Crate root, re-exports public API
├── error.rs            # Error types (CrushError, Result)
├── engine.rs           # Compression engine coordination
├── algorithm/          # Algorithm implementations
│   ├── mod.rs          # CompressionAlgorithm trait
│   ├── deflate.rs      # DEFLATE implementation
│   └── null.rs         # Null compression (testing)
├── stream.rs           # Streaming I/O wrappers
├── block.rs            # Block processing utilities
├── pool.rs             # Memory pool management
└── plugins/            # Plugin system (future)
    └── mod.rs
```

---

## Validation Requirements

### Compilation

```bash
# Library must compile successfully
cargo build -p crush-core

# Expected: 0 errors, 0 warnings
```

### Documentation

```bash
# Documentation must build without warnings
cargo doc -p crush-core --no-deps

# Expected: HTML docs in target/doc/crush_core/
# Verify docs render correctly: open target/doc/crush_core/index.html
```

### Tests

```bash
# All tests must pass
cargo test -p crush-core

# Expected output:
# running 3 tests
# test tests::test_hello_returns_expected_message ... ok
# test tests::test_hello_message_not_empty ... ok
# test tests::test_hello_message_contains_crate_name ... ok
# test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
```

### Doc Tests

```bash
# Doc-test examples must compile and run
cargo test -p crush-core --doc

# Expected output:
# running 1 test
# test src/lib.rs - hello (line X) ... ok
# test result: ok. 1 passed; 0 failed; 0 ignored
```

### Code Coverage

```bash
# Coverage must exceed 80%
cargo llvm-cov --package crush-core --html

# Expected: >80% line coverage (100% achievable with minimal code)
# Report: target/llvm-cov/html/index.html
```

### Linting

```bash
# No clippy warnings allowed
cargo clippy -p crush-core --all-targets -- -D warnings

# Expected: 0 warnings (pedantic lints pass)
```

### Formatting

```bash
# Code must be formatted correctly
cargo fmt -p crush-core -- --check

# Expected: no diffs, exit code 0
```

---

## Usage Examples

### Basic Import

```rust
use crush_core::hello;

fn main() {
    println!("{}", hello());
    // Output: Hello from crush-core!
}
```

### Testing Against the API

```rust
#[test]
fn test_library_available() {
    use crush_core::hello;
    let message = hello();
    assert!(!message.is_empty());
}
```

### Documentation Generation

```bash
# Generate and open documentation
cargo doc -p crush-core --no-deps --open
```

Expected: Browser opens showing crate documentation with:
- Crate-level module docs
- `hello()` function documentation with examples
- Search functionality
- Source code links

---

## Breaking Changes Policy

### Version 0.1.0 (This Feature)

- Placeholder implementation
- No stability guarantees
- API will be completely replaced in 0.2.0

### Version 0.2.0+ (Future)

- Introduce real compression APIs
- Remove `hello()` placeholder function
- Establish semver stability guarantees
- Follow Rust API Guidelines: https://rust-lang.github.io/api-guidelines/

### Deprecation Strategy

When replacing placeholder functions:

1. Mark as deprecated in 0.1.1:
   ```rust
   #[deprecated(since = "0.1.1", note = "Use compression APIs instead")]
   pub fn hello() -> &'static str { ... }
   ```

2. Remove in 0.2.0 (breaking change, major version bump)

---

## Constitution Compliance

### Principle II: Correctness & Safety

- ✅ No unsafe code in placeholder implementation
- ✅ No `.unwrap()` or `.expect()` calls
- ✅ Function is infallible (cannot panic)
- ✅ Pure function (no mutable state)

### Principle III: Modularity & Extensibility

- ✅ Library crate separated from binary crate
- ✅ Public API clearly defined
- ✅ Future trait-based design documented

### Principle IV: Test-First Development

- ✅ Tests written for all public functions
- ✅ Doc-test examples provided
- ✅ 100% code coverage achievable

### Rust Style & Standards

- ✅ All public items documented with `///` comments
- ✅ Examples included in doc comments
- ✅ Documentation sections: Examples, Errors (if applicable), Panics (if applicable)

---

## Integration Points

### crush-cli Integration

The CLI crate will import and use this library:

```rust
// In crush-cli/src/main.rs
use crush_core::hello;

fn main() {
    println!("{}", hello());
}
```

Dependency declared in `crush-cli/Cargo.toml`:
```toml
[dependencies]
crush-core = { path = "../crush-core" }
```

### CI/CD Integration

GitHub Actions workflows (feature 002) will:
- Build the library: `cargo build -p crush-core`
- Run tests: `cargo test -p crush-core`
- Generate coverage: `cargo llvm-cov --package crush-core`
- Lint code: `cargo clippy -p crush-core`
- Build docs: `cargo doc -p crush-core --no-deps`

---

## Success Criteria

A valid library API implementation must:

1. ✅ Export at least one public function (`hello()`)
2. ✅ Compile without errors or warnings
3. ✅ Pass all unit tests (3+ tests)
4. ✅ Pass all doc-tests (1+ example)
5. ✅ Achieve >80% code coverage
6. ✅ Generate documentation without warnings
7. ✅ Pass clippy pedantic lints
8. ✅ Follow rustfmt formatting rules
9. ✅ Be importable by crush-cli crate
10. ✅ Align with constitution principles

---

## References

- Rust API Guidelines: https://rust-lang.github.io/api-guidelines/
- Rust Doc Book: https://doc.rust-lang.org/rustdoc/
- Cargo Book - Library: https://doc.rust-lang.org/cargo/reference/cargo-targets.html#library
- Constitution: `.specify/memory/constitution.md` (quality gates, safety requirements)
