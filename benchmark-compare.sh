#!/bin/bash
# Benchmark comparison: crush vs gzip vs pigz (Linux/WSL).
# Thin wrapper kept for existing docs; see perf/compare.py for options.
set -e
cd "$(dirname "$0")"
[ -n "$CRUSH" ] || [ -x target/release/crush ] || cargo build --release -p crush-cli
exec python3 perf/compare.py "$@"
