---
name: perf
description: Benchmark-driven performance work for Crush. `diagnose` explains a slowdown from profiles, without changing code. `check` runs a quick A/B against the develop merge base and flags >5% regressions (auto-suggested by a Stop hook after significant hot-path edits). `goal "<text>"` takes an optimization goal such as "improve CPU compression by 10%", pushes back if it's vague or unrealistic, measures, profiles, plans with an Opus planner in plan mode, runs TDD experiments, and ships a PR with before/after numbers. `baseline` refreshes the committed per-machine baseline. Use for "perf check", "benchmark", "is this slower", "make X faster", "optimize", "performance goal", "regression".
argument-hint: "check | goal \"<goal>\" | baseline"
---

# Perf

Make the constitution's benchmark-driven development repeatable. All numbers
come from `.claude/skills/perf/bench.py` (run as `python .claude/skills/perf/bench.py ...`),
dispatched through the `perf-bench` agent so criterion output stays out of
this context. `perf/README.md` explains baselines for humans.

## Hard rules

- **Decide only on `bench.py ab`.** A claim of "faster" or "no regression"
  needs both sides measured now, on this machine, interleaved: `ab --ref <base>`
  builds the base commit in a temporary worktree (own `target/perf/target-base`, reuses our
  `Cargo.lock`) and runs base, new, new, base. Running one side after the
  other lets thermal/turbo drift show up as a fake regression of whichever ran
  second (seen in testing: identical code, +4–6% "slower"). The committed
  `perf/baselines/*.json` files give history and context only.
- **Significance or it didn't happen.** Only report a change when the 95% CI
  excludes zero *and* it exceeds that benchmark's noise floor (`bench.py
  compare` applies both). A target below the noise floor can't be verified.
  If `target/perf/noise.json` has no floor for a benchmark you're judging,
  run `noise` for its suite first.
- **Microbenchmarks are advisory.** Benchmarks under 100 µs (1 KB
  compress/decompress, small roundtrips) moved 5–15% between runs of the
  same binary in testing, so `compare` never calls them a REGRESSION. Mention a consistent advisory slowdown, but judge goals and
  gates on ms-scale benchmarks (1 MB and up, the 128 MB throughput suite).
- **One benchmark job at a time.** Never run benches in parallel or while
  building something else; they'd measure each other.
- **Constitution gates still apply**: TDD, no `.unwrap()`/`.expect()` in
  production, < 5% regression anywhere, no ratio loss unless agreed, allowed
  dependencies only, fuzz when decode/format code changes.
- **Don't re-enter `/perf` from its own Stop hook.** `perf-trigger.sh` asks
  for `/perf check` after hot-path edits. It stays silent when `bench.py
  session` marks a `/perf goal` as owning the branch, or when the ledger shows
  the current change set was measured or is being measured by `run`/`noise`/`ab`
  (`bench.py changeset --hook` prints the decision). If it fires during `/perf`
  anyway, say so and don't start a second benchmark job; an in-flight or
  finished `ab` on that tree is the check.
- **Find the cause before changing code.** Name a root cause only from
  profile evidence (`/perf diagnose`). If the tools aren't available, say so
  and report the cause as unknown; never present a guess as a cause. Setup is
  in `CONTRIBUTING.md`, "Performance Tooling"; `bench.py doctor` checks it.
- Commits follow security-patch step 6 (signed, never `--no-verify`,
  signing handoff to the user). Auto mode can't commit `.claude/` files; hand
  those to the user too.

## Suites

| suite | what it runs |
|---|---|
| `quick-core` / `quick-parallel` / `quick-gpu` | a short representative set per crate (`--crates a,b` picks them) |
| `cpu-compress` | `crush-parallel` `compress_throughput` (128 MB, all thread/block combos) + `crush-core` `compress_medium`/`compress_large` |
| `cpu-decompress` | `crush-parallel` `decompress_throughput` + `crush-core` `decompress_*` |
| `full` | every bench in core, parallel, gpu |

Add `--quick` (1 s warm-up, 3 s measurement) for `check`; never for gating.

## Measurement tiers

An `ab` on one benchmark takes about a minute once its base commit is built
(the first run for a commit takes a few minutes; `bench.py clean` frees the
kept checkouts and target dirs), so measure only what changed while iterating
and widen as a change earns it:

| tier | command | use | can decide |
|---|---|---|---|
| Screen | `ab --ref <base> --changed --quick` | each iteration | drop an idea early, nothing else |
| Keep | `ab --ref <base> --changed` | before keeping an experiment | keep or revert |
| Gate | `ab --ref <merge base> --suite full` | final, before a PR | ship |

`--filter '<regex>'` narrows further, and `bench.py targets --ref <base>` shows
what `--changed` would pick. `--changed` follows a table of files to
benchmarks, so it can miss an indirect effect; that's why the Gate tier stays
mandatory. If `crush-cli` changed, it also prints the `perf/compare.py` command
to run, because criterion doesn't cover the CLI.

Identical source built twice measured +1.1%, +1.8% and +3.2% on
`decompress_medium/1MB`: separate builds lay code out differently. Run
`noise --filter '<regex>'` before trusting a small targeted result, or use
`callgrind`, which reads exactly 0 on identical code.

---

## Mode `check`

Fast regression check. No plan mode, no code changes.

1. `BASE=$(git merge-base HEAD develop)`. List changed crates:
   `git diff --name-only $BASE -- crush-core/src crush-parallel/src crush-gpu/src`
   plus untracked files there. If none: report "no performance-crate changes"
   and stop.
2. Dispatch `perf-bench`: `ab --ref $BASE --crates <changed crates> --quick
   --name check-<HEAD sha7>` (append `-dirty` if the tree is dirty).
3. Report the table and verdict in a few lines.
   - `NEUTRAL` / `IMPROVED`: done.
   - `REGRESSION`: quick runs are noisier than full ones. Confirm before
     reporting: rerun `ab` for the regressed crate without `--quick`
     (new name). Report a regression only if both runs show it. Then dispatch
     `perf-profiler` on the worst benchmark, name the likely cause (file:line
     from the diff that matches the hot functions), and suggest fixing it or
     running `/perf goal`. Don't fix it automatically. If the confirmation
     run clears it, run `noise` for that crate so its floor reflects what you
     just saw.
   - `slower (<gate)` on a ms-scale benchmark in both a quick and a full run:
     mention it and suggest `/perf diagnose`, even though it doesn't block.

## Mode `diagnose "<benchmark or symptom>"`

Explain a slowdown or a shortfall with evidence. No code changes, no plan mode.

1. Name one criterion id and the base ref. If the symptom doesn't identify one,
   ask. Note the platform the problem was measured on: Windows and WSL builds
   differ, and criterion A/Bs normally run on Windows.
2. Start in WSL so the job runs unattended: dispatch `perf-profiler` for
   `callgrind` and `profile-diff --ref <base>` (`--env wsl`, the default).
3. Classify from what came back. If that settles it, stop here. Add a Windows
   `profile-diff --env here` only if the WSL evidence can't explain a slowdown
   measured on Windows. Do it as the last step, after everything unattended has
   finished and been reported, and tell the user first: it stalls on one UAC
   prompt per side until someone approves. The prompt is expected, and shows
   `samply.exe` from an unknown publisher. Don't change group membership,
   user rights or scheduled tasks to avoid it; those are the developer's
   choice (`CONTRIBUTING.md`, "Performance Tooling").
4. Use this table:

   | evidence | conclusion |
   |---|---|
   | instructions up, in a named function | code change: name the function and the diff that caused it |
   | instructions flat, time up, same functions and shares | layout, alignment or inlining in the hot code |
   | instructions flat, a function's share moved | look at that function: memory, cache or a dependency, not instruction count |
   | flat on WSL, slower on Windows | platform-specific: the Windows `profile-diff` decides |
   | a repeated A/A shows the same delta | noise: run `noise --filter` and stop |

5. Report the class, the numbers behind it, what would confirm it, and what it
   doesn't settle. Sampling shows shares of time, not absolute cost. Callgrind
   counts instructions, not cycles, so it can rule a code change in or out but
   says nothing about cache or branch behaviour, and WSL here has no hardware
   counters. Log it in `perf/log/<YYYY-MM-DD>-<slug>.md`, or in the feature's
   `perf-log.md`.

## Mode `goal "<text>"`

### 1. Intake

Parse the request into a goal record and show it to the user:

```
metric:     throughput | wall time | peak memory | ratio
operation:  compress | decompress | both
engine:     crush-parallel (libdeflate, block-parallel) | crush-core (flate2 plugin) | crush-gpu CPU path
workload:   <suite / bench ids, data size>
threads:    1 | all | specific
target:     +x% | absolute value
constraints: ratio may not drop; safe Rust only; allowed deps only; <user extras>
```

### 2. Push back on vagueness

If metric, operation, or engine is ambiguous, ask with AskUserQuestion,
offering concrete options with a recommended default. Never guess silently.
Examples:
- "CPU compression": crush-parallel 128 MB throughput at all threads
  (recommended: it's what users of `crush compress` hit), single-thread
  throughput, or crush-core flate2 path?
- "make it faster": which operation and which number?
- "use less memory": peak RSS during compress, or per-thread buffers?

### 3. Measure

Once the goal is confirmed, start the session that keeps the perf Stop hook
quiet while this goal owns the branch (expires after 12 h):
`python .claude/skills/perf/bench.py session begin --mode goal --name <slug>`.
Run it again after creating the experiment branch in step 7: the session is
tied to the branch it was started on.

Dispatch `perf-bench` for, in order:
1. `fingerprint`, and `noise` on the target suite (name `goal-<slug>-noise`).
2. `run --suite <target suite> --name goal-<slug>-before` (no `--quick`).
   This is the reference for feasibility and ceiling; record
   `GOAL_BASE=$(git rev-parse HEAD)` for the A/Bs later.
3. `ceiling --name goal-<slug>-before` for throughput goals.

If the power profile isn't High/Ultimate Performance (or the Linux governor
isn't `performance`), tell the user the numbers will be noisier.

### 4. Push back on unrealistic goals

Check feasibility with numbers before planning. Reply with the numbers and a
counter-offer; continue only on a goal the user confirms.

- **Below noise**: target smaller than the `min detectable` floor of the
  goal's own benchmarks → "can't be verified on this machine; smallest
  verifiable target is N%". (Floors come from A/A runs of HEAD vs HEAD in ABBA
  order; 1 KB microbenchmarks are often 10–50%, 128 MB throughput benches 1–3%.)
- **Already met**: current numbers already meet it → say so, offer a stretch.
- **Over the ceiling**: target exceeds `ceiling` headroom (or, for decompression,
  memory bandwidth; for single-thread, the raw codec) → report current % of
  ceiling, the highest realistic target, and what it would take to go beyond
  (lower level, different algorithm, `unsafe`/SIMD).
- **Conflicts with the rules**: needs `unsafe`, a ratio drop, a new
  dependency, or more than 32 MB/thread → state the trade-off and ask.
- If `pigz` is installed, `benchmark-compare.sh` gives the constitution's
  external reference point; quote it when relevant.

### 5. Profile

Dispatch `perf-profiler` on the most representative benchmark of the goal
(e.g. `compress_throughput/threads=default/block=1024KB`). For thread-scaling
goals, also profile the `threads=1` variant. If it returns `PROFILER: none`,
stop and give the user the `bench.py doctor` output: don't plan without a
profile.

### 6. Plan (plan mode, Opus planner)

1. Call `EnterPlanMode`.
2. Dispatch `perf-planner` (runs on Opus) with: the goal record, the baseline
   table, noise floor, ceiling lines, the profile summary, and the engine's
   source paths.
3. Write the plan file from its answer: ranked hypotheses, each with its
   profile evidence, a prediction a profile can prove wrong, expected gain,
   risk, files, tests-first, experiment, and keep criterion. Surface its
   `NEEDS USER DECISION` items as explicit questions.
4. `ExitPlanMode` for approval. Don't change code before approval.

### 7. Experiments

Create or use a feature branch (`NNN-perf-<slug>`, next free number, from
`develop`). Log to `specs/<branch>/perf-log.md` when a spec dir exists,
otherwise `perf/log/<YYYY-MM-DD>-<slug>.md`.

For each approved hypothesis, in rank order:
1. **Tests first**: add or extend round-trip/property tests the plan named;
   run them and confirm they pass on the current code (they guard behaviour;
   performance is the thing that changes).
2. Implement the change.
3. `bash .claude/hooks/rust-quality-gate.sh` (fmt, clippy, tests).
4. `perf-bench`: `ab --ref HEAD --changed --name goal-<slug>-exp<N>` (HEAD =
   last kept state; the experiment is the uncommitted working tree). Use the
   Screen tier first when an idea is doubtful. Add `--filter` if the goal's
   target benchmark isn't in the `--changed` selection.
5. **Keep** only if the target benchmark improved significantly, nothing
   regressed > 5%, and the ratio didn't drop; then commit it locally
   (signed, security-patch step 6) so the next experiment's base is HEAD.
   Otherwise **revert** the change (keep the tests if they're useful).
6. Re-profile the target benchmark (`callgrind`, or `profile-diff --ref HEAD`)
   and compare it with the hypothesis's prediction. A kept change whose
   prediction failed means the theory was wrong: say so, and revise the
   remaining hypotheses before the next experiment.
7. Append to the log: hypothesis, prediction, what the profile showed, change
   summary, table, verdict, kept/reverted.

Agree a time budget at intake (the default is one working session). Stop when
the goal is met, hypotheses run out, or the budget is spent. Measure
cumulative progress with `ab --ref $GOAL_BASE --suite <target suite>` and
report it against the target either way.

### 8. Gate and ship

1. `quality-gate` agent (full gate).
2. `perf-bench`: `ab --ref $(git merge-base HEAD develop) --suite full
   --name goal-<slug>-final`. Any significant regression > 5% blocks shipping.
3. If decode, format, or index code changed: fuzz 100k iterations
   (`cargo +nightly fuzz run <target> -- -runs=100000` for each relevant
   target in `fuzz/`).
4. `perf-bench`: `save-summary --name goal-<slug>-final-B1` (updates
   `perf/baselines/<fingerprint>.json`).
5. Commit (security-patch step 6). The body must include the before/after
   table for the goal benchmarks (constitution requirement for optimization
   PRs) and the machine slug.
6. PR and CI: follow security-patch steps 7–9 (PR fallback, CI monitor,
   auto-fix rounds, report). Put the goal record, the tables, and the
   experiment log summary in the PR body.
7. `python .claude/skills/perf/bench.py session end`. Also end the session
   whenever the goal stops early (rejected as unrealistic, abandoned, stopped
   by the user, or the experiment cap reached without shipping), so later
   edits on the branch get the normal `/perf check` reminder.

## Mode `baseline`

Refresh the committed per-machine baseline from `develop`:
1. `perf-bench`: `worktree-run --ref develop --suite full --name develop-<sha7>`,
   then `save-summary --name develop-<sha7> --commit develop --branch develop`.
2. Show the diff of `perf/baselines/<fingerprint>.json` against its previous
   version (headline throughputs) and commit it on a branch for a PR, or hand
   it to the user. Run this after each release.
