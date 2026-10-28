---
name: security-patch
description: Sweep Crush for vulnerable and out-of-date dependencies (Rust crates, fuzz lockfiles, Rust toolchain, GitHub Actions), patch them, run the full quality gate, commit, open a PR against develop, and watch CI until it is green, fixing failures automatically. Use for "security patch", "dependency sweep", "patch vulnerabilities", "update packages", "check for outdated dependencies".
argument-hint: "[--resume] [--no-pr] [--security-only]"
---

# Security patch sweep

Run the end-to-end dependency/security sweep for this repo. Work through the
steps in order. Keep raw tool output out of the main context: use `scan.sh`
for scanning and the `quality-gate` / `ci-fixer` subagents for verification
and CI repair.

Arguments:
- `--resume`: continue an interrupted sweep on the current branch (see step 0;
  usually auto-detected).
- `--no-pr`: stop after step 6 (commit); no push, PR, or CI monitoring.
- `--security-only`: patch only RUSTSEC findings; skip routine version bumps
  (still bump the toolchain if a patch requires it).

## Hard rules

- Never add entries to `ignore` in `.cargo/audit.toml` or `.cargo/deny.toml`,
  and never lower a CI gate (coverage threshold, removed job,
  `continue-on-error`) without asking the user first.
- Never commit with `--no-gpg-sign`, `-c commit.gpgsign=false`, or `--no-verify`.
- `Cargo.lock` is gitignored. A fix that only lives in the lockfile does not
  reach CI or consumers. Vulnerability fixes in direct dependencies MUST raise
  the version floor in a `Cargo.toml`.
- PRs target `develop`, never `main` (constitution).

## 0. Resume detection (always run first)

A sweep can be interrupted anywhere: a signing handoff, a session ending
mid-CI, or the user stopping before the PR. Detect in-progress work before
starting a new sweep.

Resume when either is true:
- `--resume` was passed, or
- the current branch is not `develop`/`main`, matches
  `^[0-9]{3}-.*(security|dependency|dependencies|deps|toolchain)`, and has
  commits ahead of `origin/develop` or tracked changes.

Otherwise go to step 1.

Gather state (read-only):
```
git fetch origin --prune
git status --porcelain --untracked-files=no      # tracked changes only
git diff --cached --quiet; echo staged=$?
git log --oneline origin/develop..HEAD
git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null   # upstream?
git rev-list --count @{u}..HEAD 2>/dev/null      # unpushed commits
gh pr list --head <branch> --state all --json number,state,url,headRefOid
```
Untracked files are not part of the sweep, e.g. new `.claude/` files. Mention
them and leave them alone.

Pick the resume point from the first matching row:

| State | Resume at |
|---|---|
| PR `MERGED` or `CLOSED` | Nothing to do. Report, and suggest a fresh sweep. |
| Unstaged or staged tracked changes, nothing new committed for them | Step 5 (verify), then 6. |
| Changes staged after a signing handoff (message file exists in scratchpad) | Step 6. Ask the user to run the `! git commit -F` command. |
| Commits ahead of `origin/develop`, no upstream or unpushed commits, no PR | Step 7 (push + PR) |
| Pushed, no PR | Step 7.2 (PR create) |
| PR `OPEN`, `headRefOid` ≠ local HEAD | Push, then step 8 |
| PR `OPEN` and up to date | Step 8. Look at the runs for HEAD: watch in-progress runs, classify failed ones, and go to step 9 if all are green. |

Before resuming at step 5–7, re-check that the work is still sound:
1. Check that `git merge-base --is-ancestor origin/develop HEAD` holds. If
   `develop` moved, rebase onto it (merge instead if the branch is already
   pushed), resolve any manifest conflicts, and re-run `cargo update` so the
   local lockfile matches.
2. Run `scan.sh`. New VULN/WARN lines since the commit go back through step 4.
   Only fold them into the existing branch; don't start a new one.
3. Reconstruct what the branch already changed from
   `git log -p origin/develop..HEAD -- '*Cargo.toml' rust-toolchain.toml .github/workflows`
   and the commit bodies, for the PR findings table.

Then continue from the chosen step, with all of its rules (quality gate,
signing, CI caps).

## 1. Preflight

1. `git status --porcelain --untracked-files=no` must be empty. If not, stop and tell the user.
2. `git fetch --all --prune`, then `git checkout develop && git pull --ff-only`.
3. Pick the branch number: the highest `NNN` among `git branch -a` names
   matching `^(remotes/origin/)?[0-9]{3}-` and `specs/[0-9]{3}-*` dirs, plus 1,
   zero-padded to 3 digits. This is the same rule as
   `.specify/scripts/powershell/create-new-feature.ps1`. Do not create a spec dir.
4. `git checkout -b NNN-security-patches`.

## 2. Scan

Run `bash .claude/skills/security-patch/scan.sh` from the repo root. It prints
`VULN`, `WARN`, `MAJOR`, `MSRV`, `TOOLCHAIN behind`, and `ACTION` lines, plus
per-lockfile counts of compatible updates.

If there are no VULN/WARN lines, no updates, no MAJOR/ACTION lines, and the
toolchain is current, report "nothing to patch", delete the branch, return to
`develop`, and stop.

## 3. Toolchain first (when needed)

Invoke the `toolchain-update` skill (Skill tool, `skill: "toolchain-update"`,
args `--from-security-patch`) BEFORE patching crates when either:
- a fix requires a newer rustc: `cargo update -p <crate> --precise <ver>`
  fails mentioning `rust-version`/`requires rustc`, a scan line starts with
  `MSRV`, or the patched crate's `rust-version` exceeds the pinned toolchain; or
- the scan shows `TOOLCHAIN behind` and `--security-only` was not passed.

It makes its own commit and returns. Then continue with step 4.

## 4. Patch

For each VULN/WARN (from `scan.sh`):
1. Find how it is pulled in: `cargo tree -i <crate>@<version> -e all`.
2. Update it: `cargo update -p <crate>@<version> --precise <patched>` (or plain
   `cargo update -p <crate>`). For a fuzz lockfile, add `--manifest-path <dir>/Cargo.toml`.
3. If it is a direct dependency (in `[workspace.dependencies]` or a crate
   `Cargo.toml`), raise the floor to the patched version and add a comment in
   the existing style:
   ```toml
   # 0.9.11 patches RUSTSEC-2026-0186: <one-line description>. Keep this floor
   # explicit — Cargo.lock is gitignored, so the manifest is the only thing
   # pinning consumers off the unsound versions.
   ```
4. If it is transitive and no compatible patched version exists, look for an
   upgrade of the direct dependency that drops it. If none exists, stop and ask
   the user. Do not ignore the advisory on your own.

Unless `--security-only`:
- `cargo update` in the workspace and in each `*/fuzz` dir that has a `Cargo.lock`.
- `MAJOR` lines: read the crate's CHANGELOG or release notes for the skipped
  versions. Bump it in `Cargo.toml` if the breaking changes don't touch our
  usage (`grep` for the crate's paths in `crush-*/src`). If it needs real
  refactoring, skip it and record why for the PR body.
- `ACTION` lines: read the release notes of every major in between
  (`gh api repos/<owner>/<repo>/releases/tags/<tag> --jq .body`). Bump in all
  `.github/workflows/*.yml`. Keep `upload-artifact`/`download-artifact` on
  compatible majors (both v4+). Check `pull_request_target`/`workflow_run`
  usage before moving `actions/checkout` to v7+.

## 5. Verify

1. Dispatch the `quality-gate` subagent (Agent tool,
   `subagent_type: "quality-gate"`). Prompt: "Run the full quality gate on the
   working tree and report." It returns PASS, or failures with file:line.
2. On failure, fix the reported issues yourself. For new clippy lints, prefer
   `cargo clippy --fix --allow-dirty --all-targets` and review the diff. Then
   re-dispatch. After 3 failed rounds, stop and report.
3. Re-run `scan.sh`. It must show 0 VULN lines everywhere. WARN lines that
   remain need an explanation for the PR body.

## 6. Commit

Match the repo's conventional-commit style (`git log --format=%s -10 develop`).
Usually `fix(deps): patch RUSTSEC-…` when vulnerabilities were fixed,
otherwise `build: update dependencies …`. The body lists each advisory ID,
crate bumps, action bumps, and anything skipped. End with the Co-Authored-By
trailer given in the session's attribution instructions.

Write the message to a file in the scratchpad dir, then
`git add -A && git commit -F <file>`.

**If commit signing fails** (`gpg failed to sign the data`, or pinentry
errors): leave everything staged, and tell the user to run, from the prompt:
```
! git commit -F "<absolute path to message file>"
```
(and `gpgconf --kill gpg-agent` first if the passphrase prompt doesn't appear).
Wait for them to confirm, verify with `git log -1`, then continue.

The cargo-husky pre-commit hook reruns fmt and clippy. If it fails, fix the
issue and commit again. Never bypass it. cargo-husky installs the hook into
`.git/hooks/` the first time `crush-core`'s tests are built, so in a fresh
clone or worktree, run the quality gate (step 5) before the first commit.
That gate already includes `cargo test`.

If `--no-pr`, stop here and report.

## 7. Pull request

1. `git push -u origin HEAD`.
2. Build the body from `.github/pull_request_template.md`. Fill the
   Description with a findings table:

   | Kind | Item | From → To | Notes |
   |---|---|---|---|
   | Advisory | RUSTSEC-YYYY-NNNN (`crate`) | 1.2.3 → 1.2.4 | floor raised in Cargo.toml |
   | Crate | `dirs` | 6 → 7 | major; API used unchanged |
   | Action | `actions/checkout` | v4 → v7 | |
   | Toolchain | rustc / MSRV | 1.97.1 → 1.99.1 | MSRV is consumer-facing |
   | Skipped | `foo` 2 → 3 | — | needs refactor of X |

   Tick "Infrastructure/tooling change" (plus "Bug fix" for advisories), and
   only the checklist items you actually verified. End with the PR attribution
   line from the session's attribution instructions.
3. `gh pr create --base develop --title "<commit subject>" --body-file <file>`.
4. If that fails with `Resource not accessible by personal access token` (403),
   the token in `GITHUB_TOKEN`/`GH_TOKEN` lacks `pull_requests=write`. A
   fine-grained PAT can push without being allowed to create PRs. Then:
   - Retry once with the user's keyring `gh` login, for that one command only:
     `env -u GITHUB_TOKEN -u GH_TOKEN gh pr create …` (same arguments). Don't
     change env vars, `gh auth` state, or config.
   - If that fails too (or `gh auth status` shows no keyring login), stop and
     give the user the compare link
     `https://github.com/<owner>/<repo>/compare/develop...<branch>?expand=1`
     plus the absolute path of the body file. Tell them the PAT needs
     "Pull requests: Read and write" on this repo. Resume at step 8 once they
     confirm the PR exists. CI runs on `pull_request`, so it won't start before then.
   - Don't try the GitHub MCP connector as a fallback. It is usually backed by
     the same PAT.
   - Report which credential created the PR.

## 8. Monitor CI and auto-fix (max 3 fix rounds, max 2 infra re-runs)

1. Find the runs for the pushed commit. Both `CI` and `Security Audit` run on PRs:
   `gh run list --branch <branch> --commit $(git rev-parse HEAD) --json databaseId,workflowName,status`.
   Runs take a few seconds to appear, so retry briefly.
2. Watch each run with Bash `run_in_background: true`:
   `gh run watch <id> --exit-status --interval 60 > /dev/null`. You are
   re-invoked when each one exits. Do not poll, and don't sleep in the foreground.
3. When every watched run exits 0, all checks are green. Go to step 9.
   `gh run view` shows the latest attempt, so after a re-run, the same
   commands look at the new attempt.
4. On failure, classify each non-successful job:
   ```
   gh run view <id> --json jobs --jq '.jobs[] | select(.conclusion != "success" and .conclusion != "skipped") | {name, conclusion, databaseId, steps: [.steps[] | select(.conclusion=="failure") | .name]}'
   gh api repos/{owner}/{repo}/check-runs/<job databaseId>/annotations --jq '.[].message'
   ```
   - **Infra**: conclusion `cancelled` or `failure` with no steps run, or
     annotations like "not acquired by Runner", "runner has received a
     shutdown signal", or "lost communication with the server". Run
     `gh run rerun <id> --failed` and watch again. This doesn't count as a fix
     round, but after 2 infra re-runs, stop and report. Jobs `skipped` only
     because an infra-failed job they depend on didn't finish are not separate
     failures.
   - **Code/config**: anything else. Dispatch the `ci-fixer` subagent
     (`subagent_type: "ci-fixer"`) with the run id, failed job names, and the
     failing step names. It edits files, verifies locally, and returns a
     summary plus a suggested commit message. It does not commit.
5. After `ci-fixer` returns:
   - "needs human": stop and report its explanation.
   - Otherwise, review `git diff`, commit with step 6's rules (subject like
     `ci: fix <job> after dependency updates`), `git push`, and go back to 8.1.
6. After the third fix round still fails, stop and report what is failing and
   what was tried.

## 9. Report

Tell the user: the PR link, advisories fixed (IDs), what was bumped, what was
skipped and why, the toolchain/MSRV change if any, and the final CI status,
including any infra re-runs.
