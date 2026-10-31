#!/usr/bin/env python3
"""Deterministic benchmark driver for the /perf skill.

Wraps `cargo bench` (criterion) so callers see compact tables instead of
criterion's console output. Every run is saved as a named criterion baseline;
comparisons are always between two named baselines, computed here from the raw
samples, never from criterion's `change/` directory (which only reflects the
most recent comparison criterion happened to make).

Subcommands
  fingerprint [--json]                  machine identity used for baseline files
  run --suite S --name N [--quick]      cargo bench, saved as criterion baseline N
  worktree-run --ref R --suite S --name N [--quick]
                                        same, but builds commit R in a temporary
                                        git worktree with its own target dir
  ab --ref R --suite S --name N [--quick] [--filter RE] [--changed] [--dry-run]
                                        the A/B to decide on: R (worktree) vs the
                                        working tree, run A B B A, then compared.
                                        --filter runs matching benchmarks only;
                                        --changed runs what `targets` picks
  targets --ref R [--json]              which benchmarks cover the files changed
                                        since R (the same choice as ab --changed)
  clean                                 remove the base checkouts and target dirs
                                        ab and the profilers keep
  doctor                                check the profiling tools; see
                                        CONTRIBUTING.md "Performance Tooling"
  profile --bench-id ID [--side tree|base --ref R] [--env wsl|here] [--seconds N]
                                        sampling profile of one benchmark: top
                                        functions and hot source lines
  profile-diff --bench-id ID --ref R [--env wsl|here]
                                        the same for R and the working tree, as
                                        changes in each function's share.
                                        --env defaults to wsl, which needs no
                                        approval; on Windows, here raises a UAC
                                        prompt per side
  callgrind --bench-id ID --ref R       instruction counts per function on both
                                        sides, in WSL/Linux; no timing noise
  compare BASE NEW [--threshold 5]      A/B table with bootstrap 95% CI; each side
                                        may be a comma list of names (pooled).
                                        Benchmarks < 100 µs are advisory only.
  noise --suite S --name N [--quick]    A/A `ab` (HEAD in a worktree vs HEAD here);
                                        records per-benchmark noise floors that
                                        compare then applies
  ceiling --name N                      thread-scaling numbers for feasibility
  save-summary --name N [--commit REF]  write perf/baselines/<fingerprint>.json
  changeset [--json | --hook]           fingerprint of the perf-crate changes since
                                        develop; --hook tells perf-trigger.sh
                                        whether to stay silent
  session begin|end|status [--mode M --name N]
                                        mark a /perf goal as owning this branch's
                                        changes, so the Stop hook stays silent

run, noise and ab record the change set they measured in
.claude/state/perf-measured.json (while running and when done).

Suites: quick-core, quick-parallel, quick-gpu, cpu-compress, cpu-decompress,
full. `--crates` on run picks quick suites for the named crates.

Exit codes: 0 ok, 1 usage/IO error, 2 cargo bench failed. `compare` prints
`VERDICT: REGRESSION|IMPROVED|NEUTRAL` as its last line and exits 0.
"""

from __future__ import annotations

import argparse
import bisect
import datetime as dt
import gzip
import io
import json
import os
import platform
import random
import re
import shutil
import subprocess
import sys
import tarfile
import time
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=False
    ).stdout.strip()
    or "."
).resolve()
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
CRITERION = Path(os.environ.get("CRITERION_HOME", TARGET / "criterion")).resolve()
PERF_DIR = TARGET / "perf"
NOISE_FILE = PERF_DIR / "noise.json"
QUICK_ARGS = ["--warm-up-time", "1", "--measurement-time", "3"]
# Benchmarks faster than this are too noisy to gate on: on the reference
# machine (i9-12900K, hybrid P/E cores) they moved 5-15% between runs of the
# same binary, while ms-scale benchmarks stayed within ~3%. Their changes are
# reported but never produce a REGRESSION verdict alone.
MICRO_NS = 100_000

# Performance crates and hot-path files: what .claude/hooks/perf-trigger.sh
# watches. The hook delegates to `changeset --hook`, so this is the only copy.
PERF_SRC = ["crush-core/src", "crush-parallel/src", "crush-gpu/src", "crush-lz/src"]
HOT_RE = re.compile(
    r"^(crush-core/src/(compression|decompression|plugin/default)\.rs"
    r"|crush-parallel/src/(engine|block|format)\.rs"
    r"|crush-gpu/src/(engine|gdeflate|lz77|entropy|format)\.rs"
    r"|crush-lz/src/(compress|decompress|estimate)\.rs)$"
)
STATE_DIR = ROOT / ".claude" / "state"
LEDGER_FILE = STATE_DIR / "perf-measured.json"
SESSION_FILE = STATE_DIR / "perf-session.json"
LEDGER_KEEP = 50
# A `running` entry older than this is treated as dead (crashed run, or a
# pid we can't check).
RUNNING_MAX_AGE = dt.timedelta(hours=3)
SESSION_HOURS = 12

# (package, bench target, criterion filter regex or None for all)
SUITES: dict[str, list[tuple[str, str, str | None]]] = {
    "quick-core": [("crush-core", "compression", None)],
    "quick-parallel": [
        ("crush-parallel", "throughput", r"^(compress_throughput/threads=(1|default)/block=1024KB|decompress_throughput/threads/(1|default))$"),
    ],
    "quick-gpu": [("crush-gpu", "throughput", r"/cpu/")],
    "cpu-compress": [
        ("crush-parallel", "throughput", r"^compress_throughput/"),
        ("crush-core", "compression", r"^compress_(medium|large)/"),
    ],
    "cpu-compress-random": [
        ("crush-parallel", "throughput", r"^(compress_incompressible/|compress_with_options_incompressible/|compress_mixed/|compress_throughput/threads=(1|default)/block=1024KB$)"),
    ],
    "cpu-decompress": [
        ("crush-parallel", "throughput", r"^decompress_throughput/"),
        ("crush-core", "compression", r"^decompress_"),
    ],
    "fast-lz": [("crush-lz", "codec", None)],
    "full": [
        ("crush-core", "", None),
        ("crush-parallel", "", None),
        ("crush-gpu", "", None),
        ("crush-lz", "", None),
    ],
}
CRATE_SUITE = {
    "crush-core": "quick-core",
    "crush-parallel": "quick-parallel",
    "crush-gpu": "quick-gpu",
    "crush-lz": "fast-lz",
}
# Changed file -> the benchmarks that exercise it: (path regex, package, bench
# target, criterion filter). First match wins; a file with no row falls back
# to its crate's quick suite, so a change is never left unmeasured.
TARGET_MAP = [
    (r"crush-core/src/decompression\.rs$", "crush-core", "compression", r"^decompress_"),
    (r"crush-core/src/compression\.rs$", "crush-core", "compression", r"^compress_"),
    (r"crush-core/src/plugin/selector\.rs$", "crush-core", "plugin_selection", None),
    (r"crush-core/src/plugin/registry\.rs$", "crush-core", "plugin_discovery", None),
]
# Criterion doesn't cover the CLI pipeline.
CLI_HINT = "python3 perf/compare.py --cores 1 --tools crush-fast-lz,lz4-1 --data real --abba --reps 10"
TARGET_CRATES = [*CRATE_SUITE, "crush-cli"]
# Benchmark id -> a callgrind example doing the same work once: (id regex,
# package, op, size). Criterion's timing loop is too slow under valgrind.
CALLGRIND_CASES = [
    (r"^compress_small/", "crush-core", "compress", 1024),
    (r"^compress_medium/", "crush-core", "compress", 1024 * 1024),
    (r"^compress_large/", "crush-core", "compress", 10 * 1024 * 1024),
    (r"^decompress_small/", "crush-core", "decompress", 1024),
    (r"^decompress_medium/", "crush-core", "decompress", 1024 * 1024),
    (r"^fast_lz/compress/", "crush-lz", "compress", 4 * 1024 * 1024),
    (r"^fast_lz/decompress/", "crush-lz", "decompress", 4 * 1024 * 1024),
]
PROFILE_ROWS = 15
# Blocked threads, not work: a sample ending here says nothing about where the
# benchmark's time goes, and thread-pool idlers would skew every share.
IDLE_RE = re.compile(r"^(Nt|Zw)(Wait|Delay|Remove)|^futex|^__?(lll_lock_wait|pthread_cond_wait)|^epoll_wait")
# Criterion's default warm-up + measurement, and the same for --quick.
BENCH_SECONDS = 8
QUICK_BENCH_SECONDS = 4


def die(msg: str, code: int = 1) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(code)


def sh(cmd: list[str]) -> str:
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=False, timeout=30).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return ""


# --- fingerprint -------------------------------------------------------------

def fingerprint() -> dict:
    system = platform.system()
    cpu, physical, power = "", 0, ""
    if system == "Windows":
        ps = "Get-CimInstance Win32_Processor | Select-Object -First 1 Name,NumberOfCores | ConvertTo-Json"
        try:
            info = json.loads(sh(["powershell", "-NoProfile", "-Command", ps]) or "{}")
            cpu, physical = info.get("Name", ""), int(info.get("NumberOfCores") or 0)
        except (ValueError, TypeError):
            pass
        m = re.search(r"\(([^)]+)\)\s*$", sh(["powercfg", "/getactivescheme"]))
        power = m.group(1) if m else ""
    elif system == "Linux":
        try:
            text = Path("/proc/cpuinfo").read_text()
            m = re.search(r"^model name\s*:\s*(.+)$", text, re.M)
            cpu = m.group(1) if m else ""
            cores = set(re.findall(r"^physical id\s*:\s*(\d+)\s*$.*?^core id\s*:\s*(\d+)", text, re.M | re.S))
            physical = len(cores)
        except OSError:
            pass
        gov = Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        power = gov.read_text().strip() if gov.exists() else ""
    elif system == "Darwin":
        cpu = sh(["sysctl", "-n", "machdep.cpu.brand_string"])
        physical = int(sh(["sysctl", "-n", "hw.physicalcpu"]) or 0)
    logical = os.cpu_count() or 0
    cpu = re.sub(r"\s+", " ", cpu or platform.processor() or "unknown-cpu").strip()
    cpu_slug = re.sub(r"[^a-z0-9]+", "-", re.sub(r"\((r|tm)\)|cpu|processor|\d+-core", "", cpu.lower())).strip("-")
    slug = f"{system.lower()}-{cpu_slug}-{physical or '?'}c{logical}t"
    return {
        "slug": slug,
        "os": f"{system} {platform.release()}",
        "cpu": cpu,
        "physical_cores": physical,
        "logical_cores": logical,
        "power_profile": power,
    }


def cmd_fingerprint(args: argparse.Namespace) -> None:
    fp = fingerprint()
    if args.json:
        print(json.dumps(fp, indent=2))
    else:
        print(fp["slug"])
        for k in ("cpu", "physical_cores", "logical_cores", "os", "power_profile"):
            print(f"  {k}: {fp[k]}")


# --- run ---------------------------------------------------------------------

def suite_entries(suite: str | None, crates: str | None) -> list[tuple[str, str, str | None]]:
    if crates:
        out = []
        for c in (x.strip() for x in crates.split(",") if x.strip()):
            if c not in CRATE_SUITE:
                die(f"no quick suite for crate {c!r}; choose from {', '.join(CRATE_SUITE)}")
            out += SUITES[CRATE_SUITE[c]]
        return out
    if suite not in SUITES:
        die(f"unknown suite {suite!r}; choose from {', '.join(SUITES)}")
    return SUITES[suite]


def with_filter(entries, flt: str):
    """The same bench targets, each running only what `flt` matches."""
    return sorted({(pkg, bench, flt) for pkg, bench, _ in entries})


def select_entries(args: argparse.Namespace):
    if getattr(args, "changed", False):
        entries, notes, _ = target_entries(changed_files(args.ref))
        for note in notes:
            print(f"note: {note}")
        return entries
    flt = getattr(args, "filter", None)
    entries = suite_entries(args.suite or (None if args.crates else "full"), args.crates)
    return with_filter(entries, flt) if flt else entries


def present_at(checkout: Path, entries):
    """Drop crates that don't exist in `checkout`: a new crate has no base side."""
    kept = [e for e in entries if (checkout / e[0] / "Cargo.toml").exists()]
    for pkg in sorted({e[0] for e in entries} - {e[0] for e in kept}):
        print(f"note: {pkg} does not exist at the base ref, skipped")
    return kept


def build_exes(cwd: Path, entries, env: dict) -> dict[tuple[str, str], list[str]]:
    """Compile once and return (package, bench) -> executables, so the ABBA
    passes don't go through cargo again."""
    exes: dict[tuple[str, str], list[str]] = {}
    for pkg, bench in sorted({(e[0], e[1]) for e in entries}):
        # Select bench targets explicitly: with no selector, or with --benches
        # (which includes the lib, bench = true by default), cargo also builds
        # the lib's libtest harness, which rejects criterion flags.
        cmd = ["cargo", "bench", "-p", pkg, "--bench", bench or "*", "--no-run", "--message-format=json"]
        r = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
        if r.returncode != 0:
            print("\n".join(r.stderr.splitlines()[-30:]), file=sys.stderr)
            die(f"cargo bench --no-run failed for {pkg} (exit {r.returncode})", 2)
        found = []
        for line in r.stdout.splitlines():
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if msg.get("reason") == "compiler-artifact" and msg.get("executable") and "bench" in msg["target"]["kind"]:
                found.append(msg["executable"])
        if not found:
            die(f"no bench executable built for {pkg} {bench or '*'}", 2)
        exes[(pkg, bench)] = found
    return exes


def run_benches(cwd: Path, entries, name: str, quick: bool, env: dict, exes=None) -> list[str]:
    """Run each entry; return the criterion ids saved under baseline `name`."""
    PERF_DIR.mkdir(parents=True, exist_ok=True)
    log = PERF_DIR / f"{name}.log"
    exes = exes or build_exes(cwd, entries, env)
    started = dt.datetime.now().timestamp()
    with log.open("w", encoding="utf-8") as fh:
        for pkg, bench, flt in entries:
            for exe in exes[(pkg, bench)]:
                cmd = [exe, "--bench"]
                if flt:
                    cmd.append(flt)
                cmd += ["--save-baseline", name, "--noplot"]
                if quick:
                    cmd += QUICK_ARGS
                fh.write(f"$ {' '.join(cmd)}\n")
                fh.flush()
                # cargo bench runs targets from the package root.
                rc = subprocess.run(
                    cmd, cwd=cwd / pkg, env=dict(env, CARGO_MANIFEST_DIR=str(cwd / pkg)),
                    stdout=fh, stderr=subprocess.STDOUT, check=False,
                ).returncode
                if rc != 0:
                    fh.close()
                    tail = log.read_text(encoding="utf-8", errors="replace").splitlines()[-30:]
                    print("\n".join(tail), file=sys.stderr)
                    die(f"benchmark failed for {pkg} (exit {rc}); full log: {log}", 2)
    ids = sorted(
        load_bench(p.parent)["full_id"]
        for p in CRITERION.glob(f"**/{name}/benchmark.json")
        if p.stat().st_mtime >= started - 1
    )
    manifest = {"name": name, "ids": ids, "date": dt.datetime.now().isoformat(timespec="seconds")}
    (PERF_DIR / f"{name}.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(f"saved criterion baseline '{name}': {len(ids)} benchmarks (log: {log})")
    return ids


def cmd_run(args: argparse.Namespace) -> None:
    env = dict(os.environ, CRITERION_HOME=str(CRITERION))
    with measuring("run", args.name):
        run_benches(ROOT, select_entries(args), args.name, args.quick, env)


def cmd_worktree_run(args: argparse.Namespace) -> None:
    with worktree(args.ref) as (wt, env):
        run_benches(wt, suite_entries(args.suite, args.crates), args.name, args.quick, env)


def worktree_usable(wt: Path) -> bool:
    if not (wt / ".git").exists():
        return False
    r = subprocess.run(["git", "status", "--porcelain"], cwd=wt, capture_output=True, text=True, check=False)
    return r.returncode == 0 and not r.stdout.strip()


@contextmanager
def worktree(ref: str, keep: bool = False):
    """Check out `ref` in a worktree with its own target dir.

    The base side must not share our target dir: both checkouts build the same
    package ids, so they'd write the same artifact paths, and while the
    worktree exists cargo's fingerprints point at its unchanged sources and
    consider the base binary fresh. The working-tree side would then silently
    benchmark the base build. For the same reason each commit gets its own
    target/perf/target-base-<sha>: two kept checkouts of different commits
    share package ids too. It persists between runs of that commit.

    With `keep`, the checkout stays for the next run of the same commit: a new
    checkout has new mtimes, which makes cargo rebuild the workspace crates.
    `clean` removes kept worktrees.
    """
    sha = sh(["git", "rev-parse", "--verify", f"{ref}^{{commit}}"])
    if not sha:
        die(f"cannot resolve ref {ref!r}")
    wt = PERF_DIR / f"wt-{sha[:7]}"
    if not (keep and worktree_usable(wt)):
        if wt.exists():
            subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=ROOT, check=False)
        PERF_DIR.mkdir(parents=True, exist_ok=True)
        r = subprocess.run(["git", "worktree", "add", "--detach", str(wt), sha], cwd=ROOT, capture_output=True, text=True, check=False)
        if r.returncode != 0:
            die(f"git worktree add failed: {r.stderr.strip()}")
    try:
        # Cargo.lock is gitignored; reuse ours so both sides build the same dependency versions.
        lock = ROOT / "Cargo.lock"
        if lock.exists() and (not (wt / "Cargo.lock").exists() or lock.read_bytes() != (wt / "Cargo.lock").read_bytes()):
            shutil.copy2(lock, wt / "Cargo.lock")
        yield wt, dict(os.environ, CARGO_TARGET_DIR=str(PERF_DIR / f"target-base-{sha[:7]}"), CRITERION_HOME=str(CRITERION))
    finally:
        if not keep:
            subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=ROOT, check=False)


def cmd_doctor(_args: argparse.Namespace) -> None:
    """Report each profiling tool as ok or missing, with the fix."""
    problems = 0

    def report(ok: bool, what: str, fix: str) -> None:
        nonlocal problems
        problems += 0 if ok else 1
        print(f"{'ok     ' if ok else 'MISSING'} {what}" + ("" if ok else f": {fix}"))

    def has(tool: str, prefix: list[str]) -> bool:
        r = sh([*prefix, "bash", "-lc", f"command -v {tool}"] if prefix else ["which", tool])
        return bool(r) or (not prefix and shutil.which(tool) is not None)

    local = platform.system().lower()
    linux_prefix: list[str] = []
    report(has("samply", []), f"samply ({local})", "cargo install --locked samply")
    if local == "windows":
        linux_prefix = ["wsl", "-d", "Ubuntu", "--"]
        if not sh([*linux_prefix, "uname"]):
            print("MISSING WSL Ubuntu: callgrind and the lz4 comparison need it")
            sys.exit(1)
        report(has("samply", linux_prefix), "samply (wsl)", "cargo install --locked samply")
    report(has("valgrind", linux_prefix), "valgrind", "sudo apt install valgrind")
    report(has("lz4", linux_prefix), "lz4", "sudo apt install lz4")
    level = sh([*linux_prefix, "cat", "/proc/sys/kernel/perf_event_paranoid"])
    report(level.lstrip("-").isdigit() and int(level) <= 1, f"perf_event_paranoid = {level or '?'}",
           "echo 1 | sudo tee /proc/sys/kernel/perf_event_paranoid")
    if not problems:
        # Windows would raise a UAC prompt, so only the Linux side is exercised.
        probe = "samply record --save-only -o /tmp/crush-doctor.json.gz -- true && rm -f /tmp/crush-doctor.json.gz && echo rc=0"
        out = subprocess.run([*linux_prefix, "bash", "-lc", probe], capture_output=True, text=True, check=False).stdout
        report("rc=0" in out, "samply can record", "see CONTRIBUTING.md, Performance Tooling")
    sys.exit(1 if problems else 0)


def cmd_clean(_args: argparse.Namespace) -> None:
    removed = 0
    for wt in sorted(PERF_DIR.glob("wt-*")):
        subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=ROOT, check=False)
        removed += 1
    subprocess.run(["git", "worktree", "prune"], cwd=ROOT, check=False)
    for pattern in ("target-base*", "target-prof-*", "src-*"):
        for path in PERF_DIR.glob(pattern):
            shutil.rmtree(path, ignore_errors=True)
            removed += 1
    print(f"removed {removed} base checkout(s) and target dir(s)")


def ab_run(ref: str, entries, name: str, quick: bool) -> tuple[str, str]:
    """Measure `ref` (A, in a worktree) against the working tree (B) as A B B A.

    Running one side after the other lets thermal and turbo drift masquerade
    as a regression of whichever side runs second. ABBA ordering cancels
    linear drift; compare() pools each side's samples. Returns the two
    comma-joined name lists to pass to compare.
    """
    env = dict(os.environ, CRITERION_HOME=str(CRITERION))
    a1, b1, b2, a2 = (f"{name}-{x}" for x in ("A1", "B1", "B2", "A2"))
    with worktree(ref, keep=True) as (wt, wt_env):
        a_entries = present_at(wt, entries)
        if not a_entries:
            die("none of the selected crates exist at the base ref")
        a_exes = build_exes(wt, a_entries, wt_env)
        b_exes = build_exes(ROOT, entries, env)
        if not run_benches(wt, a_entries, a1, quick, wt_env, a_exes):
            die("no benchmark matched the selection")
        run_benches(ROOT, entries, b1, quick, env, b_exes)
        run_benches(ROOT, entries, b2, quick, env, b_exes)
        run_benches(wt, a_entries, a2, quick, wt_env, a_exes)
    return f"{a1},{a2}", f"{b1},{b2}"


def changed_files(ref: str) -> list[str]:
    """Rust sources and benches in the benchmarked crates that differ from `ref`."""
    names = git_out(["diff", "--name-only", ref, "--", *TARGET_CRATES]).decode().split()
    names += git_out(["ls-files", "--others", "--exclude-standard", "--", *TARGET_CRATES]).decode().split()
    return sorted({n for n in names if n.endswith(".rs") and re.match(r"[^/]+/(src|benches)/", n)})


def target_entries(files: list[str]):
    """Map changed files to entries; returns (entries, notes, cli_changed)."""
    picks: dict[tuple[str, str], set] = {}
    notes, cli = [], False
    for f in files:
        crate = f.split("/")[0]
        rows = [(pkg, bench, flt) for rx, pkg, bench, flt in TARGET_MAP if re.match(rx, f)][:1]
        if crate == "crush-cli":
            cli = True
        elif not rows and crate in CRATE_SUITE:
            rows = SUITES[CRATE_SUITE[crate]]
        for pkg, bench, flt in rows:
            picks.setdefault((pkg, bench), set()).add(flt)
    if cli:
        notes.append(f"crush-cli changed and criterion doesn't cover it; also run: {CLI_HINT}")
    entries = []
    for (pkg, bench), flts in sorted(picks.items()):
        entries.append((pkg, bench, None if None in flts else "|".join(sorted(flts))))
    return entries, notes, cli


def cmd_targets(args: argparse.Namespace) -> None:
    files = changed_files(args.ref)
    entries, notes, cli = target_entries(files)
    if args.json:
        print(json.dumps({"files": files, "entries": entries, "notes": notes, "cli": cli}, indent=2))
        return
    print(f"{len(files)} changed file(s) since {args.ref}")
    for pkg, bench, flt in entries:
        print(f"  {pkg} --bench {bench}: {flt or 'all'}")
    for note in notes:
        print(f"note: {note}")
    if not entries and not cli:
        print("no benchmark covers these changes")


def list_ids(exe: str, cwd: Path, env: dict, flt: str | None) -> list[str]:
    cmd = [exe, "--bench", "--list"] + ([flt] if flt else [])
    out = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False).stdout
    return [line.rsplit(":", 1)[0] for line in out.splitlines() if line.endswith(": benchmark")]


def dry_run(entries, quick: bool) -> None:
    env = dict(os.environ, CRITERION_HOME=str(CRITERION))
    count = 0
    exes = build_exes(ROOT, entries, env)
    for pkg, bench, flt in entries:
        for exe in exes[(pkg, bench)]:
            ids = list_ids(exe, ROOT / pkg, env, flt)
            count += len(ids)
            print(f"{pkg} --bench {Path(exe).stem.rsplit('-', 1)[0]}: {len(ids)} benchmark(s)")
            for bid in ids:
                print(f"  {bid}")
    seconds = count * (QUICK_BENCH_SECONDS if quick else BENCH_SECONDS) * 4
    print(f"{count} benchmark(s), 4 passes: at least {seconds / 60:.1f} min of measurement, plus builds")


def cmd_ab(args: argparse.Namespace) -> None:
    entries = select_entries(args)
    if not entries:
        print("no benchmark covers these changes; nothing to measure")
        return
    if args.dry_run:
        dry_run(entries, args.quick)
        return
    started = time.monotonic()
    with measuring("ab", args.name):
        base, new = ab_run(args.ref, entries, args.name, args.quick)
    print(f"elapsed: {(time.monotonic() - started) / 60:.1f} min")
    print(f"A = {args.ref} ({base}), B = working tree ({new})")
    rows, overall = compare(base, new, args.threshold)
    print_table(rows, f"A {args.ref}", "B")
    print(f"VERDICT: {overall}")


# --- profiling ---------------------------------------------------------------

def to_wsl(path: Path | str) -> str:
    text = str(path).replace("\\", "/")
    m = re.match(r"([A-Za-z]):/(.*)", text)
    return f"/mnt/{m.group(1).lower()}/{m.group(2)}" if m else text


def delegate_to_wsl(argv: list[str]) -> None:
    """Run this bench.py inside WSL with its own target dir (the Windows one
    holds Windows binaries)."""
    quoted = " ".join("'" + a.replace("'", "'\\''") + "'" for a in argv)
    script = (f"cd '{to_wsl(ROOT)}' && CARGO_TARGET_DIR=\"$HOME/.cache/crush-perf\" "
              f"python3 .claude/skills/perf/bench.py {quoted}")
    sys.exit(subprocess.run(["wsl", "-d", "Ubuntu", "--", "bash", "-lc", script], check=False).returncode)


def resolve_sha(ref: str) -> str:
    sha = sh(["git", "rev-parse", "--verify", f"{ref}^{{commit}}"])
    if not sha:
        die(f"cannot resolve ref {ref!r}")
    return sha


def export_ref(sha: str) -> Path:
    """A plain copy of commit `sha`. Profiling doesn't need a registered
    worktree, and WSL git would write Linux paths into the Windows repo's."""
    out = PERF_DIR / f"src-{sha[:7]}"
    if not (out / "Cargo.toml").exists():
        out.mkdir(parents=True, exist_ok=True)
        with tarfile.open(fileobj=io.BytesIO(git_out(["archive", sha]))) as tar:
            # The filter argument appeared in Python 3.12 and in 3.8+ patch releases.
            tar.extractall(out, **({"filter": "data"} if hasattr(tarfile, "data_filter") else {}))
    if (ROOT / "Cargo.lock").exists():
        lock = (ROOT / "Cargo.lock").read_bytes()
        if not (out / "Cargo.lock").exists() or (out / "Cargo.lock").read_bytes() != lock:
            (out / "Cargo.lock").write_bytes(lock)
    return out


def symbol_env(side: str, sha: str | None) -> dict:
    """Debug info and no stripping, in a target dir per side and commit:
    checkouts of different commits build the same package ids (see `worktree`)."""
    tag = f"{side}-{sha[:7]}" if sha else side
    env = dict(os.environ, CARGO_TARGET_DIR=str(PERF_DIR / f"target-prof-{tag}"), CRITERION_HOME=str(CRITERION))
    for profile in ("BENCH", "RELEASE"):
        env[f"CARGO_PROFILE_{profile}_DEBUG"] = "true"
        env[f"CARGO_PROFILE_{profile}_STRIP"] = "false"
    return env


def locate_bench(checkout: Path, bench_id: str, target: str | None) -> tuple[str, str]:
    """(package, bench target) defining `bench_id`, from its group name."""
    if target:
        pkg, _, bench = target.partition(":")
        return pkg, bench
    group = bench_id.split("/")[0]
    for crate in CRATE_SUITE:
        for src in sorted((checkout / crate / "benches").glob("*.rs")):
            if f'"{group}' in src.read_text(encoding="utf-8", errors="replace"):
                return crate, src.stem
    die(f"can't find the bench defining {bench_id!r}; pass --target package:bench")
    return "", ""


class PdbSymbols:
    """Windows addresses to names and lines through dbghelp.dll.

    samply leaves our own executable as `fun_<rva>`: rustc records only the
    PDB's file name in it. dbghelp finds the PDB next to the executable.
    """

    def __init__(self, exe: str):
        import ctypes
        from ctypes import wintypes

        class SymbolInfo(ctypes.Structure):
            _fields_ = [
                ("SizeOfStruct", wintypes.ULONG), ("TypeIndex", wintypes.ULONG),
                ("Reserved", ctypes.c_ulonglong * 2), ("Index", wintypes.ULONG), ("Size", wintypes.ULONG),
                ("ModBase", ctypes.c_ulonglong), ("Flags", wintypes.ULONG), ("Value", ctypes.c_ulonglong),
                ("Address", ctypes.c_ulonglong), ("Register", wintypes.ULONG), ("Scope", wintypes.ULONG),
                ("Tag", wintypes.ULONG), ("NameLen", wintypes.ULONG), ("MaxNameLen", wintypes.ULONG),
                ("Name", ctypes.c_char * 1024),
            ]

        class LineInfo(ctypes.Structure):
            _fields_ = [
                ("SizeOfStruct", wintypes.DWORD), ("Key", ctypes.c_void_p), ("LineNumber", wintypes.DWORD),
                ("FileName", ctypes.c_char_p), ("Address", ctypes.c_ulonglong),
            ]

        self.ctypes, self.wintypes, self.SymbolInfo, self.LineInfo = ctypes, wintypes, SymbolInfo, LineInfo
        self.dll = ctypes.WinDLL("dbghelp")
        self.dll.SymInitialize.argtypes = [ctypes.c_void_p, ctypes.c_char_p, wintypes.BOOL]
        self.dll.SymLoadModuleEx.restype = ctypes.c_ulonglong
        self.dll.SymLoadModuleEx.argtypes = [
            ctypes.c_void_p, ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p,
            ctypes.c_ulonglong, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD,
        ]
        self.dll.SymFromAddr.argtypes = [
            ctypes.c_void_p, ctypes.c_ulonglong, ctypes.POINTER(ctypes.c_ulonglong), ctypes.POINTER(SymbolInfo),
        ]
        self.dll.SymGetLineFromAddr64.argtypes = [
            ctypes.c_void_p, ctypes.c_ulonglong, ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(LineInfo),
        ]
        self.dll.SymCleanup.argtypes = [ctypes.c_void_p]
        self.handle = ctypes.c_void_p(os.getpid())
        self.dll.SymSetOptions(0x12)  # SYMOPT_UNDNAME | SYMOPT_LOAD_LINES
        self.base = 0
        if self.dll.SymInitialize(self.handle, str(Path(exe).parent).encode(), False):
            self.base = self.dll.SymLoadModuleEx(self.handle, None, exe.encode(), None, 0x10000000, 0, None, 0)

    def lookup(self, rva: int) -> tuple[str, str | None] | None:
        if not self.base:
            return None
        ctypes = self.ctypes
        sym = self.SymbolInfo(SizeOfStruct=88, MaxNameLen=1000)
        offset = ctypes.c_ulonglong(0)
        if not self.dll.SymFromAddr(self.handle, self.base + rva, ctypes.byref(offset), ctypes.byref(sym)):
            return None
        line = self.LineInfo(SizeOfStruct=ctypes.sizeof(self.LineInfo))
        column = self.wintypes.DWORD(0)
        found = self.dll.SymGetLineFromAddr64(self.handle, self.base + rva, ctypes.byref(column), ctypes.byref(line))
        where = f"{line.FileName.decode(errors='replace')}:{line.LineNumber}" if found and line.FileName else None
        return sym.Name.decode(errors="replace"), where

    def close(self) -> None:
        self.dll.SymCleanup(self.handle)


def parse_profile(path: Path) -> dict:
    """Self and inclusive sample counts per function and hot lines, from a
    samply profile and the .syms.json next to it. Inlined frames of the leaf
    count as functions of their own: in a hot loop nearly everything is
    inlined."""
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        profile = json.load(fh)
    syms = json.loads(Path(str(path).replace(".json.gz", ".json.syms.json")).read_text(encoding="utf-8"))
    strings = syms["string_table"]

    def text(i) -> str:
        return strings[i] if isinstance(i, int) and 0 <= i < len(strings) else "?"

    def clean(name: str) -> str:
        return re.sub(r"::h[0-9a-f]{16}$", "", name)

    tables = {}
    for lib in syms["data"]:
        table = lib["symbol_table"]
        ordered = sorted(table, key=lambda e: e["rva"])
        tables[lib["debug_name"]] = (
            {a: table[i] for a, i in lib["known_addresses"]},
            ordered,
            [e["rva"] for e in ordered],
        )

    def resolve(lib_name: str | None, address: int) -> list[tuple[str, str | None]]:
        """(function, file:line) for one address, innermost inlined frame first."""
        known = tables.get(lib_name)
        if not known:
            return []
        entry = known[0].get(address)
        if entry is None:
            i = bisect.bisect_right(known[2], address) - 1
            if i < 0 or address >= known[2][i] + known[1][i].get("size", 0):
                return []
            entry = known[1][i]
        frames = [
            (clean(text(f["function"] if "function" in f else entry.get("symbol"))), f"{text(f.get('file'))}:{f.get('line')}")
            for f in entry.get("frames") or []
        ]
        return frames or [(clean(text(entry.get("symbol"))), None)]

    pdbs: dict[str, PdbSymbols | None] = {}

    def pdb_for(exe: str) -> PdbSymbols | None:
        if exe not in pdbs:
            # Only what we built: system images resolve to the nearest export,
            # which reads like an answer but isn't one.
            usable = platform.system() == "Windows" and exe.lower().startswith(str(PERF_DIR).lower())
            pdbs[exe] = PdbSymbols(exe) if usable else None
        return pdbs[exe]

    self_time: dict[str, float] = {}
    inclusive: dict[str, float] = {}
    lines: dict[str, float] = {}
    total, threads = 0.0, 0
    libs = profile["libs"]
    for t in profile["threads"]:
        count = t["samples"]["length"]
        if t.get("name") == "samply" or not count:
            continue
        threads += 1
        frames_at: dict[int, list] = {}

        def frames_of(frame: int) -> list:
            if frame not in frames_at:
                func = t["frameTable"]["func"][frame]
                res = t["funcTable"]["resource"][func]
                lib = t["resourceTable"]["lib"][res] if res is not None and res >= 0 else None
                lib_name = libs[lib]["debugName"] if lib is not None else None
                address = t["frameTable"]["address"][frame]
                name = t["stringArray"][t["funcTable"]["name"][func]]
                frames_at[frame] = resolve(lib_name, address) or [(name, None)]
                if frames_at[frame][0][0].startswith("fun_") and lib is not None:
                    pdb = pdb_for(libs[lib].get("path") or "")
                    found = pdb.lookup(address) if pdb else None
                    if found:
                        frames_at[frame] = [found]
            return frames_at[frame]

        stacks = t["stackTable"]
        weights = t["samples"].get("weight")
        for i in range(count):
            stack = t["samples"]["stack"][i]
            if stack is None:
                continue
            weight = weights[i] if weights else 1
            leaf = frames_of(stacks["frame"][stack])
            if IDLE_RE.match(leaf[0][0]):
                continue
            total += weight
            self_time[leaf[0][0]] = self_time.get(leaf[0][0], 0) + weight
            if leaf[0][1]:
                lines[leaf[0][1]] = lines.get(leaf[0][1], 0) + weight
            seen = {name for name, _ in leaf}
            stack = stacks["prefix"][stack]
            # Callers are return addresses: they resolve to whatever follows
            # the call, often the next inlined function, so of each caller
            # only the enclosing function (the outermost frame) can be trusted.
            while stack is not None:
                seen.add(frames_of(stacks["frame"][stack])[-1][0])
                stack = stacks["prefix"][stack]
            for name in seen:
                inclusive[name] = inclusive.get(name, 0) + weight
    for pdb in pdbs.values():
        if pdb:
            pdb.close()
    if not total:
        die(f"no samples in {path}")
    return {"total": total, "threads": threads, "self": self_time, "inclusive": inclusive, "lines": lines}


def record_profile(side: str, sha: str | None, args: argparse.Namespace) -> dict:
    out = PERF_DIR / "profiles"
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{re.sub(r'[^A-Za-z0-9_.-]+', '_', args.bench_id)}-{side}.json.gz"
    if args.reuse:
        if not path.exists():
            die(f"no saved profile {path}")
        return parse_profile(path)
    checkout = ROOT if side == "tree" else export_ref(sha or "")
    env = symbol_env(side, sha)
    pkg, bench = locate_bench(checkout, args.bench_id, args.target)
    exe = build_exes(checkout, [(pkg, bench, None)], env)[(pkg, bench)][0]
    # With no match, --profile-time would still run the bench's setup, and
    # that would be reported as the benchmark's profile.
    ids = list_ids(exe, checkout / pkg, env, None)
    if args.bench_id not in ids:
        group = args.bench_id.split("/")[0]
        die(f"no benchmark {args.bench_id!r}; {group} has: {', '.join(i for i in ids if i.startswith(group)) or 'nothing'}")
    cmd = ["samply", "record", "--save-only", "--unstable-presymbolicate", "--symbol-dir", str(Path(exe).parent),
           "-o", str(path), "--", exe, "--bench", "--profile-time", str(args.seconds), f"^{re.escape(args.bench_id)}$"]
    r = subprocess.run(cmd, cwd=checkout / pkg, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
    if r.returncode != 0 or not path.exists():
        print("\n".join((r.stdout + r.stderr).splitlines()[-15:]), file=sys.stderr)
        die(f"samply failed (exit {r.returncode}); check `bench.py doctor`", 2)
    print(f"profile: {path}")
    return parse_profile(path)


def short_path(where: str) -> str:
    """A source location's last three path parts; other names unchanged."""
    if re.search(r"[\\/].*:\d+$", where):
        return "/".join(re.split(r"[\\/]", where)[-3:])
    return where


def print_shares(title: str, counts: dict[str, float], total: float) -> None:
    print(f"\n{title}\n|  % | where |\n|---|---|")
    for name, n in sorted(counts.items(), key=lambda kv: -kv[1])[:PROFILE_ROWS]:
        print(f"| {n / total * 100:5.1f} | {short_path(name)} |")


def cmd_profile(args: argparse.Namespace) -> None:
    if args.env == "wsl" and platform.system() == "Windows":
        sha = resolve_sha(args.ref) if args.side == "base" else "HEAD"
        delegate_to_wsl(["profile", "--bench-id", args.bench_id, "--side", args.side, "--ref", sha, "--seconds", str(args.seconds), *(["--target", args.target] if args.target else []), *(["--reuse"] if args.reuse else [])])
    sha = resolve_sha(args.ref) if args.side == "base" else None
    data = record_profile(args.side, sha, args)
    print(f"BENCH: {args.bench_id}  samples: {data['total']:.0f}, {data['threads']} thread(s)")
    print_shares("TOP SELF TIME", data["self"], data["total"])
    print_shares("TOP INCLUSIVE TIME", data["inclusive"], data["total"])
    print_shares("HOT LINES", data["lines"], data["total"])


def cmd_profile_diff(args: argparse.Namespace) -> None:
    sha = resolve_sha(args.ref)
    if args.env == "wsl" and platform.system() == "Windows":
        delegate_to_wsl(["profile-diff", "--bench-id", args.bench_id, "--ref", sha, "--seconds", str(args.seconds), *(["--target", args.target] if args.target else []), *(["--reuse"] if args.reuse else [])])
    base = record_profile("base", sha, args)
    tree = record_profile("tree", None, args)
    print(f"BENCH: {args.bench_id}  A {args.ref}: {base['total']:.0f} samples, B tree: {tree['total']:.0f} samples")
    print("Shares of each side's own samples; a fixed profile time runs a different number of iterations.")
    for title, key in (("SELF TIME", "self"), ("HOT LINES", "lines")):
        rows = []
        for name in set(base[key]) | set(tree[key]):
            a, b = base[key].get(name, 0) / base["total"] * 100, tree[key].get(name, 0) / tree["total"] * 100
            rows.append((b - a, a, b, name))
        print(f"\n{title}\n| A % | B % | Δ points | where |\n|---|---|---|---|")
        for delta, a, b, name in sorted(rows, key=lambda r: -abs(r[0]))[:PROFILE_ROWS]:
            print(f"| {a:5.1f} | {b:5.1f} | {delta:+.1f} | {short_path(name)} |")


def callgrind_counts(path: Path) -> tuple[int, dict[str, int]]:
    """(total instructions, instructions per function) from a callgrind file."""
    out = subprocess.run(["callgrind_annotate", "--threshold=100", "--inclusive=no", str(path)],
                         capture_output=True, text=True, encoding="utf-8", errors="replace", check=False).stdout
    total = 0
    funcs: dict[str, int] = {}
    for line in out.splitlines():
        m = re.match(r"\s*([\d,]+)\s+(.*)$", line)
        if not m:
            continue
        count, rest = int(m.group(1).replace(",", "")), m.group(2)
        if "PROGRAM TOTALS" in rest:
            total = count
            continue
        name = re.sub(r"\s+\[[^\]]*\]$", "", rest.split(":", 1)[-1])
        funcs[name] = funcs.get(name, 0) + count
    return total, funcs


def callgrind_side(side: str, sha: str | None, checkout: Path, pkg: str, op: str, size: int, stem: str) -> tuple[int, dict[str, int]]:
    example = checkout / pkg / "examples" / "callgrind_codec.rs"
    source = ROOT / pkg / "examples" / "callgrind_codec.rs"
    if not (checkout / pkg / "Cargo.toml").exists():
        die(f"{pkg} does not exist at the base ref")
    # The example is measuring equipment and may be newer than the base commit.
    if example != source:
        example.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, example)
    env = symbol_env(side, sha)
    cmd = ["cargo", "build", "--release", "-p", pkg, "--example", "callgrind_codec", "--message-format=json"]
    r = subprocess.run(cmd, cwd=checkout, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
    if r.returncode != 0:
        print("\n".join(r.stderr.splitlines()[-20:]), file=sys.stderr)
        die(f"building the callgrind example failed for {side}", 2)
    exe = next((m["executable"] for m in map(json.loads, (x for x in r.stdout.splitlines() if x.startswith("{")))
                if m.get("reason") == "compiler-artifact" and m.get("executable")), None)
    if not exe:
        die("no callgrind example executable built", 2)
    out = PERF_DIR / "profiles"
    out.mkdir(parents=True, exist_ok=True)
    data = out / f"callgrind-{stem}-{side}.out"
    v = subprocess.run(["valgrind", "--tool=callgrind", "--toggle-collect=*measured*", f"--callgrind-out-file={data}", exe, op, str(size)],
                       cwd=checkout, capture_output=True, text=True, encoding="utf-8", errors="replace", check=False)
    if v.returncode != 0:
        print("\n".join(v.stderr.splitlines()[-15:]), file=sys.stderr)
        die(f"valgrind failed on the {side} side (exit {v.returncode})", 2)
    return callgrind_counts(data)


def cmd_callgrind(args: argparse.Namespace) -> None:
    sha = resolve_sha(args.ref)
    if platform.system() == "Windows":
        delegate_to_wsl(["callgrind", "--bench-id", args.bench_id, "--ref", sha])
    case = next((c for c in CALLGRIND_CASES if re.match(c[0], args.bench_id)), None)
    if case is None:
        die(f"no callgrind case for {args.bench_id!r}; add one to CALLGRIND_CASES")
    _, pkg, op, size = case
    stem = re.sub(r"[^A-Za-z0-9_.-]+", "_", args.bench_id)
    base_total, base = callgrind_side("base", sha, export_ref(sha), pkg, op, size, stem)
    tree_total, tree = callgrind_side("tree", None, ROOT, pkg, op, size, stem)
    if not base_total or not tree_total:
        die("callgrind counted no instructions; is the example's `measured` function still there?")
    delta = tree_total / base_total - 1
    print(f"BENCH: {args.bench_id} ({pkg} {op} {size} bytes, Linux build)")
    print(f"instructions: A {base_total:,}  B {tree_total:,}  Δ {delta * 100:+.2f}%")
    rows = [(tree.get(n, 0) - base.get(n, 0), n) for n in set(base) | set(tree)]
    print("| A | B | Δ instructions | function |\n|---|---|---|---|")
    for diff, name in sorted(rows, key=lambda r: -abs(r[0]))[:PROFILE_ROWS]:
        if diff:
            print(f"| {base.get(name, 0):,} | {tree.get(name, 0):,} | {diff:+,} | {name} |")


# --- data loading ------------------------------------------------------------

def load_bench(baseline_dir: Path) -> dict:
    return json.loads((baseline_dir / "benchmark.json").read_text(encoding="utf-8"))


def per_iter(baseline_dir: Path) -> list[float]:
    s = json.loads((baseline_dir / "sample.json").read_text(encoding="utf-8"))
    return [t / i for t, i in zip(s["times"], s["iters"]) if i]


def collect(names: str) -> dict[str, dict]:
    """full_id -> {samples, mean, ci, bytes} for every bench saved under `names`.

    `names` may be comma-separated (e.g. the A1,A2 halves of an ABBA run);
    samples are pooled and the mean recomputed from them. `ci` is criterion's
    estimate for a single name and None for a pooled one.
    """
    out: dict[str, dict] = {}
    for name in (n.strip() for n in names.split(",") if n.strip()):
        for p in CRITERION.glob(f"**/{name}/benchmark.json"):
            d = p.parent
            try:
                meta = load_bench(d)
                samples = per_iter(d)
                est = json.loads((d / "estimates.json").read_text(encoding="utf-8"))["mean"]
            except (OSError, KeyError, ValueError):
                continue
            tp = meta.get("throughput") or {}
            e = out.get(meta["full_id"])
            if e is None:
                ci = est["confidence_interval"]
                out[meta["full_id"]] = {
                    "samples": samples,
                    "ci": (ci["lower_bound"], ci["upper_bound"]),
                    "bytes": tp.get("Bytes") or tp.get("BytesDecimal"),
                }
            else:
                e["samples"] = e["samples"] + samples
                e["ci"] = None
    for e in out.values():
        e["mean"] = sum(e["samples"]) / len(e["samples"]) if e["samples"] else 0.0
    return {k: v for k, v in out.items() if v["samples"]}


def mb_s(entry: dict) -> float | None:
    return entry["bytes"] / (entry["mean"] / 1e9) / 1e6 if entry.get("bytes") else None


def fmt_time(ns: float) -> str:
    for unit, div in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= div:
            return f"{ns / div:.3g} {unit}"
    return f"{ns:.3g} ns"


def bootstrap_delta(base: list[float], new: list[float], n: int = 2000) -> tuple[float, float, float]:
    """Relative change of mean time (new/base - 1) with a 95% bootstrap CI."""
    rng = random.Random(0)
    point = (sum(new) / len(new)) / (sum(base) / len(base)) - 1
    deltas = []
    for _ in range(n):
        b = [rng.choice(base) for _ in base]
        w = [rng.choice(new) for _ in new]
        deltas.append((sum(w) / len(w)) / (sum(b) / len(b)) - 1)
    deltas.sort()
    return point, deltas[int(0.025 * n)], deltas[int(0.975 * n) - 1]


# --- compare / noise ---------------------------------------------------------

def load_noise() -> dict[str, float]:
    try:
        return json.loads(NOISE_FILE.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def compare(base: str, new: str, threshold: float, use_noise: bool = True) -> tuple[list[dict], str]:
    a, b = collect(base), collect(new)
    floors = load_noise() if use_noise else {}
    rows = []
    for bid in sorted(set(a) & set(b)):
        d, lo, hi = bootstrap_delta(a[bid]["samples"], b[bid]["samples"])
        # The bootstrap only sees variance within a run; run-to-run variance
        # (code layout, frequency state) is captured by `noise` floors.
        sig = (lo > 0 or hi < 0) and abs(d) > floors.get(bid, 0.0)
        micro = a[bid]["mean"] < MICRO_NS
        if sig and micro:
            verdict = f"{'slower' if d > 0 else 'faster'} (micro, advisory)"
        elif sig and d * 100 > threshold:
            verdict = "REGRESSION"
        elif sig and d > 0:
            verdict = "slower (<gate)"
        elif sig:
            verdict = "improved"
        else:
            verdict = "noise"
        rows.append({"id": bid, "base": a[bid], "new": b[bid], "delta": d, "lo": lo, "hi": hi, "verdict": verdict})
    missing = sorted(set(a) ^ set(b))
    if not rows:
        die(f"no benchmarks present in both '{base}' and '{new}' under {CRITERION}")
    if any(r["verdict"] == "REGRESSION" for r in rows):
        overall = "REGRESSION"
    elif any(r["verdict"] == "improved" for r in rows):
        overall = "IMPROVED"
    else:
        overall = "NEUTRAL"
    if missing:
        print(f"note: {len(missing)} benchmark(s) only in one side, skipped: {', '.join(missing[:6])}{' …' if len(missing) > 6 else ''}")
    return rows, overall


def print_table(rows: list[dict], base: str, new: str) -> None:
    print(f"| benchmark | {base} | {new} | Δ time | 95% CI | throughput | verdict |")
    print("|---|---|---|---|---|---|---|")
    for r in rows:
        tp_b, tp_n = mb_s(r["base"]), mb_s(r["new"])
        tp = f"{tp_b:,.0f} → {tp_n:,.0f} MB/s" if tp_b and tp_n else ""
        print(
            f"| {r['id']} | {fmt_time(r['base']['mean'])} | {fmt_time(r['new']['mean'])} "
            f"| {r['delta'] * 100:+.1f}% | [{r['lo'] * 100:+.1f}%, {r['hi'] * 100:+.1f}%] | {tp} | {r['verdict']} |"
        )


def cmd_compare(args: argparse.Namespace) -> None:
    rows, overall = compare(args.base, args.new, args.threshold)
    if args.json:
        print(json.dumps([{k: r[k] for k in ("id", "delta", "lo", "hi", "verdict")} for r in rows], indent=2))
    else:
        print_table(rows, args.base, args.new)
    print(f"VERDICT: {overall}")


def cmd_noise(args: argparse.Namespace) -> None:
    """A/A test shaped exactly like `ab`: HEAD in a worktree vs HEAD here, ABBA.

    Two runs of one binary understate noise; separately built binaries differ
    in embedded paths and code layout, which alone moves microbenchmarks by
    several percent. That is the noise every worktree-based A/B carries.
    """
    entries = select_entries(args)
    dirty = sh(["git", "status", "--porcelain", "--", *(f"{pkg}/" for pkg, _, _ in entries)])
    if dirty:
        print("warning: uncommitted changes in the measured crates; this is not an A/A test "
              "and the floors will include those changes", file=sys.stderr)
    with measuring("noise", args.name):
        a, b = ab_run("HEAD", entries, args.name, args.quick)
    rows, _ = compare(a, b, threshold=100.0, use_noise=False)
    # Each A/A run is one sample of build-to-build layout noise; keep the worst
    # seen so floors converge upward as evidence accumulates.
    floors = {} if args.reset else load_noise()
    print("| benchmark | Δ between identical runs | CI half-width | min detectable |")
    print("|---|---|---|---|")
    worst = 0.0
    for r in rows:
        half = (r["hi"] - r["lo"]) / 2
        floor = 2 * max(abs(r["delta"]), half)
        floors[r["id"]] = round(max(floor, floors.get(r["id"], 0.0)), 4)
        worst = max(worst, floors[r["id"]])
        print(f"| {r['id']} | {r['delta'] * 100:+.1f}% | ±{half * 100:.1f}% | {floors[r['id']] * 100:.1f}% |")
    NOISE_FILE.write_text(json.dumps(floors, indent=2, sort_keys=True), encoding="utf-8")
    vals = sorted(floors[r["id"]] for r in rows)
    median = vals[len(vals) // 2]
    print(f"NOISE_FLOOR: median {median * 100:.1f}%, worst {worst * 100:.1f}% "
          f"(per benchmark: the smallest change it can verify on this machine; saved to {NOISE_FILE} "
          f"and applied by compare)")


# --- ceiling -----------------------------------------------------------------

def cmd_ceiling(args: argparse.Namespace) -> None:
    data = collect(args.name)
    fp = fingerprint()
    cores = fp["physical_cores"] or fp["logical_cores"]
    printed = False
    for op, pattern in (("compress", r"^compress_throughput/threads=(\w+)/block=(\d+)KB$"), ("decompress", r"^decompress_throughput/threads/(\w+)$")):
        by_block: dict[str, dict[str, float]] = {}
        for bid, e in data.items():
            m = re.match(pattern, bid)
            if m and mb_s(e):
                block = m.group(2) if m.lastindex and m.lastindex >= 2 else "1024"
                by_block.setdefault(block, {})[m.group(1)] = mb_s(e)  # type: ignore[assignment]
        for block, t in sorted(by_block.items(), key=lambda kv: int(kv[0])):
            if "1" not in t or "default" not in t:
                continue
            printed = True
            single, full = t["1"], t["default"]
            ideal = single * cores
            print(f"{op} block={block}KB: 1 thread {single:,.0f} MB/s; all threads {full:,.0f} MB/s; "
                  f"linear ceiling ({cores} physical cores) {ideal:,.0f} MB/s; "
                  f"current = {full / ideal * 100:.0f}% of ceiling; headroom ≤ {(ideal / full - 1) * 100:.0f}%")
            ordered = sorted(t.items(), key=lambda kv: (kv[0] == "default", int(kv[0]) if kv[0].isdigit() else 0))
            scaling = ", ".join(f"{'all' if k == 'default' else k + 't'} {v:,.0f}" for k, v in ordered)
            print(f"  scaling: {scaling}")
    if not printed:
        die(f"baseline '{args.name}' has no threads=1 and threads=default pairs; run --suite cpu-compress or cpu-decompress first")
    print("The linear ceiling is an upper bound: hybrid CPUs (P+E cores), SMT, turbo limits and memory "
          "bandwidth (decompression especially) all keep real scaling below it. Single-thread speed is "
          "bounded by libdeflate itself; beating the ceiling needs a lower level, a different algorithm, "
          "or less work per byte.")


# --- summary -----------------------------------------------------------------

def cmd_save_summary(args: argparse.Namespace) -> None:
    data = collect(args.name)
    if not data:
        die(f"no benchmarks saved under '{args.name}'")
    fp = fingerprint()
    out = ROOT / "perf" / "baselines" / f"{fp['slug']}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    summary = {
        "machine": fp,
        "commit": sh(["git", "rev-parse", args.commit]) or args.commit,
        "branch": args.branch or sh(["git", "rev-parse", "--abbrev-ref", "HEAD"]),
        "date": dt.date.today().isoformat(),
        "rustc": sh(["rustc", "--version"]),
        "criterion_baseline": args.name,
        "benchmarks": {
            bid: {
                "mean_ns": round(e["mean"], 1),
                **({"ci95_ns": [round(e["ci"][0], 1), round(e["ci"][1], 1)]} if e.get("ci") else {}),
                **({"mb_per_s": round(mb_s(e), 1)} if mb_s(e) else {}),
            }
            for bid, e in sorted(data.items())
        },
    }
    out.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)} ({len(data)} benchmarks)")


# --- cli ---------------------------------------------------------------------

# --- change sets, measurement ledger, perf sessions ---------------------------
#
# perf-trigger.sh (a Stop hook) asks for /perf check after hot-path edits. It
# must stay silent when the change set is already measured or being measured,
# or while a /perf goal owns the branch; otherwise the perf skill ends up
# re-invoking itself and starting a second benchmark job.

def git_out(args: list[str], stdin: bytes | None = None) -> bytes:
    return subprocess.run(["git", *args], cwd=ROOT, input=stdin, capture_output=True, check=False).stdout


def develop_base() -> str:
    """The newest merge base with develop: a local develop that is behind
    origin/develop would count already merged work as this branch's changes."""
    bases = [git_out(["merge-base", "HEAD", ref]).decode().strip() for ref in ("develop", "origin/develop")]
    bases = [b for b in bases if b]
    for b in bases:
        if all(subprocess.run(["git", "merge-base", "--is-ancestor", o, b], cwd=ROOT, check=False).returncode == 0 for o in bases):
            return b
    return bases[0] if bases else ""


def changeset() -> dict | None:
    """Perf-crate changes since the merge base with develop (committed, staged,
    unstaged, untracked). The hash input is what perf-trigger.sh used to hash:
    `git diff <base> -- PERF_SRC` followed by the untracked files' contents."""
    base = develop_base()
    if not base:
        return None
    diff = git_out(["diff", base, "--", *PERF_SRC])
    names = git_out(["diff", "--name-only", base, "--", *PERF_SRC]).decode().split()
    lines = 0
    for row in git_out(["diff", "--numstat", base, "--", *PERF_SRC]).decode().splitlines():
        added, removed = (row.split("\t") + ["0", "0"])[:2]
        lines += (int(added) if added.isdigit() else 0) + (int(removed) if removed.isdigit() else 0)
    untracked = git_out(["ls-files", "--others", "--exclude-standard", "--", *PERF_SRC]).decode().split()
    contents = b""
    for f in untracked:
        try:
            data = (ROOT / f).read_bytes()
        except OSError:
            continue
        contents += data
        lines += data.count(b"\n")
    changed = sorted({n for n in names + untracked if n.endswith(".rs")})
    return {
        "hash": git_out(["hash-object", "--stdin"], diff + contents).decode().strip(),
        "changed": changed,
        "hot": [n for n in changed if HOT_RE.match(n)],
        "lines": lines,
        "crates": sorted({n.split("/")[0] for n in changed}),
    }


def read_state(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def write_state(path: Path, data: dict) -> None:
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")


def now() -> dt.datetime:
    return dt.datetime.now(dt.timezone.utc)


@contextmanager
def measuring(cmd: str, name: str):
    """Record in the ledger that this change set is being measured and, when
    the command finishes, that it was measured. Not credited if the command
    failed or the perf crates changed during the run."""
    cs = changeset()
    if cs is None or not cs["changed"]:
        yield
        return
    ledger = read_state(LEDGER_FILE)
    ledger["running"] = {"changeset": cs["hash"], "pid": os.getpid(), "cmd": cmd, "name": name,
                         "started": now().isoformat()}
    write_state(LEDGER_FILE, ledger)
    ok = False
    try:
        yield
        ok = True
    finally:
        ledger = read_state(LEDGER_FILE)
        ledger.pop("running", None)
        after = changeset()
        if ok and after is not None and after["hash"] == cs["hash"]:
            measured = ledger.get("measured", [])
            measured.append({"changeset": cs["hash"], "cmd": cmd, "name": name, "finished": now().isoformat()})
            ledger["measured"] = measured[-LEDGER_KEEP:]
        elif ok:
            print("note: the perf crates changed during the run; not recorded as measured", file=sys.stderr)
        write_state(LEDGER_FILE, ledger)


def pid_alive(pid: int) -> bool | None:
    """True/False when it can be checked, None when it can't. Never signals the
    process: on Windows os.kill(pid, 0) would terminate it."""
    if pid <= 0:
        return False
    if os.name == "nt":
        out = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH"],
                             capture_output=True, text=True, check=False).stdout
        return str(pid) in out.split() if out else None
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except OSError:
        return None
    return True


def started_within(iso: str, limit: dt.timedelta) -> bool:
    try:
        return now() - dt.datetime.fromisoformat(iso) < limit
    except (TypeError, ValueError):
        return False


def current_branch() -> str:
    return git_out(["rev-parse", "--abbrev-ref", "HEAD"]).decode().strip()


def active_session() -> dict | None:
    s = read_state(SESSION_FILE)
    if not s or s.get("branch") != current_branch():
        return None
    try:
        if now() >= dt.datetime.fromisoformat(s["expires"]):
            return None
    except (KeyError, TypeError, ValueError):
        return None
    return s


def silence_reason(cs: dict) -> str | None:
    """Why perf-trigger.sh should stay silent for this change set, or None."""
    session = active_session()
    if session:
        return f"/perf {session.get('mode', '?')} session '{session.get('name', '?')}' owns this branch"
    ledger = read_state(LEDGER_FILE)
    if any(m.get("changeset") == cs["hash"] for m in ledger.get("measured", [])):
        return "this change set was already measured"
    running = ledger.get("running") or {}
    if running.get("changeset") == cs["hash"] and started_within(running.get("started", ""), RUNNING_MAX_AGE):
        if pid_alive(int(running.get("pid", -1))) is not False:
            return f"a {running.get('cmd', 'bench')} run is measuring this change set"
    return None


def cmd_changeset(args: argparse.Namespace) -> None:
    cs = changeset()
    if args.hook:
        # One line for perf-trigger.sh: QUIET, SKIP <reason>, or FIRE <hash> <why>.
        if cs is None or not cs["changed"] or (not cs["hot"] and cs["lines"] < args.threshold):
            print("QUIET")
        elif (reason := silence_reason(cs)) is not None:
            print(f"SKIP {reason}")
        else:
            why = f"{cs['lines']} changed lines in {','.join(cs['crates'])}"
            if cs["hot"]:
                why += f"; hot-path files: {' '.join(cs['hot'])}"
            print(f"FIRE {cs['hash']} {why}")
        return
    if cs is None:
        die("no merge base with develop")
    if args.json:
        print(json.dumps(cs, indent=2))
    else:
        print(f"{cs['hash']}  {cs['lines']} lines  changed: {' '.join(cs['changed']) or '-'}  "
              f"hot: {' '.join(cs['hot']) or '-'}")


def cmd_session(args: argparse.Namespace) -> None:
    if args.action == "begin":
        if not args.name:
            die("session begin needs --name")
        start = now()
        s = {"branch": current_branch(), "mode": args.mode, "name": args.name, "started": start.isoformat(),
             "expires": (start + dt.timedelta(hours=SESSION_HOURS)).isoformat()}
        write_state(SESSION_FILE, s)
        print(f"perf session '{args.name}' ({args.mode}) on {s['branch']} until {s['expires']}")
    elif args.action == "end":
        try:
            SESSION_FILE.unlink()
            print("perf session ended")
        except FileNotFoundError:
            print("no perf session")
    else:
        s = read_state(SESSION_FILE)
        if not s:
            print("no perf session")
        else:
            state = "active" if active_session() else "inactive (expired or another branch)"
            print(f"perf session '{s.get('name')}' ({s.get('mode')}) on {s.get('branch')}: {state}, "
                  f"expires {s.get('expires')}")


def main() -> None:
    # Windows consoles default to cp1252, which can't encode Δ/µ/→.
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("fingerprint")
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_fingerprint)

    for name, fn in (("run", cmd_run), ("noise", cmd_noise)):
        s = sub.add_parser(name)
        s.add_argument("--suite", default=None)
        s.add_argument("--crates", default=None, help="comma list; selects quick suites")
        s.add_argument("--name", required=True)
        s.add_argument("--quick", action="store_true", help="1s warm-up, 3s measurement")
        s.add_argument("--filter", default=None, help="criterion regex; only matching benchmarks run")
        s.set_defaults(fn=fn)

    sub.choices["noise"].add_argument("--reset", action="store_true", help="discard previously recorded floors")

    s = sub.add_parser("worktree-run")
    s.add_argument("--ref", required=True)
    s.add_argument("--suite", default=None)
    s.add_argument("--crates", default=None)
    s.add_argument("--name", required=True)
    s.add_argument("--quick", action="store_true")
    s.set_defaults(fn=cmd_worktree_run)

    s = sub.add_parser("ab")
    s.add_argument("--ref", required=True, help="base commit, built in a temporary worktree")
    s.add_argument("--suite", default=None)
    s.add_argument("--crates", default=None)
    s.add_argument("--name", required=True)
    s.add_argument("--quick", action="store_true")
    s.add_argument("--filter", default=None, help="criterion regex; only matching benchmarks run")
    s.add_argument("--changed", action="store_true", help="run the benchmarks `targets` picks for the changes since --ref")
    s.add_argument("--dry-run", action="store_true", help="list what would run and estimate the time")
    s.add_argument("--threshold", type=float, default=5.0)
    s.set_defaults(fn=cmd_ab)

    s = sub.add_parser("targets")
    s.add_argument("--ref", required=True)
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_targets)

    s = sub.add_parser("clean")
    s.set_defaults(fn=cmd_clean)

    s = sub.add_parser("doctor")
    s.set_defaults(fn=cmd_doctor)

    for name, fn in (("profile", cmd_profile), ("profile-diff", cmd_profile_diff)):
        s = sub.add_parser(name)
        s.add_argument("--bench-id", required=True, help="one criterion id, e.g. decompress_medium/1MB")
        s.add_argument("--ref", default="HEAD", help="base commit (profile: with --side base)")
        s.add_argument("--env", choices=["wsl", "here"], default="wsl", help="wsl runs unattended; here on Windows needs UAC approval")
        s.add_argument("--seconds", type=int, default=15)
        s.add_argument("--target", default=None, help="package:bench, if the id can't be found")
        s.add_argument("--reuse", action="store_true", help="re-read the saved profiles instead of recording")
        s.set_defaults(fn=fn)
    sub.choices["profile"].add_argument("--side", choices=["tree", "base"], default="tree")

    s = sub.add_parser("callgrind")
    s.add_argument("--bench-id", required=True)
    s.add_argument("--ref", required=True)
    s.set_defaults(fn=cmd_callgrind)

    s = sub.add_parser("compare")
    s.add_argument("base")
    s.add_argument("new")
    s.add_argument("--threshold", type=float, default=5.0, help="regression gate in percent (constitution: 5)")
    s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_compare)

    s = sub.add_parser("ceiling")
    s.add_argument("--name", required=True)
    s.set_defaults(fn=cmd_ceiling)

    s = sub.add_parser("save-summary")
    s.add_argument("--name", required=True)
    s.add_argument("--commit", default="HEAD", help="commit the measurements belong to")
    s.add_argument("--branch", default=None)
    s.set_defaults(fn=cmd_save_summary)

    s = sub.add_parser("changeset")
    s.add_argument("--json", action="store_true")
    s.add_argument("--hook", action="store_true", help="one-line verdict for perf-trigger.sh")
    s.add_argument("--threshold", type=int, default=40, help="changed lines that count as significant")
    s.set_defaults(fn=cmd_changeset)

    s = sub.add_parser("session")
    s.add_argument("action", choices=["begin", "end", "status"])
    s.add_argument("--mode", default="goal")
    s.add_argument("--name", default=None)
    s.set_defaults(fn=cmd_session)

    args = p.parse_args()
    selected = any(getattr(args, a, None) for a in ("suite", "crates", "filter", "changed"))
    if not selected and args.cmd in ("run", "noise", "worktree-run", "ab"):
        die("pass --suite, --crates, --filter or (ab) --changed")
    args.fn(args)


if __name__ == "__main__":
    main()
