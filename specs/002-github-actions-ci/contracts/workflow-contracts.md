# Workflow Contracts

**Feature**: GitHub Actions CI/CD Pipeline
**Purpose**: Define the contract for each workflow, including triggers, jobs, inputs, outputs, and success criteria
**Date**: 2026-01-17

## Overview

This document specifies the contracts for all GitHub Actions workflows implementing the CI/CD pipeline for Crush. Each workflow contract defines:
- **Triggers**: Events that activate the workflow
- **Jobs**: Execution units with dependencies
- **Permissions**: Required GitHub token permissions
- **Inputs/Outputs**: Parameters and artifacts
- **Success Criteria**: Validation requirements

---

## Workflow 1: CI Pipeline (ci.yml)

### Purpose
Enforce quality gates and multi-platform compatibility for all pull requests and pushes to integration branches.

### Contract

**Triggers**:
```yaml
on:
  pull_request:
    branches: [develop, main]
  push:
    branches: [develop, main]
```

**Concurrency**:
```yaml
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
```
- Cancels outdated PR runs when new commits pushed
- Preserves branch push runs for historical record

**Jobs**:

1. **format_check**
   - **Command**: `cargo fmt --all -- --check`
   - **Runner**: `ubuntu-latest`
   - **Fail Condition**: Any unformatted code
   - **Duration Target**: <30 seconds

2. **lint**
   - **Command**: `cargo clippy --all-targets --all-features -- -D warnings`
   - **Runner**: `ubuntu-latest`
   - **Fail Condition**: Any clippy warnings
   - **Duration Target**: <2 minutes
   - **Dependencies**: None (runs in parallel with format_check)

3. **build_matrix**
   - **Matrix**:
     ```yaml
     os: [ubuntu-latest, windows-latest, macos-latest]
     rust: [stable, beta]
     experimental: [false, true]  # true for beta
     exclude:
       - rust: stable
         experimental: true
     ```
   - **Strategy**: `fail-fast: true` (except beta which has `continue-on-error: true`)
   - **Command**: `cargo build --all-targets --all-features`
   - **Caching**: Swatinem/rust-cache@v2
   - **Duration Target**: <5 minutes per platform
   - **Dependencies**: format_check, lint (must pass first)

4. **test**
   - **Matrix**:
     ```yaml
     os: [ubuntu-latest, windows-latest, macos-latest]
     ```
   - **Strategy**: `fail-fast: true`
   - **Test Runner**: cargo-nextest
   - **Command**: `cargo nextest run --all-features --config-file .config/nextest.toml`
   - **Output**: JUnit XML to `test-results/`
   - **Duration Target**: <3 minutes per platform
   - **Dependencies**: build_matrix (must complete successfully)

5. **coverage**
   - **Runner**: `ubuntu-latest`
   - **Tool**: cargo-llvm-cov
   - **Command**: `cargo llvm-cov nextest --all-features --lcov --output-path lcov.info`
   - **Threshold**: 90% minimum
   - **Fail Condition**: Coverage below 90%
   - **Reporting**: Codecov integration for PR comments
   - **Duration Target**: <4 minutes
   - **Dependencies**: test (must complete)

**Permissions**:
```yaml
permissions:
  contents: read
  checks: write  # For test result reporting
  pull-requests: write  # For coverage comments
```

**Artifacts**:
- Test results (JUnit XML): Retained for 7 days
- Coverage reports (lcov.info): Retained for 7 days
- Build binaries: Not retained (only for verification)

**Success Criteria**:
- ✅ All format checks pass (cargo fmt)
- ✅ All lint checks pass (cargo clippy)
- ✅ All platform builds succeed (3 OS × stable)
- ✅ Beta builds informational only (allowed to fail)
- ✅ All tests pass on all platforms
- ✅ Code coverage ≥ 90%
- ✅ Total duration < 10 minutes for typical PR

---

## Workflow 2: Security Audit (security-audit.yml)

### Purpose
Detect known security vulnerabilities in dependencies on every PR and daily schedule.

### Contract

**Triggers**:
```yaml
on:
  pull_request:
    branches: [develop, main]
  push:
    branches: [develop, main]
  schedule:
    - cron: '0 0 * * *'  # Daily at midnight UTC
```

**Concurrency**:
```yaml
concurrency:
  group: security-audit-${{ github.ref }}
  cancel-in-progress: true
```

**Jobs**:

1. **audit**
   - **Command**: `cargo audit --deny warnings`
   - **Runner**: `ubuntu-latest`
   - **Tool**: cargo-audit (latest)
   - **Fail Condition**: Critical or high severity vulnerabilities
   - **Advisory Database**: RustSec Advisory Database (auto-updated)
   - **Duration Target**: <1 minute

2. **supply_chain**
   - **Command**: `cargo deny check`
   - **Runner**: `ubuntu-latest`
   - **Checks**: licenses, bans, sources, advisories
   - **Configuration**: `.cargo/deny.toml`
   - **Duration Target**: <1 minute

**Permissions**:
```yaml
permissions:
  contents: read
  security-events: write  # For GitHub Security tab integration
```

**Success Criteria**:
- ✅ No critical or high severity vulnerabilities
- ✅ All dependencies from allowed sources (crates.io)
- ✅ All licenses approved (MIT, Apache-2.0, BSD)
- ✅ No banned dependencies
- ✅ Total duration < 2 minutes

**Failure Handling**:
- Medium/low severity: Warning with manual review required
- Critical/high severity: Block merge
- Scheduled run failures: Create GitHub issue automatically

---

## Workflow 3: Release (release.yml)

### Purpose
Automate version management, quality verification, crates.io publishing, and branch synchronization for releases.

### Contract

**Triggers**:
```yaml
on:
  push:
    branches:
      - 'release/**'
  workflow_dispatch:
    inputs:
      version:
        description: 'Version to release (e.g., 0.1.0)'
        required: true
        type: string
```

**Concurrency**:
```yaml
concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false  # Never cancel release workflows
```

**Jobs**:

1. **validate_version**
   - **Runner**: `ubuntu-latest`
   - **Checks**:
     - Cargo.toml version is semver compliant
     - Version does not exist on crates.io (query API)
     - Release branch naming matches version (if branch-triggered)
   - **Outputs**: `version` (validated version string)
   - **Duration Target**: <30 seconds

2. **run_ci**
   - **Type**: Reusable workflow call
   - **Workflow**: `.github/workflows/ci.yml`
   - **Purpose**: Run all quality gates (formatting, linting, builds, tests, coverage)
   - **Dependencies**: validate_version
   - **Duration Target**: <10 minutes

3. **build_musl_static**
   - **Runner**: `ubuntu-latest`
   - **Target**: `x86_64-unknown-linux-musl`
   - **Tool**: cross
   - **Command**: `cross build --release --target x86_64-unknown-linux-musl`
   - **Verification**:
     ```bash
     file target/x86_64-unknown-linux-musl/release/crush
     ldd target/x86_64-unknown-linux-musl/release/crush 2>&1 | grep "not a dynamic"
     docker run --rm -v $(pwd)/target:/artifacts alpine:latest /artifacts/x86_64-unknown-linux-musl/release/crush --version
     ```
   - **Artifact**: `crush-${{ version }}-x86_64-unknown-linux-musl`
   - **Dependencies**: run_ci
   - **Duration Target**: <5 minutes

4. **publish**
   - **Runner**: `ubuntu-latest`
   - **Method**: Cargo trusted publishing (OIDC)
   - **Command**: `cargo publish`
   - **Permissions**:
     ```yaml
     id-token: write  # For OIDC token exchange
     contents: read
     ```
   - **Authentication**: Uses `rust-lang/crates-io-auth-action@v1` (OIDC)
   - **Retry Strategy**: Exponential backoff (3 attempts, 10s/30s/60s delays)
   - **Dependencies**: validate_version, run_ci, build_musl_static
   - **Duration Target**: <2 minutes

5. **create_github_release**
   - **Runner**: `ubuntu-latest`
   - **Tool**: `gh release create`
   - **Tag**: `v${{ version }}`
   - **Artifacts**:
     - musl static binary (crush-x86_64-unknown-linux-musl.tar.gz)
     - SHA256 checksums
   - **Release Notes**: Auto-generated from commits since last release
   - **Dependencies**: publish
   - **Duration Target**: <1 minute

6. **merge_to_develop**
   - **Runner**: `ubuntu-latest`
   - **Strategy**: Merge release branch to develop with merge commit
   - **Command**:
     ```bash
     git fetch origin develop
     git checkout develop
     git merge --no-ff ${{ github.ref }} -m "Merge release ${{ version }} to develop"
     git push origin develop
     ```
   - **Dependencies**: create_github_release
   - **Duration Target**: <30 seconds

7. **merge_to_main**
   - **Runner**: `ubuntu-latest`
   - **Strategy**: Merge release branch to main with merge commit
   - **Command**:
     ```bash
     git fetch origin main
     git checkout main
     git merge --no-ff ${{ github.ref }} -m "Merge release ${{ version }} to main"
     git push origin main
     ```
   - **Dependencies**: merge_to_develop
   - **Duration Target**: <30 seconds

**Permissions**:
```yaml
permissions:
  id-token: write      # OIDC token for crates.io trusted publishing
  contents: write      # Create GitHub release and push to branches
  pull-requests: read  # Access PR data for release notes
```

**Success Criteria**:
- ✅ Version is unique (not on crates.io)
- ✅ All CI checks pass (formatting, linting, builds, tests, coverage)
- ✅ musl static binary builds successfully
- ✅ Static binary verified (file, ldd, Alpine container test)
- ✅ Package published to crates.io
- ✅ GitHub release created with artifacts
- ✅ Release branch merged to develop
- ✅ Release branch merged to main
- ✅ Total duration < 15 minutes

**Failure Handling**:
- Version validation failure: Stop immediately, notify maintainer
- CI failure: Stop before publish, fix issues on release branch
- Publish failure: Retry with backoff, manual intervention if all retries fail
- Merge conflict: Manual resolution required, pause workflow

---

## Common Patterns

### Caching Strategy

All workflows use consistent caching via `Swatinem/rust-cache@v2`:

```yaml
- uses: Swatinem/rust-cache@v2
  with:
    shared-key: ${{ matrix.os }}-${{ matrix.rust }}
    cache-on-failure: true
    save-if: ${{ github.ref == 'refs/heads/develop' || github.ref == 'refs/heads/main' }}
```

**Cache Keys**:
- Primary: `${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}`
- Shared across: Same OS + Rust version
- Invalidated: When Cargo.lock changes

**Cached Directories**:
- `~/.cargo/registry/index/`
- `~/.cargo/registry/cache/`
- `~/.cargo/git/db/`
- `target/`

**Cache Behavior**:
- Save on success: Only for develop/main branch pushes
- Save on failure: Yes (partial builds still useful)
- Automatic cleanup: After 7 days of no access

### Rust Toolchain Setup

All workflows use `dtolnay/rust-toolchain` for consistent toolchain management:

```yaml
- uses: dtolnay/rust-toolchain@stable  # or @beta
  with:
    components: rustfmt, clippy
```

**Toolchain Pinning**:
- Respects `rust-toolchain.toml` in repository root
- Components: rustfmt, clippy (installed automatically)
- Update strategy: Manual via `rust-toolchain.toml` updates

### Test Result Reporting

All test jobs use `EnricoMi/publish-unit-test-result-action` for GitHub PR integration:

```yaml
- name: Publish Test Results
  uses: EnricoMi/publish-unit-test-result-action@v2
  if: always()
  with:
    files: test-results/**/*.xml
    check_name: Test Results (${{ matrix.os }})
```

---

## Validation Checklist

Use this checklist to verify workflow implementations match contracts:

### CI Workflow (ci.yml)
- [ ] Triggers on pull_request and push to develop/main
- [ ] Concurrency configured with cancel-in-progress for PRs
- [ ] format_check job runs cargo fmt --all -- --check
- [ ] lint job runs cargo clippy with -D warnings
- [ ] build_matrix covers 3 OS × 2 Rust versions
- [ ] Beta builds marked as continue-on-error
- [ ] test job uses cargo-nextest
- [ ] coverage job enforces 90% threshold
- [ ] All jobs use Swatinem/rust-cache@v2
- [ ] Test results published to PR checks
- [ ] Coverage reported to PR comments

### Security Audit Workflow (security-audit.yml)
- [ ] Triggers on pull_request, push, and daily schedule
- [ ] cargo audit fails on critical/high vulnerabilities
- [ ] cargo deny checks licenses, bans, sources, advisories
- [ ] Results integrated with GitHub Security tab
- [ ] Scheduled failures create GitHub issues

### Release Workflow (release.yml)
- [ ] Triggers on release/** branch pushes and workflow_dispatch
- [ ] Concurrency configured with cancel-in-progress: false
- [ ] validate_version checks crates.io for duplicate versions
- [ ] run_ci reuses ci.yml workflow
- [ ] build_musl_static uses cross for static binary
- [ ] musl binary verified with file, ldd, Alpine test
- [ ] publish uses trusted publishing (id-token: write)
- [ ] GitHub release created with musl artifact
- [ ] Release branch merged to both develop and main
- [ ] Workflow completes in <15 minutes

---

## Contract Updates

When modifying workflows, update this contract document:

1. Document the change (what, why, impact)
2. Update relevant contract sections
3. Re-validate against checklist
4. Update quickstart.md with new testing procedures
5. Commit contract updates alongside workflow changes

**Version**: 1.0.0 | **Last Updated**: 2026-01-17
