#!/usr/bin/env bash
# Tests for the benchmark selection in bench.py: `targets` (changed file to
# benchmark mapping) and the handling of crates missing at the base ref.
#
# Builds a throwaway git repo with fake crate files, copies bench.py into it,
# and checks what it would run. Nothing is compiled or benchmarked.
#
#   bash .claude/hooks/tests/test-bench-targets.sh
set -u

SRC="$(cd "$(dirname "$0")/../../.." && pwd)"
# shellcheck source=../lib/find-python.sh
. "$SRC/.claude/hooks/lib/find-python.sh"
find_python || { echo "SKIP: no python >= 3.8"; exit 0; }

REPO="$(mktemp -d)"
trap 'cd /; rm -rf "$REPO"' EXIT
cd "$REPO" || exit 1
git init -q && git checkout -q -b develop  # no `init -b`: git < 2.28
git config user.email t@example.com
git config user.name test
git config commit.gpgsign false
mkdir -p crush-core/src/plugin crush-lz/src crush-parallel/src crush-cli/src crush-cli/tests .claude/skills/perf
for f in crush-core/src/compression.rs crush-core/src/decompression.rs crush-core/src/plugin/selector.rs \
         crush-core/src/lib.rs crush-lz/src/compress.rs crush-parallel/src/engine.rs \
         crush-cli/src/main.rs crush-cli/tests/cli.rs; do
    printf '// base\n' > "$f"
done
cp "$SRC/.claude/skills/perf/bench.py" .claude/skills/perf/
git add -A && git commit -q -m base

PASS=0 FAIL=0
targets() { "$PY" .claude/skills/perf/bench.py targets --ref develop; }
touch_file() { printf '// edit\n' >> "$1"; }
reset() { git checkout -q -- . ; git clean -fdq -- crush-core crush-lz crush-parallel crush-cli; }
expect() {
    local out; out="$(targets)"
    if printf '%s' "$out" | grep -qF -- "$2"; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $1: wanted '$2' in: $out"; fi
}
reject() {
    local out; out="$(targets)"
    if printf '%s' "$out" | grep -qF -- "$2"; then FAIL=$((FAIL + 1)); echo "FAIL: $1: did not want '$2' in: $out"; else PASS=$((PASS + 1)); fi
}

expect "no change" "no benchmark covers these changes"

touch_file crush-core/src/decompression.rs
expect "decompression maps to its benches" "crush-core --bench compression: ^decompress_"
reject "decompression does not run compress benches" "^compress_"
reset

touch_file crush-core/src/decompression.rs
touch_file crush-core/src/compression.rs
expect "two files on one bench merge filters" "^compress_|^decompress_"
reset

touch_file crush-core/src/plugin/selector.rs
expect "selector maps to its bench" "crush-core --bench plugin_selection: all"
reset

touch_file crush-core/src/lib.rs
expect "unmapped file falls back to the crate suite" "crush-core --bench compression: all"
reset

touch_file crush-core/src/lib.rs
touch_file crush-core/src/compression.rs
expect "the fallback's wider filter wins" "crush-core --bench compression: all"
reset

touch_file crush-lz/src/compress.rs
expect "crush-lz maps to its codec bench" "crush-lz --bench codec: all"
reset

touch_file crush-parallel/src/engine.rs
expect "parallel falls back to its quick suite" "crush-parallel --bench throughput"
reset

printf '// new\n' > crush-lz/src/new_module.rs
expect "untracked files count" "crush-lz --bench codec: all"
reset

touch_file crush-cli/src/main.rs
expect "cli changes point at compare.py" "perf/compare.py"
reject "cli changes are not 'uncovered'" "no benchmark covers"
reset

touch_file crush-cli/tests/cli.rs
expect "tests are not performance changes" "no benchmark covers these changes"
reset

# A crate missing at the base ref is skipped, with a note.
"$PY" - "$REPO" <<'EOF' > missing.out 2>&1
import importlib.util, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location("bench", Path(sys.argv[1]) / ".claude/skills/perf/bench.py")
b = importlib.util.module_from_spec(spec); sys.modules["bench"] = b; spec.loader.exec_module(b)
base = Path(sys.argv[1]) / "base"
(base / "crush-core").mkdir(parents=True)
(base / "crush-core" / "Cargo.toml").write_text("")
kept = b.present_at(base, [("crush-core", "", None), ("crush-new", "", None)])
print("kept", [e[0] for e in kept])
print("filtered", b.with_filter([("a", "x", "old"), ("a", "x", None)], "new"))
EOF
if grep -q "crush-new does not exist at the base ref" missing.out \
    && grep -q "kept \['crush-core'\]" missing.out \
    && grep -q "filtered \[('a', 'x', 'new')\]" missing.out; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1)); echo "FAIL: missing crate handling:"; cat missing.out
fi

echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
