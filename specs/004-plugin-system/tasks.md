# Tasks: Plugin System for Crush Library

**Input**: Design documents from `/specs/004-plugin-system/`
**Prerequisites**: plan.md (required), spec.md (required - user stories), research.md (completed)

**Tests**: Following constitution's Test-First Development principle - tests written and approved BEFORE implementation

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

From plan.md - Rust workspace structure:
- **Core library**: `crush-core/src/`
- **CLI wrapper**: `crush-cli/src/`
- **Tests**: `crush-core/tests/`
- **Benchmarks**: `benches/`
- **Fuzz tests**: `fuzz/fuzz_targets/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and workspace structure

- [X] T001 Add `linkme` dependency to workspace Cargo.toml (research decision: compile-time plugin registration)
- [X] T002 Add `crossbeam` dependency to workspace Cargo.toml (research decision: timeout enforcement)
- [X] T003 [P] Create plugin module structure at crush-core/src/plugin/mod.rs
- [X] T004 [P] Create error types module at crush-core/src/error.rs using thiserror
- [X] T005 [P] Configure clippy lints in crush-core/Cargo.toml (deny warnings, pedantic)
- [X] T006 [P] Create test fixtures directory at crush-core/tests/fixtures/

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T007 Define CrushError enum in crush-core/src/error.rs (PluginError, TimeoutError, ValidationError variants)
- [X] T008 Define PluginMetadata struct in crush-core/src/plugin/metadata.rs (name, version, magic_number, throughput, compression_ratio)
- [X] T009 Define CompressionAlgorithm trait in crush-core/src/plugin/contract.rs (compress, decompress, detect, metadata methods with Arc<AtomicBool> cancellation)
- [X] T010 Create distributed_slice registry in crush-core/src/plugin/mod.rs using linkme (COMPRESSION_ALGORITHMS static)
- [X] T011 Define CrushHeader struct in crush-core/src/plugin/metadata.rs (16-byte header: magic[4], flags, original_size, crc32)

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Core Compression Operations (Priority: P1) 🎯 MVP

**Goal**: Deliver basic compress/decompress functionality with default DEFLATE algorithm, enabling data integrity verification without requiring any plugins

**Independent Test**: Compress a file and decompress it back, verifying byte-for-byte identical output

### Tests for User Story 1 (TDD - Write First)

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T012 [P] [US1] Roundtrip test for default compression in crush-core/tests/roundtrip.rs (compress→decompress→verify)
- [X] T013 [P] [US1] Corrupted data test in crush-core/tests/roundtrip.rs (verify error handling)
- [X] T014 [P] [US1] Property-based roundtrip test in crush-core/tests/roundtrip.rs using proptest (random data)

### Implementation for User Story 1

- [X] T015 [P] [US1] Implement default DEFLATE plugin in crush-core/src/plugin/default.rs (name="deflate", magic=CR0100, uses flate2)
- [X] T016 [P] [US1] Register DEFLATE plugin via distributed_slice in crush-core/src/plugin/default.rs
- [X] T017 [US1] Implement compress() function in crush-core/src/compression.rs (routes to plugin, writes CrushHeader)
- [X] T018 [US1] Implement decompress() function in crush-core/src/decompression.rs (reads CrushHeader, routes to plugin by magic number)
- [X] T019 [US1] Implement header serialization/deserialization in crush-core/src/plugin/metadata.rs (little-endian, CRC32 validation)
- [X] T020 [US1] Add error handling for missing plugin during decompression in crush-core/src/decompression.rs (FR-001: helpful error message)
- [X] T021 [US1] Export public API in crush-core/src/lib.rs (compress, decompress functions)

**Checkpoint**: At this point, User Story 1 should be fully functional - can compress/decompress files with default algorithm

---

## Phase 4: User Story 2 - Plugin Discovery and Registration (Priority: P2)

**Goal**: Enable explicit plugin initialization that discovers and registers compile-time linked plugins with validation

**Independent Test**: Call init_plugins(), verify DEFLATE plugin appears in registry, confirm it can be listed and invoked

### Tests for User Story 2 (TDD - Write First)

- [ ] T022 [P] [US2] Plugin discovery test in crush-core/tests/integration/plugin_discovery.rs (verify DEFLATE plugin found)
- [ ] T023 [P] [US2] Multiple plugin registration test in crush-core/tests/integration/plugin_discovery.rs (mock 3 plugins, verify all registered)
- [ ] T024 [P] [US2] Duplicate plugin warning test in crush-core/tests/integration/plugin_discovery.rs (register same magic twice, verify warning logged)
- [ ] T025 [P] [US2] Re-initialization test in crush-core/tests/integration/plugin_discovery.rs (call init twice, verify refresh works)

### Implementation for User Story 2

- [ ] T026 [P] [US2] Implement PluginRegistry struct in crush-core/src/plugin/registry.rs (Arc<RwLock<HashMap<[u8; 4], Box<dyn CompressionAlgorithm>>>>)
- [ ] T027 [P] [US2] Implement init_plugins() function in crush-core/src/plugin/registry.rs (iterates COMPRESSION_ALGORITHMS, validates, registers)
- [ ] T028 [US2] Add plugin validation logic in crush-core/src/plugin/registry.rs (check metadata non-zero, magic number unique per FR-013)
- [ ] T029 [US2] Add duplicate magic number detection in crush-core/src/plugin/registry.rs (FR-013a: log warning, use first-registered)
- [ ] T030 [US2] Implement list_plugins() function in crush-core/src/plugin/registry.rs (returns Vec<PluginMetadata>)
- [ ] T031 [US2] Add re-initialization support in crush-core/src/plugin/registry.rs (clear + re-scan per FR-003a)
- [ ] T032 [US2] Export init_plugins() and list_plugins() in crush-core/src/lib.rs

**Checkpoint**: At this point, Users can discover plugins and see what's available

---

## Phase 5: User Story 3 - Intelligent Plugin Selection (Priority: P3)

**Goal**: Implement metadata-based plugin scoring (70% throughput, 30% compression ratio) with manual override capability

**Independent Test**: Create 2 mock plugins with different metadata, compress a file, verify highest-scoring plugin is selected

### Tests for User Story 3 (TDD - Write First)

- [ ] T033 [P] [US3] Plugin scoring test in crush-core/tests/integration/plugin_selection.rs (3 plugins, verify highest score wins)
- [ ] T034 [P] [US3] Manual override test in crush-core/tests/integration/plugin_selection.rs (specify plugin by name, verify used)
- [ ] T035 [P] [US3] Tied score test in crush-core/tests/integration/plugin_selection.rs (2 plugins same score, verify deterministic selection)
- [ ] T036 [P] [US3] Custom weights test in crush-core/tests/integration/plugin_selection.rs (50/50 weights, verify scoring changes)

### Implementation for User Story 3

- [ ] T037 [P] [US3] Define ScoringWeights struct in crush-core/src/plugin/selector.rs (throughput: f64, ratio: f64, validation)
- [ ] T038 [P] [US3] Implement calculate_score() in crush-core/src/plugin/selector.rs (logarithmic throughput scaling per research, min-max normalization)
- [ ] T039 [US3] Implement PluginSelector struct in crush-core/src/plugin/selector.rs (select() method with scoring logic)
- [ ] T040 [US3] Add manual plugin override to compress() in crush-core/src/compression.rs (Option<&str> plugin_name parameter)
- [ ] T041 [US3] Add configurable weights to PluginSelector in crush-core/src/plugin/selector.rs (default 70/30 per clarification)
- [ ] T042 [US3] Handle tied scores in crush-core/src/plugin/selector.rs (alphabetical by name per FR-015)
- [ ] T043 [US3] Integrate selector into compress() function in crush-core/src/compression.rs (call selector.select() if multiple plugins match)

**Checkpoint**: Plugin selection now optimizes for performance based on metadata

---

## Phase 6: User Story 4 - Plugin Timeout Protection (Priority: P4)

**Goal**: Protect against slow plugins using thread-based timeout with cooperative cancellation (30s default, configurable)

**Independent Test**: Create deliberately slow mock plugin, set 1s timeout, verify operation cancelled and fallback occurs

### Tests for User Story 4 (TDD - Write First)

- [ ] T044 [P] [US4] Timeout enforcement test in crush-core/tests/integration/timeout.rs (slow plugin, verify timeout triggers)
- [ ] T045 [P] [US4] Timeout success test in crush-core/tests/integration/timeout.rs (fast plugin, verify completes within timeout)
- [ ] T046 [P] [US4] Default timeout test in crush-core/tests/integration/timeout.rs (no timeout specified, verify 30s default applied)
- [ ] T047 [P] [US4] Cancellation flag test in crush-core/tests/integration/timeout.rs (plugin checks flag, cancels early)
- [ ] T048 [P] [US4] Plugin crash fallback test in crush-core/tests/integration/timeout.rs (plugin panics, verify fallback to default per FR-012a)

### Implementation for User Story 4

- [ ] T049 [P] [US4] Implement TimeoutGuard struct in crush-core/src/plugin/timeout.rs (RAII drop guard sets cancellation flag)
- [ ] T050 [P] [US4] Implement run_with_timeout() in crush-core/src/plugin/timeout.rs (spawn thread, crossbeam recv_timeout per research)
- [ ] T051 [US4] Add timeout parameter to compress()/decompress() in crush-core/src/compression.rs and crush-core/src/decompression.rs (Option<Duration>, default 30s)
- [ ] T052 [US4] Wrap plugin calls in run_with_timeout() in crush-core/src/compression.rs (pass Arc<AtomicBool> to plugin)
- [ ] T053 [US4] Implement fallback to default plugin in crush-core/src/compression.rs (on timeout or panic per FR-012a)
- [ ] T054 [US4] Add timeout logging in crush-core/src/plugin/timeout.rs (warn on timeout, include plugin name)
- [ ] T055 [US4] Update DEFLATE plugin to check cancellation flag in crush-core/src/plugin/default.rs (check every block)

**Checkpoint**: All user stories complete - system is fully functional with timeout protection

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Quality improvements, performance validation, and documentation

- [ ] T056 [P] Add benchmark for plugin discovery in benches/plugin_discovery.rs (criterion, verify <500ms per SC-002)
- [ ] T057 [P] Add benchmark for plugin selection in benches/plugin_selection.rs (criterion, verify <10ms per SC-004)
- [ ] T058 [P] Add benchmark for compression throughput in benches/compression.rs (criterion, verify >500 MB/s on 8-core per plan)
- [ ] T059 [P] Setup fuzz target for compress in fuzz/fuzz_targets/compress.rs (cargo-fuzz, 100k iterations minimum per constitution)
- [ ] T060 [P] Setup fuzz target for decompress in fuzz/fuzz_targets/decompress.rs (cargo-fuzz, verify no panics)
- [ ] T061 Run clippy on all targets (cargo clippy --all-targets -- -D warnings per quality gates)
- [ ] T062 Verify no .unwrap() in production code (grep audit per constitution)
- [ ] T063 [P] Add rustdoc documentation to all public APIs in crush-core/src/lib.rs (examples for compress/decompress/init_plugins)
- [ ] T064 [P] Add rustdoc documentation to plugin trait in crush-core/src/plugin/contract.rs (guide for plugin authors)
- [ ] T065 Run cargo doc (verify builds without warnings per quality gate)
- [ ] T066 Measure code coverage (verify >80% per quality gate)
- [ ] T067 Run all integration tests (verify roundtrip, discovery, selection, timeout)
- [ ] T068 Run fuzz tests (100k iterations, verify clean per quality gate)
- [ ] T069 Final constitution check (verify all quality gates passed)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - User Story 1 (P1): Core compression - no dependencies on other stories
  - User Story 2 (P2): Plugin discovery - no dependencies on other stories (but integrates with US1)
  - User Story 3 (P3): Plugin selection - depends on US2 (needs registry) but independently testable
  - User Story 4 (P4): Timeout protection - depends on US1/US2 but independently testable
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

**Critical Path for MVP**:
1. Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3 (US1 - Core Compression) → Validate & Deploy

**Full Feature Path**:
1. Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3 (US1)
2. Phase 4 (US2 - Discovery) can start after US1 complete
3. Phase 5 (US3 - Selection) can start after US2 complete
4. Phase 6 (US4 - Timeout) can start after US1/US2 complete
5. Phase 7 (Polish) after all stories complete

### Within Each User Story

- Tests MUST be written FIRST (TDD per constitution - tests → approval → fail → implement)
- Tests must FAIL before implementation begins
- Core structs/traits before logic
- Implementation after tests fail
- Story complete and passing before moving to next priority

### Parallel Opportunities

**Setup Phase (Phase 1)**:
- T003, T004, T005, T006 can run in parallel (different files)

**Foundational Phase (Phase 2)**:
- T007, T008 can run in parallel (different files)
- T009, T010, T011 sequential (dependencies)

**User Story 1 Tests**:
- T012, T013, T014 can run in parallel (different test cases)

**User Story 1 Implementation**:
- T015, T016 can run in parallel after tests fail (plugin implementation)
- T017, T018 sequential (compression depends on decompression routing)

**User Story 2 Tests**:
- T022, T023, T024, T025 can run in parallel (independent test cases)

**User Story 2 Implementation**:
- T026, T027 can run in parallel (registry struct and init function)

**User Story 3 Tests**:
- T033, T034, T035, T036 can run in parallel (independent test scenarios)

**User Story 3 Implementation**:
- T037, T038 can run in parallel (weights struct and scoring function)

**User Story 4 Tests**:
- T044, T045, T046, T047, T048 can run in parallel (independent timeout scenarios)

**User Story 4 Implementation**:
- T049, T050 can run in parallel (guard and timeout function)

**Polish Phase**:
- T056, T057, T058, T059, T060, T063, T064 can run in parallel (independent benchmarks, fuzz, docs)

### Team Parallelization Strategy

With 2+ developers after Foundational phase completes:
- **Developer A**: User Story 1 (Core Compression) - Priority MVP
- **Developer B**: User Story 2 (Plugin Discovery) - Can start in parallel

After US1 and US2 complete:
- **Developer A**: User Story 3 (Plugin Selection)
- **Developer B**: User Story 4 (Timeout Protection)

---

## Parallel Example: User Story 1

```bash
# Step 1: Write all tests together (TDD first):
Task T012: "Roundtrip test for default compression"
Task T013: "Corrupted data test"
Task T014: "Property-based roundtrip test"

# Step 2: Run tests → verify they FAIL → get approval

# Step 3: Launch parallel implementation:
Task T015: "Implement default DEFLATE plugin"
Task T016: "Register DEFLATE plugin"

# Step 4: Sequential implementation (dependencies):
Task T017: "Implement compress() function"
Task T018: "Implement decompress() function"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only - Fastest Path to Value)

1. Complete Phase 1: Setup (T001-T006)
2. Complete Phase 2: Foundational (T007-T011) **← CRITICAL BLOCKER**
3. Complete Phase 3: User Story 1 (T012-T021)
4. **STOP and VALIDATE**: Run T012-T014 tests, verify roundtrip works
5. **DEMO**: Show compress/decompress working with default algorithm
6. **DECISION POINT**: Deploy MVP or continue to next story

### Incremental Delivery (Add Features Progressively)

1. Complete Setup + Foundational → **Foundation Ready** ✓
2. Add User Story 1 → **Test** → **Demo** → **MVP Delivered!** 🎯
3. Add User Story 2 → **Test** → **Demo** → **Plugin Discovery Works!**
4. Add User Story 3 → **Test** → **Demo** → **Smart Selection Works!**
5. Add User Story 4 → **Test** → **Demo** → **Timeout Protection Works!**
6. Polish Phase → **Final QA** → **Production Ready!**

Each story adds incremental value without breaking previous stories.

### Parallel Team Strategy (2-4 Developers)

**Stage 1: Foundation (All Hands)**
- Entire team works together on Setup + Foundational
- Critical path: must be solid before splitting

**Stage 2: Parallel Stories** (After Foundational Complete)
- Developer A: User Story 1 (Core - highest priority)
- Developer B: User Story 2 (Discovery - independent)

**Stage 3: Advanced Features**
- Developer A: User Story 3 (Selection - builds on US2)
- Developer B: User Story 4 (Timeout - builds on US1)

**Stage 4: Quality & Polish**
- All developers: Benchmarks, fuzz tests, documentation in parallel

---

## Task Format Validation

✅ All tasks follow required checklist format:
- Checkbox: `- [ ]` at start
- Task ID: Sequential (T001-T069)
- [P] marker: Present for parallelizable tasks
- [Story] label: Present for all user story phase tasks (US1-US4)
- Description: Clear action with file path
- File paths: Explicit (crush-core/src/*, crush-core/tests/*, benches/*, fuzz/*)

---

## Summary Statistics

**Total Tasks**: 69
**Setup Phase**: 6 tasks
**Foundational Phase**: 5 tasks (CRITICAL BLOCKER)
**User Story 1 (P1)**: 10 tasks (3 tests + 7 implementation) 🎯 MVP
**User Story 2 (P2)**: 11 tasks (4 tests + 7 implementation)
**User Story 3 (P3)**: 11 tasks (4 tests + 7 implementation)
**User Story 4 (P4)**: 12 tasks (5 tests + 7 implementation)
**Polish Phase**: 14 tasks

**Parallel Opportunities**: 37 tasks marked [P] (53% parallelizable)

**Independent Test Criteria**:
- US1: Compress file → decompress → verify identical bytes
- US2: Call init_plugins() → list → verify DEFLATE present
- US3: Multiple plugins → compress → verify highest-scoring selected
- US4: Slow plugin → timeout → verify fallback occurs

**MVP Scope**: Phases 1-3 (21 tasks) delivers basic compression functionality

**Test-First Approach**: All user story phases follow TDD (write tests → fail → implement per constitution)

---

## Notes

- [P] tasks = different files, no sequential dependencies
- [Story] label maps to spec.md user stories for traceability
- Each user story independently completable and testable
- Constitution mandates TDD: tests written FIRST, must FAIL before implementation
- Stop at any checkpoint to validate story independently
- Commit after each task or logical group of parallel tasks
- Quality gates in Phase 7 verify constitution compliance (80% coverage, no clippy warnings, fuzz clean)
