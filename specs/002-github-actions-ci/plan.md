# Implementation Plan: GitHub Actions CI/CD Pipeline

**Branch**: `002-github-actions-ci` | **Date**: 2026-01-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-github-actions-ci/spec.md`

## Summary

Implement a comprehensive GitHub Actions-based CI/CD pipeline for the Crush project that enforces quality gates (formatting, linting, testing, coverage), verifies multi-platform compatibility, audits dependencies for security vulnerabilities, and automates package publishing to crates.io with trusted publishing. The pipeline includes fail-fast strategies, concurrency control, build caching, and support for static Linux binaries via musl targets.

## Technical Context

**Language/Version**: YAML (GitHub Actions workflow syntax), Bash (workflow scripts), Rust toolchain (stable + beta for builds)
**Primary Dependencies**: GitHub Actions runners, cargo-nextest (test runner), cargo-tarpaulin (coverage), cargo-audit (security), musl-gcc (static builds)
**Storage**: GitHub Actions artifacts, crates.io package registry
**Testing**: Workflow validation via act (local GitHub Actions runner), integration testing with test PRs and release branches
**Target Platform**: GitHub Actions hosted runners (ubuntu-latest, windows-latest, macos-latest)
**Project Type**: CI/CD infrastructure (workflow YAML files in `.github/workflows/`)
**Performance Goals**: <10 minute CI feedback for typical PRs, <15 minute release publish cycle, 30% build time reduction via caching
**Constraints**: GitHub Actions concurrent job limits, 6-hour workflow timeout, crates.io API rate limits, OIDC token lifetime for trusted publishing
**Scale/Scope**: 3-5 workflow files, ~15-20 jobs across all workflows, 3 platforms × 2 Rust versions = 6 build matrix combinations

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Evaluation

This feature creates CI/CD infrastructure (GitHub Actions workflows) to enforce constitution compliance. Evaluating constitution principles:

**I. Performance First (NON-NEGOTIABLE)**: ✅ **PASS**
- CI workflows optimize for fast feedback (<10 min target per spec SC-002)
- Build caching reduces redundant compilation (FR-026, 30% reduction target per SC-010)
- Fail-fast strategies prevent wasted compute on known failures (FR-008, FR-011)
- Not applicable: No runtime code, no compression algorithms

**II. Correctness & Safety (NON-NEGOTIABLE)**: ✅ **PASS**
- CI enforces all safety checks defined in constitution (cargo fmt, clippy, tests)
- Dependency audit catches security vulnerabilities (FR-013, FR-014, FR-015)
- Version uniqueness prevents accidental overwrites (FR-018, SC-007)
- Not applicable: No unsafe code in workflow YAML

**III. Modularity & Extensibility (NON-NEGOTIABLE)**: ✅ **PASS**
- Workflows separated by concern: quality-gates.yml, release.yml, audit.yml
- Reusable workflow actions for common patterns (cache setup, toolchain install)
- Platform-independent job definitions using matrix strategy
- Not applicable: No plugin architecture in CI config

**IV. Test-First Development (NON-NEGOTIABLE)**: ⚠️ **MODIFIED INTERPRETATION**
- TDD not directly applicable to workflow YAML configuration
- Testing approach: Validation via act (local runner), integration tests with test PRs/branches
- Workflows enforce TDD for application code via FR-003, FR-004, FR-012
- Constitution's Quality Gates section requires CI enforcement - this feature implements that requirement

**Dependency Management**: ✅ **PASS**
- No new application dependencies added (only CI tooling)
- cargo-audit enforces dependency security per constitution (FR-013, FR-014)
- Workflows use standard GitHub Actions marketplace actions (minimal external dependencies)

**Quality Gates**: ✅ **IMPLEMENTED BY THIS FEATURE**
- This feature IS the implementation of constitution's "CI Enforcement" section
- All quality gates from constitution will be automated (cargo test, clippy, coverage, benchmarks, fuzz, miri)
- Workflows block merges on failure per constitution requirement

**Development Toolchain**: ✅ **PASS**
- Workflows respect rust-toolchain.toml pinning (FR-007: stable + beta)
- Pre-commit hooks (cargo-husky) remain developer-local, not duplicated in CI
- CI enforces fast checks from pre-commit (fmt, clippy) plus slow checks (tests, benchmarks)

**Branching & Merge Governance**: ✅ **PASS**
- Workflows trigger on PRs to develop/main (FR-027, FR-028)
- Release workflow triggers only on release/* branches (FR-029)
- Workflows enforce PR-based workflow via required status checks
- Automated branch merging after release (FR-020)

**CI Enforcement**: ✅ **IMPLEMENTED BY THIS FEATURE**
- This feature implements mandatory CI gates defined in constitution
- Formatting, linting, testing, security, coverage all automated
- Merge blocking enforced via GitHub branch protection rules (requires CI pass)

**Release & Compatibility Policy**: ✅ **PASS**
- Release workflow publishes only from release branches to main (FR-016, FR-021)
- Trusted publishing ensures releases come from verified GitHub Actions (FR-017)
- Version management prevents compatibility violations (FR-018, FR-019)

**AI Agent Behavior Guidance**: ✅ **PASS**
- CI workflows assume feature branches are unstable (concurrency cancellation on new pushes)
- No compatibility layers in workflow config
- Workflows enforce correctness via CI gates, not defensive checks

### Gate Decision: ✅ **PROCEED**

All applicable constitution principles pass. This feature directly implements the "CI Enforcement" section of the constitution. TDD gate modified for infrastructure-as-code. No violations require justification.

## Project Structure

### Documentation (this feature)

```text
specs/002-github-actions-ci/
├── plan.md              # This file
├── research.md          # Phase 0: GitHub Actions best practices, Rust CI patterns
├── quickstart.md        # Phase 1: Testing workflows, validating CI
├── contracts/           # Phase 1: Workflow schemas, success criteria
│   └── workflow-contracts.md
└── tasks.md             # Phase 2: NOT created by /speckit.plan
```

### Source Code (repository root)

```text
.github/
├── workflows/
│   ├── ci.yml                    # Main CI workflow (quality gates, multi-platform builds, tests)
│   ├── security-audit.yml        # Dependency security audit workflow
│   ├── release.yml               # Release workflow (publish to crates.io, branch merging)
│   └── _reusable/                # Reusable workflow components (optional)
│       ├── setup-rust.yml        # Reusable action for Rust toolchain setup
│       └── cache-cargo.yml       # Reusable action for cargo caching
└── actions/                      # Custom composite actions (optional)
    └── version-increment/        # Custom action for version bumping
        └── action.yml
```

**Structure Decision**: Workflows organized by purpose in `.github/workflows/`. Main CI workflow handles quality gates and builds. Separate workflows for security audits and releases provide clear separation of concerns and independent triggering. Reusable components reduce duplication across workflows.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

N/A - No constitution violations. All applicable gates pass.

## Phase 0: Research & Standards

### Research Topics

1. **GitHub Actions Workflow Best Practices for Rust**
   - Optimal job structure (sequential vs parallel)
   - Matrix strategy configuration for multi-platform builds
   - Caching strategies for Rust builds (target/ and cargo registry)
   - Fail-fast vs complete execution trade-offs

2. **cargo-nextest Integration**
   - Installation methods (prebuilt binaries vs cargo install)
   - Configuration options for GitHub Actions
   - Performance comparison vs cargo test
   - JUnit XML output for GitHub test summaries

3. **Code Coverage Tooling**
   - cargo-tarpaulin vs cargo-llvm-cov comparison
   - Coverage threshold enforcement methods
   - Integration with GitHub PR comments
   - Performance impact on CI time

4. **Cargo Trusted Publishing**
   - OIDC token configuration (GitHub → crates.io)
   - Workflow permissions requirements
   - Error handling for publish failures
   - Dry-run testing strategies

5. **Version Management Automation**
   - Semantic versioning inference from branch names or commit messages
   - Tools for version bumping (cargo-bump, cargo-release, custom scripts)
   - Version uniqueness verification against crates.io API
   - Cargo.toml parsing and updating in workflows

6. **musl Target Static Builds**
   - Cross-compilation setup for x86_64-unknown-linux-musl
   - Required dependencies (musl-gcc, musl-tools)
   - Platform-specific runners or Docker containers
   - Binary verification (ldd check for static linking)

7. **Concurrency and Cancellation**
   - Concurrency group patterns for PRs vs branches
   - cancel-in-progress trade-offs (CI time vs resource usage)
   - Handling concurrent release workflows

8. **Fail-Fast Strategies**
   - Matrix fail-fast behavior (stop all vs stop row)
   - Impact on feedback time for developers
   - Beta Rust as allowed failure configuration

### Research Deliverable

`research.md` will document:
- **Decision**: Tools and approaches chosen for each area
- **Rationale**: Why these choices fit Crush project needs
- **Alternatives**: Other options considered and why rejected
- **Configuration Examples**: Concrete YAML snippets and patterns

## Phase 1: Design & Contracts

### Workflow Contracts

`contracts/workflow-contracts.md` will define:

**CI Workflow Contract (ci.yml)**:
```yaml
Triggers:
  - pull_request: [develop, main]
  - push: [develop, main]

Jobs:
  - format_check: cargo fmt --all -- --check
  - lint: cargo clippy --all-targets --all-features -- -D warnings
  - build_matrix:
      os: [ubuntu-latest, windows-latest, macos-latest]
      rust: [stable, beta]
      fail-fast: true
  - test:
      runner: cargo-nextest
      platforms: [ubuntu-latest, windows-latest, macos-latest]
      fail-fast: true
  - coverage:
      tool: cargo-tarpaulin or cargo-llvm-cov
      threshold: 90%
      fail_below_threshold: true

Concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

Caching:
  - Cargo registry cache
  - Cargo build cache (target/)
  - Cargo git dependencies cache
```

**Security Audit Workflow Contract (security-audit.yml)**:
```yaml
Triggers:
  - pull_request: [develop, main]
  - push: [develop, main]
  - schedule: [daily at 00:00 UTC]

Jobs:
  - audit:
      tool: cargo audit
      fail_on: [critical, high]
      allow_warnings: [medium, low] with manual review

Concurrency:
  group: security-audit-${{ github.ref }}
  cancel-in-progress: true
```

**Release Workflow Contract (release.yml)**:
```yaml
Triggers:
  - push: [release/*]
  - workflow_dispatch: manual trigger with version input

Jobs:
  - validate_version:
      check_cargo_toml: version is semver compliant
      check_uniqueness: query crates.io API for existing version
  - increment_version:
      strategy: infer from branch name (release/v1.2.3) or manual input
      update: Cargo.toml version field
      commit: version bump commit to release branch
  - run_quality_gates:
      reuse: ci.yml workflow (all checks must pass)
  - build_musl_static:
      target: x86_64-unknown-linux-musl
      toolchain: stable
      verify: ldd check for static binary
  - publish:
      method: cargo publish with trusted publishing (OIDC)
      permissions: id-token: write, contents: read
      retry: exponential backoff on network failures
  - merge_release:
      targets: [develop, main]
      method: git merge --no-ff with merge commit
      push: to both branches
```

### Quickstart Guide

`quickstart.md` will provide:
- Step-by-step workflow testing procedures
- How to trigger workflows locally with act
- How to test with intentionally broken code (formatting, linting, tests)
- How to verify coverage enforcement
- How to test release workflow with dry-run
- How to validate musl static binaries
- Troubleshooting common CI failures

## Phase 1: Agent Context Update

Run `.specify/scripts/powershell/update-agent-context.ps1 -AgentType claude` to update agent context with:
- GitHub Actions workflow syntax and patterns
- Rust CI best practices
- cargo-nextest, cargo-tarpaulin, cargo-audit tooling
- Trusted publishing OIDC configuration
- Concurrency and caching strategies

## Post-Design Constitution Re-Check

After Phase 1 design, re-evaluate constitution compliance:

- ✅ No new application dependencies added
- ✅ No code requiring tests/benchmarks/linting (YAML config only)
- ✅ Workflows integrate with Git Flow (trigger on PRs to develop/main, releases from release branches)
- ✅ Workflows enforce all constitution quality gates
- ✅ No compatibility concerns for workflow infrastructure

**Final Gate Decision**: ✅ **APPROVED FOR IMPLEMENTATION**

All constitution gates remain passing. Ready to proceed to `/speckit.tasks` for task breakdown.
