# Feature Specification: LZ4-Class Fast Compression Plugin with Content-Aware Selection

**Feature Branch**: `023-lz4-fast-plugin`

**Created**: 2026-10-29

**Status**: Draft

**Input**: User description: "LZ4-class fast compression plugin with content-aware selection. Add a new compression plugin to Crush's plugin architecture, inspired by LZ4 (byte-oriented LZ77, no entropy coding), for users who want maximum single-core speed at an LZ4-like compression ratio. Performance targets (single core, in-memory codec, 128 MB real-file corpus = tar of /usr/lib and /usr/share; reference measured with `lz4 -b1` 1.10.0 on this machine: 788 MB/s compress, 4,417 MB/s decompress, ratio 2.296): compress >= 906 MB/s (lz4 -1 +15%), decompress >= 2,650 MB/s (60% of lz4), compression ratio >= 2.18 (within 5% of lz4). Constraints: 100% safe Rust (no unsafe), no new third-party dependencies (own implementation), constitution rules (TDD, fuzzing of the decoder, no unwrap in production). Content-aware selection: the plugin selector must only choose this plugin when the data actually compresses well with it (e.g. lz4 cannot compress base64 text, ratio 0.996, so such data must not go to this plugin), otherwise fall back to the existing DEFLATE plugins. Users can also select it explicitly. Phased: phase 1 is the codec and selection measured in memory; phase 2 (later user story) brings the end-to-end `crush compress` single-core throughput above the lz4 CLI (currently crush's single-core CLI pipeline caps at about 614 MB/s compress / 1,114 MB/s decompress even with no compression work, versus lz4 CLI 634 / 1,608), which requires reducing CLI pipeline overhead."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Compress with the fast plugin on request (Priority: P1)

A user who cares more about speed than size (log shipping, build caches, scratch data, data passed between processes) explicitly asks Crush for the fast plugin. Crush compresses and decompresses on a single core faster than LZ4, the established speed-first compressor, while the output stays about as small as LZ4's. The compressed file identifies itself, so decompression needs no extra options.

**Why this priority**: The codec is the core value of the feature. Without it, nothing else (automatic selection, end-to-end speed) has anything to select or speed up. It is independently useful to anyone who opts in.

**Independent Test**: Compress and decompress the reference corpus with the fast plugin selected explicitly, in memory on one core; check the round trip is exact, measure throughput and size against the reference fast compressor measured on the same machine in the same session.

**Acceptance Scenarios**:

1. **Given** any input data, **When** the user compresses it with the fast plugin and decompresses the result, **Then** the output is byte-for-byte identical to the input.
2. **Given** a file compressed with the fast plugin, **When** the user decompresses it without naming a plugin, **Then** Crush recognises the format and decompresses it.
3. **Given** the reference corpus on one core, **When** compressed with the fast plugin, **Then** compression throughput is at least 906 MB/s, decompression at least 2,650 MB/s, and the compression ratio at least 2.18.
4. **Given** a damaged or truncated fast-plugin file, **When** the user decompresses it, **Then** Crush reports an error and never crashes, hangs, or produces more output than the file declares.
5. **Given** data the fast plugin cannot shrink, **When** the user still selects it explicitly, **Then** compression succeeds, the output is at most marginally larger than the input, and the round trip is exact.

---

### User Story 2 - Automatic selection uses the fast plugin only when it pays off (Priority: P2)

A user asks Crush to favour speed but does not name a plugin. Crush looks at the data before choosing: if the data compresses well with the fast plugin, Crush uses it; if not (for example base64-encoded or already-compressed content that LZ4-style compression cannot shrink), Crush falls back to the existing DEFLATE-based plugins, which can still shrink some of that data, or to storing it. Users who do not ask for speed see no change.

**Why this priority**: It turns the codec into something users benefit from without knowing its strengths and weaknesses, and it prevents the fast plugin from producing no compression on data where DEFLATE still helps (base64 text: LZ4 ratio 0.996, DEFLATE 1.32).

**Independent Test**: With speed preferred and no plugin named, compress a set of labelled inputs (real files, source text, logs, base64 text, random data, already-compressed archives) and check which plugin Crush chose for each, how long the decision took, and that every result round-trips.

**Acceptance Scenarios**:

1. **Given** speed is preferred and no plugin is named, **When** the input is the real-file corpus or similar compressible data, **Then** Crush selects the fast plugin.
2. **Given** speed is preferred and no plugin is named, **When** the input is base64 text, random data, or already-compressed data, **Then** Crush does not select the fast plugin.
3. **Given** the user does not express a speed preference, **When** Crush compresses any input, **Then** it chooses the same plugin as before this feature and the output ratio is unchanged.
4. **Given** speed is preferred, **When** Crush examines the data to choose, **Then** the examination adds at most 2% to the total compression time of a 1 MB or larger input.

---

### User Story 3 - End-to-end single-core speed through the command line (Priority: P3)

A user runs `crush compress` / `crush decompress` with the fast plugin on one core, and the whole command — reading the file, compressing, writing the result — finishes faster than the LZ4 command-line tool on the same file and machine.

**Why this priority**: Users experience the command, not the codec. Today Crush's own command-line pipeline (file reading, copies, checksums, writing) caps one core at about 614 MB/s compress and 1,114 MB/s decompress even when no compression work is done, below the LZ4 tool's 634 / 1,608 MB/s, so the fast codec cannot show its speed end to end until that overhead shrinks. It depends on Story 1 and is a separate phase.

**Independent Test**: On the reference corpus, pinned to one core, time `crush compress`/`crush decompress` with the fast plugin against the LZ4 command-line tool, file to file in memory-backed storage, alternating the tools and taking the median of repeated runs.

**Acceptance Scenarios**:

1. **Given** the reference corpus on one core, **When** the user compresses it with `crush compress` using the fast plugin, **Then** the command completes faster than `lz4 -1` on the same file.
2. **Given** the resulting file, **When** the user decompresses it with `crush decompress` on one core, **Then** the command completes faster than `lz4 -d` on the LZ4 file of the same input.
3. **Given** the pipeline improvements, **When** existing plugins are used end to end, **Then** none of them gets slower.

---

### Edge Cases

- Empty input and inputs smaller than the fast plugin's minimum match length round-trip exactly.
- Highly repetitive input (long runs of one byte or one phrase) compresses without excessive memory use or slowdown.
- Input that is compressible in some regions and not others (e.g. an archive mixing text and compressed images): the fast plugin still produces correct output; automatic selection decides on the whole input, not only its first bytes.
- Very large inputs (multiple GB) are processed with bounded memory per thread.
- A fast-plugin file whose declared sizes are inconsistent, whose back-references point before the start of the output, or whose length fields overflow: decompression fails cleanly with an error.
- A fast-plugin file given to a build of Crush that predates this feature: it is rejected with a clear "unknown format/plugin" error, not misread.
- Explicit selection of the fast plugin together with options that only apply to other plugins (e.g. GPU options): Crush warns or rejects consistently with how it treats other plugin-specific options today.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Crush MUST provide a new fast compression plugin registered alongside the existing plugins, listed by the plugin listing command, and selectable by name.
- **FR-002**: The fast plugin MUST reproduce any input exactly on decompression, including empty input.
- **FR-003**: Files produced by the fast plugin MUST identify their plugin so that decompression and inspection work without the user naming it.
- **FR-004**: The fast plugin MUST carry the same integrity protection as other Crush formats, and decompression MUST detect corrupted data.
- **FR-005**: Decompression of fast-plugin data MUST reject malformed or truncated input with an error, MUST NOT crash or hang, and MUST NOT produce or allocate more output than the data's declared original size.
- **FR-006**: The fast plugin MUST achieve the throughput and ratio targets in SC-001 to SC-003 on the reference corpus.
- **FR-007**: When the user prefers speed and names no plugin, Crush MUST estimate how well the data compresses with the fast plugin before choosing, and MUST select it only if the estimated compression ratio is at least 1.10 (i.e. it saves at least about 9%); otherwise it MUST fall back to the existing selection among the other plugins.
- **FR-008**: The estimate in FR-007 MUST examine data from across the whole input (not only its beginning) and MUST cost no more than 2% of the input's total compression time for inputs of 1 MB or more.
- **FR-009**: When the user names the fast plugin explicitly, Crush MUST use it regardless of the estimate.
- **FR-010**: Automatic selection without a speed preference MUST continue to choose exactly as it does today.
- **FR-011**: The inspection command MUST report the fast plugin's name, original size, compressed size and ratio for fast-plugin files, as it does for other plugins.
- **FR-012**: The fast plugin MUST stay within the project's memory limit of 32 MB per thread.
- **FR-013**: Existing plugins' performance and output MUST NOT regress by more than 5% as a result of this feature.
- **FR-014** *(Story 3)*: The command-line pipeline MUST be able to compress and decompress with the fast plugin end to end on one core faster than the LZ4 command-line tool on the reference corpus, without making any existing plugin slower end to end.

### Key Entities

- **Fast-plugin compressed data**: The output of the fast plugin: identifies its plugin, records the original size and an integrity check, and holds the compressed content. Must be decodable without any external information.
- **Selection estimate**: A quick measurement of how much the fast plugin would shrink a given input, taken from samples spread over the input, used only by automatic selection when speed is preferred.
- **Reference corpus and reference measurements**: The 128 MB real-file corpus (the first 128 MB of a tar of `/usr/lib` and `/usr/share`) plus the reference fast compressor's throughput and ratio measured on the same machine; all speed targets are relative to these.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On one core, in memory, the fast plugin compresses the reference corpus at 906 MB/s or more — at least 15% faster than the reference fast compressor (788 MB/s on the reference machine), re-measured in the same session when verifying.
- **SC-002**: On one core, in memory, the fast plugin decompresses the reference corpus at 2,650 MB/s or more (at least 60% of the reference compressor's 4,417 MB/s).
- **SC-003**: The fast plugin's compression ratio on the reference corpus is 2.18 or higher (within 5% of the reference compressor's 2.296).
- **Measured outcome (2026-10-29, after tuning)**: compress 0.92x of the reference compressor (SC-001 not met), decompress 0.57x (SC-002 not met, close), ratio 2.230 (SC-003 met). The targets stand unchanged; the user chose to record the measured speed/ratio frontier and continue with the remaining stories. Details: `perf-log.md`, "Outcome of T023".
- **SC-004**: With speed preferred and no plugin named, 100% of base64-text, random, and already-compressed test inputs are routed away from the fast plugin, and 100% of the real-file, source-code and log test inputs are routed to it.
- **SC-005**: The selection estimate adds no more than 2% to compression time for inputs of 1 MB or more.
- **SC-006**: The fast plugin's decoder survives at least 100,000 fuzzing iterations with zero crashes, hangs or round-trip mismatches.
- **SC-007**: No existing benchmark regresses by more than 5%, and users who do not ask for speed see identical compressed output size.
- **SC-008** *(Story 3)*: On one core, `crush compress` and `crush decompress` with the fast plugin finish faster than `lz4 -1` and `lz4 -d` on the reference corpus (currently 634 MB/s and 1,608 MB/s end to end on the reference machine).

## Assumptions

- **Speed preference**: "Prefers speed" means the existing fast compression level (`--level fast`). The default level and `best` keep today's plugin choice and ratio; the fast plugin's ratio (about 2.3 vs 3.5 on real files) is too large a size trade-off to impose on users who did not ask for it.
- **Reference compressor**: LZ4 1.10.0 at its default level, measured with its built-in in-memory benchmark pinned to one core. Targets are relative to it; the absolute numbers above come from the reference machine (i9-12900K, WSL2 Ubuntu) and are re-measured in the same session when verifying.
- **Measurement method**: Codec targets (SC-001 to SC-003) are in-memory, single-threaded, on the 128 MB reference corpus; end-to-end targets (SC-008) are file to file on memory-backed storage, pinned to one core, median of repeated alternating runs.
- **Compatibility**: The fast plugin's format is new; older Crush builds are not expected to read it (per the project's release-boundary compatibility policy). The format may change until the first release that includes it.
- **Multi-core**: Multi-threaded use of the fast plugin is out of scope for this feature; it is single-core. Existing multi-core plugins are unaffected.
- **Fallback**: When automatic selection rejects the fast plugin, it falls back to whatever the existing selection logic chooses (today a DEFLATE-based plugin), which also handles storing incompressible data.
- **Selection threshold**: 1.10 is the starting threshold for "compresses well enough to be worth it"; it may be tuned during planning if measurements show a better cut-off, as long as SC-004 holds.

### Constraints (from the project constitution and the request)

- The plugin is implemented within the project without new third-party dependencies, in memory-safe code: the codec has no `unsafe`; the one exception, approved by the user on 2026-10-29 for Story 3, is the memory map the CLI uses to read large input files (same pattern as the parallel engine), test-first, with no panicking shortcuts in production code, and with the decoder fuzzed (SC-006).
- Performance claims are verified with interleaved A/B measurements on the same machine, per the project's benchmark rules.
