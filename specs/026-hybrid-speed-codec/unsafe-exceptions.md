# Unsafe Exceptions

Every use of `unsafe` under FR-017 and FR-018.

## E1. Mapped decompression output

| Field | Value |
|---|---|
| Status | Approved by the maintainer 2026-10-31. Implemented. Fuzz clean. Miri: no undefined behaviour on the slice path under Tree Borrows |
| Location | `crush-cli/src/commands/mapped_output.rs`, `map_output` |
| Scope | One `unsafe` call, `MmapMut::map_mut`. `crush-sprint` stays `forbid(unsafe_code)` |
| Gate missed without it | SC-005 at 4 and all cores, SC-003 at all cores |
| Measurement | `perf/log/2026-11-hybrid-speed-codec.md`, "Profiles" |

**Safe alternatives tried**

- One `write`: about 35 ms for 128 MiB on tmpfs, at any core count.
- Positional writes on up to 16 threads: 37-40 ms, with no scaling at all.
- Removing the 12 ms serial payload copy: done, but still computed to reach only 2.3x and 3.0x.

**Gain with `unsafe`**: Whole-file decompression on `real` went from 1,960 to 3,692 MB/s at all cores, and from 1,598 to 2,426 MB/s at 4 cores, together with the removal of the payload copy.

**Safety conditions**

1. `fallocate` reserves every page first, so a full disk is a normal error and not a crash. It costs nothing measurable in total.
2. A file system that cannot reserve space uses the safe writer. Platforms other than Linux and Windows always do.
3. Output is a new file made by this process, and is never mapped when it resolves to the input file.
4. All sizes come from `decompressed_len`, which validates the index first.

**Accepted risk**: Another process truncating the output file during decompression makes the process fault. The existing input mapping accepts the same risk.

**Dependency**: `rustix` on Linux only, for `fallocate` through a safe API with no C code. It avoids a second `unsafe` block.

**Fuzz coverage**: `crush-parallel/fuzz/fuzz_targets/fuzz_decompress_into.rs` drives the slice API that the mapping feeds. 100,000 runs seeded with the golden files were clean. Unseeded runs barely reach decode, so always seed this target.

**Memory checks**: Miri cannot run a real file mapping or the C libdeflate, so it covers the engine's slice path with the pure-Rust test codec in `crush-parallel/tests/block_codec.rs`. Four tests passed with no undefined behaviour under `-Zmiri-tree-borrows -Zmiri-permissive-provenance -Zmiri-disable-isolation`. Miri then exited non-zero because Rayon pool threads were still alive when `main` returned, so that leak check is not clean. The default Stacked Borrows model stops earlier, inside `crossbeam-epoch` after its integer-to-pointer casts, so that model gives no result for the engine. The mapping itself is covered by the tests in `mapped_output.rs` and `crush-cli/tests/direct_output.rs`.
