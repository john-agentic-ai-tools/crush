#!/usr/bin/env bash
# Tests for .claude/hooks/perf-trigger.sh and the bench.py state it reads
# (change sets, measurement ledger, perf sessions).
#
# Builds a throwaway git repo with a develop branch and a fake hot-path file,
# copies the hook and bench.py into it, and checks when the hook blocks.
#
#   bash .claude/hooks/tests/test-perf-trigger.sh
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
mkdir -p crush-core/src .claude/hooks/lib .claude/skills/perf
printf 'pub fn f() {}\n' > crush-core/src/compression.rs
printf '.claude/state/\n' > .gitignore
cp "$SRC/.claude/hooks/perf-trigger.sh" .claude/hooks/
cp "$SRC/.claude/hooks/lib/find-python.sh" .claude/hooks/lib/
cp "$SRC/.claude/skills/perf/bench.py" .claude/skills/perf/
git add -A && git commit -q -m base
git checkout -q -b 099-feature

PASS=0 FAIL=0
hook() { printf '%s' "${1:-{\"stop_hook_active\":false\}}" | bash .claude/hooks/perf-trigger.sh; }
bench() { "$PY" .claude/skills/perf/bench.py "$@"; }
reset_stamp() { rm -f .claude/state/perf-last-check; }
edit() { printf '// %s\n' "$1" >> crush-core/src/compression.rs; reset_stamp; }
expect_block() {
    if hook | grep -q '"decision":"block"'; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL (expected block): $1"; fi
}
expect_silent() {
    local out; out="$(hook "${2:-}")"
    if [ -z "$out" ]; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL (expected silence): $1: $out"; fi
}
current_hash() { bench changeset --json | "$PY" -c 'import json,sys; print(json.load(sys.stdin)["hash"])'; }
# A live process with an OS pid, standing in for a running bench.py (which
# records os.getpid()). Under Git Bash, $$ is an MSYS pid Windows tools don't know.
"$PY" -c 'import os, sys, time; open(sys.argv[1], "w").write(str(os.getpid())); time.sleep(300)' \
    "$REPO/live.pid" &
SLEEPER=$!
trap 'kill "$SLEEPER" 2>/dev/null; wait "$SLEEPER" 2>/dev/null; cd /; rm -rf "$REPO"' EXIT
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$REPO/live.pid" ] && break; sleep 0.2; done
LIVE_PID="$(cat "$REPO/live.pid")"
iso() { "$PY" -c "import datetime as d; print((d.datetime.now(d.timezone.utc) + d.timedelta(hours=$1)).isoformat())"; }

expect_silent "no changes"

edit one
expect_block "hot-path edit"
expect_silent "same change set again (stamp)"
expect_silent "stop_hook_active" '{"stop_hook_active":true}'

edit two
mkdir -p .claude/state
printf '{"measured":[{"changeset":"%s","cmd":"ab","name":"t","finished":"%s"}]}' "$(current_hash)" "$(iso 0)" \
    > .claude/state/perf-measured.json
expect_silent "change set already measured"

edit three
expect_block "measured ledger holds an older change set"

edit four
printf '{"running":{"changeset":"%s","pid":%d,"cmd":"ab","name":"t","started":"%s"}}' "$(current_hash)" "$LIVE_PID" "$(iso 0)" \
    > .claude/state/perf-measured.json
expect_silent "run in progress on this change set"
reset_stamp
printf '{"running":{"changeset":"%s","pid":%d,"cmd":"ab","name":"t","started":"%s"}}' "$(current_hash)" "$LIVE_PID" "$(iso -4)" \
    > .claude/state/perf-measured.json
expect_block "running entry older than 3 h"
reset_stamp
printf '{"running":{"changeset":"0000","pid":%d,"cmd":"ab","name":"t","started":"%s"}}' "$LIVE_PID" "$(iso 0)" \
    > .claude/state/perf-measured.json
expect_block "run in progress on a different change set"
rm -f .claude/state/perf-measured.json

# measuring(): credited when the tree is unchanged, not when it changes mid-run.
edit five
"$PY" - <<'PYEOF'
import sys
sys.path.insert(0, ".claude/skills/perf")
import bench
with bench.measuring("ab", "unchanged"):
    pass
with bench.measuring("ab", "edited"):
    with open("crush-core/src/compression.rs", "a") as f:
        f.write("// edited during the run\n")
PYEOF
reset_stamp
if grep -q '"unchanged"' .claude/state/perf-measured.json && ! grep -q '"edited"' .claude/state/perf-measured.json \
    && ! grep -q '"running"' .claude/state/perf-measured.json; then
    PASS=$((PASS + 1))
else
    FAIL=$((FAIL + 1)); echo "FAIL: measuring() ledger"; cat .claude/state/perf-measured.json
fi
expect_block "tree edited during a run is not credited"

edit six
bench session begin --mode goal --name t >/dev/null
expect_silent "active perf session on this branch"
"$PY" - <<PYEOF
import json
p = ".claude/state/perf-session.json"
s = json.load(open(p))
s["expires"] = "$(iso -1)"
json.dump(s, open(p, "w"))
PYEOF
expect_block "expired perf session"
reset_stamp
bench session begin --mode goal --name t >/dev/null
"$PY" - <<'PYEOF'
import json
p = ".claude/state/perf-session.json"
s = json.load(open(p))
s["branch"] = "some-other-branch"
json.dump(s, open(p, "w"))
PYEOF
expect_block "perf session for another branch"
bench session end >/dev/null

reset_stamp
mv .claude/skills/perf/bench.py /tmp/bench.py.$$
expect_silent "bench.py missing"
mv /tmp/bench.py.$$ .claude/skills/perf/bench.py

echo "perf-trigger tests: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
