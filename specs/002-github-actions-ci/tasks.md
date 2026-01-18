# Tasks: GitHub Actions CI/CD Pipeline

**Input**: Design documents from `/specs/002-github-actions-ci/`
**Prerequisites**: plan.md, spec.md, contracts/workflow-contracts.md, research.md, quickstart.md

**Tests**: This feature does not require automated tests. Validation is performed through manual testing using quickstart.md procedures and real GitHub Actions workflow execution.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4, US5)
- Include exact file paths in descriptions

## Path Conventions

- `.github/workflows/` - GitHub Actions workflow files
- `.config/` - Tool configuration files
- `.cargo/` - Cargo-specific configuration

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create directory structure and configuration files needed by all workflows

- [X] T001 Create .github/workflows directory at repository root
- [X] T002 Create .config directory for tool configurations
- [X] T003 Create .cargo directory for cargo configuration

---

## Phase 2: User Story 1 - Automated Quality Gates (Priority: P1) 🎯 MVP

**Goal**: Enforce code quality automatically on every PR through formatting, linting, multi-platform builds, tests, and coverage checks

**Independent Test**: Create test PR with intentional formatting violations, lint warnings, failing tests, or low coverage. Verify CI catches each issue and blocks merge. See quickstart.md sections 1.1-1.5 for detailed testing procedures.

### Configuration for User Story 1

- [X] T004 [P] [US1] Create .config/nextest.toml with CI profile configuration for cargo-nextest test runner
- [X] T005 [P] [US1] Create .cargo/deny.toml with license and dependency policies for cargo deny

### Implementation for User Story 1

- [X] T006 [US1] Create .github/workflows/ci.yml with complete CI pipeline workflow including format_check, lint, build_matrix, test, and coverage jobs per contracts/workflow-contracts.md
- [X] T007 [US1] Configure format_check job in .github/workflows/ci.yml to run `cargo fmt --all -- --check` on ubuntu-latest
- [X] T008 [US1] Configure lint job in .github/workflows/ci.yml to run `cargo clippy --all-targets --all-features -- -D warnings` on ubuntu-latest in parallel with format_check
- [X] T009 [US1] Configure build_matrix job in .github/workflows/ci.yml with matrix strategy for 3 OS (ubuntu/windows/macos-latest) × 2 Rust versions (stable/beta), fail-fast enabled, beta marked as continue-on-error
- [X] T010 [US1] Add Swatinem/rust-cache@v2 caching to build_matrix job in .github/workflows/ci.yml with shared-key per OS and Rust version
- [X] T011 [US1] Configure test job in .github/workflows/ci.yml with cargo-nextest runner on 3 platforms (ubuntu/windows/macos-latest), fail-fast strategy, JUnit XML output
- [X] T012 [US1] Add test result reporting to .github/workflows/ci.yml using EnricoMi/publish-unit-test-result-action for GitHub PR integration
- [X] T013 [US1] Configure coverage job in .github/workflows/ci.yml using cargo-llvm-cov with 90% threshold enforcement and Codecov integration
- [X] T014 [US1] Add concurrency configuration to .github/workflows/ci.yml with group: ci-${{ github.ref }} and cancel-in-progress for pull_request events
- [X] T015 [US1] Configure workflow triggers in .github/workflows/ci.yml for pull_request and push events on develop and main branches
- [X] T016 [US1] Set workflow permissions in .github/workflows/ci.yml: contents: read, checks: write, pull-requests: write

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently. Create test PR, verify all quality gates run and enforce standards. This is the MVP.

---

## Phase 3: User Story 2 - Multi-Platform Build Verification (Priority: P2)

**Goal**: Verify code compiles and runs correctly across Linux, Windows, macOS with both Rust stable and beta toolchains

**Independent Test**: Introduce platform-specific code (e.g., Windows path handling with backslashes). Verify builds succeed on Windows but may fail on Linux/macOS, demonstrating matrix catches platform issues. See quickstart.md section 2.1-2.2.

### Implementation for User Story 2

- [X] T017 [US2] Verify build_matrix job in .github/workflows/ci.yml correctly configures 6 build combinations (3 OS × stable, 3 OS × beta) with proper matrix exclusions
- [X] T018 [US2] Verify fail-fast: true is enabled for build_matrix job in .github/workflows/ci.yml to cancel remaining jobs on first failure
- [X] T019 [US2] Verify beta Rust builds in .github/workflows/ci.yml are marked with continue-on-error: true using matrix.experimental condition
- [X] T020 [US2] Add dtolnay/rust-toolchain action to build_matrix job in .github/workflows/ci.yml for consistent toolchain management with rustfmt and clippy components

**Checkpoint**: At this point, User Story 2 should be fully functional. Introduce platform-specific bug, verify CI catches it. All platform builds run correctly.

---

## Phase 4: User Story 3 - Security and Dependency Auditing (Priority: P3)

**Goal**: Automatically detect security vulnerabilities in dependencies on every PR and daily schedule

**Independent Test**: Add dependency with known vulnerability (old version). Verify cargo audit fails CI with CVE details. Test daily scheduled run. See quickstart.md sections 3.1-3.3.

### Implementation for User Story 3

- [X] T021 [P] [US3] Create .github/workflows/security-audit.yml with security audit workflow per contracts/workflow-contracts.md
- [X] T022 [US3] Configure audit job in .github/workflows/security-audit.yml to run `cargo audit --deny warnings` on ubuntu-latest
- [X] T023 [US3] Configure supply_chain job in .github/workflows/security-audit.yml to run `cargo deny check` for license and source validation
- [X] T024 [US3] Add workflow triggers to .github/workflows/security-audit.yml for pull_request, push (develop/main), and daily schedule (cron: '0 0 * * *')
- [X] T025 [US3] Add concurrency configuration to .github/workflows/security-audit.yml with group: security-audit-${{ github.ref }} and cancel-in-progress: true
- [X] T026 [US3] Set workflow permissions in .github/workflows/security-audit.yml: contents: read, security-events: write

**Checkpoint**: At this point, User Story 3 should be fully functional. Test with vulnerable dependency, verify audit catches it. Verify daily scheduled run works.

---

## Phase 5: User Story 4 - Automated Package Publishing (Priority: P4)

**Goal**: Automate version validation, quality checks, crates.io publishing with trusted publishing, and branch synchronization for releases

**Independent Test**: Create test release branch (release/v0.2.0), update version in Cargo.toml, push. Verify validate_version, CI runs, publish succeeds (or dry-run), and branches merge automatically. See quickstart.md sections 4.1-4.5.

### Implementation for User Story 4

- [X] T027 [P] [US4] Create .github/workflows/release.yml with release workflow per contracts/workflow-contracts.md
- [X] T028 [US4] Configure validate_version job in .github/workflows/release.yml to check Cargo.toml version is semver compliant and query crates.io API for uniqueness
- [X] T029 [US4] Configure run_ci job in .github/workflows/release.yml to reuse .github/workflows/ci.yml via workflow_call for complete quality gate validation
- [X] T030 [US4] Configure publish job in .github/workflows/release.yml with cargo publish using trusted publishing (rust-lang/crates-io-auth-action@v1) and exponential backoff retry
- [X] T031 [US4] Set OIDC permissions in publish job: id-token: write, contents: read for trusted publishing authentication
- [X] T032 [US4] Configure create_github_release job in .github/workflows/release.yml using gh release create with tag v${{ version }} and auto-generated release notes
- [X] T033 [US4] Configure merge_to_develop job in .github/workflows/release.yml to merge release branch to develop with --no-ff merge commit
- [X] T034 [US4] Configure merge_to_main job in .github/workflows/release.yml to merge release branch to main with --no-ff merge commit after merge_to_develop completes
- [X] T035 [US4] Add workflow triggers to .github/workflows/release.yml for push to release/** branches and workflow_dispatch with version input
- [X] T036 [US4] Add concurrency configuration to .github/workflows/release.yml with group: release-${{ github.ref }} and cancel-in-progress: false
- [X] T037 [US4] Set workflow permissions in .github/workflows/release.yml: id-token: write, contents: write, pull-requests: read

**Checkpoint**: At this point, User Story 4 should be fully functional. Create test release branch, verify version validation, CI run, publish (dry-run), and branch merges work.

---

## Phase 6: User Story 5 - Static Binary Distribution (Priority: P5)

**Goal**: Build and distribute musl-target static Linux binaries that work on any Linux distribution without system dependencies

**Independent Test**: Trigger release workflow, verify musl binary builds successfully. Download artifact, test with `ldd` (should show "not a dynamic executable"). Run in Alpine container. See quickstart.md sections 5.1-5.3.

### Implementation for User Story 5

- [X] T038 [US5] Configure build_musl_static job in .github/workflows/release.yml to cross-compile for x86_64-unknown-linux-musl target using cross tool
- [X] T039 [US5] Add musl binary verification steps to build_musl_static job in .github/workflows/release.yml: file command check, ldd check for static linking, Alpine container test
- [X] T040 [US5] Configure build_musl_static job in .github/workflows/release.yml to upload static binary as artifact with name crush-${{ version }}-x86_64-unknown-linux-musl
- [X] T041 [US5] Add musl binary artifact to create_github_release job in .github/workflows/release.yml with SHA256 checksum file

**Checkpoint**: At this point, User Story 5 should be fully functional. Verify musl binary builds, is statically linked, and runs on Alpine. Artifact available in GitHub release.

---

## Phase 7: Polish & Verification

**Purpose**: Validate all workflows, update documentation, configure repository settings

- [X] T042 [P] Validate .github/workflows/ci.yml YAML syntax using yamllint or GitHub Actions workflow validator
- [X] T043 [P] Validate .github/workflows/security-audit.yml YAML syntax
- [X] T044 [P] Validate .github/workflows/release.yml YAML syntax
- [X] T045 [P] Validate .config/nextest.toml TOML syntax
- [X] T046 [P] Validate .cargo/deny.toml TOML syntax
- [X] T047 Test ci.yml workflow locally using act: `act pull_request -W .github/workflows/ci.yml` (format_check and lint jobs only, matrix requires GitHub runners)
- [X] T048 Test security-audit.yml workflow locally using act: `act pull_request -W .github/workflows/security-audit.yml`

**⚠️ REMINDER: Tasks T049-T061 require Rust project implementation to be completed first**

The following tasks depend on having actual Rust code (Cargo.toml, source files, tests) to validate CI/CD workflows with real scenarios. These should be revisited after the core Crush library implementation is in place.

**Dependencies:**
- T049-T054: Need Rust codebase to test CI failure scenarios (format violations, clippy warnings, test failures, coverage)
- T055: Requires time passage (verify daily scheduled runs)
- T059: Requires external service setup (Codecov)
- T060: Requires external service setup (crates.io trusted publishing)
- T061: Final comprehensive verification using quickstart.md

**When to resume:** After implementing the basic Rust workspace structure (crush-core and crush-cli crates) from the project's expected structure in CLAUDE.md.

---

- [ ] T049 Create test PR with intentional formatting violation, verify format_check job fails and blocks PR (quickstart.md Test 1.1)
- [ ] T050 Create test PR with clippy warning, verify lint job fails and shows specific violations (quickstart.md Test 1.2)
- [ ] T051 Create test PR with failing test, verify test job fails on all platforms with clear error messages (quickstart.md Test 1.4)
- [ ] T052 Create test PR with coverage below 90%, verify coverage job fails with percentage comparison (quickstart.md Test 1.5)
- [ ] T053 Test multi-platform build matrix by introducing platform-specific code, verify builds succeed on target platform but may fail on others (quickstart.md Test 2.1)
- [ ] T054 Test security-audit workflow by adding dependency with known vulnerability, verify audit job fails with CVE details (quickstart.md Test 3.1)
- [ ] T055 Verify daily scheduled security audit runs at midnight UTC, check workflow run history
- [X] T056 Update CONTRIBUTING.md to add CI/CD Workflow section documenting how CI works, what checks run, and how to troubleshoot failures
- [X] T057 Configure GitHub branch protection rules for develop branch: require ci.yml workflow to pass before merge, require 1 review
- [X] T058 Configure GitHub branch protection rules for main branch: require ci.yml workflow to pass before merge, require 1 review
- [ ] T059 Set up Codecov integration: create codecov.io account, add CODECOV_TOKEN to repository secrets, verify coverage PR comments work
- [ ] T060 Configure crates.io trusted publishing: add Crush repository to crates.io trusted publishers via OIDC, verify GitHub Actions can publish without manual token
- [ ] T061 Run complete quickstart.md verification guide to ensure all success criteria from spec.md are met (SC-001 through SC-011)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **User Story 1 (Phase 2)**: Depends on Setup completion - Create ci.yml workflow (MVP!)
- **User Story 2 (Phase 3)**: Depends on User Story 1 - Enhances build_matrix already in ci.yml
- **User Story 3 (Phase 4)**: Depends on Setup completion - Independent workflow, can run after Setup
- **User Story 4 (Phase 5)**: Depends on User Story 1 and 3 - Release workflow reuses ci.yml
- **User Story 5 (Phase 6)**: Depends on User Story 4 - Adds musl build to release.yml
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Setup - No dependencies on other stories (MVP!)
- **User Story 2 (P2)**: Depends on User Story 1 - Verifies/enhances build_matrix in ci.yml
- **User Story 3 (P3)**: Can start after Setup - Independent of US1/US2 (separate workflow)
- **User Story 4 (P4)**: Depends on User Story 1 and 3 - Reuses ci.yml, needs security audit
- **User Story 5 (P5)**: Depends on User Story 4 - Adds to release.yml workflow

### Within Each User Story

- **US1**: T004-T005 [P] can run in parallel (config files), then T006-T016 build ci.yml sequentially
- **US2**: All tasks (T017-T020) verify/enhance existing ci.yml, can be done together
- **US3**: T021 [P] can run parallel with other phases, then T022-T026 build security-audit.yml
- **US4**: T027 [P] can run parallel, then T028-T037 build release.yml sequentially
- **US5**: All tasks (T038-T041) enhance release.yml, must follow US4
- **Polish**: T042-T046 [P] validation can run parallel, T047-T061 are sequential tests

### Parallel Opportunities

- After Setup (T001-T003) completes:
  - Start US1 (T004-T016)
  - Start US3 in parallel (T021-T026) - independent workflow

- Within US1:
  - T004 and T005 can run in parallel (different config files)

- Within Polish:
  - T042-T046 can run in parallel (syntax validation)

---

## Parallel Example: After Setup

```bash
# Once Setup completes, launch these in parallel:

# US1 config files (parallel)
Task T004 [US1]: Create nextest.toml
Task T005 [US1]: Create deny.toml

# US3 (parallel with US1, independent workflow)
Task T021 [US3]: Create security-audit.yml

# Then continue with US1 ci.yml implementation (T006-T016)
# Then US2 enhancements to US1 (T017-T020)
# Then US4 release workflow (T027-T037)
# Then US5 musl builds (T038-T041)
# Finally Polish (T042-T061)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T003)
2. Complete Phase 2: User Story 1 (T004-T016 - CI workflow)
3. **STOP and VALIDATE**: Create test PR, verify all quality gates work
4. Test independently per quickstart.md sections 1.1-1.5
5. Deploy/demo if ready - repository now has automated quality enforcement (MVP!)

### Incremental Delivery

1. Complete Setup → Foundation ready
2. Add User Story 1 → Test independently → Commit (MVP - Quality gates working!)
3. Add User Story 2 → Test independently → Commit (Multi-platform verification complete)
4. Add User Story 3 → Test independently → Commit (Security auditing in place)
5. Add User Story 4 → Test independently → Commit (Publishing automation ready)
6. Add User Story 5 → Test independently → Commit (Static binaries available)
7. Run Polish phase → Comprehensive validation → Create PR to develop
8. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers (or parallel agent execution):

1. Team completes Setup together (T001-T003)
2. Once Setup is done:
   - Developer A: User Story 1 (T004-T016)
   - Developer B: User Story 3 (T021-T026) - runs parallel with US1
3. After US1 completes:
   - Developer A: User Story 2 (T017-T020)
   - Developer C: User Story 4 (T027-T037) - depends on US1 being done
4. After US4 completes:
   - Developer D: User Story 5 (T038-T041)
5. Team runs Polish phase together (T042-T061)

---

## Notes

- [P] tasks = different files, no dependencies - safe for parallel execution
- [Story] label maps task to specific user story for traceability
- Each user story is independently testable per quickstart.md
- No automated tests required - validation through manual workflow execution
- Commit after each user story completion for incremental delivery
- Stop at any checkpoint to validate story independently
- All file paths are relative to repository root
- Use research.md for tool decisions and best practices
- Use contracts/workflow-contracts.md for exact workflow specifications
- Use quickstart.md for comprehensive testing procedures
- Avoid: Implementation details in workflow YAML, hardcoded secrets, duplicate job logic

---

## Task Count Summary

- **Total Tasks**: 61
- **Setup Phase**: 3 tasks
- **User Story 1 (P1)**: 13 tasks (2 config + 11 ci.yml implementation)
- **User Story 2 (P2)**: 4 tasks (verify/enhance build matrix)
- **User Story 3 (P3)**: 6 tasks (security-audit workflow)
- **User Story 4 (P4)**: 11 tasks (release workflow with publishing)
- **User Story 5 (P5)**: 4 tasks (musl static builds)
- **Polish Phase**: 20 tasks (5 validation + 15 testing/configuration)
- **Parallel Opportunities**: 4 tasks can run in parallel (T004-T005 config files, T021 US3 workflow, T042-T046 validation)

---

## Validation Checklist

After implementation, verify using quickstart.md:

### User Story 1 (P1 - MVP)
- [ ] format_check job catches unformatted code
- [ ] lint job catches clippy warnings
- [ ] build_matrix builds on 3 platforms × 2 Rust versions
- [ ] Beta builds don't block PRs (continue-on-error)
- [ ] test job runs with cargo-nextest on all platforms
- [ ] coverage job enforces 90% threshold
- [ ] PR comments show test results and coverage
- [ ] CI completes in <10 minutes for typical PR

### User Story 2 (P2)
- [ ] Platform-specific bugs caught by build matrix
- [ ] Fail-fast cancels remaining builds on first failure
- [ ] Beta Rust builds are informational only
- [ ] All 6 build combinations execute correctly

### User Story 3 (P3)
- [ ] cargo audit detects vulnerable dependencies
- [ ] cargo deny validates licenses and sources
- [ ] Daily scheduled audit runs at midnight UTC
- [ ] Security failures block PRs with clear CVE details

### User Story 4 (P4)
- [ ] validate_version prevents duplicate versions on crates.io
- [ ] CI runs before publish (reuses ci.yml)
- [ ] Trusted publishing succeeds without manual token
- [ ] Release branch merges to develop and main automatically
- [ ] GitHub release created with artifacts
- [ ] Total release workflow <15 minutes

### User Story 5 (P5)
- [ ] musl binary builds successfully
- [ ] Binary is statically linked (ldd verification)
- [ ] Binary runs on Alpine container
- [ ] Artifact included in GitHub release with SHA256

### Success Criteria (from spec.md)
- [ ] SC-001: 100% of PRs automatically checked
- [ ] SC-002: <10 minute CI feedback time
- [ ] SC-003: Platform bugs caught before production
- [ ] SC-004: 90% coverage maintained
- [ ] SC-005: Vulnerabilities detected within 24h
- [ ] SC-006: <15 minute publish cycle
- [ ] SC-007: Zero version conflicts
- [ ] SC-008: <5 minute branch sync
- [ ] SC-009: Static binaries work on minimal distros
- [ ] SC-010: 30% build time reduction via caching
- [ ] SC-011: Actionable CI failure messages

This completes the task breakdown for the GitHub Actions CI/CD Pipeline feature.
