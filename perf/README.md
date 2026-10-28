# Performance baselines

`baselines/<machine>.json` holds the latest benchmark summary for one machine,
written by `python .claude/skills/perf/bench.py save-summary`. The file name is
the machine fingerprint (`bench.py fingerprint`): OS, CPU model, and
physical/logical core counts, for example
`windows-12th-gen-intel-core-i9-12900k-16c24t.json`.

Each file records the commit, date, toolchain, power profile, and for every
criterion benchmark its mean time, 95% confidence interval, and throughput
(MB/s) where the benchmark declares one. Git history of the file is the
performance history for that machine.

## What these files are for

- **Context**: what "normal" looks like on a machine, and how numbers moved
  across releases.
- **Feasibility checks**: `/perf goal` uses them alongside fresh measurements
  to judge whether a target is realistic.

They are **not** evidence for a performance claim on their own. Numbers drift
with thermal state, power plan, OS updates, and background load, so every
"faster"/"no regression" claim compares two commits measured in the same
session on the same machine (`bench.py worktree-run` builds the base commit
in a temporary worktree with its own target dir, `target/perf/target-base`).

## Refreshing

Run `/perf baseline` in Claude Code after a release, or by hand:

```sh
python .claude/skills/perf/bench.py worktree-run --ref develop --suite full --name develop-$(git rev-parse --short develop)
python .claude/skills/perf/bench.py save-summary --name develop-$(git rev-parse --short develop) --commit develop --branch develop
```

Use the High/Ultimate Performance power plan (Windows) or the `performance`
governor (Linux), close heavy background work, and plug laptops in.

## Comparing two runs by hand

```sh
python .claude/skills/perf/bench.py run --suite cpu-compress --name before
# ...change code...
python .claude/skills/perf/bench.py run --suite cpu-compress --name after
python .claude/skills/perf/bench.py compare before after
```

`compare` bootstraps a 95% CI for each benchmark's change in mean time. A
change counts only when the CI excludes zero; a significant slowdown above 5%
is a `REGRESSION`, which fails the constitution's benchmark gate.
