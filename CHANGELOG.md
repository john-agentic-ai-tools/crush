# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The workspace publishes these crates, which version independently:

| Crate | Description |
|---|---|
| `crush-core` | Plugin system, compression traits, cancellation |
| `crush-parallel` | Parallel DEFLATE engine, CRSH block format |
| `crush-gpu` | GPU tile-based engine (wgpu / optional CUDA) |
| `crush-lz` | `fast-lz` plugin, an LZ4-class codec |
| `crush-cli` | `crush` command-line binary |

This changelog begins at the versions below. For earlier history, see the git log.

## [Unreleased]

Everything since v0.4.0. The highlights are a new LZ4-class codec, `fast-lz`,
and much faster decompression and tile compression on multi-core machines.
Existing archives still decompress, and `fast-lz` and tile output is identical
at any core count.

### Added

- **`crush-lz`, a new crate providing the `fast-lz` plugin.** An LZ4-class codec
  in safe Rust. Use it with `crush compress --plugin fast-lz`, or let
  `--level fast` choose it when a quick sample shows the data compresses well.
- **`perf/compare.py` covers zstd and lz4** alongside gzip and pigz, and can
  interleave tools and compare several builds of crush.
- **`perf/chart.py` charts the results** as speed against ratio, with an
  interactive page and SVGs for the README.
- **`/perf`**, a benchmark-driven workflow for checking changes and diagnosing
  slowdowns, with profiling setup in `CONTRIBUTING.md`.

### Changed

- **`fast-lz` compresses on all cores**, about 5,200 MB/s in memory. It
  decompresses on one core.
- **`crush-gpu` tile compression runs on all cores**, with a faster bit writer
  and shared lookup tables. A 16 MiB run went from about 48 to about 1,330 MB/s,
  and from about 48 to 141 MB/s on one core.
- **Parallel decompression writes straight into a mapped output file.**
- **The parallel engine skips DEFLATE on blocks that look incompressible**,
  reuses scratch space per worker, and computes the outer CRC32 in parallel.
- **The CLI streams to and from files and memory-maps inputs of 1 MiB or more.**

### Fixed

- **`crush-lz`:** forged lengths that exceed available memory now return an error.

### Performance

`crush` with the default plugin, v0.4.0 against this release, measured in one
session (median MB/s of 7 interleaved runs, 128 MiB, i9-12900K, WSL2, tmpfs).
Ratios are unchanged.

| Data, all cores | Compress | Decompress |
|---|---|---|
| real files | 927 → 1,002 | 1,669 → 2,927 |
| base64 text | 687 → 787 | 1,023 → 1,932 |
| repetitive text | 2,236 → 2,761 | 2,541 → 4,763 |
| random | 611 → 1,142 | 1,094 → 2,897 |

| Data, one core | Compress | Decompress |
|---|---|---|
| real files | 127 → 129 | 755 → 836 |
| base64 text | 113 → 115 | 440 → 517 |
| repetitive text | 854 → 873 | 2,072 → 2,173 |
| random | 110 → 669 | 1,059 → 1,795 |

Against other tools on real files, same setup, commit 5964e93 (gzip 1.10,
pigz 2.8, zstd 1.5.7, lz4 1.10.0):

| All cores | Compress | Decompress | Ratio |
|---|---|---|---|
| `crush` | 989 | 3,472 | 3.503 |
| `crush --plugin fast-lz` | 1,823 | 1,799 | 2.230 |
| `zstd -1` | 2,303 | 1,300 | 3.233 |
| `zstd -3` | 1,421 | 1,215 | 3.700 |
| `lz4 -1` | 2,049 | 2,300 | 2.295 |
| `pigz -6` | 611 | 343 | 3.481 |

- On real files, `crush` is the fastest decompressor at all cores, of the
  configurations measured. `fast-lz` is the fastest at one core. At 4 cores
  `lz4 -9` leads narrowly, 2,382 to 2,334 MB/s.
- `zstd` remains ahead on compression speed at its ratio: `zstd -3` is both
  faster and smaller than `crush` when compressing.
- `fast-lz` trails `lz4 -1` through the CLI at 4 and all cores (0.80x and 0.89x).
  On one core the two are level.
- At all cores, `fast-lz` on incompressible data is 4-7% slower than its old
  single-threaded path, though at 4 cores it is faster.

Multi-core runs on this machine vary, with spreads of up to about 40% around
these medians. `perf/comparisons/chart.html` has every dataset and core count.

## [2026-10-27]

Edition 2024, toolchain, and dependency update. Every crate receives a **minor**
bump because the minimum supported Rust version rises from 1.97 to 1.99, which
can break consumers pinned to an older compiler. No public Rust API changed in
any library crate (`crush-cli` is a binary, so its internal functions are not
API).

- `crush-core` 0.3.0 → **0.4.0**
- `crush-parallel` 0.2.0 → **0.3.0**
- `crush-gpu` 0.2.0 → **0.3.0**
- `crush-cli` 0.3.0 → **0.4.0**

### Security

- No open advisories. `cargo audit --deny warnings` (workspace and fuzz
  lockfiles) and `cargo deny check` pass with **zero ignores** after updating
  to current compatible releases of all dependencies.
- The scheduled Security Audit workflow had been disabled by GitHub for
  inactivity since 2026-10-05. It is re-enabled and now also supports manual
  dispatch.

### Changed

- **Minimum supported Rust version is now 1.99** (`rust-version = "1.99"`); the
  pinned toolchain moves 1.97.1 → 1.99.1. Clippy 1.99's new `assert_is_empty`
  lint was applied to nine assertions; no behaviour change.
- **`crush-cli`: `dirs` 6 → 7.** `config_dir()` is unchanged, so the config
  file location on every platform is the same as before.
- Compatible dependency updates across the workspace, including wgpu 30.0.1,
  cudarc 0.19.10, clap 4.6.7, flate2 1.1.10, and memmap2 0.9.12.
- CI actions updated: `actions/checkout` v7, `actions/upload-artifact` v7,
  `actions/download-artifact` v8, `codecov/codecov-action` v7,
  `Jimver/cuda-toolkit` v0.2.36.
- **Migrated the workspace to Rust edition 2024** (all four crates and the three
  fuzz crates). Most of the diff is rustfmt's 2024 style edition reordering
  `use` items.
- `crush-cli`: `merge_env_vars` now delegates to a new `merge_env_vars_from(config,
  vars)`, which takes the variables explicitly instead of reading the process
  environment. Added so the parsing rules can be tested without calling
  `std::env::set_var`, which became `unsafe` in edition 2024.

### Fixed

- **CI coverage gate could never fail.** The threshold check parsed the last
  column of `cargo llvm-cov`'s table, which is branch coverage and prints `-`,
  so the comparison silently evaluated false; it also measured a different file
  set than the report. Now enforced by `cargo llvm-cov report
  --fail-under-lines 80` with the same `crush-gpu` exclusion. Line coverage is
  85.5% after adding tests.
- Removed the last source of environment mutation from `crush-cli`'s unit
  tests. `cargo fix --edition` had wrapped 34 call sites in `unsafe` with
  "FIXME: Audit that the environment access only happens in single-threaded
  code" — an audit that would have failed, since `cargo test` runs them as
  threads while siblings read the environment. The tests now use
  `merge_env_vars_from` and a `#[cfg(test)]` thread-local config-path override,
  so **no `unsafe` was added** and the tests are genuinely parallel-safe rather
  than merely serialised behind the mutex added in the previous release.
- Pinned the drop order of two `recv_timeout` tail expressions in
  `crush_core::plugin::timeout` by binding the scrutinee to a local. Edition
  2024 drops such temporaries before locals rather than after
  (`tail-expr-drop-order`); binding makes the order identical in both editions
  instead of dependent on the edition.
- Rewrote six nested `if let` blocks as let-chains, now that edition 2024
  enables them (`clippy::collapsible_if`).

## [2026-07-30]

Toolchain and dependency modernisation. Every crate receives a **minor** bump
because the minimum supported Rust version rises from 1.93 to 1.97, which can
break consumers pinned to an older compiler, even though no public Rust API
changed in any crate.

- `crush-core` 0.2.1 → **0.3.0**
- `crush-parallel` 0.1.0 → **0.2.0**
- `crush-gpu` 0.1.1 → **0.2.0**
- `crush-cli` 0.2.1 → **0.3.0**

### Security

- **`memmap2` floor raised to 0.9.11**, patching
  [RUSTSEC-2026-0186](https://rustsec.org/advisories/RUSTSEC-2026-0186):
  unchecked pointer offset in `advise_range` / `flush_range`, whose result is
  passed to `madvise` / `msync`. Both `crush-parallel` and `crush-gpu` memory-map
  directly, so this was reachable. `Cargo.lock` is not committed, so the declared
  floor is what actually protects consumers.
- Cleared [RUSTSEC-2026-0204](https://rustsec.org/advisories/RUSTSEC-2026-0204)
  (`crossbeam-epoch` invalid pointer dereference) and
  [RUSTSEC-2026-0097](https://rustsec.org/advisories/RUSTSEC-2026-0097)
  (`rand` unsoundness, dev-dependency only).
- Removed the last advisory exemption. `RUSTSEC-2024-0436` (unmaintained `paste`)
  was reached via `wgpu-hal → metal`; wgpu 30 moved the macOS backend to
  `objc2-metal`, which does not depend on it. `cargo audit` and `cargo deny check`
  now pass with **zero ignores**.
- Fuzz crates keep their own lockfiles outside the workspace and were never
  audited; `crush-core/fuzz` was carrying RUSTSEC-2026-0204. They are now covered
  by the Security Audit workflow.

### Changed

- **Minimum supported Rust version is now 1.97**, declared as
  `rust-version = "1.97"` in `[workspace.package]` and inherited by all crates.
  The pinned toolchain moves 1.93.1 → 1.97.1. No source changes and no new or
  widened clippy lints resulted.
- **`crush-gpu`: wgpu 28 → 30.** Internal only — wgpu types do not appear in the
  crate's public API. Migrated `Instance::new` (now by value, and
  `InstanceDescriptor` lost `Default`), the new
  `RequestAdapterOptions::apply_limit_buckets` field, `bind_group_layouts`
  becoming `&[Option<&BindGroupLayout>]`, and `get_mapped_range` now returning
  `Result` rather than panicking.
- **`crush-cli`: `toml` 0.9 → 1.1** and **`pollster` 0.4 → 1.0**. The on-disk
  config file layout is unchanged, so existing `config.toml` files keep parsing.
- Declared dependency floors raised across the workspace to match the versions
  actually built against: linkme 0.3.37, crossbeam 0.8.4, rayon 1.12, flate2
  1.1.9, crc32fast 1.5, thiserror 2.0.19, bytemuck 1.25, clap 4.6, indicatif
  0.18.6, termcolor 1.4.1, serde 1.0.229, serde_json 1.0.151, tracing 0.1.44,
  tracing-subscriber 0.3.23, is-terminal 0.4.17, ctrlc 3.5, filetime 0.2.29,
  cudarc 0.19.8, and dev-dependencies assert_cmd 2.2, predicates 3.1.4,
  tempfile 3.27, proptest 1.11.

### Fixed

- **CI build matrix was silently collapsed.** It declared `rust: [stable, beta]`,
  but `rust-toolchain.toml` takes precedence over `rustup default`, so both legs
  compiled the pinned toolchain and the `beta` leg tested nothing. Now driven by
  `RUSTUP_TOOLCHAIN`, which does override the toolchain file.
- **Flaky `crush-cli` config tests.** `merge_env_vars` iterates every `CRUSH_*`
  variable, so a concurrent test setting
  `CRUSH_COMPRESSION_TIMEOUT_SECONDS=not_a_number` made unrelated tests panic;
  separately, `config_file_path` reads a shared `CRUSH_TEST_CONFIG_FILE`. All 17
  environment-mutating tests are now serialised. Reproduced at roughly 2 failures
  in 5 runs before the fix.

### Performance

No regressions. Every benchmark improved against a pre-upgrade baseline on
identical source, at p < 0.05 — roughly 10–40% faster on compression,
decompression, and round-trip paths, and 5–10% on plugin selection. Not
bisected; plausibly rustc codegen improvements plus `crc32fast` 1.4 → 1.5 and
`rayon` 1.11 → 1.12.

### Compatibility

- **Archive format unchanged.** `FORMAT_VERSION` remains `1`. The crate version
  written into the CRSH header is informational metadata and is not used for
  acceptance, so archives produced by earlier releases still decompress
  byte-identically.
