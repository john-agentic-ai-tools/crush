#!/usr/bin/env python3
"""Chart perf/compare.py results: speed against ratio, crush against the rest.

Standard library only, and it runs on any platform:

    python perf/chart.py [RESULTS.json] [--out perf/comparisons]

RESULTS.json defaults to the newest file in perf/comparisons. This writes:

    chart.html                     every dataset, core count and direction, with
                                   filters, hover details and a table
    real-all-compress.svg          the two views the top-level README shows
    real-all-decompress.svg

Up and to the right is better. A tool is "on the frontier" when no other tool
in that view is at least as good on both speed and ratio. Measure first with
perf/compare.py: this script only draws what is in the file.
"""
import argparse
import glob
import html
import json
import math
import os
import re

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
COMPARISONS = os.path.join(REPO, "perf", "comparisons")

DATASETS = {
    "real": "real files",
    "text": "base64 text",
    "repetitive": "repetitive text",
    "random": "random data",
}
CORES = {"all": "all cores", "4": "4 cores", "1": "1 core"}
OPS = {"compress": ("comp_mbs", "Compression"),
       "decompress": ("decomp_mbs", "Decompression")}
NAMES = {
    "crush": "crush",
    "crush-fast-lz": "crush fast-lz",
    "gzip-6": "gzip -6", "gzip-1": "gzip -1",
    "pigz-6": "pigz -6", "pigz-1": "pigz -1",
    "zstd-1": "zstd -1", "zstd-3": "zstd -3",
    "lz4-1": "lz4 -1", "lz4-9": "lz4 -9",
}

W, H = 720, 460
LEFT, RIGHT, TOP, BOTTOM = 68, 24, 44, 52
CHAR_W, LINE_H = 6.6, 14

STYLE = """
.c-root{--surface:#fcfcfb;--ink:#0b0b0b;--ink2:#52514e;--muted:#898781;
--grid:#e1e0d9;--axis:#c3c2b7;--crush:#2a78d6;--other:#898781}
@media (prefers-color-scheme:dark){.c-root{--surface:#1a1a19;--ink:#f0efec;
--ink2:#c3c2b7;--grid:#2c2c2a;--axis:#383835;--crush:#3987e5}}
.c-root text{font:12px system-ui,-apple-system,"Segoe UI",sans-serif;fill:var(--ink2)}
.c-root .bg{fill:var(--surface)}
.c-root .title{font-size:15px;font-weight:600;fill:var(--ink)}
.c-root .tick{fill:var(--muted);font-variant-numeric:tabular-nums}
.c-root .own{fill:var(--ink);font-weight:600}
.c-root .grid{stroke:var(--grid);stroke-width:1}
.c-root .axis{stroke:var(--axis);stroke-width:1}
.c-root .dot-crush{fill:var(--crush);stroke:var(--surface);stroke-width:2}
.c-root .dot-other{fill:var(--other);stroke:var(--surface);stroke-width:2}
.c-root .hit{fill:transparent}
"""

PAGE = """<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Crush Benchmarks</title>
<style>
:root{--plane:#f9f9f7;--ink:#0b0b0b;--ink2:#52514e;--line:#e1e0d9;--tip:#0b0b0b;--tipink:#fcfcfb}
@media (prefers-color-scheme:dark){:root{--plane:#0d0d0d;--ink:#f0efec;--ink2:#c3c2b7;
--line:#2c2c2a;--tip:#f0efec;--tipink:#1a1a19}}
body{margin:0;background:var(--plane);color:var(--ink);
font:15px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:760px;margin:0 auto;padding:24px 16px 48px}
h1{font-size:24px;margin:0 0 4px}
p{color:var(--ink2);margin:0 0 16px}
.filters{display:flex;flex-wrap:wrap;gap:12px;margin:0 0 16px}
label{font-size:13px;color:var(--ink2)}
select{display:block;margin-top:2px;font:inherit;color:var(--ink);background:var(--plane);
border:1px solid var(--line);border-radius:6px;padding:4px 8px}
svg{display:block;width:100%;height:auto;border-radius:8px}
.scroll{overflow-x:auto}
table{border-collapse:collapse;width:100%;margin-top:16px;font-size:14px}
th,td{text-align:right;padding:6px 8px;border-bottom:1px solid var(--line);
font-variant-numeric:tabular-nums;white-space:nowrap}
th:first-child,td:first-child,th:last-child,td:last-child{text-align:left}
tr.mine td{font-weight:600}
.view[hidden]{display:none}
#tip{position:fixed;pointer-events:none;background:var(--tip);color:var(--tipink);
padding:6px 8px;border-radius:6px;font-size:13px;white-space:pre;display:none;z-index:2}
</style></head><body><main>
<h1>Crush benchmarks</h1>
<p>Speed against ratio. Up and to the right is better. __META__</p>
<div class="filters">__FILTERS__</div>
__VIEWS__
<p style="margin-top:16px">__METHOD__</p>
</main><div id="tip" role="status"></div>
<script>
const sel=[...document.querySelectorAll('select')],views=[...document.querySelectorAll('.view')];
function show(){const k=sel.map(s=>s.value).join('|');views.forEach(v=>v.hidden=v.dataset.key!==k)}
sel.forEach(s=>s.addEventListener('change',show));show();
const tip=document.getElementById('tip');
function place(x,y){tip.style.left=Math.min(x+12,innerWidth-tip.offsetWidth-8)+'px';
tip.style.top=Math.max(y-tip.offsetHeight-12,8)+'px'}
document.querySelectorAll('.pt').forEach(p=>{
const on=e=>{tip.textContent=p.dataset.tip;tip.style.display='block';
const r=p.getBoundingClientRect();place(e.clientX||r.x,e.clientY||r.y)};
const off=()=>tip.style.display='none';
p.addEventListener('mouseenter',on);p.addEventListener('mousemove',on);
p.addEventListener('mouseleave',off);p.addEventListener('focus',on);p.addEventListener('blur',off)});
</script></body></html>
"""


def mine(tool):
    return tool.startswith("crush")


def label(tool):
    return NAMES.get(tool, tool)


def view_rows(results, data, cores, op):
    """Rows for one view, fastest first, each with its speed and frontier flag."""
    key = OPS[op][0]
    rows = [dict(r, speed=r[key]) for r in results
            if r["data"] == data and r["cores"] == cores and "-b" not in r["tool"]]
    rows.sort(key=lambda r: -r["speed"])
    for r in rows:
        r["frontier"] = not any(
            o is not r and o["speed"] >= r["speed"] and o["ratio"] >= r["ratio"]
            and (o["speed"] > r["speed"] or o["ratio"] > r["ratio"])
            for o in rows)
    return rows


def speed_ticks(low, high):
    """1, 2, 5 steps across the range, thinned to at most eight."""
    start = 10 ** math.floor(math.log10(low))
    ticks = []
    decade = start
    while decade <= high * 10:
        ticks += [m * decade for m in (1, 2, 5)]
        decade *= 10
    ticks = [t for t in ticks if low <= t <= high]
    while len(ticks) > 8:
        ticks = ticks[::2]
    return ticks


def ratio_ticks(low, high):
    span = high - low
    step = next(s for s in (0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500)
                if span / s <= 6)
    first = math.ceil(low / step)
    return [round(i * step, 2) for i in range(first, math.floor(high / step) + 1)]


def overlaps(a, b):
    return a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]


def place_labels(points):
    """Put each label beside its dot where it fits; crush labels go first.

    A label that fits nowhere is dropped: the hover detail and the table still
    carry that tool.
    """
    x1, x2, y1, y2 = LEFT, W - RIGHT, TOP, H - BOTTOM
    dots = [(p["x"] - 6, p["y"] - 6, p["x"] + 6, p["y"] + 6) for p in points]
    taken = []
    for p in sorted(points, key=lambda q: not mine(q["tool"])):
        w = len(label(p["tool"])) * CHAR_W
        x, y = p["x"], p["y"]
        spots = [(x + 9, y + 4, "start"), (x - 9, y + 4, "end"),
                 (x, y - 11, "middle"), (x, y + 19, "middle"),
                 (x + 8, y - 8, "start"), (x - 8, y - 8, "end"),
                 (x + 8, y + 16, "start"), (x - 8, y + 16, "end")]
        for tx, ty, anchor in spots:
            left = {"start": tx, "end": tx - w, "middle": tx - w / 2}[anchor]
            box = (left, ty - LINE_H + 3, left + w, ty + 3)
            inside = box[0] >= x1 and box[2] <= x2 and box[1] >= y1 and box[3] <= y2
            if inside and not any(overlaps(box, b) for b in taken + dots):
                taken.append(box)
                p["label"] = (tx, ty, anchor)
                break


def svg(rows, data, cores, op, standalone=False):
    speeds = [r["speed"] for r in rows]
    ratios = [r["ratio"] for r in rows]
    low, high = min(speeds) * 0.8, max(speeds) * 1.25
    log_x = max(ratios) / min(ratios) > 20
    if log_x:
        xlow, xhigh = min(ratios) * 0.7, max(ratios) * 1.6
        sx = lambda v: LEFT + (math.log10(v) - math.log10(xlow)) / (
            math.log10(xhigh) - math.log10(xlow)) * (W - LEFT - RIGHT)
    else:
        middle = (min(ratios) + max(ratios)) / 2
        half = max((max(ratios) - min(ratios)) * 0.56, 0.1)
        xlow, xhigh = max(middle - half, 0), middle + half
        sx = lambda v: LEFT + (v - xlow) / (xhigh - xlow) * (W - LEFT - RIGHT)
    sy = lambda v: (H - BOTTOM) - (math.log10(v) - math.log10(low)) / (
        math.log10(high) - math.log10(low)) * (H - TOP - BOTTOM)

    title = f"{OPS[op][1]} speed against ratio: {DATASETS[data]}, {CORES[cores]}"
    out = [f'<svg class="c-root" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}"'
           f' role="img" aria-label="{html.escape(title)}">']
    if standalone:
        out.append(f"<style>{STYLE}</style>")
    out.append(f'<rect class="bg" width="{W}" height="{H}"/>')
    out.append(f'<text class="title" x="{LEFT}" y="20">{html.escape(title)}</text>')
    for x, kind, text in ((W - RIGHT - 104, "crush", "Crush"),
                          (W - RIGHT - 54, "other", "Other tools")):
        out.append(f'<circle class="dot-{kind}" cx="{x}" cy="16" r="5"/>')
        out.append(f'<text x="{x + 8}" y="20">{text}</text>')

    for t in speed_ticks(low, high):
        y = round(sy(t), 1)
        out.append(f'<line class="grid" x1="{LEFT}" x2="{W - RIGHT}" y1="{y}" y2="{y}"/>')
        out.append(f'<text class="tick" x="{LEFT - 8}" y="{y + 4}" text-anchor="end">{t:,}</text>')
    for t in speed_ticks(xlow, xhigh) if log_x else ratio_ticks(xlow, xhigh):
        x = round(sx(t), 1)
        out.append(f'<text class="tick" x="{x}" y="{H - BOTTOM + 18}" text-anchor="middle">{t:,g}</text>')
    out.append(f'<line class="axis" x1="{LEFT}" x2="{W - RIGHT}" y1="{H - BOTTOM}" y2="{H - BOTTOM}"/>')
    out.append(f'<text x="{(LEFT + W - RIGHT) / 2}" y="{H - 10}" text-anchor="middle">'
               f"Compression ratio{', log scale' if log_x else ''} (higher is smaller output)</text>")
    out.append(f'<text transform="translate(16 {(TOP + H - BOTTOM) / 2}) rotate(-90)"'
               ' text-anchor="middle">MB/s, log scale</text>')

    points = [dict(r, x=round(sx(r["ratio"]), 1), y=round(sy(r["speed"]), 1)) for r in rows]
    place_labels(points)
    for p in sorted(points, key=lambda q: mine(q["tool"])):
        kind = "crush" if mine(p["tool"]) else "other"
        tip = (f"{label(p['tool'])}\n{p['speed']:,.0f} MB/s\nratio {p['ratio']:.3f}"
               + ("\non the frontier" if p["frontier"] else ""))
        out.append(f'<g class="pt" tabindex="0" data-tip="{html.escape(tip)}">'
                   f"<title>{html.escape(tip)}</title>"
                   f'<circle class="hit" cx="{p["x"]}" cy="{p["y"]}" r="12"/>'
                   f'<circle class="dot-{kind}" cx="{p["x"]}" cy="{p["y"]}" r="{6 if kind == "crush" else 5}"/></g>')
    for p in points:
        if "label" in p:
            tx, ty, anchor = p["label"]
            cls = ' class="own"' if mine(p["tool"]) else ""
            out.append(f'<text{cls} x="{tx}" y="{ty}" text-anchor="{anchor}">'
                       f"{html.escape(label(p['tool']))}</text>")
    out.append("</svg>")
    return "\n".join(out)


def table(rows, op):
    head = ("<tr><th>Tool</th><th>Speed rank</th><th>MB/s</th><th>Spread</th>"
            "<th>Ratio</th><th>On the frontier</th></tr>")
    spread = "comp_spread_pct" if op == "compress" else "decomp_spread_pct"
    body = []
    for rank, r in enumerate(rows, 1):
        cls = ' class="mine"' if mine(r["tool"]) else ""
        pct = f"±{r[spread]:.0f}%" if spread in r else ""
        body.append(f"<tr{cls}><td>{html.escape(label(r['tool']))}</td><td>{rank}</td>"
                    f"<td>{r['speed']:,.0f}</td><td>{pct}</td><td>{r['ratio']:.3f}</td>"
                    f"<td>{'Yes' if r['frontier'] else 'No'}</td></tr>")
    return f'<div class="scroll"><table>{head}{"".join(body)}</table></div>'


def select(name, options):
    items = "".join(f'<option value="{k}">{html.escape(v)}</option>' for k, v in options.items())
    return f'<label>{name}<select>{items}</select></label>'


def version(text):
    m = re.search(r"v?(\d+\.\d+(?:\.\d+)?)", text)
    return m.group(1) if m else text


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("results", nargs="?", help="default: newest perf/comparisons/*.json")
    ap.add_argument("--out", default=COMPARISONS, help="output directory")
    a = ap.parse_args()
    files = glob.glob(os.path.join(COMPARISONS, "*.json"))
    path = a.results or max(files, key=os.path.getmtime, default=None)
    if not path:
        raise SystemExit("no results: run perf/compare.py first")
    with open(path, encoding="utf8") as f:
        doc = json.load(f)
    results = doc["results"]

    views = []
    for data in DATASETS:
        for cores in CORES:
            for op in OPS:
                rows = view_rows(results, data, cores, op)
                if rows:
                    views.append(f'<section class="view" data-key="{data}|{cores}|{op}" hidden>'
                                 f"{svg(rows, data, cores, op)}{table(rows, op)}</section>")

    machine = doc["machine"]
    tools = ", ".join(f"{t} {version(v)}" for t, v in doc["tools"].items())
    method = doc["method"]
    meta = html.escape(f"{machine['cpu']}, {doc['date']}, commit {doc['commit']}.")
    how = html.escape(
        f"Median of {method['reps']} runs of {method['size_mb']} MiB, through each tool's"
        f" command line, with input and output in tmpfs, {'alternating tools, ' if method.get('abba') else ''}"
        f"pinned with taskset. Each run was checked to round-trip. gzip is single-threaded, so it"
        f" appears at 1 core only. A tool is on the frontier when no other tool in the view is"
        f" at least as good on both speed and ratio. {tools}.")
    filters = (select("Data", DATASETS) + select("Cores", CORES)
               + select("Direction", {k: v[1] for k, v in OPS.items()}))
    page = (PAGE.replace("__META__", meta).replace("__FILTERS__", filters)
            .replace("__VIEWS__", f'<div class="c-root"><style>{STYLE}</style>{"".join(views)}</div>')
            .replace("__METHOD__", how))

    os.makedirs(a.out, exist_ok=True)
    with open(os.path.join(a.out, "chart.html"), "w", encoding="utf8", newline="\n") as f:
        f.write(page)
    for op in OPS:
        rows = view_rows(results, "real", "all", op)
        with open(os.path.join(a.out, f"real-all-{op}.svg"), "w", encoding="utf8", newline="\n") as f:
            f.write(svg(rows, "real", "all", op, standalone=True) + "\n")
    print(f"wrote {a.out}: chart.html and two SVGs, {len(views)} views, from {os.path.basename(path)}")


if __name__ == "__main__":
    main()
