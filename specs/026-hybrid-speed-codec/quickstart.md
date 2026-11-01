# Quickstart: Validating the Hybrid High-Speed Codec

Run commands in WSL unless noted, on the reference machine, with no other heavy work running.

## Prerequisites

```bash
export PATH="$HOME/.local/bin:$PATH"
zstd --version     # expect 1.5.7
cargo build --release
```

## 1. Baseline and head-to-head (US3)

```bash
python3 perf/compare.py --tools crush,zstd-3,zstd-1 --data real --abba --reps 10
zstd -b1e3 <real corpus file>     # in-memory reference
```

**Expected**: Ratio and speed at 1, 4 and all cores with spreads, plus in-memory figures that explain the 274 vs 580 MB/s gap.

## 2. Engine scaling (US1)

```bash
CRUSH_TIMINGS=1 target/release/crush decompress <large archive> -o /dev/null -f
```

Then use `/perf diagnose` for a profile, and re-run step 1 after each fix.

**Expected**: Scaling of at least 3.0x at 4 cores and 3.3x at all cores (SC-005). Output for archives made before this feature is byte-identical.

## 3. Block-size sweep

Split `real` at 1, 2, 4 and 8 MiB, compress each chunk with `zstd -3 --single-thread`, and sum the sizes.

**Expected**: 4 MiB stays close enough to full-file ratio to reach SC-001. Otherwise follow R5.

## 4. Codec round trip (US2)

```bash
target/release/crush compress --plugin sprint input -o input.crush -f
target/release/crush decompress input.crush -o output -f
cmp input output
target/release/crush decompress --block 3 input.crush -o block3 -f
cat input | target/release/crush compress --plugin sprint > piped.crush
```

**Expected**: Identical output, correct block, and the piped case round-trips.

## 5. Safety

```bash
cd crush-sprint/fuzz
cargo +nightly fuzz run fuzz_decompress -- -runs=100000
cargo +nightly fuzz run fuzz_roundtrip  -- -runs=100000
```

**Expected**: No crash, hang or memory error (SC-007). `crush-sprint` must also have no `unsafe` unless `unsafe-exceptions.md` records an approved exception.

## 6. Gates

```bash
python3 perf/compare.py --tools crush-sprint,zstd-3,zstd-1 --data real --abba --reps 10
python3 perf/compare.py --tools crush-sprint,zstd-3 --data text --abba --reps 10
```

Check each result against SC-001 to SC-011. Any win under 10% must repeat across separate builds.

## 7. Selection unchanged

```bash
cargo test -p crush-core selector
```

**Expected**: Automatic selection is identical with and without sprint registered, until promotion.

## 8. Quality gate

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```
