# GitHub Actions Workflows

This directory contains CI/CD workflow definitions for the Crush project.

## Workflows

### CI (`ci.yml`)
Main CI pipeline running on all PRs and pushes to develop/main branches.

**Jobs:**
- `format_check`: Verify code formatting with `cargo fmt`
- `lint`: Run Clippy linter with strict warnings
- `build_matrix`: Build on 3 platforms (Linux, Windows, macOS) × 2 Rust versions (stable, beta)
- `test`: Run tests with cargo-nextest on all platforms
- `coverage`: Code coverage with cargo-llvm-cov (90% threshold)

### Security Audit (`security-audit.yml`)
Dependency security scanning on PRs, pushes, and daily schedule.

**Jobs:**
- `audit`: Check for known vulnerabilities with `cargo audit`
- `supply_chain`: Validate licenses and sources with `cargo deny`

### Release (`release.yml`)
Automated release pipeline triggered by pushes to `release/**` branches.

**Jobs:**
- `validate_version`: Verify semver compliance and uniqueness on crates.io
- `run_ci`: Run full CI pipeline (reuses ci.yml)
- `build_musl_static`: Build static Linux binary for x86_64-unknown-linux-musl
- `publish`: Publish to crates.io with trusted publishing (OIDC)
- `create_github_release`: Create GitHub release with artifacts
- `merge_to_develop`: Merge release branch to develop
- `merge_to_main`: Merge release branch to main

## Configuration Files

- `.config/nextest.toml`: cargo-nextest configuration with CI profile
- `.cargo/deny.toml`: cargo-deny license and dependency policies

## Testing Workflows Locally

Use [act](https://github.com/nektos/act) to test workflows locally:

```bash
# Test CI format check
act pull_request -j format_check -W .github/workflows/ci.yml

# Test security audit
act pull_request -W .github/workflows/security-audit.yml
```

Note: Matrix builds and platform-specific jobs require actual GitHub runners.
