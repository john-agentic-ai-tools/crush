# Quickstart Guide: Testing GitHub Actions CI/CD Pipeline

**Feature**: GitHub Actions CI/CD Pipeline
**Purpose**: Step-by-step guide for testing, validating, and troubleshooting CI workflows
**Date**: 2026-01-17

## Overview

This guide provides comprehensive testing procedures for the GitHub Actions CI/CD pipeline. Use this to:
- Validate workflow implementations match contracts
- Test each user story independently
- Troubleshoot common CI failures
- Verify success criteria from spec.md

---

## Prerequisites

Before testing workflows:

1. **Repository Setup**:
   - [ ] Branch protection rules configured for develop and main
   - [ ] Required status checks enabled (ci.yml jobs)
   - [ ] Crates.io trusted publishing configured (OIDC)

2. **Local Tools** (optional for local testing):
   - [ ] [act](https://github.com/nektos/act) installed (`brew install act` or `choco install act`)
   - [ ] Docker running (required for act)
   - [ ] GitHub CLI (`gh`) installed and authenticated

3. **Test Branch**:
   ```bash
   git checkout develop
   git pull origin develop
   git checkout -b test/ci-validation
   ```

---

## User Story 1: Automated Quality Gates (P1 - MVP)

**Goal**: Verify CI automatically checks formatting, linting, builds, tests, and coverage on PRs.

### Test 1.1: Formatting Check

**Purpose**: Verify cargo fmt catches unformatted code

**Steps**:
1. Introduce formatting violation:
   ```bash
   # Edit any Rust file with intentional bad formatting
   echo 'fn bad_format(   ){   println!("test"  )   ;}' >> src/lib.rs
   git add src/lib.rs
   git commit -m "test: introduce formatting violation"
   git push origin test/ci-validation
   ```

2. Create PR:
   ```bash
   gh pr create --base develop --title "Test: CI Formatting Check" --body "Testing format_check job"
   ```

3. **Expected Result**:
   - ✅ CI workflow triggers automatically
   - ✅ `format_check` job fails with clear error
   - ✅ PR blocked from merging
   - ✅ Error message shows unformatted file path

4. Fix and verify:
   ```bash
   cargo fmt --all
   git add .
   git commit -m "fix: apply formatting"
   git push origin test/ci-validation
   ```
   - ✅ `format_check` job now passes

### Test 1.2: Linting Check

**Purpose**: Verify cargo clippy catches warnings

**Steps**:
1. Introduce clippy warning:
   ```rust
   // Add to any Rust file
   fn unused_function() {
       let x = 5; // unused variable warning
   }
   ```

2. Commit and push:
   ```bash
   git add .
   git commit -m "test: introduce clippy warning"
   git push origin test/ci-validation
   ```

3. **Expected Result**:
   - ✅ `lint` job fails
   - ✅ Error shows specific clippy warning
   - ✅ PR blocked

4. Fix:
   ```bash
   # Remove or fix the violation
   cargo clippy --fix --all-targets --all-features
   git add .
   git commit -m "fix: resolve clippy warnings"
   git push
   ```

### Test 1.3: Multi-Platform Builds

**Purpose**: Verify builds succeed on Linux, Windows, macOS with stable and beta Rust

**Steps**:
1. Check workflow run for build matrix:
   ```bash
   gh run view --log | grep "build_matrix"
   ```

2. **Expected Result**:
   - ✅ 6 build jobs triggered (3 OS × stable, 3 OS × beta)
   - ✅ All stable builds pass
   - ✅ Beta builds informational only (continue-on-error)
   - ✅ Build duration < 5 minutes per platform

3. Test fail-fast:
   - Introduce platform-specific compilation error
   - Verify remaining matrix jobs cancel when first fails

### Test 1.4: Test Execution

**Purpose**: Verify tests run with cargo-nextest on all platforms

**Steps**:
1. Introduce failing test:
   ```rust
   #[test]
   fn test_failure() {
       assert_eq!(1, 2, "This test should fail");
   }
   ```

2. Commit and push:
   ```bash
   git add .
   git commit -m "test: introduce failing test"
   git push
   ```

3. **Expected Result**:
   - ✅ `test` job fails
   - ✅ JUnit XML output shows specific test failure
   - ✅ GitHub check shows test name and failure message
   - ✅ Test runs on all 3 platforms

4. View test results:
   ```bash
   gh pr checks
   # Or view in GitHub UI: PR → Checks tab → Test Results
   ```

### Test 1.5: Coverage Enforcement

**Purpose**: Verify 90% coverage threshold is enforced

**Steps**:
1. Add untested code to reduce coverage:
   ```rust
   pub fn uncovered_function() -> i32 {
       42
   }
   ```

2. Commit and push (without tests for new function)

3. **Expected Result**:
   - ✅ `coverage` job fails if coverage drops below 90%
   - ✅ PR comment shows coverage percentage
   - ✅ Coverage report uploaded to Codecov

4. Fix by adding tests:
   ```rust
   #[test]
   fn test_uncovered_function() {
       assert_eq!(uncovered_function(), 42);
   }
   ```

---

## User Story 2: Multi-Platform Build Verification (P2)

**Goal**: Verify cross-platform compatibility is automatically tested

### Test 2.1: Platform-Specific Code

**Purpose**: Catch platform-specific bugs early

**Steps**:
1. Introduce Windows-specific path handling bug:
   ```rust
   #[cfg(windows)]
   fn get_path() -> String {
       "C:\\invalid\path".to_string() // Missing escape
   }
   ```

2. **Expected Result**:
   - ✅ Build succeeds on Linux and macOS
   - ✅ Build fails on Windows with clear error
   - ✅ PR blocked
   - ✅ Error message shows Windows platform context

### Test 2.2: Beta Rust Compatibility

**Purpose**: Verify beta builds don't block PRs but provide early warning

**Steps**:
1. Check workflow for beta Rust results
2. **Expected Result**:
   - ✅ Beta builds run in matrix
   - ✅ Beta failures marked as `continue-on-error: true`
   - ✅ Beta failures don't block PR merge
   - ✅ Beta results visible in checks for awareness

---

## User Story 3: Security Auditing (P3)

**Goal**: Verify dependency vulnerabilities are detected

### Test 3.1: Vulnerable Dependency

**Purpose**: Verify cargo audit catches known CVEs

**Steps**:
1. Add a dependency with known vulnerability (for testing):
   ```toml
   # In Cargo.toml (use an old version with known issues)
   [dependencies]
   # Example: old version of time crate with CVE
   # DO NOT MERGE - for testing only
   ```

2. **Expected Result**:
   - ✅ `security-audit` workflow fails
   - ✅ Specific CVE listed in error output
   - ✅ Guidance on updating dependency provided
   - ✅ PR blocked

### Test 3.2: License Compliance

**Purpose**: Verify cargo deny checks licenses

**Steps**:
1. Configure `.cargo/deny.toml` to allow only MIT/Apache-2.0
2. Attempt to add dependency with GPL license
3. **Expected Result**:
   - ✅ `supply_chain` job fails
   - ✅ License violation clearly shown
   - ✅ PR blocked

### Test 3.3: Daily Scheduled Audit

**Purpose**: Verify scheduled audits run independently

**Steps**:
1. Check workflow runs:
   ```bash
   gh run list --workflow=security-audit.yml
   ```
2. **Expected Result**:
   - ✅ Daily run at 00:00 UTC
   - ✅ Results visible in Actions tab
   - ✅ Failures create GitHub issues automatically

---

## User Story 4: Automated Publishing (P4)

**Goal**: Verify release workflow publishes to crates.io and merges branches

### Test 4.1: Version Validation

**Purpose**: Verify version uniqueness check prevents overwrites

**Steps**:
1. Create release branch with existing version:
   ```bash
   git checkout -b release/v0.1.0
   # Don't change version in Cargo.toml
   git push origin release/v0.1.0
   ```

2. **Expected Result**:
   - ✅ `validate_version` job runs
   - ✅ Workflow fails if version exists on crates.io
   - ✅ Clear error: "Version 0.1.0 already exists"

### Test 4.2: Version Increment

**Purpose**: Verify version is updated before publish

**Steps**:
1. Create release branch with new version:
   ```bash
   git checkout develop
   git checkout -b release/v0.2.0
   # Update version in Cargo.toml to 0.2.0
   git commit -am "chore: bump version to 0.2.0"
   git push origin release/v0.2.0
   ```

2. **Expected Result**:
   - ✅ `validate_version` job passes
   - ✅ Version 0.2.0 confirmed unique
   - ✅ Workflow proceeds to CI checks

### Test 4.3: Quality Gates Before Publish

**Purpose**: Verify all CI checks run before publish

**Steps**:
1. Monitor release workflow run:
   ```bash
   gh run watch
   ```

2. **Expected Result**:
   - ✅ `run_ci` job executes (reuses ci.yml)
   - ✅ All quality gates pass (format, lint, build, test, coverage)
   - ✅ Workflow only proceeds to publish after CI success

### Test 4.4: Trusted Publishing

**Purpose**: Verify OIDC-based publish to crates.io

**Steps**:
1. Check publish job logs:
   ```bash
   gh run view --log | grep "publish"
   ```

2. **Expected Result**:
   - ✅ OIDC token exchange succeeds
   - ✅ `cargo publish` uses trusted publishing (no token in env)
   - ✅ Package published to crates.io
   - ✅ Publish completes in < 2 minutes

3. Verify on crates.io:
   - Navigate to https://crates.io/crates/crush
   - Confirm new version appears

### Test 4.5: Branch Merging

**Purpose**: Verify release merges to develop and main

**Steps**:
1. After publish succeeds, check branch state:
   ```bash
   git fetch origin
   git log origin/develop --oneline -5
   git log origin/main --oneline -5
   ```

2. **Expected Result**:
   - ✅ Release branch merged to develop
   - ✅ Release branch merged to main
   - ✅ Merge commits include release version
   - ✅ Both merges complete within 5 minutes of publish

---

## User Story 5: Static Binary Distribution (P5)

**Goal**: Verify musl static binaries are built and distributed

### Test 5.1: musl Build

**Purpose**: Verify static Linux binary compilation

**Steps**:
1. Trigger release workflow (as in Test 4.2)
2. Monitor `build_musl_static` job
3. **Expected Result**:
   - ✅ cross tool used for x86_64-unknown-linux-musl
   - ✅ Build completes successfully
   - ✅ Binary artifact created

### Test 5.2: Static Binary Verification

**Purpose**: Verify binary is truly static (no dynamic dependencies)

**Steps**:
1. Check workflow logs for verification steps:
   ```bash
   gh run view --log | grep -A 10 "build_musl_static"
   ```

2. **Expected Result**:
   - ✅ `file` command shows "statically linked"
   - ✅ `ldd` command shows "not a dynamic executable"
   - ✅ Alpine container test succeeds

3. Download and test locally:
   ```bash
   gh release download v0.2.0 --pattern '*musl*'
   tar xzf crush-v0.2.0-x86_64-unknown-linux-musl.tar.gz
   docker run --rm -v $(pwd):/app alpine:latest /app/crush --version
   ```
   - ✅ Binary runs on Alpine without errors

### Test 5.3: GitHub Release Artifacts

**Purpose**: Verify static binary included in release

**Steps**:
1. Check GitHub release page:
   ```bash
   gh release view v0.2.0
   ```

2. **Expected Result**:
   - ✅ musl binary artifact listed
   - ✅ SHA256 checksum file included
   - ✅ Release notes auto-generated
   - ✅ Assets downloadable

---

## Local Testing with `act`

Test workflows locally before pushing (optional):

### Setup

```bash
# Install act
brew install act  # macOS
# or
choco install act  # Windows

# Verify Docker is running
docker ps
```

### Run CI Workflow Locally

```bash
# Test format check
act pull_request -j format_check

# Test full CI pipeline
act pull_request -W .github/workflows/ci.yml

# Test with secrets (for coverage/codecov)
act pull_request --secret-file .secrets
```

### Run Security Audit Locally

```bash
act pull_request -W .github/workflows/security-audit.yml
```

### Limitations of Local Testing

- ❌ Cannot test OIDC trusted publishing (requires GitHub infrastructure)
- ❌ Cannot test cross-platform matrix fully (only local OS)
- ❌ Cannot test branch protection integration
- ✅ Can test job logic, command execution, artifact generation
- ✅ Can validate YAML syntax and job dependencies

---

## Troubleshooting Common Failures

### Format Check Fails

**Symptom**: `format_check` job fails with "files are not formatted"

**Diagnosis**:
```bash
cargo fmt --all -- --check
```

**Fix**:
```bash
cargo fmt --all
git add .
git commit -m "fix: apply formatting"
```

### Lint Fails with Clippy Warnings

**Symptom**: `lint` job fails with clippy warnings

**Diagnosis**:
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Fix**:
```bash
# Auto-fix where possible
cargo clippy --fix --all-targets --all-features

# Review and fix manual changes
git add .
git commit -m "fix: resolve clippy warnings"
```

### Tests Fail in CI But Pass Locally

**Symptom**: Tests pass with `cargo test` locally but fail in CI

**Possible Causes**:
1. **Test pollution**: Tests depend on execution order
   - Fix: Use `cargo nextest run` locally (reproduces CI environment)
   - Add `#[test]` isolation or use test fixtures

2. **Environment differences**: Tests assume local paths/config
   - Fix: Use `env!("CARGO_MANIFEST_DIR")` for paths
   - Mock external dependencies

3. **Timing issues**: Race conditions or flaky tests
   - Fix: Add proper synchronization or increase timeouts
   - Use cargo-nextest retries for truly flaky tests

### Coverage Below Threshold

**Symptom**: `coverage` job fails with "coverage is 87%, threshold is 90%"

**Diagnosis**:
```bash
cargo install cargo-llvm-cov
cargo llvm-cov --html
# Open target/llvm-cov/html/index.html
```

**Fix**:
1. Identify uncovered code in HTML report
2. Add tests for uncovered branches/functions
3. Verify locally: `cargo llvm-cov --summary-only`

### Build Fails on Specific Platform

**Symptom**: Build succeeds on Linux but fails on Windows/macOS

**Diagnosis**:
- Check workflow logs for platform-specific error
- Look for `#[cfg(target_os = "...")]` code

**Fix**:
- Use cross-platform libraries (e.g., `std::path` instead of string manipulation)
- Add platform-specific tests
- Test locally with Docker: `docker run -v $(pwd):/app rust:latest cargo build`

### Publish Fails: Version Already Exists

**Symptom**: `publish` job fails with "crate version `X.Y.Z` is already uploaded"

**Diagnosis**:
- Check crates.io: https://crates.io/crates/crush/versions
- Verify Cargo.toml version was incremented

**Fix**:
```bash
# Update version in Cargo.toml
# Follow semver: MAJOR.MINOR.PATCH
git add Cargo.toml
git commit -m "chore: bump version to X.Y.Z"
git push
```

### musl Build Fails

**Symptom**: `build_musl_static` job fails with linker errors

**Possible Causes**:
1. **Dependency uses C code requiring glibc**
   - Fix: Find alternative pure-Rust dependency
   - Or: Use vendored C library with musl support

2. **Missing musl target**
   - Fix: Ensure `cross` tool is used (not rustup target add)

**Diagnosis**:
```bash
# Test locally with cross
cargo install cross
cross build --target x86_64-unknown-linux-musl --release
```

---

## Verification Checklist

Use this checklist after implementing workflows:

### User Story 1: Quality Gates
- [ ] Format check catches unformatted code
- [ ] Lint check catches clippy warnings
- [ ] Builds succeed on all 3 platforms
- [ ] Beta builds don't block PRs
- [ ] Tests run with cargo-nextest
- [ ] Coverage enforced at 90%
- [ ] PR comments show coverage changes
- [ ] Total CI time < 10 minutes

### User Story 2: Multi-Platform
- [ ] 3 OS × 2 Rust versions = 6 builds
- [ ] Platform-specific bugs caught
- [ ] Fail-fast cancels on first failure
- [ ] Beta failures informational only

### User Story 3: Security
- [ ] cargo audit runs on all PRs
- [ ] cargo deny checks licenses
- [ ] Daily scheduled audits run
- [ ] Failures create GitHub issues
- [ ] Critical/high vulns block PRs

### User Story 4: Publishing
- [ ] Version uniqueness validated
- [ ] CI runs before publish
- [ ] Trusted publishing succeeds
- [ ] Release branch merges to develop
- [ ] Release branch merges to main
- [ ] GitHub release created
- [ ] Total publish time < 15 minutes

### User Story 5: Static Binaries
- [ ] musl target builds
- [ ] Binary is statically linked (ldd check)
- [ ] Binary runs on Alpine
- [ ] Artifact included in GitHub release
- [ ] SHA256 checksum provided

---

## Success Criteria Validation

Map spec.md success criteria to test procedures:

| Success Criterion | Test Procedure | Validation Method |
|-------------------|----------------|-------------------|
| SC-001: 100% PRs checked | Test 1.1-1.5 | Verify all quality gates run |
| SC-002: <10 min feedback | Monitor workflow duration | Check CI logs for timing |
| SC-003: Zero platform bugs | Test 2.1 | Introduce platform bug, verify caught |
| SC-004: 90% coverage | Test 1.5 | Reduce coverage, verify blocked |
| SC-005: 24h vuln detection | Test 3.3 | Check scheduled run logs |
| SC-006: <15 min publish | Test 4.4 | Monitor release workflow duration |
| SC-007: Zero version conflicts | Test 4.1 | Attempt duplicate version |
| SC-008: 5 min branch sync | Test 4.5 | Check merge job timing |
| SC-009: Static binary works | Test 5.2 | Alpine container test |
| SC-010: 30% cache improvement | Compare run times | Before/after caching metrics |
| SC-011: Actionable failures | All tests | Verify error messages are clear |

---

## Next Steps

After completing quickstart validation:

1. **Update CONTRIBUTING.md** with CI workflow information
2. **Configure branch protection** to require CI checks
3. **Set up Codecov** integration for coverage reporting
4. **Configure crates.io** trusted publishing OIDC
5. **Run `/speckit.tasks`** to generate implementation task breakdown

**Version**: 1.0.0 | **Last Updated**: 2026-01-17
