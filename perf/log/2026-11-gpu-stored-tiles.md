# GPU decoding of stored tiles

2026-11-02, i9-12900K, RTX 3060 Ti, Windows, driver 617.42, CUDA toolkit 13.1, 128 MiB.

## The bug

`decompress_tiles_gpu` ignored the stored flag and sent raw tiles to the GDeflate kernels. The checksum failed, so the whole file fell back to the CPU. With checksums off, the output was corrupt and no error was raised. `text` and `random` failed at tile 0 and `real` at tile 327, on both backends. `log` and `repetitive` have no stored tiles and worked.

This was not a regression. The code had the same gap at 3d01d9d, the commit that added the 560 MiB/s figure, and no test sent a stored tile through the GPU path. The content of that 1.8 GB file is not known.

The fix decodes stored tiles without the GPU. Four unit tests use a fake backend, and failed before it. All five corpora now round-trip through both backends with no fallback.

## Measurement

`perf/compare_gpu.py`, all cores, on disk, median of 7 alternating runs, commit 3b66133. MB/s, compress then decompress:

| Data | Parallel | wgpu | CUDA | GPU ratio |
|---|---|---|---|---|
| `real` | 597, 1,177 | 532, 146 | 531, 242 | 2.359 |
| `log` | 686, 1,303 | 577, 162 | 592, 251 | 2.800 |
| `repetitive` | 1,294, 1,528 | 842, 180 | 845, 278 | 55.723 |
| `text` | 458, 933 | 291, 227 | 282, 329 | 0.998 |
| `random` | 556, 970 | 287, 230 | 294, 341 | 0.998 |

Every tile in `text` and `random` is stored, so those GPU figures measure no GPU decoding. Decompress interquartile ranges on `repetitive` are 73% and 52%, so treat those two figures as rough. Parallel's compress interquartile ranges reach 25%.

On `real`, the GPU dispatch took about 156 ms with CUDA and 318 ms with wgpu in single verbose runs, of about 0.55 s and 0.9 s in all. What takes the rest was not profiled. Short files likely suffer most if it is start-up cost, which would fit the 560 MiB/s on a much larger file, but that is untested.

An early 33 MB/s GPU compress reading on `real` was one first run, and did not repeat.

## Performance check

The quick and full A/Bs against 98b4a9a flagged `decompress_throughput/cpu`: +2.7% to +7.4%, three of four over the gate. The CPU decode code did not change. With `-C llvm-args=-align-all-functions=6` on both sides the same four read -0.7% to +0.8%, and the base lost its advantage, so this is binary placement, not added work. Callgrind has no case for these benchmarks. The GPU benchmarks were not part of this filter.
