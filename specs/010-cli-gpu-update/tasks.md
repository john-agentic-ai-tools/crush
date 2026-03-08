# Tasks: CLI GPU Integration Update

**Input**: Design documents from `/specs/010-cli-gpu-update/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Tests are OPTIONAL for this feature. The existing `crush plugins test` roundtrip validation covers GPU plugin correctness. Integration tests may be added in the Polish phase.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Link crush-gpu into the CLI binary so the gpu-deflate plugin is registered at startup

- [x] T001 Add `crush-gpu = { version = "0.1.0", path = "../crush-gpu" }` dependency to crush-cli/Cargo.toml
- [x] T002 Add `use crush_gpu as _;` force-link import to crush-cli/src/main.rs (same pattern as crush-parallel)

**Checkpoint**: `cargo build -p crush-cli` succeeds. Running `crush plugins list` shows `gpu-deflate` alongside `default` and `parallel-deflate`.

---

## Phase 2: Foundational (GPU Configuration API)

**Purpose**: Add the process-global configuration API to crush-gpu so the CLI can pass GPU settings (force-cpu, device index) to the plugin

**⚠️ CRITICAL**: No GPU control flag or config work can begin until this phase is complete

- [x] T003 Add `GpuPluginConfig` struct, `configure()` function, and `get_config()` accessor using `OnceLock` to crush-gpu/src/lib.rs per contracts/gpu-config.md inter-crate API contract
- [x] T004 [P] Re-export `discover_gpu`, `GpuInfo`, and `GpuVendor` from `crush_gpu::backend` in crush-gpu/src/lib.rs for CLI `plugins info` usage
- [x] T005 Update `GpuDeflatePlugin` `CompressionAlgorithm` impl to read `force_cpu` and `device_index` from `get_config()` when constructing `EngineConfig` in crush-gpu/src/lib.rs

**Checkpoint**: `cargo test -p crush-gpu` passes. `configure()` and `get_config()` are callable from external crates. Plugin reads global config for `force_cpu`.

---

## Phase 3: User Story 1 - GPU Plugin Available in CLI (Priority: P1) 🎯 MVP

**Goal**: Users can discover the GPU plugin via `crush plugins list` and inspect GPU device details via `crush plugins info gpu-deflate`

**Independent Test**: Run `crush plugins list` and verify `gpu-deflate` appears. Run `crush plugins info gpu-deflate` and verify GPU device details (or "not available" message) are shown.

### Implementation for User Story 1

- [x] T006 [US1] Enhance `PluginsAction::Info` handler to detect `gpu-deflate` plugin name and call `crush_gpu::discover_gpu()` to display GPU device details (name, vendor, VRAM, backend) or "Not available" message in crush-cli/src/commands/plugins.rs per contracts/cli-flags.md output format
- [x] T007 [US1] Add GPU device info formatting helper function to crush-cli/src/output.rs for human-readable GPU device display (name, vendor, VRAM in MB, API backend)

**Checkpoint**: `crush plugins list` shows gpu-deflate. `crush plugins info gpu-deflate` shows GPU device info or "Not available" message.

---

## Phase 4: User Story 2 - GPU Compression and Decompression (Priority: P2)

**Goal**: Users can compress files with `--plugin gpu-deflate` and decompress CGPU-format files with automatic format detection. Backend usage is logged.

**Independent Test**: Run `crush compress --plugin gpu-deflate <file> -o out.crush` then `crush decompress out.crush -o recovered` and verify data integrity. Check logs for backend info.

### Implementation for User Story 2

- [x] T008 [P] [US2] Add `tracing::info!` log line in compress command after algorithm selection to log which plugin was selected (plugin name and file size) in crush-cli/src/commands/compress.rs
- [x] T009 [P] [US2] Add `tracing::info!` log line in decompress command after decompression to log which plugin handled the file (detected via format/magic) in crush-cli/src/commands/decompress.rs

**Checkpoint**: `crush compress --plugin gpu-deflate testfile -o test.crush` produces a valid CGPU file. `crush decompress test.crush -o recovered` recovers original data. Logs show plugin/backend info at `-v` verbosity.

---

## Phase 5: User Story 3 - GPU Control Flags (Priority: P3)

**Goal**: Users can control GPU behavior with `--force-cpu` (decompress) and `--gpu-device <index>` (compress/decompress) CLI flags

**Independent Test**: Run `crush decompress --force-cpu <gpu-file>` and verify CPU fallback is used. Run `crush compress --plugin gpu-deflate --gpu-device 0 <file>` and verify device selection.

### Implementation for User Story 3

- [x] T010 [US3] Add `--force-cpu` bool flag to `DecompressArgs` and `--gpu-device` optional u32 flag to both `CompressArgs` and `DecompressArgs` in crush-cli/src/cli.rs per contracts/cli-flags.md
- [x] T011 [US3] Wire GPU CLI flags into `crush_gpu::configure()` call in `run()` function — extract force_cpu from DecompressArgs and gpu_device from CompressArgs/DecompressArgs, call configure() before command dispatch in crush-cli/src/main.rs
- [x] T012 [P] [US3] Add warning log when `--gpu-device` is specified without `--plugin gpu-deflate` in crush-cli/src/commands/compress.rs
- [x] T013 [P] [US3] Add info log when `--force-cpu` is active during CGPU-format decompression in crush-cli/src/commands/decompress.rs

**Checkpoint**: `crush decompress --force-cpu <gpu-file>` decompresses using CPU. `crush compress --gpu-device 0 <file>` without `--plugin gpu-deflate` emits warning. Help text shows new flags.

---

## Phase 6: User Story 4 - GPU Configuration Persistence (Priority: P4)

**Goal**: Users can set GPU preferences in config file (`gpu.enabled`, `gpu.device`, `gpu.force-cpu`) that persist across invocations

**Independent Test**: Run `crush config set gpu.enabled true`, then `crush config get gpu.enabled` returns `true`. Config values merge with CLI flags (CLI wins).

### Implementation for User Story 4

- [x] T014 [US4] Add `GpuConfig` struct with `enabled`, `device`, `force_cpu` fields and `Default` impl to crush-cli/src/config.rs, and add `gpu: GpuConfig` field to `Config` struct per data-model.md
- [x] T015 [US4] Add `CRUSH_GPU_ENABLED`, `CRUSH_GPU_DEVICE`, `CRUSH_GPU_FORCE_CPU` env var merging in `merge_env_vars()` and add `--force-cpu`/`--gpu-device` CLI arg merging in `merge_cli_args()` in crush-cli/src/config.rs
- [x] T016 [US4] Add `gpu.enabled`, `gpu.device`, `gpu.force-cpu` key support to `get_config_value()` and `set_config_value()` functions, and add GPU config validation in `validate()` in crush-cli/src/config.rs
- [x] T017 [US4] Update `crush_gpu::configure()` call in main.rs to merge config.gpu values with CLI flags (CLI flags take precedence) in crush-cli/src/main.rs
- [x] T018 [US4] Update `select_algorithm()` to accept `gpu_enabled` parameter — when true and file >= 25 MB threshold, return `"gpu-deflate"` instead of `"parallel-deflate"` in crush-cli/src/algorithm.rs

**Checkpoint**: `crush config set gpu.enabled true` persists to TOML. `crush config list` shows `[gpu]` section. Auto-selection prefers gpu-deflate for large files when enabled. CLI flags override config values.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Quality gates, cleanup, and cross-cutting validation

- [x] T019 Run `cargo clippy --all-targets -- -D warnings` across workspace and fix all warnings
- [x] T020 Run `cargo test` across workspace and ensure all tests pass
- [x] T021 Run `cargo doc --no-deps` and fix any documentation warnings
- [x] T022 Verify end-to-end: compress with gpu-deflate → decompress → verify data integrity → check logs

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - US1 → US2 → US3 → US4 (sequential, each builds on previous)
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Independent of US1 (compression/decompression already works after setup)
- **User Story 3 (P3)**: Depends on Foundational (Phase 2) for configure() API - Independent of US1/US2
- **User Story 4 (P4)**: Depends on US3 (CLI flags wired in main.rs) for configure() call to merge with

### Within Each User Story

- Core changes before edge case handling
- Same-file tasks run sequentially
- Different-file tasks marked [P] can run in parallel

### Parallel Opportunities

- T003 and T004 can run in parallel (different concerns in same file, but T004 is a simple re-export)
- T008 and T009 can run in parallel (different files: compress.rs vs decompress.rs)
- T012 and T013 can run in parallel (different files: compress.rs vs decompress.rs)

---

## Parallel Example: User Story 2

```bash
# These two tasks can run in parallel (different files):
Task T008: "Add plugin selection logging in crush-cli/src/commands/compress.rs"
Task T009: "Add format detection logging in crush-cli/src/commands/decompress.rs"
```

## Parallel Example: User Story 3

```bash
# After T010 and T011 complete, these can run in parallel:
Task T012: "Add --gpu-device warning in crush-cli/src/commands/compress.rs"
Task T013: "Add --force-cpu logging in crush-cli/src/commands/decompress.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T002)
2. Complete Phase 2: Foundational (T003-T005)
3. Complete Phase 3: User Story 1 (T006-T007)
4. **STOP and VALIDATE**: `crush plugins list` shows gpu-deflate, `crush plugins info gpu-deflate` shows GPU details

### Incremental Delivery

1. Setup + Foundational → GPU plugin registered and configurable
2. Add User Story 1 → Plugin discovery works (MVP!)
3. Add User Story 2 → Compression/decompression with logging
4. Add User Story 3 → Fine-grained GPU control via CLI flags
5. Add User Story 4 → Configuration persistence and auto-selection
6. Each story adds value without breaking previous stories

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- crush-core is NOT modified — the CompressionAlgorithm trait remains unchanged
- The configure() API uses OnceLock — can only be called once per process
- GPU device discovery is lazy — only happens when plugins info or gpu-deflate plugin is used
- Total files modified: ~10 across crush-cli and crush-gpu
