#!/usr/bin/env bash
# Stop hook: ask for /perf check after significant hot-path changes.
#
# Looks at everything that differs from the merge base with develop (commits
# plus working tree) in the performance crates' src/ directories. The change
# counts as significant when it touches a hot-path file or adds/removes at
# least PERF_TRIGGER_LINES lines (default 40). Then it blocks the stop once,
# with a reason telling Claude to run /perf check.
#
# The change-set logic lives in `bench.py changeset --hook`, which also says
# when to stay silent even though the change is significant:
#   - a /perf goal session owns this branch (`bench.py session begin`), or
#   - this exact change set was already measured by bench.py run/noise/ab, or
#   - a bench.py run is measuring it right now.
# Without that, the hook asked /perf to run /perf check on its own experiments
# while their A/B was in flight.
#
# Fires at most once per distinct change set: the hash is recorded in
# .claude/state/perf-last-check, and stop_hook_active is honoured, so it can't
# loop. Never fails the session: every non-trigger path exits 0 silently.

set -u
cd "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || exit 0

# Claude is already continuing because of a Stop hook: don't stack another block.
INPUT="$(cat 2>/dev/null || true)"
case "$INPUT" in *'"stop_hook_active":true'* | *'"stop_hook_active": true'*) exit 0 ;; esac

BENCH=.claude/skills/perf/bench.py
[ -f "$BENCH" ] || exit 0
# shellcheck source=lib/find-python.sh
. .claude/hooks/lib/find-python.sh 2>/dev/null || exit 0
find_python || exit 0

VERDICT="$("$PY" "$BENCH" changeset --hook --threshold "${PERF_TRIGGER_LINES:-40}" 2>/dev/null)" || exit 0
case "$VERDICT" in
    "FIRE "*) ;;
    *) exit 0 ;;  # QUIET, SKIP <reason>, or anything unexpected
esac
REST="${VERDICT#FIRE }"
HASH="${REST%% *}"
WHY="${REST#* }"

STATE_DIR=".claude/state"
STAMP="$STATE_DIR/perf-last-check"
mkdir -p "$STATE_DIR" 2>/dev/null
[ "$(cat "$STAMP" 2>/dev/null)" = "$HASH" ] && exit 0
echo "$HASH" > "$STAMP"

printf '{"decision":"block","reason":"Significant performance-crate changes since develop (%s). Run /perf check before finishing and report the result. This reminder fires once per change set."}\n' "$WHY"
exit 0
