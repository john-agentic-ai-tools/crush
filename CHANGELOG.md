# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The workspace publishes four crates that version independently:

| Crate | Description |
|---|---|
| `crush-core` | Plugin system, compression traits, cancellation |
| `crush-parallel` | Parallel DEFLATE engine, CRSH block format |
| `crush-gpu` | GPU tile-based engine (wgpu / optional CUDA) |
| `crush-cli` | `crush` command-line binary |

This changelog begins at the versions below. For earlier history, see the git log.

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
