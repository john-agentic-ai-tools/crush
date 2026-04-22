# Quickstart — Benchmark & Validation Recipes

**Feature**: 011-perf-optimizations | **Date**: 2026-04-17

Every measurement in [spec.md § Success Criteria](./spec.md) is reproduced from this document. If a claim here disagrees with spec.md, spec.md wins and this file gets a PR.

## Reference Hardware

| Field | Value |
|-------|-------|
| CPU | Fill in at T001 time (e.g. AMD Ryzen 9 7950X, 16 physical cores / 32 threads @ 4.5 GHz base) |
| RAM | 64 GB DDR5-5200 (minimum 32 GB for 1 GB fixture comfort) |
| Storage | NVMe SSD with ≥3 GB/s sustained read (so I/O is not the bottleneck) |
| OS | Windows 11 Pro 64-bit / Ubuntu 24.04 / macOS 14 — whichever the maintainer is on |
| Rust | Pinned via `rust-toolchain.toml` (stable channel) |
| Power profile | Performance / High Performance — not Balanced |

**Why this matters**: SC-001 through SC-004 are *relative* percentages. Absolute numbers only compare against a baseline captured on the same machine (same thermal envelope, same kernel, same background load). Run `cargo bench --save-baseline pre-011` once on clean `develop`; every later run uses `--baseline pre-011`.

## Fixtures

All fixtures live in `target/bench-fixtures/` (gitignored). Generate once with:

```bash
# Deterministic mixed-entropy 1 GB fixture (SC-001/SC-002/SC-003)
cargo run --release --example gen_fixture -- mixed 1073741824 target/bench-fixtures/mixed_1gb.bin

# 100 MB all-zeros (round-trip edge-case, cheap)
dd if=/dev/zero of=target/bench-fixtures/zeros_100mb.bin bs=1M count=100

# 100 MB of /dev/urandom (incompressible — stored-fallback path)
dd if=/dev/urandom of=target/bench-fixtures/random_100mb.bin bs=1M count=100

# 10,000-block CRSH file for SC-004 random-access
cargo run --release --example gen_fixture -- mixed 10737418240 target/bench-fixtures/mixed_10gb.bin
cargo run --release --bin crush -- compress target/bench-fixtures/mixed_10gb.bin \
    --block-size $((1024 * 1024)) \
    --output target/bench-fixtures/mixed_10gb.crsh
# 10 GB / 1 MB block = 10 240 blocks ≈ 10k
```

> **Note**: If `examples/gen_fixture.rs` does not yet exist, T005 creates it as a tiny XorShift64-seeded producer that interleaves `./crush-parallel/benches/throughput.rs::generate_corpus` tokens with pseudo-random ASCII. The 2–4× ratio target matches the [throughput.rs](../../crush-parallel/benches/throughput.rs) corpus shape.

On Windows, replace `dd` with:

```powershell
# 100 MB all-zeros
$bytes = New-Object byte[] 104857600
[IO.File]::WriteAllBytes("target\bench-fixtures\zeros_100mb.bin", $bytes)

# 100 MB random
$bytes = New-Object byte[] 104857600
(New-Object Random).NextBytes($bytes)
[IO.File]::WriteAllBytes("target\bench-fixtures\random_100mb.bin", $bytes)
```

## Commands

### Build

```bash
cargo build --release --workspace
```

### Pre-change baseline (T001) — run once on clean `develop`

```bash
cargo bench --workspace --save-baseline pre-011
```

Writes `target/criterion/**/pre-011/`. **Commit a human-readable summary** under § Baseline in this file.

### Per-slice measurement

```bash
# After each slice lands:
cargo bench --bench throughput --baseline pre-011
cargo bench --bench random_access --baseline pre-011
```

### Full quality gate (Phase 6)

```bash
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --workspace
cargo fmt --all -- --check
cargo bench --workspace --baseline pre-011
# Fuzz: 100 000 iterations minimum (constitution)
cd crush-parallel/fuzz && cargo fuzz run fuzz_roundtrip -- -runs=100000
cd crush-parallel/fuzz && cargo fuzz run fuzz_decompress -- -runs=100000
# Public-API surface diff (SC-007)
cargo public-api diff --package crush-core --deny=all
cargo public-api diff --package crush-parallel --deny=all
```

### Peak-RSS measurement (SC-003)

Measure peak resident set while the process runs. Numbers below are captured on the 1 GB mixed-entropy fixture.

**Linux**:

```bash
/usr/bin/time -v \
    ./target/release/crush compress target/bench-fixtures/mixed_1gb.bin \
        --output /dev/null \
        2>&1 | grep "Maximum resident set size"
```

**macOS**:

```bash
/usr/bin/time -l \
    ./target/release/crush compress target/bench-fixtures/mixed_1gb.bin \
        --output /dev/null \
    2>&1 | grep "maximum resident set size"
```

**Windows (PowerShell)**:

```powershell
$proc = Start-Process -FilePath ".\target\release\crush.exe" `
    -ArgumentList "compress target\bench-fixtures\mixed_1gb.bin --output NUL" `
    -NoNewWindow -PassThru -Wait
# PeakWorkingSet64 = peak RSS in bytes. Divide by 1MB for human-readable.
"$([math]::Round($proc.PeakWorkingSet64 / 1MB, 2)) MB"
```

## Baseline (T001 — to be filled in)

> **NOT YET CAPTURED**. The baseline numbers below are placeholders. T001 fills them in after running `cargo bench --workspace --save-baseline pre-011` on the reference hardware.

| Metric | Command | Pre-011 | Post-011 (target) | SC |
|--------|---------|---------|-------------------|----|
| Compress 1 GB wall-clock | `cargo bench --bench throughput -- compress_1gb` | TBD | ≥ 15% less | SC-001 |
| Decompress 1 GB wall-clock | `cargo bench --bench throughput -- decompress_1gb` | TBD | ≥ 25% less | SC-002 |
| Peak RSS compress 1 GB | `/usr/bin/time -v` recipe above | TBD | ≤ 1.25× input | SC-003 |
| Peak RSS decompress 1 GB | `/usr/bin/time -v` recipe above | TBD | ≤ 1.25× input | SC-003 |
| 10k random-access lookups | `cargo bench --bench random_access -- lookup_10k_blocks` | TBD | ≥ 100× less total | SC-004 |
| Non-hot-path regression | `cargo bench --workspace --baseline pre-011` | 0% | ≤ 5% | SC-005 |

## Round-trip sanity (every slice)

```bash
# Tiny round-trip — any mistake shows up fast
cargo test -p crush-parallel engine::tests::test_compress_roundtrip_small
cargo test -p crush-parallel engine::tests::test_decompress_roundtrip
cargo test -p crush-parallel engine::tests::proptest_compress_decompress_roundtrip

# CRSH backward compat (FR-002)
# Compress a file with the pre-011 binary (kept at target/pre-011/crush-parallel.rlib)
# and decompress it with the current build. Script in
# specs/011-perf-optimizations/scripts/crsh-compat-check.sh — to be written at T049.
```
