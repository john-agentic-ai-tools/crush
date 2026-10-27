---
name: toolchain-update
description: Update Crush's pinned Rust toolchain (rust-toolchain.toml) and MSRV (workspace rust-version) to a newer release, fix the new clippy lints, rustfmt changes, and CI breakage it causes, verify with the full quality gate, and commit. Called by security-patch when a patch needs a newer rustc; also usable directly ("update the Rust toolchain", "bump rustc", "bump MSRV").
argument-hint: "[version, default latest stable] [--from-security-patch]"
---

# Toolchain update

Arguments:
- A version such as `1.99.1`. Default: the latest stable from
  `https://static.rust-lang.org/dist/channel-rust-stable.toml`
  (`[pkg.rustc] version`).
- `--from-security-patch`: called from the security-patch skill. Work on the
  current branch, commit, and return. Skip branching, PR, and CI (the caller
  does those).

Follow the hard rules in `.claude/skills/security-patch/SKILL.md` (no gate
weakening, no signing bypass, PRs to `develop`).

## 1. Setup

- Standalone: clean worktree, branch from up-to-date `develop` as
  `NNN-toolchain-<version>`, using the numbering rule from security-patch step 1.
- Read the current pin (`rust-toolchain.toml` `channel`) and MSRV (root
  `Cargo.toml` `[workspace.package] rust-version`). If the pin already equals
  the target, report "already on <version>" and return.

## 2. Bump

1. `rust-toolchain.toml`: `channel = "<X.Y.Z>"`.
2. Root `Cargo.toml`: `rust-version = "<X.Y>"`. The comment there requires the
   MSRV to match the pin. If any crate or `*/fuzz/Cargo.toml` overrides
   `rust-version`, update it too.
3. Read the release notes for every minor in between
   (`https://blog.rust-lang.org/` and the clippy CHANGELOG) for new
   lints, newly stabilized replacements, and edition or lint-level changes.

The toolchain file makes rustup install the new version on the next `cargo`
call. Check with `rustc --version`.

## 3. Fix fallout

1. `cargo fmt --all`. rustfmt output can change between releases, so accept its changes.
2. `cargo clippy --all-targets -- -D warnings`. Each crate stops at its first
   errors, so loop:
   - `cargo clippy --fix --allow-dirty --allow-no-vcs --all-targets` for
     machine-applicable suggestions, then review `git diff -- '*.rs'`.
   - Fix the rest by hand. Add `#[allow(clippy::…)]` only when the suggestion
     makes the code worse, always with a one-line justification comment.
   - Never introduce `.unwrap()`/`.expect()`/`panic!` in non-test code
     (constitution and crate lint config).
3. `cargo check --workspace --all-features` if the CUDA toolkit is installed
   locally (`nvcc --version`). Otherwise `cargo check -p crush-gpu` and note
   in the report that CUDA-feature checks rely on CI's `CUDA Compile Check` job.
4. Scan `.github/workflows/*.yml` for version-specific settings:
   `dtolnay/rust-toolchain` with an explicit `toolchain:` version, the
   `stable`/`beta` build matrix, MSRV jobs, and `cargo +<ver>` invocations.
   Update anything pinned to the old version.

## 4. Verify

Dispatch the `quality-gate` subagent (`subagent_type: "quality-gate"`). Fix
whatever it reports and re-dispatch, up to 3 rounds. Then stop and report.

## 5. Commit

Separate commit, so the toolchain change can be reviewed or reverted on its own:

```
build: update Rust toolchain to <X.Y.Z>

- rust-toolchain <old> -> <new>; rust-version <old> -> <new> (MSRV change:
  consumers now need Rust <X.Y>)
- <new clippy lints fixed, files touched>
- <CI workflow adjustments, if any>

Co-Authored-By: <trailer from the session's attribution instructions>
```

Use security-patch step 6's signing rules (message file, `! git commit -F`
handoff on GPG failure).

## 6. Return or finish

- `--from-security-patch`: report the new version and the commit SHA, and
  return control.
- Standalone: run security-patch steps 7–9 (PR to `develop`, CI monitoring
  with auto-fix, report). Note the MSRV change prominently in the PR description.
