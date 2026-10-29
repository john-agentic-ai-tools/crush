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
SINGLE_THREADED = ("gzip-6", "gzip-1")
# Optional tools: (binary, version command).
OPTIONAL = {"zstd": "zstd --version", "lz4": "lz4 --version"}


def tools(crush, threads):
    """Compress and decompress commands; optional tools only when installed."""
    all_tools = {
        "crush": (f"{crush} compress {{i}} -o {{o}} -f >/dev/null 2>&1",
                  f"{crush} decompress {{o}} -o {{d}} -f >/dev/null 2>&1"),
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
    a = ap.parse_args()

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
            for ds in DATASETS:
                i = os.path.join(a.data_dir, f"{ds}.bin")
                for tool, (c, x) in tools(a.crush, threads).items():
                    if cores != "1" and tool in SINGLE_THREADED:
                        continue
                    pin = f"taskset -c {cpus} "
                    ct, dt = [], []
                    for _ in range(a.reps):
                        ct.append(timed(pin + c.format(i=i, o=o, d=d)))
                        dt.append(timed(pin + x.format(i=i, o=o, d=d)))
                    sh(f"cmp -s {i} {d}")
                    mb = size / 1e6
                    r = {"cores": cores, "data": ds, "tool": tool,
                         "comp_mbs": round(mb / statistics.median(ct), 1),
                         "decomp_mbs": round(mb / statistics.median(dt), 1),
                         "ratio": round(size / os.path.getsize(o), 3),
                         "comp_spread_pct": round(
                             (max(ct) - min(ct)) / statistics.median(ct) * 100, 1)}
                    results.append(r)
                    print(f"{cores:>3} {ds:<10} {tool:<8} compress {r['comp_mbs']:>8} MB/s"
                          f"  decompress {r['decomp_mbs']:>8} MB/s  ratio {r['ratio']}",
                          flush=True)
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
                   "io": "tmpfs", "pinning": "taskset",
                   "core_sets": {k: v[0] for k, v in core_sets().items()}},
        "results": results,
    }
    path = a.out or os.path.join(REPO, "perf", "comparisons", f"{slug(m)}.json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
