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
  ab --ref R --suite S --name N [--quick]
                                        the A/B to decide on: R (worktree) vs the
                                        working tree, run A B B A, then compared
  compare BASE NEW [--threshold 5]      A/B table with bootstrap 95% CI; each side
                                        may be a comma list of names (pooled).
                                        Benchmarks < 100 µs are advisory only.
  noise --suite S --name N [--quick]    A/A `ab` (HEAD in a worktree vs HEAD here);
                                        records per-benchmark noise floors that
                                        compare then applies
  ceiling --name N                      thread-scaling numbers for feasibility
  save-summary --name N [--commit REF]  write perf/baselines/<fingerprint>.json

Suites: quick-core, quick-parallel, quick-gpu, cpu-compress, cpu-decompress,
full. `--crates` on run picks quick suites for the named crates.

Exit codes: 0 ok, 1 usage/IO error, 2 cargo bench failed. `compare` prints
`VERDICT: REGRESSION|IMPROVED|NEUTRAL` as its last line and exits 0.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import platform
import random
import re
import shutil
import subprocess
import sys
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
    "cpu-decompress": [
        ("crush-parallel", "throughput", r"^decompress_throughput/"),
        ("crush-core", "compression", r"^decompress_"),
    ],
    "full": [
        ("crush-core", "", None),
        ("crush-parallel", "", None),
        ("crush-gpu", "", None),
    ],
}
CRATE_SUITE = {"crush-core": "quick-core", "crush-parallel": "quick-parallel", "crush-gpu": "quick-gpu"}


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


def run_benches(cwd: Path, entries, name: str, quick: bool, env: dict) -> list[str]:
    """Run each entry; return the criterion ids saved under baseline `name`."""
    PERF_DIR.mkdir(parents=True, exist_ok=True)
    log = PERF_DIR / f"{name}.log"
    started = dt.datetime.now().timestamp()
    with log.open("w", encoding="utf-8") as fh:
        for pkg, bench, flt in entries:
            cmd = ["cargo", "bench", "-p", pkg]
            # Select bench targets explicitly: with no selector, or with
            # --benches (which includes the lib, bench = true by default),
            # cargo also runs the lib's libtest harness, and that rejects
            # criterion flags like --save-baseline.
            cmd += ["--bench", bench or "*"]
            cmd += ["--"]
            if flt:
                cmd.append(flt)
            cmd += ["--save-baseline", name, "--noplot"]
            if quick:
                cmd += QUICK_ARGS
            fh.write(f"$ {' '.join(cmd)}\n")
            fh.flush()
            rc = subprocess.run(cmd, cwd=cwd, env=env, stdout=fh, stderr=subprocess.STDOUT, check=False).returncode
            if rc != 0:
                fh.close()
                tail = log.read_text(encoding="utf-8", errors="replace").splitlines()[-30:]
                print("\n".join(tail), file=sys.stderr)
                die(f"cargo bench failed for {pkg} (exit {rc}); full log: {log}", 2)
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
    run_benches(ROOT, suite_entries(args.suite, args.crates), args.name, args.quick, env)


def cmd_worktree_run(args: argparse.Namespace) -> None:
    with worktree(args.ref) as (wt, env):
        run_benches(wt, suite_entries(args.suite, args.crates), args.name, args.quick, env)


@contextmanager
def worktree(ref: str):
    """Check out `ref` in a temporary worktree with its own target dir.

    The base side must not share our target dir: both checkouts build the same
    package ids, so they'd write the same artifact paths, and while the
    worktree exists cargo's fingerprints point at its unchanged sources and
    consider the base binary fresh. The working-tree side would then silently
    benchmark the base build. target/perf/target-base persists between runs so
    only the workspace crates rebuild.
    """
    sha = sh(["git", "rev-parse", "--verify", f"{ref}^{{commit}}"])
    if not sha:
        die(f"cannot resolve ref {ref!r}")
    wt = PERF_DIR / f"wt-{sha[:7]}"
    if wt.exists():
        subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=ROOT, check=False)
    PERF_DIR.mkdir(parents=True, exist_ok=True)
    r = subprocess.run(["git", "worktree", "add", "--detach", str(wt), sha], cwd=ROOT, capture_output=True, text=True, check=False)
    if r.returncode != 0:
        die(f"git worktree add failed: {r.stderr.strip()}")
    try:
        # Cargo.lock is gitignored; reuse ours so both sides build the same dependency versions.
        if (ROOT / "Cargo.lock").exists():
            shutil.copy2(ROOT / "Cargo.lock", wt / "Cargo.lock")
        yield wt, dict(os.environ, CARGO_TARGET_DIR=str(PERF_DIR / "target-base"), CRITERION_HOME=str(CRITERION))
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", str(wt)], cwd=ROOT, check=False)


def ab_run(ref: str, entries, name: str, quick: bool) -> tuple[str, str]:
    """Measure `ref` (A, in a worktree) against the working tree (B) as A B B A.

    Running one side after the other lets thermal and turbo drift masquerade
    as a regression of whichever side runs second. ABBA ordering cancels
    linear drift; compare() pools each side's samples. Returns the two
    comma-joined name lists to pass to compare.
    """
    env = dict(os.environ, CRITERION_HOME=str(CRITERION))
    a1, b1, b2, a2 = (f"{name}-{x}" for x in ("A1", "B1", "B2", "A2"))
    with worktree(ref) as (wt, wt_env):
        run_benches(wt, entries, a1, quick, wt_env)
        run_benches(ROOT, entries, b1, quick, env)
        run_benches(ROOT, entries, b2, quick, env)
        run_benches(wt, entries, a2, quick, wt_env)
    return f"{a1},{a2}", f"{b1},{b2}"


def cmd_ab(args: argparse.Namespace) -> None:
    base, new = ab_run(args.ref, suite_entries(args.suite, args.crates), args.name, args.quick)
    print(f"A = {args.ref} ({base}), B = working tree ({new})")
    rows, overall = compare(base, new, args.threshold)
    print_table(rows, f"A {args.ref}", "B")
    print(f"VERDICT: {overall}")


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
    entries = suite_entries(args.suite, args.crates)
    dirty = sh(["git", "status", "--porcelain", "--", *(f"{pkg}/" for pkg, _, _ in entries)])
    if dirty:
        print("warning: uncommitted changes in the measured crates; this is not an A/A test "
              "and the floors will include those changes", file=sys.stderr)
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
    s.add_argument("--threshold", type=float, default=5.0)
    s.set_defaults(fn=cmd_ab)

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

    args = p.parse_args()
    if getattr(args, "suite", None) is None and getattr(args, "crates", None) is None and args.cmd in ("run", "noise", "worktree-run", "ab"):
        die("pass --suite or --crates")
    args.fn(args)


if __name__ == "__main__":
    main()
