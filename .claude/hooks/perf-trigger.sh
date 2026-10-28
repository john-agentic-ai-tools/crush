#!/usr/bin/env bash
# Stop hook: ask for /perf check after significant hot-path changes.
#
# Looks at everything that differs from the merge base with develop (commits
# plus working tree) in the performance crates' src/ directories. The change
# counts as significant when it touches a hot-path file or adds/removes at
# least PERF_TRIGGER_LINES lines (default 40). Then it blocks the stop once,
# with a reason telling Claude to run /perf check.
#
# Fires at most once per distinct change set: the diff hash is recorded in
# .claude/state/perf-last-check, and stop_hook_active is honoured, so it can't
# loop. Never fails the session: every non-trigger path exits 0 silently.

set -u
cd "$(git rev-parse --show-toplevel 2>/dev/null)" 2>/dev/null || exit 0

# Claude is already continuing because of a Stop hook: don't stack another block.
INPUT="$(cat 2>/dev/null || true)"
case "$INPUT" in *'"stop_hook_active":true'* | *'"stop_hook_active": true'*) exit 0 ;; esac

PATHS=(crush-core/src crush-parallel/src crush-gpu/src)
HOT_RE='^(crush-core/src/(compression|decompression|plugin/default)\.rs|crush-parallel/src/(engine|block|format)\.rs|crush-gpu/src/(engine|gdeflate|lz77|entropy|format)\.rs)$'
THRESHOLD="${PERF_TRIGGER_LINES:-40}"

BASE_REF=develop
git rev-parse --verify -q "$BASE_REF" >/dev/null || BASE_REF=origin/develop
BASE="$(git merge-base HEAD "$BASE_REF" 2>/dev/null)" || exit 0
[ -n "$BASE" ] || exit 0

# Tracked changes since the merge base (committed + uncommitted) ...
CHANGED="$(git diff --name-only "$BASE" -- "${PATHS[@]}" 2>/dev/null)"
LINES="$(git diff --numstat "$BASE" -- "${PATHS[@]}" 2>/dev/null | awk '{s += $1 + $2} END {print s + 0}')"
# ... plus new files not yet added.
UNTRACKED="$(git ls-files --others --exclude-standard -- "${PATHS[@]}" 2>/dev/null)"
if [ -n "$UNTRACKED" ]; then
    CHANGED="$(printf '%s\n%s' "$CHANGED" "$UNTRACKED")"
    LINES=$((LINES + $(cat $UNTRACKED 2>/dev/null | wc -l)))
fi
CHANGED="$(printf '%s\n' "$CHANGED" | grep -E '\.rs$' | sort -u)"
[ -n "$CHANGED" ] || exit 0

HOT="$(printf '%s\n' "$CHANGED" | grep -E "$HOT_RE" | tr '\n' ' ')"
if [ -z "$HOT" ] && [ "$LINES" -lt "$THRESHOLD" ]; then
    exit 0
fi

STATE_DIR=".claude/state"
STAMP="$STATE_DIR/perf-last-check"
mkdir -p "$STATE_DIR" 2>/dev/null
HASH="$( { git diff "$BASE" -- "${PATHS[@]}"; [ -n "$UNTRACKED" ] && cat $UNTRACKED; } 2>/dev/null | git hash-object --stdin)"
[ "$(cat "$STAMP" 2>/dev/null)" = "$HASH" ] && exit 0
echo "$HASH" > "$STAMP"

CRATES="$(printf '%s\n' "$CHANGED" | cut -d/ -f1 | sort -u | tr '\n' ',' | sed 's/,$//')"
WHY="${LINES} changed lines in ${CRATES}"
[ -n "$HOT" ] && WHY="${WHY}; hot-path files: ${HOT% }"
printf '{"decision":"block","reason":"Significant performance-crate changes since develop (%s). Run /perf check before finishing and report the result. This reminder fires once per change set."}\n' "$WHY"
exit 0
