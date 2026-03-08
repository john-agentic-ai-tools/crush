# Feature Specification: CLI GPU Integration Update

**Feature Branch**: `010-cli-gpu-update`
**Created**: 2026-03-02
**Status**: Draft
**Input**: User description: "Update the cli to support latest version of the library and gpu module"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - GPU Plugin Available in CLI (Priority: P1)

As a user, I want the GPU compression plugin to be automatically available when I run the CLI, so that I can compress and decompress files using GPU acceleration without manual setup.

**Why this priority**: Without the GPU plugin being linked into the CLI binary, no other GPU-related CLI features can function. This is the foundational integration that enables all subsequent GPU capabilities.

**Independent Test**: Can be fully tested by running `crush plugins list` and verifying that `gpu-deflate` appears in the plugin list alongside `default` and `parallel-deflate`.

**Acceptance Scenarios**:

1. **Given** a user has installed the crush CLI, **When** they run `crush plugins list`, **Then** `gpu-deflate` appears in the list of available plugins with its metadata (throughput, compression ratio, description).
2. **Given** a user runs `crush plugins info gpu-deflate`, **When** GPU hardware is available, **Then** the output displays GPU device information (name, vendor, estimated VRAM, graphics API backend).
3. **Given** a user runs `crush plugins info gpu-deflate`, **When** no compatible GPU hardware is detected, **Then** the output clearly indicates that GPU acceleration is unavailable and CPU fallback will be used.

---

### User Story 2 - GPU-Accelerated Compression and Decompression (Priority: P2)

As a user, I want to compress files using the GPU-accelerated engine via the CLI, so that I can achieve higher throughput on large datasets when compatible GPU hardware is available.

**Why this priority**: This is the core value proposition of GPU integration. Users need to be able to select GPU compression explicitly and have the system intelligently route decompression based on file format.

**Independent Test**: Can be tested by running `crush compress --plugin gpu-deflate input.dat -o output.crush` and verifying the output file uses the CGPU format, then decompressing it and confirming data integrity.

**Acceptance Scenarios**:

1. **Given** a user has a file to compress and GPU hardware is available, **When** they run `crush compress --plugin gpu-deflate <file>`, **Then** the file is compressed using the GPU-accelerated GDeflate engine and the output uses CGPU format.
2. **Given** a user has a CGPU-format compressed file, **When** they run `crush decompress <file>`, **Then** the file is decompressed using GPU acceleration automatically (format auto-detected via magic bytes).
3. **Given** a user has a CGPU-format compressed file but no GPU hardware, **When** they run `crush decompress <file>`, **Then** the file is decompressed using the CPU fallback path and the user is informed via log output that CPU fallback was used.
4. **Given** a user requests GPU compression but no GPU is available, **When** they run `crush compress --plugin gpu-deflate <file>`, **Then** the system reports a clear error indicating GPU hardware is not available, rather than silently falling back.

---

### User Story 3 - GPU Control Flags (Priority: P3)

As a user, I want CLI flags to control GPU behavior (force CPU, select device), so that I have fine-grained control over which hardware resources are used for compression and decompression.

**Why this priority**: Power users and automated pipelines need explicit control over GPU behavior for reproducibility, debugging, and resource management. This builds on the base GPU integration from P1 and P2.

**Independent Test**: Can be tested by running `crush decompress --force-cpu <gpu-file>` and verifying that decompression completes using CPU only, and by running `crush compress --plugin gpu-deflate --gpu-device 0 <file>` to verify device selection.

**Acceptance Scenarios**:

1. **Given** a user has a CGPU-format file, **When** they run `crush decompress --force-cpu <file>`, **Then** the file is decompressed using the CPU fallback path regardless of GPU availability.
2. **Given** a user has multiple GPUs, **When** they run `crush compress --plugin gpu-deflate --gpu-device 1 <file>`, **Then** compression uses the specified GPU device.
3. **Given** a user provides an invalid `--gpu-device` index, **When** they run the compress command, **Then** a clear error message is shown listing available devices.

---

### User Story 4 - GPU Configuration Persistence (Priority: P4)

As a user, I want to set default GPU preferences in my configuration file, so that I do not have to specify GPU flags on every invocation.

**Why this priority**: Quality-of-life improvement that builds on the control flags from P3. Not essential for GPU functionality but improves daily workflow for users who consistently use GPU acceleration.

**Independent Test**: Can be tested by running `crush config set gpu.enabled true` and then running `crush compress <large-file>` without `--plugin gpu-deflate`, verifying that GPU is used when the file exceeds the auto-selection threshold.

**Acceptance Scenarios**:

1. **Given** a user sets `gpu.enabled` to `true` in configuration, **When** they compress a file that meets the auto-selection criteria (large enough to benefit from GPU), **Then** the GPU plugin is preferred over parallel-deflate in the auto-selection scoring.
2. **Given** a user sets `gpu.device` to a specific device index in configuration, **When** they compress using GPU, **Then** the configured device is used by default.
3. **Given** a user sets `gpu.force-cpu` to `true` in configuration, **When** they decompress a CGPU file, **Then** CPU fallback is used without requiring the `--force-cpu` flag.
4. **Given** a user has GPU configuration set, **When** they provide an explicit CLI flag that conflicts (e.g., `--force-cpu` with `gpu.enabled = true`), **Then** the CLI flag takes precedence over the configuration.

---

### Edge Cases

- What happens when GPU initialization fails mid-compression (e.g., driver crash, device lost)?
  - The operation fails with a clear error message. Partial output files are cleaned up.
- What happens when the user pipes data via stdin with `--plugin gpu-deflate`?
  - GPU compression works with streaming input, buffering tiles as needed.
- What happens when GPU memory is insufficient for the input data?
  - The engine processes data in tile-sized chunks (64KB default), so memory pressure is bounded. If the GPU cannot allocate even minimal buffers, a clear error is returned.
- What happens when `--force-cpu` is used with a non-GPU format file?
  - The flag is silently ignored since CPU decompression is already the default path for non-GPU formats.
- What happens when `--gpu-device` is used without `--plugin gpu-deflate`?
  - A warning is emitted that `--gpu-device` has no effect without GPU plugin selection.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST include the GPU compression engine so that the `gpu-deflate` plugin is automatically registered and available at CLI startup without user intervention.
- **FR-002**: System MUST display GPU plugin information (name, throughput, ratio, description) in `crush plugins list` output.
- **FR-003**: System MUST display GPU device details (device name, vendor, VRAM estimate, graphics API backend) when running `crush plugins info gpu-deflate`, or indicate unavailability if no GPU is detected.
- **FR-004**: Users MUST be able to select the GPU compression engine explicitly via `--plugin gpu-deflate`.
- **FR-005**: System MUST automatically detect CGPU-format files during decompression and route to the GPU engine (with CPU fallback if no GPU hardware is available).
- **FR-006**: System MUST provide a `--force-cpu` flag on the `decompress` subcommand to bypass GPU acceleration for CGPU-format files.
- **FR-007**: System MUST provide a `--gpu-device <index>` flag on both `compress` and `decompress` subcommands for GPU device selection.
- **FR-008**: System MUST report a clear error when GPU compression is explicitly requested but no compatible GPU hardware is available.
- **FR-009**: System MUST support the following GPU-related configuration keys: `gpu.enabled` (bool), `gpu.device` (integer), `gpu.force-cpu` (bool).
- **FR-010**: CLI flags MUST take precedence over configuration file values for all GPU settings.
- **FR-011**: System MUST log (at info level) which backend (GPU device name or CPU fallback) was used for compression or decompression operations.
- **FR-012**: System MUST clean up partial output files if GPU operations fail mid-stream.

### Key Entities

- **GPU Device**: Represents an available GPU compute device — has a name, vendor, VRAM capacity, and supported graphics API backend.
- **GPU Configuration**: User preferences for GPU behavior — enabled state, preferred device index, force-CPU override. Persisted in the user configuration file.
- **Plugin Metadata (extended)**: Existing plugin metadata enriched with optional hardware capability information (GPU availability, device details).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Users can compress a file using GPU acceleration in a single CLI command (`crush compress --plugin gpu-deflate <file>`) with no additional setup steps.
- **SC-002**: GPU-compressed files (CGPU format) are automatically decompressed without users needing to specify the plugin — format detection is seamless.
- **SC-003**: Users with no GPU hardware can still decompress GPU-compressed files via automatic CPU fallback, with no data loss or corruption.
- **SC-004**: GPU device information is discoverable via `crush plugins info gpu-deflate` — users can determine GPU availability in under 5 seconds.
- **SC-005**: All GPU-related CLI flags and configuration options are documented in `crush --help`, `crush compress --help`, and `crush decompress --help` output.
- **SC-006**: GPU compression throughput on files larger than 10 MB exceeds parallel-deflate throughput by at least 50% on supported hardware.
- **SC-007**: Configuration persistence works correctly — users who set GPU preferences via `crush config set` see those preferences applied in subsequent commands without re-specifying flags.

## Assumptions

- The `crush-gpu` crate's public API (compression, decompression, device detection) is stable and does not require changes to support CLI integration.
- The existing plugin registry system (`linkme` distributed slice) supports the `gpu-deflate` plugin without modification — the plugin is already registered in `crush-gpu`.
- GPU device enumeration and capability detection via `wgpu` are synchronous or can be made synchronous (via `pollster`) without blocking the CLI for more than a few seconds.
- The CUDA backend remains an optional compile-time feature (`cuda` feature flag) and is not required for default CLI builds.
- The existing `--plugin` flag on `compress` is sufficient for GPU plugin selection — no new top-level subcommand is needed.
- Auto-selection (when no `--plugin` is specified) currently uses file size thresholds. GPU will be integrated into auto-selection only when `gpu.enabled` configuration is set, to avoid unexpected GPU usage on systems where GPU resources may be shared.
