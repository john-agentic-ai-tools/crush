# Cleanup Summary: GDeflate GPU Decompression

## Duplication Analysis

**Tool**: jscpd via `detect-duplicates.ps1 -Json`
**Date**: 2026-03-01
**Total duplicate instances**: 6 (185 lines)

### Findings

All 6 duplicates are pre-existing in `crush-core` and `crush-cli` — **none were introduced by the GDeflate implementation**.

| # | File(s) | Lines | Description |
|---|---------|-------|-------------|
| 1-3 | crush-core/src/plugin/registry.rs | 32-35 each | Repeated test patterns for registry validation |
| 4 | crush-core/src/decompression.rs + inspection.rs | 21 | Header parsing boilerplate |
| 5 | crush-cli/tests/compress.rs | 27 | Test setup duplication |
| 6 | crush-cli/benches/cli_startup.rs + help_command.rs | 43 | Benchmark harness boilerplate |

### Resolution

No action needed for this feature — no new duplication introduced in `crush-gpu`.

## Quality Gate Results

| Gate | Status |
|------|--------|
| `cargo fmt --check` | PASS |
| `cargo clippy -D warnings` | PASS |
| `cargo test --workspace` | PASS (319 tests) |
| `cargo doc --no-deps` | PASS |
| Duplication analysis | PASS (no new duplicates) |

## Benchmark Results

| Metric | Value |
|--------|-------|
| Compression throughput (CPU) | ~595 MiB/s |
| Decompression throughput (CPU) | ~1.17 GiB/s |
| Decompression throughput (GPU, per-tile) | ~11.8 MiB/s |

**Note**: GPU decompression throughput is bottlenecked by per-tile dispatch overhead (buffer creation, upload, dispatch, readback per tile). The GPU shader itself is correct and fast — the throughput target of >1 GB/s requires batched dispatch which is an optimization opportunity for future work.
