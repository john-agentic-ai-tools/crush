#!/usr/bin/env bash
# Dependency / security scan for the security-patch skill.
#
# Prints a compact summary so the raw tool output (hundreds of lines) never
# reaches the agent's context. Always exits 0; the summary is the result.
#
# Usage: bash .claude/skills/security-patch/scan.sh   (from the repo root)

set -uo pipefail

PY="$(command -v python3 || command -v python)"
if [ -z "$PY" ]; then
    echo "ERROR: python is required to parse cargo audit JSON" >&2
    exit 0
fi

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT" || exit 0

# Mirror .github/workflows/security-audit.yml: the workspace lockfile plus each
# fuzz crate's own lockfile (fuzz crates are excluded from the workspace).
FUZZ_DIRS=()
for d in */fuzz; do
    [ -f "$d/Cargo.toml" ] && FUZZ_DIRS+=("$d")
done

summarize_audit() {
    # $1 = label; stdin = cargo audit --json
    "$PY" -c '
import json, sys
label = sys.argv[1]
try:
    d = json.load(sys.stdin)
except Exception as e:
    print(f"  {label}: could not parse cargo audit output ({e})")
    sys.exit(0)
vulns = d.get("vulnerabilities", {}).get("list", [])
warns = [(k, w) for k, ws in (d.get("warnings") or {}).items() for w in ws]
print(f"  {label}: {len(vulns)} vulnerabilities, {len(warns)} warnings")
for v in vulns:
    a, p = v["advisory"], v["package"]
    patched = ", ".join(v.get("versions", {}).get("patched", [])) or "none"
    print("    VULN", a["id"], p["name"], p["version"] + ":", a["title"], "(patched: " + patched + ")")
for kind, w in warns:
    a = w.get("advisory") or {}
    p = w["package"]
    print("    WARN", kind, a.get("id", "-"), p["name"], p["version"] + ":", a.get("title", ""))
' "$1"
}

echo "== Advisories (cargo audit) =="
cargo audit fetch -q >/dev/null 2>&1
if [ -f Cargo.lock ] || cargo generate-lockfile -q >/dev/null 2>&1; then
    cargo audit --json 2>/dev/null | summarize_audit "workspace"
fi
for d in "${FUZZ_DIRS[@]}"; do
    if [ -f "$d/Cargo.lock" ]; then
        cargo audit --json --file "$d/Cargo.lock" 2>/dev/null | summarize_audit "$d"
    else
        echo "  $d: no Cargo.lock (skipped, same as CI)"
    fi
done

echo
echo "== cargo deny check =="
if command -v cargo-deny >/dev/null 2>&1; then
    cargo deny check 2>&1 \
        | grep -E "^error\[|(advisories|bans|licenses|sources) (ok|FAILED)" \
        | sort | uniq -c | sed 's/^/  /'
else
    echo "  cargo-deny not installed (CI runs it; install with: cargo install cargo-deny)"
fi

echo
echo "== Crate updates (cargo update --dry-run) =="
scan_updates() {
    # $1 = label, remaining args passed to cargo
    local label="$1"; shift
    local out
    out="$(cargo update --dry-run --verbose "$@" 2>&1)"
    local compat
    compat="$(printf '%s\n' "$out" | grep -cE '^ +Updating [a-z0-9_-]+ v')"
    echo "  $label: $compat semver-compatible updates"
    printf '%s\n' "$out" | grep -E 'Unchanged .*available:' | sed 's/^ */    MAJOR /'
    printf '%s\n' "$out" | grep -iE 'requires rustc|rust-version' | sed 's/^ */    MSRV /'
}
scan_updates "workspace"
for d in "${FUZZ_DIRS[@]}"; do
    # A dry run still writes a fresh lockfile when none exists, so only scan
    # fuzz crates that already have one.
    [ -f "$d/Cargo.lock" ] && scan_updates "$d" --manifest-path "$d/Cargo.toml"
done

echo
echo "== Rust toolchain =="
PINNED="$(sed -nE 's/^channel *= *"([^"]+)".*/\1/p' rust-toolchain.toml 2>/dev/null)"
MSRV="$(sed -nE 's/^rust-version *= *"([^"]+)".*/\1/p' Cargo.toml 2>/dev/null)"
LATEST="$(curl -fsSL https://static.rust-lang.org/dist/channel-rust-stable.toml 2>/dev/null \
    | grep -A1 '^\[pkg.rustc\]' | sed -nE 's/^version = "([0-9.]+).*/\1/p')"
echo "  pinned: ${PINNED:-?}  rust-version: ${MSRV:-?}  latest stable: ${LATEST:-unknown}"
if [ -n "$PINNED" ] && [ -n "$LATEST" ] && [ "$PINNED" != "$LATEST" ]; then
    echo "    TOOLCHAIN behind: $PINNED -> $LATEST"
fi

echo
echo "== GitHub Actions =="
if command -v gh >/dev/null 2>&1; then
    # A disabled workflow (e.g. disabled_inactivity) silently stops the daily
    # advisory audit, so flag anything that isn't active.
    gh workflow list --all --json name,state \
        --jq '.[] | select(.state != "active") | "    WORKFLOW \(.name) is \(.state)"' 2>/dev/null
    grep -rhoE 'uses: *[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+@[^ ]+' .github/workflows 2>/dev/null \
        | sed -E 's/uses: *//' | sort -u \
        | while IFS='@' read -r repo ref; do
            # Floating refs (stable, nightly, master, cargo-audit, ...) track
            # upstream on their own; only versioned tags can go stale.
            [[ "$ref" =~ ^v[0-9] ]] || continue
            latest="$(gh api "repos/$repo/releases/latest" --jq .tag_name 2>/dev/null)"
            [ -z "$latest" ] && continue
            cur_major="${ref%%.*}"; new_major="${latest%%.*}"
            if [ "$ref" = "$latest" ] || [ "$ref" = "$new_major" ]; then
                continue
            fi
            if [ "$cur_major" = "$new_major" ] && [[ "$ref" != *.* ]]; then
                continue  # e.g. @v2 already floats over v2.x
            fi
            echo "    ACTION $repo@$ref -> $latest"
        done
else
    echo "  gh not installed; skipped"
fi

exit 0
