# Implementation Tasks: CLI Implementation

**Feature**: CLI Implementation | **Branch**: `005-cli-implementation` | **Date**: 2026-01-22
**Spec**: [spec.md](spec.md) | **Plan**: [plan.md](plan.md)

## Overview

This document breaks down the CLI implementation into actionable tasks organized by user story. Each user story phase is independently testable and deliverable, enabling incremental development and early value delivery.

**Total Tasks**: 174
**User Stories**: 9 (P1-P9, including pipeline integration)
**Parallel Opportunities**: 47 tasks can run in parallel within their phases

## Implementation Strategy

### MVP First (Recommended)
Complete **Phase 1 (Setup)** → **Phase 2 (Foundational)** → **Phase 3 (User Story 1)** → Validate → Deploy

This delivers basic file compression/decompression as the minimum viable product.

### Incremental Delivery
After foundational phase, each user story can be developed independently:
- **Phase 3 (US1)**: Basic compress/decompress - Core MVP
- **Phase 4 (US2)**: File inspection - Diagnostic capability
- **Phase 5 (US3)**: Progress bars - UX enhancement
- **Phase 6 (US4)**: Verbose mode - Power user features
- **Phase 7 (US5)**: Configuration - Workflow efficiency
- **Phase 8 (US6)**: Plugin management - Advanced features
- **Phase 9 (US7)**: Help system - Discoverability
- **Phase 10 (US8)**: Production logging - Operations support
- **Phase 11 (US9)**: Pipeline integration - stdin/stdout support

### Parallel Development
After foundational tasks, different developers can work on different user stories simultaneously without conflicts.

---

## Phase 1: Setup

**Goal**: Initialize CLI crate structure and dependencies

**Tasks**:

- [X] T001 Update crush-cli/Cargo.toml with required dependencies (clap v4, indicatif, termcolor, serde, toml, tracing, is-terminal, ctrlc, dirs, filetime) and dev dependencies (assert_cmd, predicates, tempfile)
- [X] T002 Create src/cli.rs with clap derive structures for root Cli and Commands enum
- [X] T003 Create src/error.rs with CliError enum wrapping CrushError
- [X] T004 Create src/commands/mod.rs with module declarations
- [X] T005 Create empty command modules: src/commands/compress.rs, src/commands/decompress.rs, src/commands/inspect.rs, src/commands/config.rs, src/commands/plugins.rs
- [X] T006 Create src/config.rs with Config struct and default implementations
- [X] T007 Create src/output.rs with stub functions for formatting output
- [X] T008 Create src/logging.rs with tracing subscriber initialization
- [X] T009 Create src/signal.rs with Ctrl+C handler stub
- [X] T010 Create tests/integration/ directory structure
- [X] T011 Create tests/fixtures/ directory for test data
- [X] T012 Update src/main.rs to parse CLI arguments and dispatch to command handlers

**Validation**: `cargo build` succeeds, all modules compile

---

## Phase 2: Foundational

**Goal**: Implement shared infrastructure required by all user stories

**Tasks**:

- [X] T013 [P] Implement CliError Display trait with user-friendly error messages in src/error.rs
- [X] T014 [P] Implement exit code mapping (0, 1, 2, 130) in src/error.rs
- [X] T015 [P] Implement Config loading from TOML file in src/config.rs (config_file_path function using dirs crate)
- [X] T016 [P] Implement Config validation in src/config.rs
- [X] T017 [P] Implement environment variable merging in src/config.rs (CRUSH_* prefix)
- [X] T018 [P] Implement CLI argument override merging in src/config.rs
- [X] T019 [P] Implement CliState struct with terminal detection (is-terminal) in src/main.rs
- [X] T020 [P] Implement Ctrl+C signal handler with AtomicBool in src/signal.rs
- [X] T021 [P] Implement tracing subscriber setup (human and JSON formats) in src/logging.rs
- [X] T022 [P] Implement color detection and termcolor setup in src/output.rs
- [X] T023 [P] Implement CompressionLevel to ScoringWeights conversion in src/cli.rs
- [X] T024 Integrate signal handler into main() function in src/main.rs
- [X] T025 Add integration test helper functions in tests/integration/common.rs (Command::cargo_bin, tempfile setup)

**Validation**: All foundational utilities compile and unit tests pass

---

## Phase 3: User Story 1 - Basic File Compression and Decompression (P1)

**Goal**: Enable users to compress and decompress files from command line

**Independent Test Criteria**:
- Compress a file → verify .crush file created and smaller than original
- Decompress .crush file → verify restored file matches original exactly
- Error handling: missing files, corrupted data, existing outputs

**Tasks**:

### Integration Tests (TDD)
- [X] T026 [P] [US1] Write compress integration test: basic file compression in tests/integration/compress_tests.rs
- [X] T027 [P] [US1] Write compress integration test: file not found error in tests/integration/compress_tests.rs
- [X] T028 [P] [US1] Write compress integration test: output already exists error in tests/integration/compress_tests.rs
- [X] T029 [P] [US1] Write compress integration test: force overwrite in tests/integration/compress_tests.rs
- [X] T030 [P] [US1] Write compress integration test: keep input file in tests/integration/compress_tests.rs
- [X] T031 [P] [US1] Write decompress integration test: basic decompression in tests/integration/decompress_tests.rs
- [X] T032 [P] [US1] Write decompress integration test: CRC32 validation failure in tests/integration/decompress_tests.rs
- [X] T033 [P] [US1] Write decompress integration test: invalid header error in tests/integration/decompress_tests.rs
- [X] T034 [P] [US1] Write roundtrip test: compress → decompress preserves data in tests/integration/roundtrip_tests.rs

### Implementation
- [X] T035 [US1] Implement CompressArgs parsing in src/cli.rs
- [X] T036 [US1] Implement DecompressArgs parsing in src/cli.rs
- [X] T037 [US1] Implement compress command handler in src/commands/compress.rs (validate input, determine output path, call crush_core::compress_with_options, handle errors)
- [X] T038 [US1] Implement decompress command handler in src/commands/decompress.rs (validate input, determine output path, call crush_core::decompress, handle errors, verify CRC32)
- [X] T039 [US1] Implement input file validation in src/commands/compress.rs (file exists, readable, not directory)
- [X] T040 [US1] Implement output path validation in src/commands/compress.rs (parent directory exists, handle --force flag)
- [X] T041 [US1] Implement file cleanup logic in src/commands/compress.rs (delete input if not --keep)
- [X] T042 [US1] Implement file cleanup logic in src/commands/decompress.rs (delete .crush if not --keep)
- [X] T043 [US1] Implement batch processing loop for multiple files in src/commands/compress.rs
- [X] T044 [US1] Implement batch processing loop for multiple files in src/commands/decompress.rs
- [X] T045 [US1] Implement basic output formatting (success messages) in src/output.rs
- [X] T046 [US1] Wire compress command to main dispatcher in src/main.rs
- [X] T047 [US1] Wire decompress command to main dispatcher in src/main.rs

### File Metadata Preservation
- [X] T048 [P] [US1] Write test: compressed file preserves mtime on Linux in tests/integration/compress_tests.rs
- [X] T049 [P] [US1] Write test: compressed file preserves mtime on macOS in tests/integration/compress_tests.rs
- [X] T050 [P] [US1] Write test: compressed file preserves mtime on Windows in tests/integration/compress_tests.rs
- [X] T051 [P] [US1] Write test: compressed file preserves Unix permissions in tests/integration/compress_tests.rs (cfg(unix) only)
- [X] T052 [P] [US1] Write test: decompression handles missing metadata gracefully in tests/integration/decompress_tests.rs
- [X] T053 [US1] Store original mtime in compression metadata in src/commands/compress.rs (extend metadata struct)
- [X] T054 [US1] Store Unix permissions in metadata on Unix platforms in src/commands/compress.rs (cfg(unix))
- [X] T055 [US1] Restore mtime after decompression in src/commands/decompress.rs (using filetime::set_file_mtime)
- [X] T056 [US1] Restore Unix permissions after decompression in src/commands/decompress.rs (cfg(unix))
- [X] T057 [US1] Add warning log when metadata restoration fails in src/commands/decompress.rs (warn, don't fail)

**Validation**: All US1 integration tests pass, manual compress/decompress roundtrip succeeds

---

## Phase 4: User Story 2 - File Inspection and Metadata Display (P2)

**Goal**: Enable users to inspect compressed file metadata without decompression

**Independent Test Criteria**:
- Inspect .crush file → verify metadata displayed (sizes, ratio, plugin, CRC status)
- Inspect corrupt file → verify CRC failure reported
- Inspect multiple files → verify summary statistics shown

**Tasks**:

### Integration Tests (TDD)
- [X] T058 [P] [US2] Write inspect integration test: single file metadata display in tests/integration/inspect_tests.rs
- [X] T059 [P] [US2] Write inspect integration test: CRC validation reporting in tests/integration/inspect_tests.rs
- [X] T060 [P] [US2] Write inspect integration test: invalid header error in tests/integration/inspect_tests.rs
- [X] T061 [P] [US2] Write inspect integration test: multiple files with summary in tests/integration/inspect_tests.rs
- [X] T062 [P] [US2] Write inspect integration test: JSON output format in tests/integration/inspect_tests.rs
- [X] T063 [P] [US2] Write inspect integration test: CSV output format in tests/integration/inspect_tests.rs

### Implementation
- [X] T064 [US2] Implement InspectArgs parsing in src/cli.rs (with OutputFormat enum)
- [X] T065 [US2] Implement InspectResult struct in src/output.rs (file_path, sizes, ratio, plugin, crc_valid)
- [X] T066 [US2] Implement inspect command handler in src/commands/inspect.rs (read header, extract metadata, validate CRC)
- [X] T067 [US2] Implement human-readable formatting for InspectResult in src/output.rs
- [X] T068 [US2] Implement JSON formatting for InspectResult in src/output.rs (using serde_json)
- [X] T069 [US2] Implement CSV formatting for InspectResult in src/output.rs
- [X] T070 [US2] Implement summary statistics aggregation in src/commands/inspect.rs (total sizes, average ratio)
- [X] T071 [US2] Implement batch inspection loop for multiple files in src/commands/inspect.rs
- [X] T072 [US2] Wire inspect command to main dispatcher in src/main.rs

**Validation**: All US2 integration tests pass, inspect command displays accurate metadata

---

## Phase 5: User Story 3 - Progress Feedback for Long-Running Operations (P3)

**Goal**: Display progress bars and statistics during compression/decompression

**Independent Test Criteria**:
- Compress large file → verify progress bar appears and updates
- Small file compression → verify no flicker (progress hidden or brief)
- Ctrl+C during operation → verify graceful cleanup

**Tasks**:

### Integration Tests (TDD)
- [X] T073 [P] [US3] Write progress test: large file shows progress bar in tests/integration/compress_tests.rs
- [X] T074 [P] [US3] Write progress test: final statistics displayed in tests/integration/compress_tests.rs
- [X] T075 [P] [US3] Write interrupt test: Ctrl+C cleanup in tests/integration/compress_tests.rs (using Command::kill)

### Implementation
- [X] T076 [US3] Add indicatif dependency usage in src/output.rs (ProgressBar, ProgressStyle)
- [X] T077 [US3] Implement progress bar creation in src/output.rs (detect TTY, set draw target to 10Hz)
- [X] T078 [US3] Implement progress callback for compression in src/commands/compress.rs (update bytes processed)
- [X] T079 [US3] Implement progress callback for decompression in src/commands/decompress.rs
- [X] T080 [US3] Implement progress bar hiding for small files (<1MB) in src/output.rs
- [X] T081 [US3] Implement CompressionResult struct in src/output.rs (sizes, ratio, duration, throughput, plugin)
- [X] T082 [US3] Implement DecompressionResult struct in src/output.rs
- [X] T083 [US3] Implement final statistics formatting in src/output.rs (time, throughput, ratio)
- [X] T084 [US3] Integrate interrupt check into compression loop in src/commands/compress.rs (check AtomicBool)
- [X] T085 [US3] Implement partial file cleanup on interrupt in src/commands/compress.rs
- [X] T086 [US3] Handle interrupted error (exit code 130) in src/main.rs

**Validation**: All US3 integration tests pass, progress bars display during manual testing with large files

---

## Phase 6: User Story 4 - Verbose Mode with Diagnostic Information (P4)

**Goal**: Display detailed diagnostic information when --verbose flag is used

**Independent Test Criteria**:
- Run with -v → verify plugin selection, thread count, throughput shown
- Run with -vv → verify trace-level details logged
- Verbose output includes all required fields per contract

**Tasks**:

### Integration Tests (TDD)
- [ ] T087 [P] [US4] Write verbose test: plugin selection logged in tests/integration/compress_tests.rs
- [ ] T088 [P] [US4] Write verbose test: performance metrics logged in tests/integration/compress_tests.rs
- [ ] T089 [P] [US4] Write verbose test: debug level output with -v in tests/integration/compress_tests.rs
- [ ] T090 [P] [US4] Write verbose test: trace level output with -vv in tests/integration/compress_tests.rs

### Implementation
- [ ] T091 [US4] Implement verbose level mapping in src/logging.rs (0=INFO, 1=DEBUG, 2=TRACE)
- [ ] T092 [US4] Add tracing spans to compress command in src/commands/compress.rs (operation span with file path)
- [ ] T093 [US4] Add tracing events for plugin selection in src/commands/compress.rs
- [ ] T094 [US4] Add tracing events for thread count and hardware acceleration in src/commands/compress.rs
- [ ] T095 [US4] Add tracing events for performance metrics in src/commands/compress.rs (throughput, ratio, duration)
- [ ] T096 [US4] Add tracing spans to decompress command in src/commands/decompress.rs
- [ ] T097 [US4] Implement detailed summary output in verbose mode in src/output.rs
- [ ] T098 [US4] Integrate verbose flag into CliState in src/main.rs

**Validation**: All US4 integration tests pass, -v and -vv show expected diagnostic details

---

## Phase 7: User Story 5 - Configuration Management (P5)

**Goal**: Enable persistent configuration via config file and commands

**Independent Test Criteria**:
- Set config value → verify persisted across invocations
- List config → verify all settings displayed
- Reset config → verify defaults restored
- Invalid config → verify clear error with defaults used

**Tasks**:

### Integration Tests (TDD)
- [ ] T099 [P] [US5] Write config test: set and get value in tests/integration/config_tests.rs
- [ ] T100 [P] [US5] Write config test: list all settings in tests/integration/config_tests.rs
- [ ] T101 [P] [US5] Write config test: reset to defaults in tests/integration/config_tests.rs
- [ ] T102 [P] [US5] Write config test: invalid key error in tests/integration/config_tests.rs
- [ ] T103 [P] [US5] Write config test: invalid value error in tests/integration/config_tests.rs
- [ ] T104 [P] [US5] Write config test: config affects compression in tests/integration/config_tests.rs

### Implementation
- [ ] T105 [US5] Implement ConfigArgs and ConfigAction parsing in src/cli.rs
- [ ] T106 [US5] Implement config set command in src/commands/config.rs (parse key, update Config, write TOML)
- [ ] T107 [US5] Implement config get command in src/commands/config.rs (parse key, read Config, print value)
- [ ] T108 [US5] Implement config list command in src/commands/config.rs (read Config, format as TOML)
- [ ] T109 [US5] Implement config reset command in src/commands/config.rs (confirmation prompt, write defaults)
- [ ] T110 [US5] Implement config key validation in src/commands/config.rs (valid paths: compression.*, output.*, logging.*)
- [ ] T111 [US5] Implement config value validation in src/commands/config.rs (type checking, enum validation)
- [ ] T112 [US5] Wire config command to main dispatcher in src/main.rs

**Validation**: All US5 integration tests pass, config persists correctly across CLI invocations

---

## Phase 8: User Story 6 - Plugin Discovery and Management (P6)

**Goal**: Enable users to list, inspect, and test compression plugins

**Independent Test Criteria**:
- List plugins → verify all registered plugins shown with metadata
- Plugin info → verify detailed information displayed
- Plugin test → verify roundtrip validation succeeds

**Tasks**:

### Integration Tests (TDD)
- [ ] T113 [P] [US6] Write plugins test: list all plugins in tests/integration/plugins_tests.rs
- [ ] T114 [P] [US6] Write plugins test: list JSON format in tests/integration/plugins_tests.rs
- [ ] T115 [P] [US6] Write plugins test: plugin info details in tests/integration/plugins_tests.rs
- [ ] T116 [P] [US6] Write plugins test: plugin not found error in tests/integration/plugins_tests.rs
- [ ] T117 [P] [US6] Write plugins test: plugin self-test passes in tests/integration/plugins_tests.rs

### Implementation
- [ ] T118 [US6] Implement PluginsArgs and PluginsAction parsing in src/cli.rs
- [ ] T119 [US6] Implement plugins list command in src/commands/plugins.rs (call crush_core::list_plugins, format output)
- [ ] T120 [US6] Implement human-readable plugin list formatting in src/output.rs
- [ ] T121 [US6] Implement JSON plugin list formatting in src/output.rs
- [ ] T122 [US6] Implement plugins info command in src/commands/plugins.rs (get PluginMetadata, format details)
- [ ] T123 [US6] Implement detailed plugin info formatting in src/output.rs (benchmarks, characteristics)
- [ ] T124 [US6] Implement plugins test command in src/commands/plugins.rs (compress test data, decompress, verify roundtrip)
- [ ] T125 [US6] Wire plugins command to main dispatcher in src/main.rs

**Validation**: All US6 integration tests pass, plugin discovery works correctly

---

## Phase 9: User Story 7 - Comprehensive Help System (P7)

**Goal**: Provide built-in help documentation for all commands

**Independent Test Criteria**:
- Run --help → verify all commands listed
- Run compress --help → verify detailed usage shown
- Typo command → verify "did you mean" suggestion
- Help includes examples

**Tasks**:

### Integration Tests (TDD)
- [ ] T126 [P] [US7] Write help test: root --help shows all commands in tests/integration/help_tests.rs
- [ ] T127 [P] [US7] Write help test: compress --help shows options in tests/integration/help_tests.rs
- [ ] T128 [P] [US7] Write help test: invalid command suggests alternative in tests/integration/help_tests.rs

### Implementation
- [ ] T129 [US7] Add detailed help text to Cli struct in src/cli.rs (about, long_about)
- [ ] T130 [US7] Add help text to CompressArgs in src/cli.rs (arg descriptions, examples in after_help)
- [ ] T131 [US7] Add help text to DecompressArgs in src/cli.rs
- [ ] T132 [US7] Add help text to InspectArgs in src/cli.rs
- [ ] T133 [US7] Add help text to ConfigArgs in src/cli.rs
- [ ] T134 [US7] Add help text to PluginsArgs in src/cli.rs
- [ ] T135 [US7] Enable clap's "did you mean" suggestions in src/cli.rs
- [ ] T136 [US7] Add examples section to help output using clap's after_help in src/cli.rs

**Validation**: All US7 integration tests pass, help text is comprehensive and accurate

---

## Phase 10: User Story 8 - Production Logging and Instrumentation (P8)

**Goal**: Provide structured logging for production monitoring

**Independent Test Criteria**:
- Run with --log-format=json → verify JSON events logged
- Error logging → verify full context included
- Log to file → verify file written correctly
- Concurrent operations → verify log entries distinguishable

**Tasks**:

### Integration Tests (TDD)
- [ ] T137 [P] [US8] Write logging test: JSON format output in tests/integration/logging_tests.rs
- [ ] T138 [P] [US8] Write logging test: error context in logs in tests/integration/logging_tests.rs
- [ ] T139 [P] [US8] Write logging test: log file creation in tests/integration/logging_tests.rs

### Implementation
- [ ] T140 [US8] Implement JSON tracing formatter in src/logging.rs (using tracing_subscriber::fmt::json)
- [ ] T141 [US8] Implement log file output in src/logging.rs (file appender)
- [ ] T142 [US8] Add operation IDs to tracing spans in src/commands/compress.rs (unique per file)
- [ ] T143 [US8] Add structured fields to tracing events in src/commands/compress.rs (input_size, output_size, ratio, throughput)
- [ ] T144 [US8] Implement error event logging in src/error.rs (capture error context)
- [ ] T145 [US8] Add tracing events to all command handlers in src/commands/*.rs
- [ ] T146 [US8] Integrate log-format and log-file args into logging setup in src/main.rs

**Validation**: All US8 integration tests pass, JSON logs are well-formed and contain required fields

---

## Phase 11: User Story 9 - Pipeline Integration (stdin/stdout)

**Goal**: Enable stdin/stdout support for pipeline integration

**Independent Test Criteria**:
- Read from stdin → compress to file or stdout
- Decompress file to stdout → verify output correct
- Pipeline chaining works: `cat file | crush compress | crush decompress`
- Progress bars disabled appropriately for non-seekable streams

**Tasks**:

### Integration Tests (TDD)
- [ ] T151 [P] [US9] Write pipeline test: stdin to file compression in tests/integration/compress_tests.rs
- [ ] T152 [P] [US9] Write pipeline test: stdin to stdout compression in tests/integration/compress_tests.rs
- [ ] T153 [P] [US9] Write pipeline test: file to stdout decompression in tests/integration/decompress_tests.rs
- [ ] T154 [P] [US9] Write pipeline test: full pipeline (stdin compress | decompress stdout) in tests/integration/pipeline_tests.rs
- [ ] T155 [P] [US9] Write pipeline test: verify progress bars hidden for stdin in tests/integration/compress_tests.rs

### Implementation
- [ ] T156 [US9] Implement stdin detection in src/commands/compress.rs (using is_terminal on stdin)
- [ ] T157 [US9] Implement stdin reading in src/commands/compress.rs (read to buffer or temp file)
- [ ] T158 [US9] Update progress bar logic to hide when stdin detected in src/output.rs
- [ ] T159 [US9] Implement stdout mode for decompress in src/commands/decompress.rs (--stdout flag handling)
- [ ] T160 [US9] Update output validation to allow stdout mode in src/commands/decompress.rs (skip file existence checks)
- [ ] T161 [US9] Add pipeline examples to help text in src/cli.rs (compress --help, decompress --help)

**Validation**: All US9 integration tests pass, pipeline workflows work correctly

---

## Phase 12: Polish & Cross-Cutting Concerns

**Goal**: Final quality improvements and performance validation

**Tasks**:

- [ ] T161 [P] Add benchmarks for CLI startup time in benches/cli_startup.rs (target: <50ms)
- [ ] T162 [P] Add benchmarks for help command in benches/help_command.rs (target: <100ms)
- [ ] T163 [P] Run clippy with pedantic lints on crush-cli crate
- [ ] T164 [P] Run cargo fmt on crush-cli crate
- [ ] T165 [P] Generate documentation with cargo doc for crush-cli
- [ ] T166 [P] Verify all TODO/FIXME comments resolved in crush-cli/src
- [ ] T167 Run full integration test suite (all 50+ tests)
- [ ] T168 Measure code coverage (target: >80% for crush-cli)
- [ ] T169 Run benchmarks and verify performance targets met
- [ ] T170 Update README.md with CLI usage examples
- [ ] T171 Manual testing: compress/decompress 10GB file with progress
- [ ] T172 Manual testing: batch operations with 100+ files
- [ ] T173 Manual testing: Ctrl+C interrupt during compression
- [ ] T174 Manual testing: all help commands render correctly

**Validation**: All quality gates pass, performance targets met, documentation complete

---

## Dependencies Between User Stories

### Story Completion Order

```
Phase 1 (Setup) ──┐
                  ├──> Phase 2 (Foundational) ──┬──> Phase 3 (US1) [REQUIRED MVP]
                  │                              │
                  └──────────────────────────────┼──> Phase 4 (US2) [Independent]
                                                 ├──> Phase 5 (US3) [Depends on US1]
                                                 ├──> Phase 6 (US4) [Depends on US1]
                                                 ├──> Phase 7 (US5) [Independent]
                                                 ├──> Phase 8 (US6) [Independent]
                                                 ├──> Phase 9 (US7) [Independent]
                                                 ├──> Phase 10 (US8) [Depends on US1]
                                                 └──> Phase 11 (US9) [Depends on US1]
```

**Dependencies**:
- **US1 (P1)**: No dependencies (MVP foundation)
- **US2 (P2)**: Independent (only needs crush-core)
- **US3 (P3)**: Depends on US1 (enhances compress/decompress with progress)
- **US4 (P4)**: Depends on US1 (enhances compress/decompress with verbose output)
- **US5 (P5)**: Independent (config system doesn't require other features)
- **US6 (P6)**: Independent (plugin management uses crush-core directly)
- **US7 (P7)**: Independent (help system is metadata-only)
- **US8 (P8)**: Depends on US1 (logs compress/decompress operations)
- **US9 (FR-023)**: Depends on US1 (adds stdin/stdout pipeline support to compress/decompress)

### Recommended Implementation Order

1. **MVP**: Phase 1 → Phase 2 → Phase 3 (US1) → Validate
2. **Incremental**: US2, US5, US6, US7 (all independent, any order)
3. **Enhancements**: US3, US4, US8, US9 (depend on US1)
4. **Polish**: Phase 12

---

## Parallel Execution Examples

### Phase 2 (Foundational) - 11 parallel tasks
```bash
# All T013-T023 can run in parallel (different files, no dependencies)
T013, T014, T015, T016, T017, T018, T019, T020, T021, T022, T023
```

### Phase 3 (US1) - 9 parallel test tasks
```bash
# All T026-T034 can run in parallel (integration tests, independent)
T026, T027, T028, T029, T030, T031, T032, T033, T034
```

### Phase 4 (US2) - 6 parallel test tasks
```bash
# All T048-T053 can run in parallel
T048, T049, T050, T051, T052, T053
```

### Phase 5 (US3) - 3 parallel test tasks
```bash
# T073, T074, T075 can run in parallel
T073, T074, T075
```

### Phase 11 (US9) - 5 parallel test tasks
```bash
# T151, T152, T153, T154, T155 can run in parallel
T151, T152, T153, T154, T155
```

**Total Parallel Opportunities**: 47 tasks across all phases

---

## Task Validation Checklist

- [X] All tasks follow strict checklist format: `- [ ] [TaskID] [P?] [Story?] Description with file path`
- [X] All user story phase tasks include [US#] label
- [X] All parallelizable tasks marked with [P]
- [X] Each phase has clear goal and independent test criteria
- [X] File paths are explicit in task descriptions
- [X] Integration tests written before implementation (TDD approach)
- [X] Dependencies between stories documented
- [X] MVP scope clearly identified (US1)
- [X] Parallel execution examples provided

---

## Success Metrics

**Phase 3 (US1) Completion Criteria**:
- [ ] All 32 US1 tasks completed (including metadata preservation)
- [ ] All integration tests pass (14 tests)
- [ ] Manual roundtrip test succeeds
- [ ] Compressed file is valid and smaller than input
- [ ] Decompressed file matches original byte-for-byte
- [ ] File timestamps and permissions preserved across compress/decompress

**Full Feature Completion Criteria**:
- [ ] All 174 tasks completed
- [ ] 60+ integration tests passing (including pipeline tests)
- [ ] Code coverage >80%
- [ ] CLI startup time <50ms
- [ ] Help command <100ms
- [ ] All clippy lints resolved
- [ ] Documentation generated successfully
- [ ] Manual testing scenarios pass
- [ ] Pipeline integration works (stdin/stdout)

---

**Next Step**: Run `/speckit.implement` to begin executing these tasks phase by phase.
