# Diagnosis: decompress_medium/1MB and the fast-lz gap to lz4, 2026-10-31

First use of `/perf diagnose` and of `bench.py profile`, `profile-diff` and
`callgrind`, on the two open questions from feature 023. No code was changed.

An earlier version of this log took line-level results from a parser bug: it
pinned every sample in a function to one line, so "`decompress.rs:209` 59.5%"
and "byte writes 10.4%" were wrong. The figures below come from the fixed parser
and were checked against `addr2line` on the raw addresses.

## decompress_medium/1MB, about 4% slower than 08e407f

**Class: code layout. Instructions and machine code are unchanged; the hot zlib
code moved. Nothing to fix in Crush.**

| evidence | result |
|---|---|
| `ab --ref 08e407f --filter` (Windows, ABBA) | +4.5% [+3.8, +5.3]; earlier runs +4.1% and +5.0% |
| A/A on identical source, four runs | +1.1%, +1.2%, +1.8%, +3.2% |
| `noise --filter` | floor 2.4% (from the +1.2% run; the worst A/A would give more) |
| `callgrind` (Linux build) | 6,641,316 vs 6,641,965 instructions, +0.01%; hot functions identical |
| `profile-diff` (Linux build) | largest move 1.1 points; doesn't reproduce |
| `profile-diff` (Windows) | `inflate_fast` 52.3% to 57.8%; `inffast.c:255` 34.2% to 40.1% |
| `inflate_fast` machine code (Windows) | 1,214 bytes on both sides, 6 bytes differ |
| `inflate_fast` address (Windows) | `0x20e550` to `0x20ec30`: offset in its 64-byte block 16 to 48 |

Windows builds zlib statically into the executable, through `libz-sys`; Linux
uses the system `libz.so`. That is why only Windows shows it: our change moved
`inflate_fast` within our binary, and the system library never moves.

The slowdown is real for this binary, at 4-5% against A/As of 1-3%. The time
goes to the copy loop at `inffast.c:254-255`, and the benchmark's input is 1 MB
of one repeated byte, so that loop is nearly all of the work.

Not settled: the six differing bytes look like address fields but weren't
decoded, and the link from alignment to the time is inferred. Forcing the old
alignment would confirm it.

## fast-lz vs lz4, 128 MB real-file corpus, Linux build, one core

Measured with `samply` on `fast_lz/*/file` and on `lz4 -b1 -e1 -i8`. There are no
hardware counters here, and kernel frames are absent, so page-fault time lands
on the user address. Milliseconds are estimates: profile share times the known
MB/s, per 128 MiB. Bucket boundaries differ slightly between the codecs. lz4
line numbers come from a `-g` build with identical disassembly of both codec
functions.

Calibrated this session, pinned, best of 7: fast-lz 714-733 / 2,597-2,651 MB/s;
lz4 795-798 / 4,514-4,616 MB/s. Ratio 2.230 vs 2.296.

### Decompress: about 23 ms behind

| area | fast-lz ms | lz4 ms | difference |
|---|---:|---:|---:|
| output buffer zero-fill | 12.3 | 0 | +12.3 |
| `shortcuts` entry/exit and call frame | 7.1 | none | +7.1 |
| literals | 7.9 | 3.9 | +4.0 |
| match copy | 17.7 | 17.2 | +0.5 |
| token, offset, lengths | 8.1 | 9.1 | -1.0 |
| munmap | 0.2 | 0 | +0.2 |
| **total** | **53.3** | **30.3** | **+23.0** |

- **Zero-fill is the largest cost, and the caller is measured.** Every `memset`
  sample comes from `out.resize(start + ulen + WILD, 0)` at
  `crush-lz/src/decompress.rs:122`. A separate C probe of the same 1 MiB pattern
  split it into about 6 ms of zeroing and 5 ms of first-touch page faults.
- **Match copy is level with lz4.** The earlier assumption that copies were the
  problem is ruled out.
- **The two-loop design costs about 7 ms.** The slow path takes as much time as
  `shortcuts` while handling about 13% of sequences. Some of that cost would
  move rather than vanish if the loops merged, because mispredicted exits
  probably inflate it (inferred).

SC-002 needs 0.60x, about 50.5 ms: 2.8 ms from today. Removing only the zeroing
would give about 0.64x; removing all 12.3 ms, about 0.74x. Neither is measured.
Zeroing may also warm the cache, so some of the saving could come back. An
untested idea in safe Rust: one `vec![0; n]` up front in `decode_frame`, since a
fresh mapping needs no fill, instead of a `resize` per block. The streamed CLI
path reuses a 1 MiB buffer, so this wouldn't change CLI numbers the same way.

### Compress: about 14 ms behind

| area | fast-lz ms | lz4 ms | difference |
|---|---:|---:|---:|
| search, table, post-match re-index | 95.4 | 100.5 | -5.1 |
| landing and backward extension | 31.0 | 19.8 | +11.2 |
| match count | 31.0 | 32.1 | -1.1 |
| emit | 21.4 | 18.7 | +2.7 |
| zero-fill | 7.2 | 0 | +7.2 |
| **total** | **185.6** | **171.2** | **+14.4** |

The zero-fill is at `crush-lz/src/compress.rs:332`, and fills the whole block
although about 45% is kept. fast-lz searches slightly faster than lz4 but spends
more around accepted matches; that is probably branch-mispredict recovery
(inferred, no counters).

SC-001's 1.15x needs about 149 ms. Everything in the gap, even closed completely,
reaches 1.00x, and the zero-fill alone reaches about 0.96x. So SC-001 can't come
from these findings; going beyond lz4 would need a different trade-off.

## Using the skill

The run worked end to end without a UAC prompt: saved Windows profiles covered
the Windows step with `--reuse`. It also found these, now fixed: the parser bug
above, setup samples counted as benchmark time, callgrind tables that hid
unchanged hot functions, a thread with one sample counted as busy, and every
benchmark binary started in every pass despite `--filter`.

Not covered by the tools: comparing against an external tool still took manual
work, namely the lz4 profile, its `-g` build and the bucketed tables.
