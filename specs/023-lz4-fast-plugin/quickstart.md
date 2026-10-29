# Quickstart: validating `fast-lz`

Prerequisites: the Rust toolchain from `rust-toolchain.toml`; for codec targets, WSL Ubuntu with Rust, and lz4 1.10.0 + zstd built into `~/.local/bin` (see `perf/README.md`); nightly + cargo-fuzz in WSL for fuzzing.

## 1. Correctness (any platform)

```sh
cargo test -p crush-lz                 # unit, round-trip, property, decoder-rejection tests
cargo test -p crush-core selector      # content-aware selection rules (contracts/selection.md)
cargo test -p crush-cli                # CLI integration: list, explicit plugin, --level fast routing, inspect
```

Expected: all pass; `crush plugins list` shows `fast-lz`.

## 2. CLI smoke test

```sh
cargo build --release -p crush-cli
target/release/crush compress --plugin fast-lz big.tar -o big.tar.crush
target/release/crush inspect big.tar.crush          # Plugin: fast-lz, ratio
target/release/crush decompress big.tar.crush -o out.tar && cmp big.tar out.tar
target/release/crush compress --level fast small-log.txt     # < 25 MB, compressible → fast-lz
target/release/crush compress --level fast base64.txt        # → not fast-lz (deflate)
target/release/crush compress small-log.txt                  # balanced → same plugin as before
```

## 3. Codec targets vs lz4 (SC-001..SC-003), WSL, same session

```sh
export PATH=~/.local/bin:$PATH
bash perf/lz-vs-lz4.sh          # builds crush-lz's codec bench, runs it and `lz4 -b1` on the 128 MB real-file corpus, pinned to one core
```

Expected table: fast-lz compress ≥ 1.15 × lz4, decompress ≥ 0.60 × lz4, ratio ≥ 2.18.

## 4. Selection (SC-004, SC-005)

```sh
cargo test -p crush-lz --release --test selection_report -- --ignored --nocapture
```

Prints, for each labelled input (real files, source, logs, base64 text, random, already-compressed), the estimate, the chosen plugin, and the estimate's cost as % of compression time. Expected: routing matches SC-004; cost ≤ 2% for inputs ≥ 1 MB.

## 5. Regression gate and fuzzing

```sh
python .claude/skills/perf/bench.py ab --ref $(git merge-base HEAD develop) --suite full --name lz-final
# WSL:
cd crush-lz && cargo +nightly fuzz run fuzz_decompress -- -runs=100000
cd crush-lz && cargo +nightly fuzz run fuzz_roundtrip  -- -runs=100000
```

Expected: no regression > 5% outside noise (SC-007); fuzzing clean (SC-006).

## 6. Story 3 (phase 2): end to end on one core

```sh
python3 perf/compare.py --cores 1 --tools crush-fast-lz,lz4-1     # ABBA, median, tmpfs
```

Expected: `crush` with `fast-lz` faster than `lz4 -1` compress and `lz4 -d` decompress on the real-file corpus (SC-008).
