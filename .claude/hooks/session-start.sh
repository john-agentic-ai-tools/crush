#!/usr/bin/env bash
# SessionStart hook: surface work the security-patch skill should pick up.
#
# Anything printed to stdout is added to Claude's context at session start.
# Prints nothing when there is nothing to report. Never fails the session:
# every path exits 0.
#
#  1. Resume check: an interrupted sweep on the current branch (same rule as
#     step 0 of .claude/skills/security-patch/SKILL.md).
#  2. Open /perf goal session on this branch (it silences the perf Stop hook).
#  3. Daily advisory check: at most once per 24h, cargo audit against the
#     workspace lockfile.

set -u
cd "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || exit 0

STATE_DIR=".claude/state"
mkdir -p "$STATE_DIR" 2>/dev/null

# --- 1. Resume check --------------------------------------------------------
BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null)"
if [[ "$BRANCH" =~ ^[0-9]{3}-.*(security|dependency|dependencies|deps|toolchain) ]]; then
    # Local refs only: a fetch here would slow every session start.
    AHEAD="$(git rev-list --count origin/develop..HEAD 2>/dev/null || echo 0)"
    DIRTY="$(git status --porcelain --untracked-files=no 2>/dev/null | wc -l | tr -d ' ')"
    PR_STATE=""
    if command -v gh >/dev/null 2>&1; then
        PR_STATE="$(timeout 10 gh pr list --head "$BRANCH" --state open \
            --json number,statusCheckRollup \
            --jq '.[0] | select(.) | "PR #\(.number) open, checks: \([.statusCheckRollup[] | (.conclusion // .state // "PENDING")] | unique | join("/"))"' \
            2>/dev/null)"
    fi
    if [ "$AHEAD" -gt 0 ] || [ "$DIRTY" -gt 0 ] || [ -n "$PR_STATE" ]; then
        echo "Interrupted security/dependency sweep on branch '$BRANCH' (commits ahead of develop: $AHEAD; uncommitted tracked files: $DIRTY${PR_STATE:+; $PR_STATE})."
        echo "Suggest to the user: run /security-patch --resume to continue from the right step."
    fi
fi

# --- 2. Open /perf goal session --------------------------------------------
# A session marker silences the perf-trigger Stop hook for this branch. If a
# goal run crashed or was abandoned without `session end`, say so.
SESSION="$STATE_DIR/perf-session.json"
if [ -f "$SESSION" ] && grep -q "\"branch\": \"$BRANCH\"" "$SESSION" 2>/dev/null; then
    EXPIRES="$(sed -n 's/.*"expires": "\([^"]*\)".*/\1/p' "$SESSION" | head -1)"
    echo "A /perf goal session is open on '$BRANCH' (expires $EXPIRES); the perf-trigger Stop hook stays silent until then."
    echo "If no /perf goal is in progress, end it: python .claude/skills/perf/bench.py session end"
fi

# --- 3. Daily advisory check -----------------------------------------------
STAMP="$STATE_DIR/last-audit"
if [ -n "$(find "$STAMP" -mmin -1440 2>/dev/null)" ]; then
    exit 0
fi
touch "$STAMP" 2>/dev/null

command -v cargo >/dev/null 2>&1 || exit 0
cargo audit --version >/dev/null 2>&1 || exit 0
[ -f Cargo.lock ] || exit 0

# shellcheck source=lib/find-python.sh
. .claude/hooks/lib/find-python.sh 2>/dev/null || exit 0
find_python || exit 0

FINDINGS="$(timeout 60 cargo audit --json 2>/dev/null | "$PY" -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
items = [v["advisory"]["id"] + " (" + v["package"]["name"] + ")" for v in d.get("vulnerabilities", {}).get("list", [])]
for ws in (d.get("warnings") or {}).values():
    for w in ws:
        a = w.get("advisory") or {}
        items.append(a.get("id", "warning") + " (" + w["package"]["name"] + ")")
if items:
    print(str(len(items)) + " advisories: " + ", ".join(items[:8]))
' 2>/dev/null)"

if [ -n "$FINDINGS" ]; then
    echo "Daily cargo audit found $FINDINGS."
    echo "Suggest to the user: run /security-patch to patch them."
fi

exit 0
