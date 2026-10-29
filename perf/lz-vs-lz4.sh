#!/usr/bin/env bash
# fast-lz vs lz4, in memory, one core, same session, same file (feature 023,
# SC-001..SC-003). Linux/WSL only: needs taskset, lz4 >= 1.10 and /usr/lib.
#
#   export PATH=~/.local/bin:$PATH     # lz4 built from source, see perf/README.md
#   bash perf/lz-vs-lz4.sh [RUNS]
#
# Corpus: the first 128 MB of `tar -cf - /usr/lib /usr/share` (override with
# CORPUS=path). Exits 1 if a target is missed: compress >= 1.15 x lz4,
# decompress >= 0.60 x lz4, ratio >= 2.18.
set -euo pipefail
cd "$(dirname "$0")/.."

RUNS="${1:-5}"
SIZE=134217728
CORPUS="${CORPUS:-/dev/shm/crush-lz-corpus.bin}"
if [ ! -s "$CORPUS" ] || [ "$(stat -c%s "$CORPUS")" -ne "$SIZE" ]; then
    tar -cf - /usr/lib /usr/share 2>/dev/null | head -c "$SIZE" > "$CORPUS"
fi

# Own target dir: building into the Windows checkout's target/ from WSL would
# clash with Windows builds.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/crush-lz-target}"
cargo build -q --release -p crush-lz --example codec_bench
BENCH="$CARGO_TARGET_DIR/release/examples/codec_bench"

LZ4="$(taskset -c 0 lz4 -b1 -i"$RUNS" "$CORPUS" 2>&1 | tr '\r' '\n' | grep 'MB/s' | tail -1)"
FAST="$(taskset -c 0 "$BENCH" "$CORPUS" "$RUNS")"

python3 - "$LZ4" "$FAST" "$(lz4 --version 2>&1 | head -1)" <<'PY'
import re, sys
lz4_line, fast_line, lz4_version = sys.argv[1:4]
m = re.search(r"\(\s*([\d.]+)\),\s*([\d.]+) MB/s,\s*([\d.]+) MB/s", lz4_line)
if not m:
    sys.exit(f"could not parse lz4 output: {lz4_line!r}")
lr, lc, ld = map(float, m.groups())
f = re.search(r"\(([\d.]+)\) compress best ([\d.]+) median [\d.]+ MB/s "
              r"decompress best ([\d.]+) median [\d.]+ MB/s", fast_line)
if not f:
    sys.exit(f"could not parse fast-lz output: {fast_line!r}")
fr, fc, fd = map(float, f.groups())

print(f"one core, in memory, best of runs ({lz4_version.strip('* ')})")
print("| codec | compress MB/s | decompress MB/s | ratio |")
print("|---|---|---|---|")
print(f"| lz4 -1 | {lc:,.0f} | {ld:,.0f} | {lr:.3f} |")
print(f"| fast-lz | {fc:,.0f} | {fd:,.0f} | {fr:.3f} |")
print(f"| fast-lz / lz4 | {fc / lc:.2f}x | {fd / ld:.2f}x | {fr / lr:.3f} |")
checks = [
    ("SC-001 compress >= 1.15x lz4", fc >= 1.15 * lc),
    ("SC-002 decompress >= 0.60x lz4", fd >= 0.60 * ld),
    ("SC-003 ratio >= 2.18", fr >= 2.18),
]
for name, ok in checks:
    print(f"{'PASS' if ok else 'MISS'}  {name}")
sys.exit(0 if all(ok for _, ok in checks) else 1)
PY
