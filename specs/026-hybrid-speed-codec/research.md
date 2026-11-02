# Research: Hybrid High-Speed Codec

Broader evidence is in [the research report](../../reports/Beating%20zstd%20with%20a%20Crush%20plugin.md) and its notes. This file records the decisions for this feature. Status is **Decided**, or **Provisional** when a named measurement can overturn it.

## R1. Port or new format

- **Decision (Decided)**: A Crush-native format using zstd's stream model. Not byte-compatible with zstd.
- **Rationale**: Crush files already carry their own header, so zstd compatibility has little value, and a format of its own allows changes such as block-persistent tables.
- **Alternatives**: A byte-exact port can only match zstd's curve. libzstd bindings are excluded by FR-015. Byte-aligned sequences are not supported by the Lizard evidence.

## R2. Clean implementation source

- **Decision (Decided)**: Implement from RFC 8878 and public algorithm descriptions. Do not translate zstd's C source.
- **Rationale**: Crush is MIT. Working from the specification avoids carrying BSD notices for derivative code, and a safe-Rust design differs in shape anyway. If translation does happen, notices must be retained.

## R3. Safe-Rust speed techniques

- **Decision (Decided)**: Remove bounds checks by construction rather than with `unsafe`:
  - Fixed-size tables (`Box<[T; 1 << N]>`), indexed with `& MASK`.
  - Fixed-size reads with `first_chunk::<8>()` and `u64::from_le_bytes`.
  - Fast decode loops guarded by a single margin check.
  - Non-overlapping copies with `split_at_mut` and fixed-size chunks, reusing the fixed-size copy approach in `crush-lz`.
- **Rationale**: lz4_flex's safe build pays 15-25%, and the research's 0.85x floor leaves little room, so check elimination has to be a design input.
- **Verification**: Prototype (R10), plus inspecting optimized code for remaining bounds checks in hot loops.

## R4. CPU feature dispatch

- **Decision (Decided)**: Baseline scalar x86-64 code for the MVP, with no runtime dispatch.
- **Rationale**: Calling a `#[target_feature]` function from ordinary code is `unsafe`, even though the body may be safe, so `is_x86_feature_detected!` dispatch would break `forbid(unsafe_code)`. zstd's level 1-3 match loops are scalar, and BMI2 only helps bit writing and reading. Its fast C Huffman decoder is within 2% of its assembly.
- **Alternative**: One audited dispatch shim is the most likely first exception if R10 shows a real BMI2 gain. It would go through FR-017. A `-C target-cpu` build is not suitable for distribution, and also cannot be used for a claim about a general build.

## R5. Block size

- **Decision**: 4 MiB independent blocks, final as of the 2026-11-01 sweep.
- **Rationale**: `zstd -3 -T1` on `real` gives 3.675 at 4 MiB against 3.700 whole, 0.7% short. 8 MiB gets to 3.692 but puts SC-008 (5 ms per block) at risk, and halves parallel decode tasks. A larger 8-byte match table closes the gap instead: 2^18 entries reach 3.699 at 4 MiB for 8.5% less compression speed and no decompression cost. Full figures are in `perf/log/2026-11-hybrid-speed-codec.md`.
- **Overturned by**: The prototype showing it cannot reach 3.70 at 4 MiB within the speed gates, in which case 8 MiB is next, and SC-008 must be rechecked.
- **Method**: Split `real` into chunks of 1, 2, 4, 8 and 16 MiB, compress each with `zstd -3`, and sum the sizes. Use `-T1`, as the gate does: `--single-thread` reads about 0.7% lower. This needs only stock zstd, not a custom build.

## R6. Segment model

- **Decision (Provisional)**: 128 KiB segments inside each block. Huffman tables, tANS tables and repeat offsets persist across segments, and reset at each block.
- **Rationale**: Table reuse cuts encoder cost, and it is what makes a segment cheap. Reset at the block boundary preserves independence and therefore random access.
- **Overturned by**: Measured ratio or speed loss from persistence, which has no published figure.

## R7. Match finder

- **Decision (Decided for MVP)**: Two hash tables in zstd's `dfast` shape: 8-byte hashes at 2^17 entries, 5-byte at 2^16. Repeat-offset probe at ip+1, with step growth while no match is found.
- **Rationale**: Published level-3 parameters reach the ratio target, and tables total 768 KiB, far below the memory limit.
- **Later experiment**: A tag-filtered 2-way or 4-way bucket, aimed at the steep level 2 to 3 speed drop. This is a hypothesis, and it can change the encoder without changing the format.

## R8. Entropy coding

- **Decision (Decided)**: 4-stream Huffman literals with an 11-bit code limit, and tANS for sequences. Each has predefined, repeat and explicit modes. Sampled early exit for incompressible literals.
- **Rationale**: tANS gains where symbol probabilities exceed 0.5, which suits sequence codes. Huffman is about twice as fast on flat literal data at near-equal ratio.
- **Rejected**: rANS, because of the unreviewed US11234023B2 patent and no measured need. 8-stream Huffman, because no primary benchmark exists.

## R9. CRSH container change

- **Decision (Decided)**: Sprint files use CRSH format version 2, with a codec id in header byte 22 (previously reserved zero). Existing DEFLATE files stay at version 1, byte for byte, and the reader accepts both.
- **Rationale**: Today's reader rejects any other version, and the outer magic `43 52 01 05` makes older builds report an unknown plugin. This keeps FR-003, FR-010 and the edge case for older versions.
- **Alternative**: Reusing the version 1 header with a nonzero reserved byte was rejected, because a v1 reader would ignore it and try to decode the payload as DEFLATE if it ever reached that path.

## R10. Prototype before freeze

- **Decision (Decided)**: Prototype a safe 4-stream Huffman decoder and two-table match loop on real literals, against live zstd in memory. Below 0.8x triggers the FR-017 process, not an automatic exception.
- **Rationale**: It prices the largest risk before format work begins.

## R11. Engine scaling hypotheses

- **Decision (Resolved by T009-T011)**: Results are in `perf/log/2026-11-hybrid-speed-codec.md`.
  1. **Confirmed, fix in safe Rust**: Serial per-block `vec!` copies in `decompress_from_reader` (`crush-parallel/src/engine.rs:332-349`), 12 ms. Replace with borrowed slices when input is in memory.
  2. **Confirmed, largest cost**: Serial file output, about 35 ms, which is 52% of all-core CLI time. Positional writes (`write_at`) were measured and do not scale (37 ms at 16 threads), so the safe fix planned here is rejected. Shared `mmap` output scales to about 18 ms, but needs `unsafe`, so it goes through FR-017.
  3. **Rejected**: Serial outer CRC costs 1.4 ms.
  4. **Rejected**: Freeing output costs 0.2 ms. Page faults are not separately measured, and they fall inside decode, which scales 3.7x and 7.2x.
- **Rationale**: Decode scales well. The serial phases around it are the limit.
- **Known limit**: The reference machine has 8 performance cores, 8 efficiency cores and SMT, so "all cores" is not 24 equal cores. Scaling should be read with that in mind.

## R12. Cancellation

- **Decision (Decided)**: The engine checks the cancel flag before each block, on both paths.
- **Rationale**: `parallel-deflate` currently reports cancellation only after all blocks finish. With 4 MiB blocks the worst-case delay is one block per worker.

## R13. Measurement

- **Decision (Decided)**:
  - Extend `perf/compare.py` with a `crush-sprint` row and report spreads.
  - Add an in-memory mode using `zstd -b3` and `examples/codec_bench.rs`.
  - For wins under 10%, rotate builds by padding or code layout, because identical code has shown swings of 10% in zstd and 30.7% in `fast_lz_select`.
  - Tools come from `~/.local/bin` in WSL (zstd 1.5.7, per `perf/README.md`). Interactive profiles run in WSL first, and on Windows only as a last resort, with a warning before the UAC prompt.
- **Custom zstd builds**: Not required. `--no-compress-literals` is a stock flag. Repeat-offset value is measured by toggling sprint itself.

## R14. Selection and promotion

- **Decision (Decided)**: Add a provided method to `CompressionAlgorithm`, `auto_selectable()`, defaulting to `true`. Sprint returns `false`, and the selector skips it until promotion. A test asserts selection output is unchanged by registration. Promotion flips the method and changes the 25 MiB route in `crush-cli/src/algorithm.rs`.
- **Rationale**: Hand arithmetic suggests adding any plugin shifts min-max scoring over stale metadata. Marking sprint `content_aware` would not work, because `--level fast` selects content-aware plugins from their estimates.

## Resolved Unknowns

No NEEDS CLARIFICATION items remain. R6 and R11 are provisional, and each has a named measurement in the delivery order.
