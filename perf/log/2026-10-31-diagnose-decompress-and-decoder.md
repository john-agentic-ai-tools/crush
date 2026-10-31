# Diagnosis: decompress_medium/1MB and the fast-lz decoder, 2026-10-31

First use of `bench.py profile`, `profile-diff` and `callgrind`, on the two open
questions from feature 023. No code was changed.

## decompress_medium/1MB, about 4% slower than 08e407f

**Class: code layout. The slowdown is not in code the feature changed.**

| evidence | result |
|---|---|
| `ab --ref 08e407f --filter` (Windows, ABBA) | +4.5% [+3.8, +5.3], reproduces the earlier +4.1% and +5.0% |
| `ab --ref HEAD --filter`, identical source | +3.2% [+2.4, +3.9]; earlier A/As +1.1% and +1.8% |
| `callgrind` (Linux build) | 6,641,316 vs 6,641,965 instructions, +0.01% |
| `profile-diff` (Windows) | `inflate_fast` self time 52.2% to 57.7%; nothing else moved over 2 points |
| `inflate_fast` machine code (Windows) | 1,214 bytes on both sides, 6 bytes differ |
| `inflate_fast` address (Windows) | `0x20e550` to `0x20ec30`: offset in its 64-byte block 16 to 48 |

`inflate_fast` is C code in `libz-sys`, which feature 023 didn't touch. The time
concentrates in the byte-copy loop at `zlib/inffast.c:254-255`, and this
benchmark's input is 1 MB of one repeated byte, so that loop is almost all of
the work.

The six differing bytes look like address fields changed by the move; they were
not decoded. Identical source built twice moved this benchmark by up to 3.2%,
so most or all of the 4.5% is within what layout alone does here. That
comparison came from separate runs. It was not confirmed by forcing the old
alignment, and the link between alignment and the time is inferred.

The 1 MB benchmark is a poor guide to real data: one repeated byte is the worst
case for this loop, and nothing suggests a fix belongs in Crush.

## fast-lz decoder, 0.57x of lz4 (SC-002 needs 0.60x)

Profile only, `fast_lz/decompress/text-64MB`, Linux build, 64 MB synthetic text.
This isn't the 128 MB real-file corpus, so shares may differ there.

| where | share of samples |
|---|---|
| `shortcuts`, the main decode loop (`crush-lz/src/decompress.rs:209`) | 59.5% |
| `memset` | 23.9% |
| byte writes (`ptr::write::<u8>`, slower decode paths) | 10.4% |

Most of the `memset` is probably `out.resize(start + ulen + WILD, 0)` at
`crush-lz/src/decompress.rs:122`: the decoder zero-fills each block's output
before overwriting it, and that includes first-touch page faults. Not confirmed:
the profile's caller frames are return addresses and can't attribute it
exactly.

If so, zero-filling is a much larger cost than the 5% SC-002 is short by, and it
applies to in-memory decoding. The streamed CLI path reuses one 1 MiB buffer,
so it shouldn't pay this per block. A saving isn't guaranteed: `lz4 -b` also
touches its output pages, and safe Rust can't skip initialising a buffer, so
any fix would have to reuse one. Next step is a `/perf goal` with this as its
first hypothesis, measured on the real-file corpus.

Callgrind identical-code check on this benchmark: 22,115,565 instructions on
both sides, a difference of exactly 0.
