# File Locations Contract

**Feature**: Project Structure & Open Source Foundation
**Purpose**: Define exact file paths, formats, and required content for repository infrastructure

## Contract: Repository Files

This contract specifies all files that MUST be created, their exact locations, formats, and content requirements.

### LICENSE

**Path**: `LICENSE` (repository root)
**Format**: Plain text
**Required Content**:
- Standard MIT License text from OSI
- Copyright year: 2026
- Copyright holder: "Crush Contributors"

**Validation**:
- File exists at root
- Contains exact MIT License text
- GitHub detects and displays "MIT License" badge

---

### CONTRIBUTING.md

**Path**: `CONTRIBUTING.md` (repository root)
**Format**: Markdown

**Required Sections**:
1. **Welcome** - Project overview and contribution opportunities
2. **Development Setup** - Rust toolchain, clone, build, pre-commit hooks
3. **Branching Model** - Git Flow (main/develop), feature branch naming
4. **Commit Conventions** - Semantic messages, co-authored-by format
5. **Testing Requirements** - TDD approach, running tests, coverage expectations
6. **Pull Request Process** - Branch targets, template usage, review expectations
7. **Code Style** - rustfmt, clippy, no unwrap, documentation

**Constitution References**:
- MUST reference Git Flow branching governance
- MUST reference TDD requirement
- MUST reference rust-toolchain.toml pinning
- MUST reference pre-commit hook setup (cargo-husky)

**Validation**:
- File exists at root
- All 7 sections present
- Constitution references included
- New contributors can find it in < 30 seconds

---

### CODE_OF_CONDUCT.md

**Path**: `CODE_OF_CONDUCT.md` (repository root)
**Format**: Markdown

**Required Content**:
- Stub indicating manual completion required
- Reference to Contributor Covenant standard
- Placeholder listing what full CoC will include
- Temporary behavioral expectations

**Validation**:
- File exists at root
- Clearly marked as stub/placeholder
- GitHub recognizes file in repository header

---

### .github/CODEOWNERS

**Path**: `.github/CODEOWNERS` (preferred) or `CODEOWNERS` (root)
**Format**: GitHub CODEOWNERS syntax

**Initial Content**:
```
# Default owner for all files
* @john-agentic-ai-tools

# GitHub configuration
/.github/ @john-agentic-ai-tools

# Constitution changes
/.specify/memory/constitution.md @john-agentic-ai-tools
```

**Validation**:
- File exists at .github/CODEOWNERS
- Valid GitHub CODEOWNERS syntax
- GitHub recognizes file and requests reviews on PRs

---

### .github/pull_request_template.md

**Path**: `.github/pull_request_template.md`
**Format**: Markdown with checkboxes

**Required Sections**:
1. **Description** - Clear explanation of changes
2. **Related Issues** - Links using Fixes/Closes/Relates keywords
3. **Type of Change** - Bug fix, feature, breaking change, docs, infra
4. **Testing Checklist** - TDD approach, tests pass, coverage
5. **Constitution Compliance** - Style, no unwrap, documentation, principles
6. **Branching** - Correct target branch, latest changes pulled
7. **Additional Context** - Optional extra information

**Validation**:
- File exists at specified path
- GitHub auto-populates PR description with template
- All checklist sections present
- 100% of new PRs use template

---

### .github/ISSUE_TEMPLATE/bug_report.yml

**Path**: `.github/ISSUE_TEMPLATE/bug_report.yml`
**Format**: GitHub Issue Form (YAML)

**Required Fields**:
```yaml
name: Bug Report
description: File a bug report
title: "[Bug]: "
labels: ["bug", "triage"]
body:
  - type: textarea (What happened?) - REQUIRED
  - type: textarea (Steps to Reproduce) - REQUIRED
  - type: textarea (Expected Behavior) - REQUIRED
  - type: textarea (Environment: OS, Rust, Crush version) - REQUIRED
```

**Validation**:
- File exists at path
- Valid YAML syntax
- GitHub renders as structured form
- All required fields present
- Users see template when creating bug report issue

---

### .github/ISSUE_TEMPLATE/feature_request.yml

**Path**: `.github/ISSUE_TEMPLATE/feature_request.yml`
**Format**: GitHub Issue Form (YAML)

**Required Fields**:
```yaml
name: Feature Request
description: Suggest a new feature
title: "[Feature]: "
labels: ["enhancement"]
body:
  - type: textarea (Problem Statement) - REQUIRED
  - type: textarea (Proposed Solution) - REQUIRED
  - type: textarea (Alternatives Considered) - OPTIONAL
```

**Validation**:
- File exists at path
- Valid YAML syntax
- GitHub renders as structured form
- Required fields enforced
- Users see template when creating feature request issue

---

### .github/ISSUE_TEMPLATE/config.yml (Optional)

**Path**: `.github/ISSUE_TEMPLATE/config.yml`
**Format**: YAML
**Purpose**: Configure issue template chooser

**Content** (if created):
```yaml
blank_issues_enabled: false
contact_links:
  - name: Community Support
    url: https://github.com/john-agentic-ai-tools/crush/discussions
    about: Ask questions and discuss ideas
```

**Validation**:
- If file exists, valid YAML
- GitHub template chooser reflects configuration

---

### .github/workflows/

**Path**: `.github/workflows/` (directory)
**Type**: Directory
**Content**: Empty (with optional README.md)

**Optional README.md**:
```markdown
# GitHub Actions Workflows

Workflow definitions will be added in future specs.

Planned:
- CI (build, test, lint, security)
- Release (automated publishing)
- Benchmarks (performance regression)
```

**Validation**:
- Directory exists
- GitHub recognizes as workflows location
- Ready to receive workflow YAML files

---

## Directory Structure Summary

```
Repository Root:
├── LICENSE                              # MIT License
├── CONTRIBUTING.md                      # Contribution guide
├── CODE_OF_CONDUCT.md                   # Community standards stub
└── .github/
    ├── CODEOWNERS                       # Code ownership
    ├── pull_request_template.md         # PR template
    ├── ISSUE_TEMPLATE/
    │   ├── bug_report.yml               # Bug report form
    │   ├── feature_request.yml          # Feature request form
    │   └── config.yml (optional)        # Template chooser config
    └── workflows/                       # CI/CD workflows (empty)
        └── README.md (optional)         # Explains future usage
```

## Success Criteria Mapping

| Success Criterion | Files That Satisfy It |
|-------------------|------------------------|
| SC-001: Contributors find guidelines in < 30s | CONTRIBUTING.md at root |
| SC-002: GitHub displays "MIT License" | LICENSE with standard text |
| SC-003: 100% of issues use templates | bug_report.yml, feature_request.yml |
| SC-004: 100% of PRs use template | pull_request_template.md |
| SC-005: Workflows directory exists | .github/workflows/ |
| SC-006: CODEOWNERS requests reviews | .github/CODEOWNERS |
| SC-007: Community standards pass | All files present, correct locations |

## Implementation Validation Checklist

After implementation, verify:
- [ ] All files exist at specified paths
- [ ] All files contain required content/sections
- [ ] GitHub UI recognizes and displays all files correctly
- [ ] License badge appears in repository header
- [ ] Issue template chooser shows both bug and feature options
- [ ] PR template auto-populates when creating PR
- [ ] CODEOWNERS requests review from specified owner
- [ ] All files use correct format (Markdown, YAML, plain text)
- [ ] No syntax errors in YAML files
- [ ] Constitution references present in CONTRIBUTING.md

This contract serves as the specification for implementation tasks.
