#!/usr/bin/env python3
"""Measure crush's parallel and GPU flavours on native Windows.

perf/compare.py runs under WSL, which has no real Vulkan driver and no nvrtc, so
GPU figures have to come from Windows. These results are charted beside the WSL
ones and labelled as Windows, with `crush parallel` measured on both sides as a
reference. Input and output are on disk here, not tmpfs, so the two environments
are not like for like.

Every run has a fresh output, tools alternate, and each tool gets one uncounted
first run. A checked run per dataset fails the whole job if a GPU flavour falls
back to the CPU, because the timing would then not describe the GPU.

    cargo build --release -p crush-cli --features cuda
    python perf/compare_gpu.py --crush PATH --corpus DIR --work DIR

--corpus holds text.bin, repetitive.bin, random.bin, real.bin and log.bin, copied
from the /dev/shm/crush-compare directory compare.py creates, so both
environments read identical files.

To compare two builds on decompression, alternating them (A B B A ...) on one
compressed file:

    python perf/compare_gpu.py --crush NEW --baseline OLD --corpus DIR --work DIR \\
        --data real --backend cuda --reps 10
"""
import argparse
import datetime
import filecmp
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DATASETS = ("text", "repetitive", "random", "real", "log")
OUT = os.path.join(REPO, "perf", "comparisons", "gpu", "windows.json")

# Tool: (compress arguments, decompress arguments, GPU backend expected).
TOOLS = {
    "crush-parallel-windows": (["--plugin", "parallel-deflate"], [], None),
    "crush-gpu-windows": (["--plugin", "gpu-deflate"], ["--gpu-backend", "wgpu"], "wgpu"),
    "crush-gpu-cuda-windows": (["--plugin", "gpu-deflate"], ["--gpu-backend", "cuda"], "CUDA"),
}


def backend_check(log, expected):
    """Why a verbose decompress log does not show GPU decoding, or None.

    Returns the tiles dispatched as an int when the GPU path completed.
    """
    log = re.sub(r"\x1b\[[0-9;]*m", "", log)
    if "falling back to CPU" in log or "using CPU" in log:
        return "fell back to the CPU"
    if "GPU decompression succeeded" not in log:
        return "no GPU success line"
    m = re.search(r'tile_count=\d+ backend="?([^"\s]+)', log)
    if not m or expected.lower() not in m[1].lower():
        return f"ran on {m[1] if m else 'an unknown backend'}, expected {expected}"
    return None


def timed(cmd):
    start = time.perf_counter()
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.perf_counter() - start


def remove(path):
    if os.path.exists(path):
        os.unlink(path)


def iqr_pct(values):
    q1, _, q3 = statistics.quantiles(values, n=4, method="inclusive")
    return round((q3 - q1) / statistics.median(values) * 100, 1)


def stats(seconds, size):
    speeds = [size / 1e6 / s for s in seconds]
    return round(statistics.median(speeds), 1), iqr_pct(speeds)


def measure(a, data):
    src = os.path.join(a.corpus, f"{data}.bin")
    size = os.path.getsize(src)
    packed = os.path.join(a.work, "out.crush")
    plain = os.path.join(a.work, "out.raw")
    times = {t: ([], []) for t in TOOLS}
    ratio = {}
    gpu_ran = {}

    def run(tool, record=True):
        comp, decomp, _ = TOOLS[tool]
        remove(packed)
        c = timed([a.crush, "compress", *comp, src, "-o", packed, "-f"])
        remove(plain)
        d = timed([a.crush, "decompress", *decomp, packed, "-o", plain, "-f"])
        ratio[tool] = round(size / os.path.getsize(packed), 3)
        if record:
            times[tool][0].append(c)
            times[tool][1].append(d)
        return decomp

    for tool, (_, _, expected) in TOOLS.items():
        decomp = run(tool, record=False)  # uncounted, and leaves files to check
        if not filecmp.cmp(src, plain, shallow=False):
            sys.exit(f"{tool} on {data}: output differs from the input")
        if expected:
            remove(plain)
            log = subprocess.run([a.crush, "-v", "decompress", *decomp, packed, "-o", plain, "-f"],
                                 check=True, capture_output=True, text=True, errors="replace")
            problem = backend_check(log.stdout + log.stderr, expected)
            if problem:
                sys.exit(f"{tool} on {data}: {problem}")
            gpu_ran[tool] = "GPU dispatch completed in 0.0ms" not in re.sub(
                r"\x1b\[[0-9;]*m", "", log.stdout + log.stderr)

    order = list(TOOLS)
    for i in range(a.reps):
        for tool in order if i % 2 == 0 else reversed(order):
            run(tool)

    rows = []
    for tool, (ct, dt) in times.items():
        comp, comp_iqr = stats(ct, size)
        decomp, decomp_iqr = stats(dt, size)
        row = {"cores": "all", "data": data, "tool": tool, "env": "windows", "ratio": ratio[tool],
               "comp_mbs": comp, "decomp_mbs": decomp,
               "comp_iqr_pct": comp_iqr, "decomp_iqr_pct": decomp_iqr}
        if tool in gpu_ran:
            row["gpu_used"] = gpu_ran[tool]
        print(row, flush=True)
        rows.append(row)
    return rows


def compare_builds(a):
    src = os.path.join(a.corpus, f"{a.data}.bin")
    size = os.path.getsize(src)
    packed = os.path.join(a.work, "out.crush")
    plain = os.path.join(a.work, "out.raw")
    subprocess.run([a.crush, "compress", "--plugin", "gpu-deflate", src, "-o", packed, "-f"],
                   check=True, capture_output=True)
    args = ["--gpu-backend", a.backend]
    expected = "CUDA" if a.backend == "cuda" else "wgpu"
    for build in (a.baseline, a.crush):
        remove(plain)
        log = subprocess.run([build, "-v", "decompress", *args, packed, "-o", plain, "-f"],
                             check=True, capture_output=True, text=True, errors="replace")
        problem = backend_check(log.stdout + log.stderr, expected)
        if problem:
            sys.exit(f"{build}: {problem}")
        if not filecmp.cmp(src, plain, shallow=False):
            sys.exit(f"{build}: output differs from the input")

    def once(build):
        remove(plain)
        return timed([build, "decompress", *args, packed, "-o", plain, "-f"])

    base, new = [], []
    for i in range(a.reps * 2):
        if i % 4 in (0, 3):
            base.append(once(a.baseline))
        else:
            new.append(once(a.crush))
    old_speed, old_iqr = stats(base, size)
    new_speed, new_iqr = stats(new, size)
    print(f"{a.data}, {a.backend}: baseline {old_speed} MB/s (IQR {old_iqr}%), "
          f"new {new_speed} MB/s (IQR {new_iqr}%), {new_speed / old_speed - 1:+.1%}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--crush", required=True, help="built with --features cuda")
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--work", required=True, help="directory for outputs, on disk")
    ap.add_argument("--reps", type=int, default=7)
    ap.add_argument("--out", default=OUT)
    ap.add_argument("--only", choices=DATASETS,
                    help="re-measure one dataset and replace its rows in --out")
    ap.add_argument("--baseline", help="compare --crush against this build")
    ap.add_argument("--data", default="real", choices=DATASETS, help="with --baseline")
    ap.add_argument("--backend", default="cuda", choices=("cuda", "wgpu"), help="with --baseline")
    a = ap.parse_args()
    if sys.platform != "win32":
        sys.exit("compare_gpu.py measures native Windows; use compare.py elsewhere")
    os.makedirs(a.work, exist_ok=True)
    if a.baseline:
        compare_builds(a)
        return

    if a.only:
        with open(a.out, encoding="utf8") as f:
            doc = json.load(f)
        doc["results"] = [r for r in doc["results"] if r["data"] != a.only] + measure(a, a.only)
        with open(a.out, "w", encoding="utf8", newline="\n") as f:
            json.dump(doc, f, indent=1)
            f.write("\n")
        return

    results = [row for data in DATASETS for row in measure(a, data)]
    gpu = subprocess.run(["nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader"],
                         capture_output=True, text=True, check=True).stdout.strip()
    commit = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO,
                            capture_output=True, text=True, check=True).stdout.strip()
    doc = {"date": datetime.date.today().isoformat(), "commit": commit,
           "machine": {"os": platform.platform(), "gpu": gpu},
           "method": {"size_mb": 128, "reps": a.reps, "statistic": "median", "alternating": True,
                      "io": "disk", "pinning": "none"},
           "results": results}
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w", encoding="utf8", newline="\n") as f:
        json.dump(doc, f, indent=1)
        f.write("\n")
    print(f"wrote {a.out}")


if __name__ == "__main__":
    main()
