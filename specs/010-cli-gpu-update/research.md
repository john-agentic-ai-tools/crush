# Research: CLI GPU Integration Update

**Feature**: 010-cli-gpu-update
**Date**: 2026-03-02

## R1: Plugin Configuration Side-Channel Pattern

**Question**: How to pass GPU-specific configuration (force_cpu, device_index) through the `CompressionAlgorithm` trait interface, which only accepts `(input, cancel_flag)`?

**Decision**: Process-global `OnceLock<GpuPluginConfig>` in `crush-gpu`, set once at CLI startup.

**Rationale**: The `CompressionAlgorithm` trait is a stable interface shared by 3 plugins (default, parallel-deflate, gpu-deflate). Changing it would require updating all implementations and the core library's compress/decompress dispatch. A process-global config set once at startup is:
- Thread-safe (`OnceLock` guarantees single initialization)
- Zero overhead on read path (no locking after initialization)
- Consistent with `init_plugins()` pattern already used at startup
- Testable (tests call `configure()` before exercising GPU code paths)

**Alternatives Considered**:
1. **Extend `CompressionAlgorithm` trait**: Breaking change to all plugins. Over-engineering for one plugin's needs.
2. **`HashMap<String, String>` in `CompressionOptions`**: Requires changing `compress_with_options` signature and trait dispatch. Still doesn't help `decompress()` which takes no options.
3. **Environment variables**: Fragile, not testable, bad ergonomics.
4. **Atomics in `crush-gpu::lib`**: Works but `OnceLock` is cleaner — config is set once, not mutated.

---

## R2: GPU Plugin Force-Linking Pattern

**Question**: How does `crush-gpu` get linked into the CLI binary for plugin registration?

**Decision**: Same pattern as `crush-parallel`: `use crush_gpu as _;` in `main.rs`.

**Rationale**: The `linkme` distributed slice (`COMPRESSION_ALGORITHMS`) requires the crate to be linked into the binary. Rust's linker will eliminate unused crates unless they have a side effect (like a `linkme` registration). The `use crate_name as _;` pattern is idiomatic Rust for force-linking. It's already used for `crush-parallel` and requires zero code in the imported crate.

**Alternatives Considered**:
1. **Cargo feature flag**: Would make GPU opt-in at compile time, but the spec requires GPU always available (FR-001).
2. **Dynamic loading**: Rust doesn't have a standard plugin loading mechanism. `libloading` would add complexity.

---

## R3: GPU Device Discovery Timing

**Question**: When should GPU device detection occur — at CLI startup or lazily?

**Decision**: Lazy detection. GPU devices are probed only when:
1. `crush plugins info gpu-deflate` is run
2. `--plugin gpu-deflate` is used for compression
3. Decompression encounters a CGPU-format file

**Rationale**: `wgpu` device enumeration involves driver calls that can take 1-3 seconds on some systems. Adding this to CLI startup would penalize all commands (even `crush --help`). The GPU plugin's `CompressionAlgorithm::compress/decompress` methods already call `discover_gpu()` internally, so lazy detection is the existing behavior.

**Alternatives Considered**:
1. **Eager startup detection**: Penalizes all CLI invocations. Users running `crush config list` shouldn't wait for GPU init.
2. **Background async detection**: Adds async complexity to a synchronous CLI. Over-engineering.

---

## R4: Algorithm Auto-Selection with GPU

**Question**: How should GPU interact with the existing auto-selection logic in `algorithm.rs`?

**Decision**: When `gpu.enabled = true` in config, GPU is considered as an alternative to `parallel-deflate` for files >= 25 MB. GPU is preferred when it scores higher in the existing plugin scoring system (which already favors throughput).

**Rationale**: The plugin scoring system (`calculate_plugin_score`) uses throughput and compression ratio with configurable weights. GPU-deflate reports 2000 MB/s throughput vs parallel-deflate's 500 MB/s, so it will naturally score higher when throughput is weighted. This leverages the existing scoring infrastructure rather than adding special-case logic.

**Implementation**: `select_algorithm()` gains a `gpu_enabled: bool` parameter. When true and the file exceeds the threshold, it returns `"gpu-deflate"` instead of `"parallel-deflate"`.

**Alternatives Considered**:
1. **Always prefer GPU when available**: Violates spec assumption about opt-in. Could cause issues on shared GPU systems.
2. **Separate GPU threshold**: Adds complexity. The 25 MB threshold is already reasonable for GPU (GPU benefits start around 10 MB per benchmarks).

---

## R5: Configuration Schema for GPU Settings

**Question**: What configuration keys and types are needed for GPU settings?

**Decision**: Three keys in a `[gpu]` TOML section:
- `gpu.enabled` (bool, default: false) — enable GPU in auto-selection
- `gpu.device` (integer, default: -1 meaning auto) — GPU device index
- `gpu.force-cpu` (bool, default: false) — force CPU fallback for GPU formats

**Rationale**: Maps directly to spec FR-009. Uses the same patterns as existing config sections (`compression.*`, `output.*`, `logging.*`). The `device` field uses -1 as sentinel for "auto" since TOML doesn't have null values and optional integers add complexity to the config system.

**Alternatives Considered**:
1. **String-based device**: `"auto"` vs `"0"` — type-unsafe, parsing overhead.
2. **Nested under `compression`**: e.g., `compression.gpu-enabled`. Mixes concerns — GPU config is about hardware, not compression level.
3. **Separate config file**: Over-engineering. GPU has only 3 settings.
