---
name: speckit-crush-perf-gate
description: Run /perf check (and /perf goal for spec performance criteria) when performance crates changed
compatibility: Requires spec-kit project structure with .specify/ directory
metadata:
  author: crush
  source: crush-perf:commands/speckit.crush-perf.gate.md
---

# Crush performance gate

Runs after `/speckit-implement`. The constitution requires that benchmarks show
no regression over 5% and that performance claims are measured.

## Steps

1. Find the merge base and the changed performance crates:

   ```
   git merge-base HEAD develop
   git diff --name-only <base> -- crush-core/src crush-parallel/src crush-gpu/src
   ```

   Include untracked files in those directories (`git ls-files --others --exclude-standard -- <dirs>`).
   If nothing changed there, report "Performance gate: no performance-crate
   changes, skipped" and stop.

2. Run the `/perf check` skill and wait for its result. If it confirms a
   significant regression over 5%, the feature is not complete: report the
   table, and fix the regression (or get the user's explicit acceptance)
   before declaring the implementation done.

3. Read the feature's `spec.md` (the feature directory from
   `.specify/feature.json`). For each success criterion or requirement with a
   performance target (throughput, latency, memory, startup time) that the
   implementation was meant to achieve, run `/perf goal "<criterion>"` unless
   the feature's `perf-log.md` already records a verified result for it.

4. Append the tables and verdicts to `<feature dir>/perf-log.md` and report a
   short summary: checks run, verdict, goals met or not.