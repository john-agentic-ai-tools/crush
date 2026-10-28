#!/usr/bin/env bash
# Rust Quality Gate
#
# Single source of truth for Crush's local quality gate. Used two ways:
#
#   bash .claude/hooks/rust-quality-gate.sh          Stop hook (fast path)
#   bash .claude/hooks/rust-quality-gate.sh --full   quality-gate subagent
#
# Default mode exits 0 immediately when no Rust sources or manifests changed,
# otherwise runs the same fmt/clippy/test commands as CI. --full always runs
# and adds docs, cargo audit (incl. fuzz lockfiles), and cargo deny.
#
# Exit codes: 0 = pass, 2 = fail. Exit 2 is what makes a Claude Code Stop hook
# block and feed stderr back to Claude; exit 1 would only warn the user.
#
# Requirements: bash (included with Git for Windows on Windows platforms)

set -uo pipefail

FULL=0
[ "${1:-}" = "--full" ] && FULL=1

# Find cargo executable. Support Git Bash (/c/...) and WSL (/mnt/c/...) paths.
CARGO=""
for CARGO_PATH in \
    "$(command -v cargo 2>/dev/null)" \
    "$HOME/.cargo/bin/cargo" \
    "$HOME/.cargo/bin/cargo.exe" \
    "/mnt/c/Users/Admin/.cargo/bin/cargo.exe" \
    "/c/Users/Admin/.cargo/bin/cargo.exe"; do
    if [ -n "$CARGO_PATH" ] && [ -x "$CARGO_PATH" ]; then
        CARGO="$CARGO_PATH"
        break
    fi
done

if [ -z "$CARGO" ]; then
    echo "[gate] ERROR: cargo not found. Install Rust via rustup: https://rustup.rs" >&2
    exit 2
fi

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)" || exit 2

if [ "$FULL" -eq 0 ]; then
    # Tracked modifications and untracked files alike; renames show the new path.
    if ! git status --porcelain 2>/dev/null \
        | grep -qE '(\.rs|Cargo\.toml|rust-toolchain\.toml)$'; then
        exit 0
    fi
fi

FAILED=()

# Run a check; on failure keep the tail of its output for the report.
run_check() {
    local name="$1"; shift
    local out
    if ! out="$("$@" 2>&1)"; then
        FAILED+=("$name")
        {
            echo "[gate] $name FAILED: $*"
            printf '%s\n' "$out" | grep -E '^(error|warning)|^Diff in|-->|panicked|FAILED|^test .* FAILED' | head -40
        } >&2
    fi
}

run_check fmt    "$CARGO" fmt --all -- --check
run_check clippy "$CARGO" clippy --all-targets -- -D warnings
run_check test   "$CARGO" test --workspace --quiet

if [ "$FULL" -eq 1 ]; then
    run_check doc env RUSTDOCFLAGS="-D warnings" "$CARGO" doc --no-deps --workspace
    run_check audit "$CARGO" audit --deny warnings
    for lock in */fuzz/Cargo.lock; do
        [ -f "$lock" ] && run_check "audit:$lock" "$CARGO" audit --deny warnings --file "$lock"
    done
    if "$CARGO" deny --version >/dev/null 2>&1; then
        run_check deny "$CARGO" deny check
    else
        echo "[gate] deny: skipped (cargo-deny not installed)" >&2
    fi
fi

if [ "${#FAILED[@]}" -gt 0 ]; then
    echo "[gate] Quality gate FAILED: ${FAILED[*]}. Fix these before finishing." >&2
    exit 2
fi

echo "[gate] Quality gate passed." >&2
exit 0
