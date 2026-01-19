# Quickstart: Local Testing Guide

**Feature**: 003-rust-workspace-setup | **Created**: 2026-01-18
**Purpose**: Step-by-step validation of Rust workspace implementation

This guide provides comprehensive local testing procedures to validate the Rust workspace structure before pushing to CI. Follow these steps sequentially to ensure the workspace meets all quality gates and success criteria.

## Prerequisites

### Required Tools

- **Rust Toolchain**: Installed via rustup (https://rustup.rs/)
- **Cargo**: Bundled with Rust
- **Git**: For version control operations
- **cargo-nextest**: Optional but recommended (`cargo install cargo-nextest`)
- **cargo-llvm-cov**: For coverage measurement (`cargo install cargo-llvm-cov`)

### Verification

```bash
# Verify Rust installation
rustc --version
# Expected: rustc 1.84.0 (or version specified in rust-toolchain.toml)

cargo --version
# Expected: cargo 1.84.0 (or matching Rust version)

# Verify optional tools
cargo nextest --version
# Expected: cargo-nextest 0.9.x (if installed)

cargo llvm-cov --version
# Expected: cargo-llvm-cov 0.6.x (if installed)
```

---

## Test Workflow

### Step 1: Clean Build Test

**Purpose**: Validates workspace compiles successfully from scratch (SC-001, SC-002)

**Commands**:
```bash
# Navigate to repository root
cd C:\Users\Admin\code\crush

# Remove any previous build artifacts
cargo clean

# Perform clean build with timing
time cargo build

# Alternative on Windows (PowerShell)
Measure-Command { cargo build }
```

**Expected Output**:
```
   Compiling crush-core v0.1.0 (C:\Users\Admin\code\crush\crush-core)
   Compiling crush-cli v0.1.0 (C:\Users\Admin\code\crush\crush-cli)
    Finished dev [unoptimized + debuginfo] target(s) in 2.45s
```

**Success Criteria**:
- ✅ Exit code 0 (no errors)
- ✅ Both crates compile successfully
- ✅ Build time < 30 seconds (excluding dependency downloads)
- ✅ Zero compiler warnings

**Troubleshooting**:
- **Error**: "package not found" → Verify Cargo.toml members list
- **Warning**: "unused variable" → Fix code or add `#[allow(unused)]` temporarily
- **Slow build**: First build downloads dependencies (expected), subsequent builds should be fast

---

### Step 2: Quality Gates - Formatting

**Purpose**: Validates code formatting compliance (SC-005)

**Commands**:
```bash
# Check formatting without modifying files
cargo fmt --all -- --check

# If formatting issues found, auto-fix with:
cargo fmt --all
```

**Expected Output (Passing)**:
```
# No output = all files correctly formatted
```

**Expected Output (Failing)**:
```
Diff in C:\Users\Admin\code\crush\crush-core\src\lib.rs at line 42:
 pub fn hello() -> &'static str {
-    "Hello from crush-core!"
+     "Hello from crush-core!"
 }
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ No formatting diffs reported

**Troubleshooting**:
- **Formatting diffs found**: Run `cargo fmt --all` to auto-fix
- **Unexpected format**: Verify rustfmt.toml configuration
- **Editor conflicts**: Configure editor to respect rustfmt.toml

---

### Step 3: Quality Gates - Linting

**Purpose**: Validates code quality and best practices (SC-004)

**Commands**:
```bash
# Run clippy with pedantic lints and deny warnings
cargo clippy --all-targets -- -D warnings -W clippy::pedantic -D clippy::unwrap_used -D clippy::expect_used

# Shortened version (if clippy.toml is configured)
cargo clippy --all-targets -- -D warnings
```

**Expected Output**:
```
    Checking crush-core v0.1.0 (C:\Users\Admin\code\crush\crush-core)
    Checking crush-cli v0.1.0 (C:\Users\Admin\code\crush\crush-cli)
    Finished dev [unoptimized + debuginfo] target(s) in 1.23s
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ Zero warnings reported
- ✅ No pedantic lint violations
- ✅ No unwrap/expect/panic usage detected

**Common Warnings & Fixes**:

```rust
// Warning: missing documentation for public function
// Fix: Add doc comment
/// Returns a greeting message.
pub fn hello() -> &'static str { ... }

// Warning: needless_pass_by_value
// Fix: Use reference if not consuming
pub fn process(data: &str) -> String { ... }

// Warning: must_use_candidate
// Fix: Add #[must_use] attribute
#[must_use]
pub fn hello() -> &'static str { ... }
```

**Troubleshooting**:
- **Many warnings**: Fix incrementally, commit often
- **Pedantic too strict**: Justify exceptions in code review
- **False positives**: Use `#[allow(clippy::lint_name)]` with justification comment

---

### Step 4: Compilation with Warnings Check

**Purpose**: Validates zero compiler warnings (SC-003)

**Commands**:
```bash
# Build with all warnings as errors
cargo build --all-targets

# Check output for warning count
```

**Expected Output**:
```
   Compiling crush-core v0.1.0 (C:\Users\Admin\code\crush\crush-core)
   Compiling crush-cli v0.1.0 (C:\Users\Admin\code\crush\crush-cli)
    Finished dev [unoptimized + debuginfo] target(s) in 2.15s
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ No warnings printed (search output for "warning:")
- ✅ All targets build (lib, bin, tests)

**Troubleshooting**:
- **Dead code warnings**: Add `#[cfg(test)]` for test helpers or use in tests
- **Unused imports**: Remove or conditionally compile with `#[cfg(test)]`
- **Deprecated API**: Update to recommended alternative

---

### Step 5: Test Execution - Cargo Test

**Purpose**: Validates all tests pass (SC-006)

**Commands**:
```bash
# Run all tests in workspace
cargo test

# Verbose output (shows test names)
cargo test -- --nocapture

# Run tests for specific crate
cargo test -p crush-core
cargo test -p crush-cli
```

**Expected Output**:
```
   Compiling crush-core v0.1.0 (C:\Users\Admin\code\crush\crush-core)
   Compiling crush-cli v0.1.0 (C:\Users\Admin\code\crush\crush-cli)
    Finished test [unoptimized + debuginfo] target(s) in 2.89s
     Running unittests src\lib.rs (target\debug\deps\crush_core-xxx.exe)

running 3 tests
test tests::test_hello_returns_expected_message ... ok
test tests::test_hello_message_not_empty ... ok
test tests::test_hello_message_contains_crate_name ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

     Running unittests src\main.rs (target\debug\deps\crush_cli-xxx.exe)

running 1 test
test tests::test_main_does_not_panic ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

   Doc-tests crush-core

running 1 test
test src\lib.rs - hello (line 20) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ All tests pass (no failures)
- ✅ At least 3 tests in crush-core
- ✅ At least 1 test in crush-cli
- ✅ Doc-tests pass (1+ example tested)

**Troubleshooting**:
- **Test failure**: Read assertion message, fix implementation or test
- **Test panic**: Check for unwrap/expect, add proper error handling
- **Doc-test failure**: Verify example code compiles and assertions hold

---

### Step 6: Test Execution - Nextest

**Purpose**: Validates integration with cargo-nextest from feature 002

**Commands**:
```bash
# Run tests using nextest with CI profile
cargo nextest run --profile ci

# If nextest not installed:
cargo install cargo-nextest
```

**Expected Output**:
```
    Finished test [unoptimized + debuginfo] target(s) in 0.12s
------------
     Summary [   2.456s] 4 tests run: 4 passed, 1 skipped
        PASS [   0.234s] crush-core::tests::test_hello_returns_expected_message
        PASS [   0.156s] crush-core::tests::test_hello_message_not_empty
        PASS [   0.123s] crush-core::tests::test_hello_message_contains_crate_name
        PASS [   0.089s] crush-cli::tests::test_main_does_not_panic
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ All tests pass
- ✅ Nextest respects .config/nextest.toml settings

**Note**: Nextest does not run doc-tests by default. Use `cargo test --doc` separately.

**Troubleshooting**:
- **Nextest not found**: Install with `cargo install cargo-nextest`
- **Profile not found**: Verify .config/nextest.toml exists (feature 002)
- **Slower than cargo test**: Expected, nextest runs tests in parallel

---

### Step 7: Code Coverage Measurement

**Purpose**: Validates coverage exceeds 80% (SC-009)

**Commands**:
```bash
# Generate coverage report
cargo llvm-cov --html

# Open report in browser
# Windows
start target/llvm-cov/html/index.html

# Linux/Mac
open target/llvm-cov/html/index.html
```

**Expected Output**:
```
    Finished test [unoptimized + debuginfo] target(s) in 3.45s
     Running unittests src\lib.rs (target\debug\deps\crush_core-xxx.exe)

running 3 tests
test tests::test_hello_returns_expected_message ... ok
test tests::test_hello_message_not_empty ... ok
test tests::test_hello_message_contains_crate_name ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

Coverage report generated: target/llvm-cov/html/index.html
Overall coverage: 100.00% (12/12 lines)
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ Coverage >= 80% (typically 100% for minimal code)
- ✅ HTML report generated successfully

**Coverage Report Analysis**:
- **Green lines**: Covered by tests
- **Red lines**: Not covered (requires additional tests)
- **Yellow lines**: Partially covered (branches)

**Troubleshooting**:
- **llvm-cov not found**: Install with `cargo install cargo-llvm-cov`
- **Low coverage**: Add tests for uncovered functions
- **Report not opening**: Check file path `target/llvm-cov/html/index.html`

---

### Step 8: Documentation Build

**Purpose**: Validates documentation builds cleanly (SC-007)

**Commands**:
```bash
# Build documentation without dependencies
cargo doc --no-deps

# Build and open in browser
cargo doc --no-deps --open
```

**Expected Output**:
```
 Documenting crush-core v0.1.0 (C:\Users\Admin\code\crush\crush-core)
 Documenting crush-cli v0.1.0 (C:\Users\Admin\code\crush\crush-cli)
    Finished dev [unoptimized + debuginfo] target(s) in 3.21s
   Generated target\doc\crush_core\index.html
   Generated target\doc\crush_cli\index.html
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ Zero warnings (search for "warning:")
- ✅ HTML docs generated in target/doc/
- ✅ All public items documented

**Documentation Checklist**:
- [ ] Crate-level docs (`//!`) present in lib.rs and main.rs
- [ ] All public functions have `///` comments
- [ ] Examples included in doc comments
- [ ] Examples compile (tested via doc-tests)
- [ ] No broken intra-doc links

**Troubleshooting**:
- **Warning: missing docs**: Add doc comments to public items
- **Warning: broken link**: Fix `[link]` references
- **Docs not generated**: Check for syntax errors in doc comments

---

### Step 9: CLI Execution Test

**Purpose**: Validates CLI binary runs successfully (US1 Acceptance Scenario 2)

**Commands**:
```bash
# Run via cargo
cargo run --bin crush

# Run compiled binary directly
.\target\debug\crush

# Verify exit code
echo $?  # Linux/Mac
echo $LASTEXITCODE  # Windows PowerShell
```

**Expected Output**:
```
Crush CLI - Version 0.1.0
High-performance parallel compression tool.

This is a placeholder implementation.
Future versions will support compression operations.

Library says: Hello from crush-core!
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ No panic or crash
- ✅ Output printed to stdout
- ✅ Demonstrates crush-core integration

**Verification**:
```bash
# Verify library integration
cargo run --bin crush | grep "Hello from crush-core"
# Expected: Match found (exit code 0)
```

**Troubleshooting**:
- **Binary not found**: Run `cargo build` first
- **Panic**: Check main.rs for unwrap/expect calls
- **No output**: Verify println! statements in main()

---

### Step 10: Release Build Validation

**Purpose**: Validates optimized release build compiles

**Commands**:
```bash
# Build in release mode
cargo build --release

# Check binary size
ls -lh target/release/crush      # Linux/Mac
dir target\release\crush.exe     # Windows

# Run release binary
.\target\release\crush
```

**Expected Output**:
```
   Compiling crush-core v0.1.0 (C:\Users\Admin\code\crush\crush-core)
   Compiling crush-cli v0.1.0 (C:\Users\Admin\code\crush\crush-cli)
    Finished release [optimized] target(s) in 15.67s
```

**Success Criteria**:
- ✅ Exit code 0
- ✅ Release binary smaller than debug binary
- ✅ Binary executes successfully
- ✅ LTO and optimizations applied

**Binary Size Expectations**:
- **Debug**: ~3-5 MB (includes debug symbols)
- **Release**: ~1-2 MB (stripped, optimized)

**Troubleshooting**:
- **Large binary**: Verify `strip = true` in profile.release
- **Slow build**: Release builds slower (expected)
- **Runtime errors**: Test release builds before deployment

---

### Step 11: Workspace Structure Validation

**Purpose**: Validates directory structure matches constitution (SC-010)

**Commands**:
```bash
# List workspace structure
tree -L 2  # Linux/Mac
tree /F /A  # Windows

# Verify expected files exist
ls Cargo.toml rust-toolchain.toml rustfmt.toml clippy.toml .gitignore
ls crush-core/Cargo.toml crush-core/src/lib.rs
ls crush-cli/Cargo.toml crush-cli/src/main.rs
```

**Expected Structure**:
```
crush/
├── Cargo.toml                   ✅
├── rust-toolchain.toml          ✅
├── rustfmt.toml                 ✅
├── clippy.toml                  ✅
├── .gitignore                   ✅
├── crush-core/
│   ├── Cargo.toml               ✅
│   └── src/
│       └── lib.rs               ✅
├── crush-cli/
│   ├── Cargo.toml               ✅
│   └── src/
│       └── main.rs              ✅
└── target/                      (gitignored)
```

**Success Criteria**:
- ✅ All configuration files present
- ✅ Both crate directories exist
- ✅ Source files in correct locations
- ✅ Structure matches CLAUDE.md specification

**Validation Script** (PowerShell):
```powershell
# Check all required files exist
$requiredFiles = @(
    "Cargo.toml",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "clippy.toml",
    ".gitignore",
    "crush-core/Cargo.toml",
    "crush-core/src/lib.rs",
    "crush-cli/Cargo.toml",
    "crush-cli/src/main.rs"
)

foreach ($file in $requiredFiles) {
    if (Test-Path $file) {
        Write-Host "✅ $file"
    } else {
        Write-Host "❌ $file MISSING"
    }
}
```

---

### Step 12: Git Status Check

**Purpose**: Validates clean working directory for CI

**Commands**:
```bash
# Check git status
git status

# Verify no untracked build artifacts
git status --ignored
```

**Expected Output**:
```
On branch 003-rust-workspace-setup
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        new file:   Cargo.toml
        new file:   rust-toolchain.toml
        new file:   rustfmt.toml
        new file:   clippy.toml
        new file:   .gitignore
        new file:   crush-core/Cargo.toml
        new file:   crush-core/src/lib.rs
        new file:   crush-cli/Cargo.toml
        new file:   crush-cli/src/main.rs

Untracked files:
  (none)

Ignored files:
  target/
```

**Success Criteria**:
- ✅ All source files staged
- ✅ No untracked configuration files
- ✅ Build artifacts properly ignored (target/)
- ✅ No accidental commits of Cargo.lock (for libraries)

**Troubleshooting**:
- **Untracked files**: Stage with `git add <file>`
- **target/ not ignored**: Verify .gitignore includes `/target/`
- **Cargo.lock committed**: For workspace, include it (contains binary)

---

### Step 13: CI Validation (Local Simulation)

**Purpose**: Simulates CI environment before pushing (SC-008)

**Commands**:
```bash
# Simulate CI workflow locally
cargo clean
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
cargo doc --no-deps

# Or use a script (create .specify/scripts/validate.sh)
chmod +x .specify/scripts/validate.sh
./.specify/scripts/validate.sh
```

**Validation Script** (Bash):
```bash
#!/bin/bash
set -e  # Exit on first error

echo "🔍 Running CI validation checks..."

echo "📝 Checking formatting..."
cargo fmt --all -- --check

echo "🔬 Running clippy..."
cargo clippy --all-targets -- -D warnings

echo "🏗️ Building workspace..."
cargo build --workspace

echo "🧪 Running tests..."
cargo test --workspace

echo "📚 Building documentation..."
cargo doc --no-deps

echo "✅ All CI checks passed!"
```

**Success Criteria**:
- ✅ All commands exit with code 0
- ✅ No errors or warnings reported
- ✅ Equivalent to CI workflow (feature 002)

---

### Step 14: CI Validation (Actual GitHub Actions)

**Purpose**: Validates CI pipeline runs successfully (SC-008)

**Commands**:
```bash
# Commit changes
git add .
git commit -m "feat: implement Rust workspace structure

- Create workspace Cargo.toml with crush-core and crush-cli members
- Add rust-toolchain.toml pinning Rust stable
- Configure rustfmt.toml and clippy.toml
- Implement minimal crush-core library with placeholder function
- Implement minimal crush-cli binary demonstrating library integration
- Add comprehensive tests and documentation
- Achieve >80% code coverage

Validates feature 002 CI/CD pipeline with real Rust code.

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"

# Push to remote
git push origin 003-rust-workspace-setup

# Create pull request
gh pr create --title "Feature 003: Rust Workspace Setup" --body "..."

# Monitor CI results
gh pr checks
```

**Expected CI Jobs** (from feature 002):
- ✅ `format_check` - Formatting validation
- ✅ `lint` - Clippy linting
- ✅ `build_matrix` - Multi-platform builds (Linux, Windows, macOS)
- ✅ `test` - Test execution with coverage
- ✅ `security_audit` - Dependency security scan

**Success Criteria**:
- ✅ All CI jobs pass (green checkmarks)
- ✅ No warnings or errors in CI logs
- ✅ Coverage report shows >80%
- ✅ Multi-platform builds succeed

**Monitoring**:
```bash
# Watch CI status
gh pr checks --watch

# View detailed logs
gh run view --log

# Check specific job
gh run view --job=<job-id> --log
```

**Troubleshooting**:
- **CI fails but local passes**: Check for environment differences (paths, OS)
- **Timeout**: Reduce dependency count, optimize test execution
- **Flaky tests**: Fix non-deterministic test logic

---

## Summary Checklist

Before pushing to CI, verify all local tests pass:

- [ ] **Step 1**: Clean build succeeds in < 30s
- [ ] **Step 2**: Formatting check passes (cargo fmt)
- [ ] **Step 3**: Linting passes (cargo clippy)
- [ ] **Step 4**: Zero compiler warnings
- [ ] **Step 5**: All tests pass (cargo test)
- [ ] **Step 6**: Nextest integration works
- [ ] **Step 7**: Code coverage >= 80%
- [ ] **Step 8**: Documentation builds cleanly
- [ ] **Step 9**: CLI binary runs successfully
- [ ] **Step 10**: Release build compiles
- [ ] **Step 11**: Directory structure correct
- [ ] **Step 12**: Git status clean
- [ ] **Step 13**: Local CI simulation passes

After pushing:

- [ ] **Step 14**: GitHub Actions CI passes all jobs

---

## Quick Reference Commands

```bash
# Complete validation suite (run in sequence)
cargo clean
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
cargo doc --no-deps
cargo run --bin crush
cargo build --release

# Fix common issues
cargo fmt --all                    # Auto-fix formatting
cargo fix --allow-dirty            # Auto-fix clippy suggestions
cargo update                       # Update dependencies

# Coverage and reports
cargo llvm-cov --html              # Generate coverage report
cargo tree                         # View dependency tree
cargo bloat --release              # Analyze binary size
```

---

## Performance Benchmarks

Record baseline performance for future comparison:

```bash
# Build time (clean)
time cargo clean && cargo build
# Expected: < 30s (excluding dependency downloads)

# Build time (incremental)
touch crush-core/src/lib.rs
time cargo build
# Expected: < 5s

# Test execution time
time cargo test
# Expected: < 10s

# CLI startup time
time ./target/release/crush > /dev/null
# Expected: < 10ms
```

**Baseline Metrics** (record for reference):
- Clean build time: ______s
- Incremental build time: ______s
- Test execution time: ______s
- Binary size (debug): ______MB
- Binary size (release): ______MB
- Code coverage: ______%

---

## Troubleshooting Matrix

| Symptom | Possible Cause | Solution |
|---------|----------------|----------|
| Compilation error | Syntax error in Rust code | Check error message, fix code |
| Clippy warnings | Code quality issue | Follow clippy suggestion or add `#[allow]` |
| Test failure | Logic error or invalid test | Debug with `cargo test -- --nocapture` |
| Doc-test failure | Example code doesn't compile | Verify example in doc comment |
| Low coverage | Missing tests | Add tests for uncovered functions |
| CI timeout | Excessive dependencies | Review dependency tree with `cargo tree` |
| Binary too large | Debug symbols in release | Verify `strip = true` in Cargo.toml |
| Git issues | Untracked files | Review `git status`, update .gitignore |

---

## Next Steps

After all tests pass:

1. **Commit changes**: `git commit -am "feat: implement Rust workspace"`
2. **Push to remote**: `git push origin 003-rust-workspace-setup`
3. **Create PR**: Use GitHub CLI or web interface
4. **Monitor CI**: Ensure all checks pass
5. **Resume feature 002**: Complete remaining validation tasks (T049-T061)
6. **Merge PR**: After approvals and passing CI

---

## References

- Cargo Book: https://doc.rust-lang.org/cargo/
- Rustfmt Guide: https://rust-lang.github.io/rustfmt/
- Clippy Lints: https://rust-lang.github.io/rust-clippy/
- Nextest Documentation: https://nexte.st/
- llvm-cov Guide: https://github.com/taiki-e/cargo-llvm-cov
- Constitution: `.specify/memory/constitution.md` (quality gates)
- Feature 002 Spec: `specs/002-github-actions-ci/spec.md` (CI integration)
