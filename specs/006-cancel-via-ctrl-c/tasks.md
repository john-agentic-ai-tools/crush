# Tasks: Graceful Cancellation Support

**Input**: Design documents from `specs/006-cancel-via-ctrl-c/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Tests**: TDD is MANDATORY per constitution - tests written first, must fail, then implement

**Organization**: Tasks grouped by user story to enable independent implementation and testing

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2)
- Include exact file paths in descriptions

## Path Conventions

This is a Rust workspace project:
- Core library: `crush-core/src/`
- CLI wrapper: `crush-cli/src/`
- Tests: `tests/integration/`, `tests/unit/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependency setup

- [X] T001 Add `ctrlc = "3.4"` dependency to crush-core/Cargo.toml
- [X] T002 Add `tempfile` dependency to crush-core/Cargo.toml for temp file handling
- [X] T003 [P] Run `cargo build` to verify dependencies resolve correctly
- [X] T004 [P] Create crush-core/src/cancel.rs module file (empty, just stub)
- [X] T005 [P] Add `pub mod cancel;` to crush-core/src/lib.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core cancellation infrastructure that MUST be complete before ANY user story

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T006 Create CrushError::Cancelled variant in crush-core/src/error.rs
- [X] T007 [P] Define CancellationToken trait in crush-core/src/cancel.rs
- [X] T008 [P] Define AtomicCancellationToken struct in crush-core/src/cancel.rs
- [X] T009 Implement CancellationToken trait for AtomicCancellationToken in crush-core/src/cancel.rs
- [X] T010 [P] Add ResourceTracker struct skeleton to crush-core/src/cancel.rs
- [X] T011 [P] Export CancellationToken and AtomicCancellationToken from crush-core/src/lib.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Cancel Long-Running Compression (Priority: P1) 🎯 MVP

**Goal**: Users can press Ctrl+C to gracefully cancel compress/decompress operations with automatic cleanup

**Independent Test**: Start compression on large file (>500MB), press Ctrl+C mid-operation, verify operation stops within 1 second, incomplete files deleted, exit code 130 (Unix) or 2 (Windows)

### Tests for User Story 1 (TDD - MUST write first and ensure they FAIL)

> **TDD REQUIREMENT**: Write these tests FIRST, run them, verify they FAIL, get approval, THEN implement

- [X] T012 [P] [US1] Unit test for CancellationToken::is_cancelled in tests/unit/cancel_unit_tests.rs
- [X] T013 [P] [US1] Unit test for CancellationToken::cancel idempotency in tests/unit/cancel_unit_tests.rs
- [X] T014 [P] [US1] Unit test for CancellationToken::reset in tests/unit/cancel_unit_tests.rs
- [X] T015 [P] [US1] Unit test for concurrent cancel safety in tests/unit/cancel_unit_tests.rs
- [X] T016 [P] [US1] Integration test for compress respects cancellation in tests/integration/cancel_tests.rs
- [X] T017 [P] [US1] Integration test for decompress respects cancellation in tests/integration/cancel_tests.rs
- [X] T018 [P] [US1] Integration test for file cleanup on cancellation in tests/integration/cancel_tests.rs
- [X] T019 [P] [US1] Integration test for exit code 130/2 on cancellation in tests/integration/cancel_tests.rs

**TDD Checkpoint**: Run `cargo test` - all cancellation tests should FAIL (code not implemented yet)

### Implementation for User Story 1

- [X] T020 [P] [US1] Implement AtomicCancellationToken::new() in crush-core/src/cancel.rs
- [X] T021 [P] [US1] Implement CancellationToken::is_cancelled() using AtomicBool::load in crush-core/src/cancel.rs
- [X] T022 [P] [US1] Implement CancellationToken::cancel() using AtomicBool::store in crush-core/src/cancel.rs
- [X] T023 [P] [US1] Implement CancellationToken::reset() in crush-core/src/cancel.rs
- [X] T024 [US1] Add CompressionOptions::with_cancel_token() method to crush-core/src/compression.rs
- [ ] T025 [US1] Add decompress_with_cancel method signature to crush-core/src/engine.rs (deferred to later)
- [X] T026 [US1] Implement cancellation check in compress_with_options in crush-core/src/compression.rs
- [X] T027 [US1] Create run_with_timeout_and_cancel with external token monitoring in crush-core/src/plugin/timeout.rs
- [X] T028 [US1] Return Err(CrushError::Cancelled) when cancellation detected (converted from PluginError)
- [X] T029 [P] [US1] Implement ResourceTracker::new() in crush-core/src/cancel.rs
- [X] T030 [P] [US1] Implement ResourceTracker::register_output() in crush-core/src/cancel.rs
- [X] T031 [P] [US1] Implement ResourceTracker::register_temp_file() in crush-core/src/cancel.rs
- [X] T032 [P] [US1] Implement ResourceTracker::mark_complete() in crush-core/src/cancel.rs
- [X] T033 [US1] Implement ResourceTracker::cleanup_all() in crush-core/src/cancel.rs
- [X] T034 [US1] Implement Drop trait for ResourceTracker in crush-core/src/cancel.rs
- [ ] T035 [US1] Integrate ResourceTracker into compression workflow (deferred - will be needed for CLI file cleanup)
- [X] T036 [US1] Update signal handler to use AtomicCancellationToken in crush-cli/src/signal.rs
- [X] T037 [US1] Pass Arc<dyn CancellationToken> to compress/decompress commands in crush-cli/src/main.rs
- [X] T038 [US1] Handle CrushError::Cancelled by converting to CliError::Interrupted in crush-cli/src/error.rs
- [X] T039 [US1] Set exit code 130 (Unix) on cancellation via CliError::Interrupted in crush-cli/src/error.rs
- [X] T040 [US1] Integrate cancel_token into compress_with_options in compress/decompress commands
- [ ] T041 [US1] Display "Operation cancelled" message (already handled by error display)

**TDD Checkpoint**: Run `cargo test` - all US1 tests should now PASS

**Functional Checkpoint**: At this point, User Story 1 should be fully functional:
- Compression/decompression can be cancelled via Ctrl+C
- Incomplete files are automatically deleted
- Process exits with proper exit code
- Multiple Ctrl+C presses are handled gracefully

---

## Phase 4: User Story 2 - Cancel with Progress Indication (Priority: P2)

**Goal**: Users see immediate feedback ("Cancelling...") when pressing Ctrl+C and progress updates during cleanup

**Independent Test**: Start compression on large file, press Ctrl+C, verify "Cancelling operation..." message appears within 100ms, cleanup progress shown, final "Operation cancelled" message displayed

### Tests for User Story 2 (TDD - MUST write first and ensure they FAIL)

> **TDD REQUIREMENT**: Write these tests FIRST, run them, verify they FAIL, then implement

- [ ] T042 [P] [US2] Integration test for "Cancelling..." message timing (<100ms) in tests/integration/cancel_tests.rs
- [ ] T043 [P] [US2] Integration test for cleanup progress updates in tests/integration/cancel_tests.rs
- [ ] T044 [P] [US2] Integration test for "Press Ctrl+C to cancel" hint display in tests/integration/cancel_tests.rs
- [ ] T045 [P] [US2] Integration test for hint shown only for >5s operations in tests/integration/cancel_tests.rs

**TDD Checkpoint**: Run `cargo test` - all US2 tests should FAIL (code not implemented yet)

### Implementation for User Story 2

- [ ] T046 [P] [US2] Create crush-cli/src/progress.rs module file
- [ ] T047 [P] [US2] Add `mod progress;` to crush-cli/src/main.rs
- [ ] T048 [US2] Implement should_show_progress(file_size) function in crush-cli/src/progress.rs
- [ ] T049 [US2] Display "Press Ctrl+C to cancel" for operations >5s in crush-cli/src/main.rs
- [ ] T050 [US2] Display "Cancelling operation..." immediately on signal in crush-cli/src/main.rs
- [ ] T051 [US2] Display cleanup progress updates during ResourceTracker::cleanup_all() in crush-core/src/cancel.rs
- [ ] T052 [US2] Display "Operation cancelled successfully" after cleanup in crush-cli/src/main.rs

**TDD Checkpoint**: Run `cargo test` - all US2 tests should now PASS

**Functional Checkpoint**: At this point, User Stories 1 AND 2 should both work independently:
- All US1 functionality (cancellation, cleanup, exit codes)
- Plus US2 enhancements (progress messages, hints, cleanup feedback)

---

## Phase 5: Polish & Cross-Cutting Concerns

**Purpose**: Quality improvements, documentation, and constitution compliance

- [ ] T053 [P] Add benchmark for cancellation overhead in benches/cancel_bench.rs
- [ ] T054 [P] Verify benchmark shows <1% overhead with criterion
- [ ] T055 [P] Add fuzz test for mid-operation cancellation in fuzz/fuzz_cancel.rs
- [ ] T056 [P] Run fuzz test for 100k iterations minimum
- [ ] T057 [P] Add documentation comments to CancellationToken trait in crush-core/src/cancel.rs
- [ ] T058 [P] Add documentation comments to public engine methods in crush-core/src/engine.rs
- [ ] T059 [P] Add examples to CancellationToken documentation in crush-core/src/cancel.rs
- [ ] T060 Run `cargo doc --no-deps` and verify no warnings
- [ ] T061 Run `cargo clippy --all-targets -- -D warnings` and fix all warnings
- [ ] T062 Run `cargo test` and verify >80% code coverage for cancellation paths
- [ ] T063 [P] Update README.md with cancellation feature description
- [ ] T064 [P] Add exit code documentation to CLI help text in crush-cli/src/args.rs
- [ ] T065 Run quickstart.md manual validation (test all examples)
- [ ] T066 Verify constitution quality gates checklist complete

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-4)**: All depend on Foundational phase completion
  - US1 and US2 can proceed in parallel after Foundational (if staffed)
  - Or sequentially US1 → US2 (recommended for MVP-first approach)
- **Polish (Phase 5)**: Depends on desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Builds on US1 but independently testable

### Within Each User Story (TDD Workflow)

1. **Tests FIRST**: Write all tests, run `cargo test`, verify they FAIL
2. **Get Approval**: Review test failures, confirm expected behavior
3. **Implementation**: Write code to make tests pass
4. **Verify**: Run `cargo test`, all tests for story should PASS
5. **Checkpoint**: Story is complete and independently functional

### Parallel Opportunities

**Setup Phase**:
- T003, T004, T005 can run in parallel

**Foundational Phase**:
- T007, T008, T010 can run in parallel (different structs)

**User Story 1 Tests**:
- T012-T019 can all run in parallel (independent test files)

**User Story 1 Implementation**:
- T020-T023 can run in parallel (different methods in cancel.rs)
- T029-T032 can run in parallel (different ResourceTracker methods)

**User Story 2 Tests**:
- T042-T045 can all run in parallel (independent tests)

**User Story 2 Implementation**:
- T046-T047 can run in parallel (module setup)

**Polish Phase**:
- T053-T059, T063-T064 can all run in parallel (different files)

---

## Parallel Example: User Story 1 Tests (TDD)

```bash
# Launch all unit tests together:
Task: "Unit test for CancellationToken::is_cancelled in tests/unit/cancel_unit_tests.rs"
Task: "Unit test for CancellationToken::cancel idempotency in tests/unit/cancel_unit_tests.rs"
Task: "Unit test for CancellationToken::reset in tests/unit/cancel_unit_tests.rs"
Task: "Unit test for concurrent cancel safety in tests/unit/cancel_unit_tests.rs"

# Launch all integration tests together:
Task: "Integration test for compress respects cancellation in tests/integration/cancel_tests.rs"
Task: "Integration test for decompress respects cancellation in tests/integration/cancel_tests.rs"
Task: "Integration test for file cleanup on cancellation in tests/integration/cancel_tests.rs"
Task: "Integration test for exit code 130/2 on cancellation in tests/integration/cancel_tests.rs"
```

---

## Parallel Example: User Story 1 Implementation

```bash
# Launch all CancellationToken methods together:
Task: "Implement AtomicCancellationToken::new() in crush-core/src/cancel.rs"
Task: "Implement CancellationToken::is_cancelled() using AtomicBool::load in crush-core/src/cancel.rs"
Task: "Implement CancellationToken::cancel() using AtomicBool::store in crush-core/src/cancel.rs"
Task: "Implement CancellationToken::reset() in crush-core/src/cancel.rs"

# Launch all ResourceTracker methods together:
Task: "Implement ResourceTracker::new() in crush-core/src/cancel.rs"
Task: "Implement ResourceTracker::register_output() in crush-core/src/cancel.rs"
Task: "Implement ResourceTracker::register_temp_file() in crush-core/src/cancel.rs"
Task: "Implement ResourceTracker::mark_complete() in crush-core/src/cancel.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only) - RECOMMENDED

1. Complete Phase 1: Setup (T001-T005)
2. Complete Phase 2: Foundational (T006-T011) ← CRITICAL BLOCKER
3. **TDD for US1**: Write tests T012-T019, verify FAIL
4. Complete Phase 3: User Story 1 implementation (T020-T041)
5. **STOP and VALIDATE**: Run `cargo test`, all US1 tests PASS
6. **STOP and VALIDATE**: Manual testing per independent test criteria
7. Deploy/demo MVP (core cancellation working)

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 1 (TDD) → Test independently → Deploy/Demo (MVP!)
3. Add User Story 2 (TDD) → Test independently → Deploy/Demo (enhanced UX)
4. Add Polish (Phase 5) → Benchmarks, fuzz tests, docs
5. Each story adds value without breaking previous stories

### Parallel Team Strategy

With 2+ developers:

1. Team completes Setup + Foundational together (Phase 1-2)
2. Once Foundational is done:
   - Developer A: User Story 1 (Phase 3) - TDD workflow
   - Developer B: User Story 2 (Phase 4) - TDD workflow (can start in parallel)
3. Stories complete independently, integrate seamlessly
4. Team collaborates on Polish (Phase 5)

---

## Constitution Compliance Checkpoints

### TDD Enforcement (Principle IV)

- ✅ Tests written FIRST for each user story (T012-T019 before T020-T041)
- ✅ Tests must FAIL before implementation begins
- ✅ Red-Green-Refactor cycle enforced via checkpoints
- ✅ Integration tests + unit tests for comprehensive coverage

### Quality Gates (Before Merge)

Per constitution, ALL must pass:
- ✅ All tests pass (T061: `cargo test`)
- ✅ No clippy warnings (T061: `cargo clippy --all-targets -- -D warnings`)
- ✅ Code coverage >80% (T062: cancellation paths covered)
- ✅ Benchmarks no regression (T053-T054: <1% overhead verified)
- ✅ Documentation builds (T060: `cargo doc --no-deps`)
- ✅ Fuzz testing clean (T055-T056: 100k iterations)
- ✅ No memory leaks (implicit via RAII, verified by tests)
- ✅ SpecKit task checklist complete (T066)

---

## Notes

- [P] tasks = different files, no dependencies, can run in parallel
- [Story] label maps task to US1 or US2 for traceability
- **TDD MANDATORY**: Tests written first, must fail, then implement
- Each user story is independently completable and testable
- Verify tests fail before implementing (Red phase)
- Verify tests pass after implementing (Green phase)
- Commit after each logical group of tasks
- Stop at checkpoints to validate story independently
- Constitution compliance verified at Phase 5

**Total Tasks**: 66
- Setup: 5 tasks
- Foundational: 6 tasks (blocks all stories)
- User Story 1: 30 tasks (8 tests + 22 implementation)
- User Story 2: 11 tasks (4 tests + 7 implementation)
- Polish: 14 tasks (benchmarks, fuzz, docs, quality gates)

**Parallel Opportunities**: 35 tasks marked [P] can run concurrently
**TDD Test Tasks**: 12 total (must be written and fail before implementation)
