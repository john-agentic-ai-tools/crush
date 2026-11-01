# crush-lz

The `fast-lz` compression plugin for [Crush](../README.md): an LZ4-class
codec (byte-oriented LZ77 sequences, no entropy coding) for maximum speed at
an LZ4-like ratio. It compresses on all cores and decompresses on one. Safe
Rust, no third-party codec.

## Using it

```bash
crush compress --plugin fast-lz file      # always fast-lz
crush compress --level fast file          # fast-lz only if the data compresses well
crush decompress file.crush               # recognised from the file's magic
```

From Rust, linking the crate registers the plugin with crush-core:

```rust
use crush_core::{CompressionOptions, compress_with_options, init_plugins};
use crush_lz as _;

init_plugins()?;
let packed = compress_with_options(data, &CompressionOptions::default().with_plugin("fast-lz"))?;
```

The codec is also usable directly: `crush_lz::compress(&data, &FastLzConfig::default())`
and `crush_lz::decompress(&packed, data.len())`.

## Format

Inside the Crush outer header (magic `43 52 01 04`): an 8-byte frame header
(`"LZ"`, version 1, flags, block size 64 KiB to 1 MiB, default 1 MiB), then
independent blocks (`u32` compressed length with a stored bit, `u32`
uncompressed length, data), then a zero end marker. Blocks hold LZ4-style
sequences with a 64 KiB window; a block that doesn't shrink is stored.
Full description: `specs/023-lz4-fast-plugin/contracts/fast-lz-format.md`.

The decoder validates every length and offset before copying, so corrupt
input is an error, never a panic or an oversized allocation; it is fuzzed
(`fuzz/`, run under WSL/Linux with `cargo +nightly fuzz run fuzz_decompress`
and `fuzz_roundtrip`).

## Automatic selection

The plugin is content-aware: crush-core never picks it by weight scoring
alone. With speed preferred (`--level fast`) and no plugin named, it
compresses evenly spaced samples (about 1/128 of the input, 4 x 4 KiB to
16 x 16 KiB) and is used only if they shrink by at least 10%. Base64 text,
random and already-compressed data go elsewhere.

## Performance

One core of an i9-12900K, 128 MB of real files (tar of `/usr/lib` and
`/usr/share`), lz4 1.10.0 measured in the same session:

| | compress | decompress | ratio |
|---|---|---|---|
| fast-lz codec, in memory | 721 MB/s (0.92x lz4) | 2,519 MB/s (0.57x) | 2.230 |
| `crush --plugin fast-lz`, end to end | 619 MB/s (1.01x `lz4 -1`) | 1,747 MB/s (1.15x `lz4 -d`) | 2.23 |

The spec's in-memory targets (1.15x lz4 compress, 0.60x decompress) are not
met; `specs/023-lz4-fast-plugin/perf-log.md` records the experiments and the
measured speed/ratio frontier.

Compression, same corpus, with lz4 1.10.0 multi-threaded:

| | 4 cores | all cores |
|---|---|---|
| fast-lz codec, in memory | 2,193 MB/s | 5,238 MB/s |
| `crush --plugin fast-lz`, end to end | 1,572 MB/s (0.85x `lz4 -1`) | 1,759 MB/s (0.87x) |

Output bytes do not depend on the core count. Details:
`perf/log/2026-11-parallel-fast-compression.md`.

## Benchmarks

```bash
cargo bench -p crush-lz --bench codec                    # criterion (text, random)
CRUSH_BENCH_CORPUS=/path/to/file cargo bench -p crush-lz # plus a real file
cargo run --release -p crush-lz --example codec_bench -- FILE [RUNS]
bash perf/lz-vs-lz4.sh                                   # vs lz4 -b1 (WSL/Linux)
cargo test --release -p crush-lz --test selection_report -- --ignored --nocapture
```
