---
description: Perform a non-destructive cross-artifact consistency and quality analysis across spec.md, plan.md, and tasks.md after task generation.
---

## User Input

```text
$ARGUMENTS
```

You **MUST** consider the user input before proceeding (if not empty).

## Goal

Identify inconsistencies, duplications, ambiguities, and underspecified items across the three core artifacts (`spec.md`, `plan.md`, `tasks.md`) before implementation. This command MUST run only after `/speckit.tasks` has successfully produced a complete `tasks.md`.

## Operating Constraints

**STRICTLY READ-ONLY**: Do **not** modify any files. Output a structured analysis report. Offer an optional remediation plan (user must explicitly approve before any follow-up editing commands would be invoked manually).

**Constitution Authority**: The project constitution (`.specify/memory/constitution.md`) is **non-negotiable** within this analysis scope. Constitution conflicts are automatically CRITICAL and require adjustment of the spec, plan, or tasks—not dilution, reinterpretation, or silent ignoring of the principle. If a principle itself needs to change, that must occur in a separate, explicit constitution update outside `/speckit.analyze`.

## Execution Steps

### 1. Initialize Analysis Context

1.  Run `.specify/scripts/powershell/check-prerequisites.ps1 -Json -RequireTasks -IncludeTasks` from repo root.
    -   Parse the JSON output to get `FEATURE_DIR`. If `Success` is false or `FEATURE_DIR` is missing, report the error messages and exit.
2.  Run `.specify/scripts/powershell/get-spec-data.ps1 -FeatureDir "$FEATURE_DIR" -Json` from repo root.
    -   Parse the JSON output into a variable named `$SPEC_DATA`.
    -   If `$SPEC_DATA.Success` is false, report the error messages from `$SPEC_DATA.Messages` and exit.
    -   The `$SPEC_DATA` object now contains all structured information from `spec.md`, `plan.md`, `tasks.md`, and `constitution.md`.

### 2. Use Structured Artifact Data

The `$SPEC_DATA` object from Step 1 contains all necessary structured information from `spec.md`, `plan.md`, `tasks.md`, and `constitution.md`. Refer to its properties directly for analysis.

### 3. Build Semantic Models

Create internal representations using the `$SPEC_DATA` object:

-   **Requirements inventory**: From `$SPEC_DATA.Spec.FunctionalRequirements` and `$SPEC_DATA.Spec.NonFunctionalRequirements`. Derive a stable key for each (e.g., "User can upload file" → `user-can-upload-file`).
-   **User story/action inventory**: From `$SPEC_DATA.Spec.UserStories`.
-   **Task coverage mapping**: Map tasks from `$SPEC_DATA.Tasks.Tasks` to requirements/stories.
-   **Constitution rule set**: From `$SPEC_DATA.Constitution.Principles`.

### 4. Detection Passes (Token-Efficient Analysis)

Focus on high-signal findings based on the `$SPEC_DATA` object. Limit to 50 findings total; aggregate remainder in overflow summary.

#### A. Duplication Detection

- Identify near-duplicate requirements within `$SPEC_DATA.Spec.FunctionalRequirements` and `$SPEC_DATA.Spec.NonFunctionalRequirements`.
- Mark lower-quality phrasing for consolidation.

#### B. Ambiguity Detection

- Flag vague adjectives (fast, scalable, secure, intuitive, robust) lacking measurable criteria within requirement descriptions.
- Flag unresolved placeholders (TODO, TKTK, ???, `<placeholder>`, etc.) across all text fields in `$SPEC_DATA`.

#### C. Underspecification

- Requirements from `$SPEC_DATA.Spec` with verbs but missing object or measurable outcome.
- User stories from `$SPEC_DATA.Spec.UserStories` missing acceptance criteria alignment.
- Tasks from `$SPEC_DATA.Tasks.Tasks` referencing files or components not defined in `$SPEC_DATA.Spec` or `$SPEC_DATA.Plan`.

#### D. Constitution Alignment

- Any requirement or plan element from `$SPEC_DATA.Spec` or `$SPEC_DATA.Plan` conflicting with a MUST principle in `$SPEC_DATA.Constitution.Principles`.
- Missing mandated sections or quality gates from `$SPEC_DATA.Constitution`.

#### E. Coverage Gaps

- Requirements from `$SPEC_DATA.Spec` with zero associated tasks from `$SPEC_DATA.Tasks`.
- Tasks from `$SPEC_DATA.Tasks` with no mapped requirement/story from `$SPEC_DATA.Spec`.
- Non-functional requirements from `$SPEC_DATA.Spec` not reflected in tasks from `$SPEC_DATA.Tasks` (e.g., performance, security).

#### F. Inconsistency

- Terminology drift (same concept named differently across `$SPEC_DATA.Spec` and `$SPEC_DATA.Plan`).
- Data entities referenced in `$SPEC_DATA.Plan` but absent in `$SPEC_DATA.Spec` (or vice versa).
- Task ordering contradictions within `$SPEC_DATA.Tasks` (e.g., integration tasks before foundational setup tasks without dependency note).
- Conflicting requirements within `$SPEC_DATA.Spec` (e.g., one requires Next.js while other specifies Vue).

### 5. Severity Assignment

Use this heuristic to prioritize findings:

- **CRITICAL**: Violates constitution MUST, missing core spec artifact, or requirement with zero coverage that blocks baseline functionality
- **HIGH**: Duplicate or conflicting requirement, ambiguous security/performance attribute, untestable acceptance criterion
- **MEDIUM**: Terminology drift, missing non-functional task coverage, underspecified edge case
- **LOW**: Style/wording improvements, minor redundancy not affecting execution order

### 6. Produce Compact Analysis Report

Output a Markdown report (no file writes) with the following structure:

## Specification Analysis Report

| ID | Category | Severity | Location(s) | Summary | Recommendation |
|----|----------|----------|-------------|---------|----------------|
| A1 | Duplication | HIGH | spec.md:L120-134 | Two similar requirements ... | Merge phrasing; keep clearer version |

(Add one row per finding; generate stable IDs prefixed by category initial.)

**Coverage Summary Table:**

| Requirement Key | Has Task? | Task IDs | Notes |
|-----------------|-----------|----------|-------|

**Constitution Alignment Issues:** (if any)

**Unmapped Tasks:** (if any)

**Metrics:**

- Total Requirements
- Total Tasks
- Coverage % (requirements with >=1 task)
- Ambiguity Count
- Duplication Count
- Critical Issues Count

### 7. Provide Next Actions

At end of report, output a concise Next Actions block:

- If CRITICAL issues exist: Recommend resolving before `/speckit.implement`
- If only LOW/MEDIUM: User may proceed, but provide improvement suggestions
- Provide explicit command suggestions: e.g., "Run /speckit.specify with refinement", "Run /speckit.plan to adjust architecture", "Manually edit tasks.md to add coverage for 'performance-metrics'"

### 8. Offer Remediation

Ask the user: "Would you like me to suggest concrete remediation edits for the top N issues?" (Do NOT apply them automatically.)

## Operating Principles

### Context Efficiency

- **Minimal high-signal tokens**: Focus on actionable findings, not exhaustive documentation
- **Progressive disclosure**: Load artifacts incrementally; don't dump all content into analysis
- **Token-efficient output**: Limit findings table to 50 rows; summarize overflow
- **Deterministic results**: Rerunning without changes should produce consistent IDs and counts

### Analysis Guidelines

- **NEVER modify files** (this is read-only analysis)
- **NEVER hallucinate missing sections** (if absent, report them accurately)
- **Prioritize constitution violations** (these are always CRITICAL)
- **Use examples over exhaustive rules** (cite specific instances, not generic patterns)
- **Report zero issues gracefully** (emit success report with coverage statistics)

## Context

$ARGUMENTS
