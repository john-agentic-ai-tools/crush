#!/usr/bin/env bash
# Rust Quality Gate Hook
#
# This hook enforces code quality standards before stopping work.
# Requirements: bash (included with Git for Windows on Windows platforms)
#
# If this script fails to run on Windows, ensure Git for Windows is installed
# and that Git Bash is in your PATH.

set -eo pipefail

# Find cargo executable
CARGO=""

# Try multiple common cargo installation locations
# Support both Git Bash (/c/...) and WSL (/mnt/c/...) path formats
CARGO_LOCATIONS=(
    "$HOME/.cargo/bin/cargo"
    "/mnt/c/Users/Admin/.cargo/bin/cargo.exe"
    "/c/Users/Admin/.cargo/bin/cargo"
    "/c/Users/Admin/.cargo/bin/cargo.exe"
)

for CARGO_PATH in "${CARGO_LOCATIONS[@]}"; do
    echo "[hook] Checking: $CARGO_PATH" >&2
    if [ -f "$CARGO_PATH" ]; then
        echo "[hook] Found file: $CARGO_PATH" >&2
        if [ -x "$CARGO_PATH" ]; then
            echo "[hook] File is executable" >&2
            CARGO="$CARGO_PATH"
            break
        else
            echo "[hook] File exists but not executable" >&2
        fi
    else
        echo "[hook] File not found" >&2
    fi
done

if [ -z "$CARGO" ]; then
    echo "[hook] ERROR: cargo not found. Please install Rust via rustup: https://rustup.rs" >&2
    echo "[hook] HOME=${HOME:-unset}" >&2
    exit 1
else
    echo "format success" >&2
fi

echo "[hook] Using cargo: $CARGO" >&2

echo "[hook] Running Rust quality gate..." >&2

# 1. Ensure code is formatted
if ! "$CARGO" fmt -- --check 2>&1; then
  echo "[hook] Code not formatted. Run: cargo fmt" >&2
  exit 1
fi

# 2. Clippy with warnings as errors
if ! "$CARGO" clippy -- -D warnings 2>&1; then
  echo "[hook] Clippy warnings found." >&2
  exit 1
fi

# 3. Tests
if ! "$CARGO" test --quiet 2>&1; then
  echo "[hook] Tests failed." >&2
  exit 1
fi

echo "[hook] Rust quality gate passed." >&2
exit 0
