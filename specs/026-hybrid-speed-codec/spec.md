# Feature Specification: Hybrid High-Speed Codec

**Feature Branch**: `026-hybrid-speed-codec`

**Created**: 2026-10-31

**Status**: Draft

**Input**: User description: "Hybrid high-speed compression codec plugin for Crush that aims to make Crush the fastest compression engine in the world. A new Crush-native format adopting zstd's proven structure, run on a generic independent-block parallel engine. Beat zstd -3 on real files. Use profiling to find and fix the existing engine's decompression scaling bottleneck. No C dependencies; safe Rust only unless a measured wall leaves no choice, and then only limited unsafe verified by fuzz testing. Opt-in until gates pass."

**Source research**: [Beating zstd with a Crush plugin](../../reports/Beating%20zstd%20with%20a%20Crush%20plugin.md)

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Parallel decompression scales with the cores I have (Priority: P1)

A user decompressing a large archive on a multi-core machine gets a speedup close to the number of cores they give Crush. Today, 4 cores deliver 2.19x and 24 logical cores deliver only 2.58x. Profiling finds where that time goes, and the bottleneck is fixed for the existing large-file path.

**Why this priority**: The decompression targets for the new codec depend on this work, and it pays off for every existing large-file user even if the codec never ships.

**Independent Test**: Profile, fix, and re-measure decompression of existing archives at 1, 4 and all cores. No new codec or format is needed, and existing archives must still decompress byte-for-byte.

**Acceptance Scenarios**:

1. **Given** an existing large archive and a machine with 4 or more cores, **When** the user decompresses it, **Then** throughput meets the scaling targets in SC-005.
2. **Given** a completed profile, **When** the fix is proposed, **Then** the cause and its measured share of time are recorded before any change is made.
3. **Given** an archive created before this feature, **When** it is decompressed after the fix, **Then** the output is byte-identical to the original.
4. **Given** a user who requests one block by index, **When** the fix is in place, **Then** random access still works.

---

### User Story 2 - Compress tighter and faster with the new codec (Priority: P2)

A user who wants the best speed at a strong ratio explicitly selects the new codec. Their files compress at least as tightly as zstd -3, at higher speed on multi-core machines, and decompress faster than zstd at every core count tested.

**Why this priority**: This is the headline capability, and it gains its multi-core decompression lead from the work in User Story 1.

**Independent Test**: Select the codec by name, then compress and decompress the benchmark corpora. Check byte-exact round trips and compare ratio and speed with a live zstd -3 run.

**Acceptance Scenarios**:

1. **Given** a real-file corpus, **When** the user compresses it with the new codec, **Then** the ratio is at least that of zstd -3 in the same session.
2. **Given** any compressed file from the new codec, **When** it is decompressed, **Then** the output is byte-identical to the input.
3. **Given** a large file compressed with the new codec, **When** the user requests one block by index, **Then** that block is returned without decompressing the rest.
4. **Given** a user who does not name the codec, **When** they compress a file, **Then** the existing automatic selection is unchanged.
5. **Given** a running operation, **When** the user cancels it, **Then** it stops promptly and leaves no partial output file.

---

### User Story 3 - Trust the speed claim (Priority: P3)

A maintainer or evaluator can reproduce any published speed claim from one command, against a live competitor run on the same machine in the same session, with the single-core codec result and the full-pipeline result both reported.

**Why this priority**: The goal is a public "fastest" claim. Without reproducible gates, the claim is not credible, and small differences can be measurement noise rather than real gains.

**Independent Test**: Run the head-to-head against an existing Crush plugin and live zstd. It produces every metric in the Success Criteria, with spreads, and needs no new codec.

**Acceptance Scenarios**:

1. **Given** a machine with the comparison tools installed, **When** the head-to-head runs, **Then** it reports ratio, compression speed and decompression speed for 1, 4 and all cores on each corpus.
2. **Given** a result that wins by less than 10%, **When** it is evaluated, **Then** it counts as a win only if it holds across several separate builds.
3. **Given** a gate failure, **When** results are reported, **Then** the failed metric and its measured multiple are named.

---

### User Story 4 - Get the codec automatically (Priority: P4)

Once the new codec has passed every gate, users who do not choose a plugin receive it automatically wherever it is the better choice.

**Why this priority**: It extends the benefit to everyone, but it carries the greatest compatibility risk, so it comes last and depends on the proven results of User Stories 2 and 3.

**Independent Test**: Compare automatic selection before and after the change across small, large, compressible and incompressible inputs.

**Acceptance Scenarios**:

1. **Given** all gates have passed, **When** a user compresses a large file without naming a plugin, **Then** the new codec is used.
2. **Given** any input type, **When** automatic selection runs, **Then** the choice for inputs outside the codec's intended use does not regress.

---

### Edge Cases

- Empty files, and files smaller than the minimum useful unit, must round-trip.
- Incompressible data must not expand beyond a small fixed overhead, and must not slow compression sharply.
- Truncated, corrupted or deliberately forged files must produce a clear error, with no crash, hang, out-of-bounds access or oversized allocation.
- A file claiming a huge decompressed size must be rejected rather than allocated.
- A version of Crush without the codec must report an unknown-plugin error when it meets such a file, rather than produce wrong output.
- Runs with more cores than blocks, and on single-core machines, must work correctly.
- Piped input with the codec explicitly requested must round-trip.
- Mixed archives of already-compressed and compressible data must keep their ratio and not lose speed sharply.

## Requirements *(mandatory)*

### Functional Requirements

**Engine**

- **FR-001**: The system MUST profile the existing large-file compression and decompression paths, and record how time is divided, before scaling changes are designed.
- **FR-002**: The system MUST remove the scaling bottlenecks found by profiling, and record each change's measured effect.
- **FR-003**: The parallel block engine MUST work with more than one codec, and existing archives MUST remain readable.
- **FR-004**: Blocks MUST be decodable independently, so parallel decompression and index-based random access work with any engine codec.

**Codec**

- **FR-005**: The system MUST provide a new codec, selectable by name, using a Crush-native format that does not depend on another compressor's file format.
- **FR-006**: The codec MUST restore every input byte for byte across all supported input sizes and types.
- **FR-007**: The codec MUST detect corruption through checksums and validate all header and table values before use.
- **FR-008**: The codec MUST fall back to storing data uncompressed when compression would not help.
- **FR-009**: The codec MUST ship one fixed speed and ratio profile in this feature. Multiple levels are out of scope.
- **FR-010**: Compressed files MUST carry enough version information for a later Crush release to recognise the format.
- **FR-011**: Format experiments MUST finish before the format is frozen, and the format MUST be documented when it is frozen.
- **FR-012**: The codec MUST honor existing cancellation behavior, and stop promptly on cancel on both compression and decompression paths.

**Selection and release**

- **FR-013**: The codec MUST remain opt-in until all P1 and P2 gates pass (SC-001 to SC-009).
- **FR-014**: Automatic selection for all other inputs MUST stay unchanged until the codec is promoted, and registering the codec MUST be shown not to change it.

**Constraints**

- **FR-015**: The feature MUST NOT add any C or other non-Rust compression dependency, including as a fallback.
- **FR-016**: Implementation MUST use safe Rust by default.
- **FR-017**: Unsafe code MAY be introduced only where a recorded measurement shows that safe code cannot meet a gate after reasonable safe optimization has been tried.
- **FR-018**: Each permitted use of unsafe code MUST be limited in scope, have its safety conditions documented, and have fuzz coverage that exercises it, plus the constitution's memory-safety checks.
- **FR-019**: A competing compressor MAY be used as an external benchmark comparator, but MUST NOT become a build or runtime dependency of Crush.

**Measurement**

- **FR-020**: The system MUST provide a repeatable head-to-head that runs Crush and a live competitor in one session, at 1, 4 and all cores.
- **FR-021**: Head-to-head reports MUST include both full-pipeline and in-memory single-core results.
- **FR-022**: Any public performance claim MUST state the machine, corpus, competitor version and spread.

### Key Entities

- **Block**: An independently decodable portion of the input, with its own position in the index.
- **Block index**: The mapping that locates blocks, enabling random access and parallel work.
- **Codec profile**: The single fixed trade-off of speed against ratio that this feature delivers.
- **Benchmark corpus**: A named input set used for gates. It includes `real`, `text` and an added log or text-like corpus.
- **Gate result**: A measured multiple against a live competitor, with its spread, build and machine recorded.
- **Unsafe exception record**: The measured evidence, scope, safety conditions and fuzz coverage that justify one permitted use of unsafe code.

## Success Criteria *(mandatory)*

### Measurable Outcomes

All speed and ratio multiples are against zstd -3 in the same session, on the reference workstation and `real` corpus, unless stated otherwise. The reference figures are from 2026-10-30, and the live multiple is what governs.

- **SC-001**: Compression ratio is at least 0.99x of zstd -3 at 1, 4 and all cores (reference 3.70). Revised from 1.00x on 2026-11-01.
- **SC-002**: Full-pipeline compression speed is at least 1.0x at 1 core. Speed at 4 and all cores is reported, not gated. Revised on 2026-11-01 from 1.0x at 4 cores and 1.5x at all cores.
- **SC-003**: Full-pipeline decompression speed is at least 2.0x at 4 cores and 3.0x at all cores. These are the codec's headline targets. Speed at 1 core is reported, not gated. Revised on 2026-11-01 from a further 1.0x gate at 1 core.
- **SC-004**: In-memory, single-core speed against zstd -3 is reported for both compression and decompression, not gated. Revised on 2026-11-01 from at least 0.85x.
- **SC-005**: Decompressing existing large archives speeds up by at least 3.0x at 4 cores and at least 3.3x at all cores, compared with 1 core (today 2.19x and 2.58x). The plan may raise these targets after profiling.
- **SC-006**: Memory use stays below 32 MB per thread, not counting input and output data.
- **SC-007**: All new decoding paths complete at least 100,000 fuzz iterations with no crash, hang, memory error or incorrect output.
- **SC-008**: Retrieving one block by index from a large file takes under 5 ms on the reference workstation (today 1.14 ms with smaller blocks).
- **SC-009**: No existing plugin slows by more than 5%, and all existing archives decompress byte for byte.
- **SC-010**: On the `text` corpus, single-core compression speed is at least 1.0x of zstd -3 at a ratio of at least 1.32.
- **SC-011**: A win of under 10% counts only if it holds across several separate builds.

### North-Star Goal

Crush aims to be the fastest compression engine in the world. A public "fastest" claim requires SC-001 to SC-011, reproduced under FR-020 to FR-022, and is limited to the conditions measured. Two stretch outcomes extend the claim:

- **Stretch A**: A true single-core win, at least 1.0x of zstd -3 in memory for both directions at the same ratio.
- **Stretch B**: On a same-run public benchmark of the Silesia corpus: over 300 MB/s compression, under 34.5% compressed size, and over 2,500 MB/s decompression. No surveyed codec occupies this point.

Missing a stretch outcome does not block release of the codec.

## Assumptions

- **Target users**: Developers and operators compressing large files on multi-core machines, who value speed at a strong ratio over compatibility with other tools.
- **File compatibility**: Interoperability with zstd tools is not a goal, because Crush files already carry their own header.
- **Compatibility across builds**: The new format may change freely until it is frozen, in line with the constitution's feature-branch rules.
- **Reference workstation**: Gates run on the i9-12900K under WSL2, which is the machine behind the existing comparison data. Windows and WSL results are not interchangeable.
- **Comparator**: zstd 1.5.7 is the comparison baseline, because it is the latest tagged release. Later releases are compared when available.
- **Unmeasured inputs**: Research figures for time shares and ablations derive from published numbers, so they are hypotheses. Planning must measure them on Crush hardware first.
- **Modified comparator builds**: Feature-removal experiments need a custom zstd build, which requires separate approval. If approval is not given, planning falls back to Crush-side experiments.
- **Scope exclusions**: Multiple compression levels, dictionaries, long-range matching, GPU paths, and a shared window across blocks are out of scope.
- **Entropy coding**: Designs needing a patent review are out of scope unless that review is done first.
- **Constitution**: This feature needs an amendment for head-to-head benchmarks, the performance targets and the stale dependency list. Amendments follow the constitution's process.
