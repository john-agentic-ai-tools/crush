#!/usr/bin/env python3
"""Compare the crush CLI against gzip, pigz, zstd, and lz4 (Linux, including WSL).

Every tool reads from and writes to tmpfs, is pinned to the same CPUs with
taskset, and runs REPS times; the median is reported and each run's output is
checked to round-trip. Results go to perf/comparisons/<machine>.json.

gzip and pigz are required (the constitution's targets). zstd and lz4 are
optional and skipped when not on PATH; they use their own formats, so they
are a reference point for speed and ratio, not a like-for-like DEFLATE
comparison. Use lz4 >= 1.10 for multithreading (-T); older lz4 runs
single-threaded at every core count. Tools built into ~/.local/bin take
precedence when it is first on PATH.

    cargo build --release -p crush-cli
    python3 perf/compare.py [--reps 5] [--size-mb 128] [--crush PATH]

Subsets (feature 023): --cores 1, --tools crush-fast-lz,lz4-1, --data real,
and --abba to interleave the tools' runs (A B B A ...) so drift hits all
tools alike. A subset run prints its table and writes JSON only with --out,
so it never overwrites the full comparison.

    python3 perf/compare.py --cores 1 --tools crush-fast-lz,lz4-1 --data real --abba --reps 10

Feature 026:

    --in-memory prints zstd's own in-memory benchmark (zstd -b) for each dataset,
    single-threaded, so codec speed can be told apart from CLI I/O costs.

    --builds a=PATH,b=PATH runs every crush tool once per binary, as separate
    rows named tool@a and tool@b. A win under 10% counts only if it holds across
    separately built binaries, because identical code can move that much with
    its placement. Vary placement with, for example,
    RUSTFLAGS="-C llvm-args=-align-all-functions=6" and a separate target dir.
"""
import argparse
import datetime
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DATASETS = {
    # base64 of random bytes: 6 bits of entropy per byte
    "text": "head -c {n3_4} /dev/urandom | base64 -w 76 | head -c {n}",
    "repetitive": "yes 'The quick brown fox jumps over the lazy dog. ' | head -c {n}",
    "random": "head -c {n} /dev/urandom",
    # real files: shared libraries, docs, locale data
    "real": "tar -cf - /usr/lib /usr/share 2>/dev/null | head -c {n}",
}


def sh(cmd):
    subprocess.run(cmd, shell=True, check=True, executable="/bin/bash")


def out(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True,
                          executable="/bin/bash").stdout.strip()


def core_sets():
    """1 CPU, 4 CPUs on distinct physical cores, and every CPU."""
    first_per_core = {}
    for line in out("lscpu -p=CPU,CORE").splitlines():
        if line.startswith("#"):
            continue
        cpu, core = (int(x) for x in line.split(","))
        first_per_core.setdefault(core, cpu)
    distinct = sorted(first_per_core.values())
    n = os.cpu_count() or 1
    sets = {"1": ("0", 1)}
    if len(distinct) >= 4:
        sets["4"] = (",".join(str(c) for c in distinct[:4]), 4)
    sets["all"] = (f"0-{n - 1}", n)
    return sets


# Single-threaded tools: measured at 1 core only.
SINGLE_THREADED = ("gzip-6", "gzip-1", "crush-fast-lz")
# Optional tools: (binary, version command).
OPTIONAL = {"zstd": "zstd --version", "lz4": "lz4 --version"}


def crush_tools(crush):
    """Commands for each crush configuration, for one binary."""
    return {
        "crush": (f"{crush} compress {{i}} -o {{o}} -f >/dev/null 2>&1",
                  f"{crush} decompress {{o}} -o {{d}} -f >/dev/null 2>&1"),
        "crush-fast-lz": (f"{crush} compress --plugin fast-lz {{i}} -o {{o}} -f >/dev/null 2>&1",
                          f"{crush} decompress {{o}} -o {{d}} -f >/dev/null 2>&1"),
    }


def in_memory_zstd(path, levels="1", last="3"):
    """zstd's own single-threaded benchmark: {level: (comp_mbs, decomp_mbs, ratio)}."""
    text = out(f"zstd -b{levels}e{last} --single-thread -i5 {path} 2>&1")
    rows = {}
    latest = None
    # Progress lines carry the figures; a "<level>#" line then closes the level.
    for line in re.split(r"[\r\n]+", text):
        m = re.search(r"\(x?([\d.]+)\),\s*([\d.]+) MB/s\s*,\s*([\d.]+) MB/s", line)
        if m:
            latest = (float(m[2]), float(m[3]), float(m[1]))
        level = re.match(r"\s*(-?\d+)#", line)
        if level and latest:
            rows[level[1]] = latest
            latest = None
    return rows


def tools(crush, threads, builds=None):
    """Compress and decompress commands; optional tools only when installed.

    With builds ({label: path}), each crush tool appears once per binary as
    "tool@label".
    """
    if builds:
        rotated = {}
        for label, path in builds.items():
            for name, cmds in crush_tools(path).items():
                rotated[f"{name}@{label}"] = cmds
    else:
        rotated = crush_tools(crush)
    all_tools = {
        **rotated,
        "gzip-6": ("gzip -6 -c {i} > {o}", "gzip -dc {o} > {d}"),
        "gzip-1": ("gzip -1 -c {i} > {o}", "gzip -dc {o} > {d}"),
        "pigz-6": (f"pigz -6 -p {threads} -c {{i}} > {{o}}",
                   f"pigz -dc -p {threads} {{o}} > {{d}}"),
        "pigz-1": (f"pigz -1 -p {threads} -c {{i}} > {{o}}",
                   f"pigz -dc -p {threads} {{o}} > {{d}}"),
        "zstd-1": (f"zstd -q -1 -T{threads} -c {{i}} > {{o}}", "zstd -q -dc {o} > {d}"),
        "zstd-3": (f"zstd -q -3 -T{threads} -c {{i}} > {{o}}", "zstd -q -dc {o} > {d}"),
        "lz4-1": (f"lz4 -q -1 -T{threads} -c {{i}} > {{o}}", "lz4 -q -dc {o} > {d}"),
        "lz4-9": (f"lz4 -q -9 -T{threads} -c {{i}} > {{o}}", "lz4 -q -dc {o} > {d}"),
    }
    return {name: cmds for name, cmds in all_tools.items()
            if name.split("-")[0] not in OPTIONAL or shutil.which(name.split("-")[0])}


def timed(cmd):
    t = time.perf_counter()
    sh(cmd)
    return time.perf_counter() - t


def machine():
    cpu = re.search(r"model name\s*:\s*(.+)", open("/proc/cpuinfo").read())
    wsl = "microsoft" in open("/proc/version").read().lower()
    dist = out(". /etc/os-release && echo \"$ID-$VERSION_ID\"") or "linux"
    return {
        "cpu": cpu.group(1).strip() if cpu else platform.processor(),
        "logical_cpus": os.cpu_count(),
        "os": dist,
        "kernel": platform.release(),
        "wsl": wsl,
    }


def slug(m):
    cpu = re.sub(r"\(r\)|\(tm\)|cpu|@.*$", "", m["cpu"].lower())
    cpu = re.sub(r"[^a-z0-9]+", "-", cpu).strip("-")
    env = "wsl2" if m["wsl"] else "linux"
    return f"{env}-{m['os']}-{cpu}-{m['logical_cpus']}t"


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--crush", default=os.environ.get(
        "CRUSH", os.path.join(REPO, "target", "release", "crush")))
    ap.add_argument("--reps", type=int, default=5)
    ap.add_argument("--size-mb", type=int, default=128)
    ap.add_argument("--data-dir", default="/dev/shm/crush-compare"
                    if os.path.isdir("/dev/shm") else
                    os.path.join(tempfile.gettempdir(), "crush-compare"))
    ap.add_argument("--out", help="default: perf/comparisons/<machine>.json")
    ap.add_argument("--cores", help="subset of core sets: 1,4,all")
    ap.add_argument("--tools", help="subset of tools, e.g. crush-fast-lz,lz4-1")
    ap.add_argument("--data", help="subset of datasets: " + ",".join(DATASETS))
    ap.add_argument("--abba", action="store_true",
                    help="interleave the tools' runs, alternating their order")
    ap.add_argument("--in-memory", action="store_true",
                    help="also print zstd -b single-threaded figures per dataset")
    ap.add_argument("--builds", help="label=path,... run each crush tool per binary")
    a = ap.parse_args()
    builds = dict(b.split("=", 1) for b in a.builds.split(",")) if a.builds else None
    pick = lambda arg: set(arg.split(",")) if arg else None
    want_cores, want_tools, want_data = pick(a.cores), pick(a.tools), pick(a.data)
    subset = any(x is not None for x in (want_cores, want_tools, want_data))

    if not sys.platform.startswith("linux"):
        sys.exit("compare.py needs Linux (taskset, tmpfs); on Windows use WSL")
    missing = [t for t in ("taskset", "gzip", "pigz") if not shutil.which(t)]
    if missing or not os.access(a.crush, os.X_OK):
        sys.exit(f"missing: {', '.join(missing + ([a.crush] if not os.access(a.crush, os.X_OK) else []))}"
                 " (apt-get install pigz; cargo build --release -p crush-cli)")

    size = a.size_mb * 1024 * 1024
    os.makedirs(a.data_dir, exist_ok=True)
    for name, gen in DATASETS.items():
        p = os.path.join(a.data_dir, f"{name}.bin")
        if not os.path.exists(p) or os.path.getsize(p) != size:
            sh(gen.format(n=size, n3_4=size * 3 // 4) + f" > {p}")

    o, d = os.path.join(a.data_dir, "out.z"), os.path.join(a.data_dir, "out.raw")
    results = []
    try:
        for cores, (cpus, threads) in core_sets().items():
            if want_cores and cores not in want_cores:
                continue
            for ds in DATASETS:
                if want_data and ds not in want_data:
                    continue
                i = os.path.join(a.data_dir, f"{ds}.bin")
                sel = {t: cmds for t, cmds in tools(a.crush, threads, builds).items()
                       if (not want_tools or t.split("@")[0] in want_tools)
                       and not (cores != "1" and t.split("@")[0] in SINGLE_THREADED)}
                pin = f"taskset -c {cpus} "
                times = {t: ([], []) for t in sel}
                ratio = {}

                def run(tool):
                    c, x = sel[tool]
                    ct, dt = times[tool]
                    ct.append(timed(pin + c.format(i=i, o=o, d=d)))
                    dt.append(timed(pin + x.format(i=i, o=o, d=d)))
                    sh(f"cmp -s {i} {d}")
                    ratio[tool] = round(size / os.path.getsize(o), 3)

                if a.abba:
                    order = list(sel)
                    for rep in range(a.reps):
                        for tool in (order if rep % 2 == 0 else order[::-1]):
                            run(tool)
                else:
                    for tool in sel:
                        for _ in range(a.reps):
                            run(tool)

                for tool, (ct, dt) in times.items():
                    mb = size / 1e6
                    r = {"cores": cores, "data": ds, "tool": tool,
                         "comp_mbs": round(mb / statistics.median(ct), 1),
                         "decomp_mbs": round(mb / statistics.median(dt), 1),
                         "ratio": ratio[tool],
                         "comp_spread_pct": round(
                             (max(ct) - min(ct)) / statistics.median(ct) * 100, 1),
                         "decomp_spread_pct": round(
                             (max(dt) - min(dt)) / statistics.median(dt) * 100, 1)}
                    results.append(r)
                    print(f"{cores:>3} {ds:<10} {tool:<13} compress {r['comp_mbs']:>8} MB/s"
                          f" (±{r['comp_spread_pct']}%)"
                          f"  decompress {r['decomp_mbs']:>8} MB/s"
                          f" (±{r['decomp_spread_pct']}%)  ratio {r['ratio']}",
                          flush=True)
        if a.in_memory and shutil.which("zstd"):
            for ds in DATASETS:
                if want_data and ds not in want_data:
                    continue
                i = os.path.join(a.data_dir, f"{ds}.bin")
                for level, (c, x, r) in in_memory_zstd(i).items():
                    results.append({"cores": "1", "data": ds, "tool": f"zstd-b{level}",
                                    "comp_mbs": c, "decomp_mbs": x, "ratio": r})
                    print(f"  1 {ds:<10} zstd -b{level} in memory: compress {c} MB/s"
                          f"  decompress {x} MB/s  ratio {r}", flush=True)
    finally:
        for f in (o, d):
            if os.path.exists(f):
                os.remove(f)

    m = machine()
    doc = {
        "date": datetime.date.today().isoformat(),
        "commit": out(f"git -C {REPO} rev-parse --short HEAD"),
        "machine": m,
        "tools": {
            "crush": out(f"{a.crush} --version"),
            "gzip": out("gzip --version | head -1"),
            "pigz": out("pigz --version 2>&1"),
            **{t: out(f"{cmd} 2>&1 | head -1") for t, cmd in OPTIONAL.items()
               if shutil.which(t)},
        },
        "method": {"size_mb": a.size_mb, "reps": a.reps, "statistic": "median",
                   "abba": a.abba,
                   "io": "tmpfs", "pinning": "taskset",
                   "core_sets": {k: v[0] for k, v in core_sets().items()}},
        "results": results,
    }
    if subset and not a.out:
        return
    path = a.out or os.path.join(REPO, "perf", "comparisons", f"{slug(m)}.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
