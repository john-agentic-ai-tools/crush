# Implementation Plan: Project Structure & Open Source Foundation

**Branch**: `001-project-structure` | **Date**: 2026-01-17 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-project-structure/spec.md`

## Summary

Create the foundational open source project structure for Crush, including MIT License, contribution guidelines, community standards, code ownership, and GitHub integrations (issue/PR templates, Actions directory). This establishes legal clarity, contributor onboarding pathways, and CI/CD infrastructure preparation without implementing actual build workflows.

## Technical Context

**Language/Version**: N/A (Documentation and directory structure only)
**Primary Dependencies**: None (Static files - Markdown, YAML)
**Storage**: Git repository filesystem
**Testing**: Manual verification of file presence, GitHub UI recognition, template rendering
**Target Platform**: GitHub.com (web interface and git repository)
**Project Type**: Repository infrastructure (not source code)
**Performance Goals**: Instant file loading, sub-second GitHub template rendering
**Constraints**: Must follow GitHub's expected file locations and formats
**Scale/Scope**: 10+ documentation/config files across 3 directories (.github/, root, .github/ISSUE_TEMPLATE/)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Evaluation

This feature creates repository infrastructure and documentation, not production code. Evaluating constitution compliance:

**I. Performance First (NON-NEGOTIABLE)**: ✅ **PASS**
- Not applicable - no code or algorithms involved
- Files are static Markdown/YAML with negligible performance impact

**II. Correctness & Safety (NON-NEGOTIABLE)**: ✅ **PASS**
- Not applicable - no Rust code, no memory management
- Documentation correctness verified through manual review

**III. Modularity & Extensibility (NON-NEGOTIABLE)**: ✅ **PASS**
- Not applicable - infrastructure files, not plugin architecture
- Templates are modular (separate files for bugs vs features)

**IV. Test-First Development (NON-NEGOTIABLE)**: ⚠️ **MODIFIED INTERPRETATION**
- TDD not applicable to static documentation files
- Testing approach: Verification checklist for file presence and GitHub recognition
- Acceptance criteria defined in spec serve as "tests"

**Dependency Management**: ✅ **PASS**
- No dependencies added (static files only)

**Quality Gates**: ⚠️ **PARTIAL**
- Applicable gates: SpecKit task checklist complete
- Not applicable: cargo test, clippy, coverage, benchmarks, fuzz, miri (no Rust code)
- Git/GitHub validation will verify templates render correctly

**Development Toolchain**: ✅ **PASS**
- rust-toolchain.toml and cargo-husky will be created by future features
- This feature does NOT create them (constitution setup is separate)

**Branching & Merge Governance**: ✅ **PASS**
- Feature branch 001-project-structure created from develop
- Will merge via PR with review

**CI Enforcement**: ⚠️ **DEFERRED**
- .github/workflows/ directory created but empty
- Actual CI workflows defined in future spec per FR-012

**Release & Compatibility Policy**: ✅ **PASS**
- Documentation has no backward compatibility concerns
- Changes to templates can be made freely on feature branches

**AI Agent Behavior Guidance**: ✅ **PASS**
- No compatibility layers needed for documentation

### Gate Decision: ✅ **PROCEED**

All applicable constitution principles pass. TDD and CI gates are modified for documentation-only feature. No violations require justification.

## Project Structure

### Documentation (this feature)

```text
specs/001-project-structure/
├── plan.md              # This file
├── research.md          # Phase 0: GitHub standards research
├── quickstart.md        # Phase 1: Contributor onboarding guide
├── contracts/           # Phase 1: File structure contracts
│   └── file-locations.md
└── tasks.md             # Phase 2: NOT created by /speckit.plan
```

### Source Code (repository root)

This feature creates repository infrastructure, not source code. The deliverables are:

```text
Repository Root:
├── LICENSE              # MIT License text
├── CONTRIBUTING.md      # Contribution guidelines
├── CODE_OF_CONDUCT.md   # Community standards (stub)
└── .github/
    ├── CODEOWNERS       # Code ownership mapping
    ├── pull_request_template.md
    ├── ISSUE_TEMPLATE/
    │   ├── bug_report.yml
    │   ├── feature_request.yml
    │   └── config.yml (optional - template chooser config)
    └── workflows/       # Empty directory for future CI/CD
```

**Structure Decision**: Repository infrastructure layout following GitHub's conventions. All files are documentation (Markdown) or configuration (YAML) with no executable code.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

N/A - No constitution violations. All applicable gates pass.

## Phase 0: Research & Standards

### Research Topics

1. **MIT License Format**
   - Obtain canonical MIT License text
   - Identify required placeholders (year, copyright holder)
   - Verify GitHub's license detection requirements

2. **GitHub Issue Template Best Practices**
   - Research GitHub's issue forms (YAML format) vs. legacy Markdown templates
   - Identify required fields for bug reports and feature requests
   - Understand template chooser configuration

3. **GitHub Pull Request Template Standards**
   - Common sections in PR templates (description, testing, related issues)
   - Checkbox format for PR checklists
   - Integration with constitution's branching requirements

4. **CODEOWNERS Syntax**
   - GitHub CODEOWNERS file format and location options
   - Pattern matching rules for file paths
   - Multiple owner assignment syntax

5. **Contributor Covenant (for CODE_OF_CONDUCT stub)**
   - Identify placeholder text that indicates manual completion
   - Reference standard (Contributor Covenant) for future completion

### Research Deliverable

`research.md` will document:
- **Decision**: File formats, locations, and content standards chosen
- **Rationale**: Why these standards (GitHub recognition, community expectations)
- **Alternatives**: Other formats/locations considered and rejected

## Phase 1: Design & Contracts

### File Structure Contract

`contracts/file-locations.md` will define:

**File Locations Contract:**
```yaml
FILES:
  - path: LICENSE
    format: Plain text
    required_content: MIT License text with [YEAR] and [COPYRIGHT_HOLDER] placeholders

  - path: CONTRIBUTING.md
    format: Markdown
    sections:
      - Development Setup
      - Branching Model (Git Flow)
      - Commit Conventions
      - Testing Requirements
      - Pull Request Process

  - path: CODE_OF_CONDUCT.md
    format: Markdown
    content: Stub with placeholder indicating manual completion

  - path: .github/CODEOWNERS
    format: GitHub CODEOWNERS
    pattern: "* @owner-username"

  - path: .github/pull_request_template.md
    format: Markdown
    sections:
      - Description
      - Testing Checklist
      - Related Issues

  - path: .github/ISSUE_TEMPLATE/bug_report.yml
    format: GitHub Issue Form (YAML)
    fields:
      - Bug description
      - Steps to reproduce
      - Expected vs actual behavior
      - Environment details

  - path: .github/ISSUE_TEMPLATE/feature_request.yml
    format: GitHub Issue Form (YAML)
    fields:
      - Feature description
      - Use case / motivation
      - Proposed solution

  - path: .github/workflows/
    type: Directory
    content: Empty (workflows defined in future spec)
```

### Quickstart Guide

`quickstart.md` will provide:
- Step-by-step verification of all files created
- GitHub UI checks (license badge, template rendering)
- Manual testing scenarios for each user story
- Validation checklist matching success criteria

## Phase 1: Agent Context Update

Run `.specify/scripts/powershell/update-agent-context.ps1 -AgentType claude` to update agent context with:
- GitHub template standards
- MIT License format
- CODEOWNERS syntax
- Markdown documentation conventions

## Post-Design Constitution Re-Check

After Phase 1 design, re-evaluate constitution compliance:

- ✅ No new dependencies added
- ✅ No code requiring tests/benchmarks/linting
- ✅ Documentation follows Git Flow (feature branch → develop → main)
- ✅ Changes will go through PR process
- ✅ No compatibility concerns for static files

**Final Gate Decision**: ✅ **APPROVED FOR IMPLEMENTATION**

All constitution gates remain passing. Ready to proceed to `/speckit.tasks` for task breakdown.
