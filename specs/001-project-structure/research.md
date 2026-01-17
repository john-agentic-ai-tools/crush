# Research: GitHub & Open Source Standards

**Feature**: Project Structure & Open Source Foundation
**Date**: 2026-01-17
**Purpose**: Research industry standards for open source repository infrastructure

## 1. MIT License Format

### Decision

Use the standard MIT License text from the Open Source Initiative (OSI) with two placeholders:
- `[year]` - Current year (2026)
- `[fullname]` - Copyright holder (use "Crush Contributors")

### Rationale

- MIT License is the most permissive and widely adopted open source license
- GitHub automatically detects and displays the license badge when using standard format
- Maximizes adoption by allowing commercial use, modification, and distribution
- Simple, clear, and legally vetted text from OSI

### Standard Text (verified)

```
MIT License

Copyright (c) [year] [fullname]

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Alternatives Considered

- **Apache 2.0**: Rejected - more complex, includes patent grant (unnecessary for Crush)
- **GPL v3**: Rejected - copyleft restrictions incompatible with goal of maximum adoption
- **BSD 3-Clause**: Rejected - additional advertising clause adds complexity without benefit

## 2. GitHub Issue Templates

### Decision

Use **GitHub Issue Forms** (YAML format, introduced 2021) instead of legacy Markdown templates.

**Location**: `.github/ISSUE_TEMPLATE/`
**Format**: YAML files with `.yml` extension
**Required files**:
- `bug_report.yml` - Structured bug reports
- `feature_request.yml` - Structured feature requests
- `config.yml` (optional) - Configure template chooser, add links to external resources

### Rationale

- Issue forms provide structured data (dropdowns, checkboxes, required fields)
- Better user experience than free-form Markdown
- Easier to parse and triage issues programmatically
- Reduces incomplete bug reports by making fields required
- Standard across modern GitHub projects

### Bug Report Template Structure

```yaml
name: Bug Report
description: File a bug report to help us improve Crush
title: "[Bug]: "
labels: ["bug", "triage"]
body:
  - type: markdown
    attributes:
      value: |
        Thanks for taking the time to fill out this bug report!

  - type: textarea
    id: what-happened
    attributes:
      label: What happened?
      description: Clear and concise description of the bug
    validations:
      required: true

  - type: textarea
    id: reproduce
    attributes:
      label: Steps to Reproduce
      description: How can we reproduce this?
      placeholder: |
        1. Run command '...'
        2. Provide input '...'
        3. See error
    validations:
      required: true

  - type: textarea
    id: expected
    attributes:
      label: Expected Behavior
      description: What did you expect to happen?
    validations:
      required: true

  - type: textarea
    id: environment
    attributes:
      label: Environment
      description: |
        OS: [e.g., Ubuntu 22.04, macOS 14, Windows 11]
        Rust version: [e.g., 1.75.0]
        Crush version: [e.g., 0.1.0]
    validations:
      required: true
```

### Feature Request Template Structure

```yaml
name: Feature Request
description: Suggest a new feature or enhancement for Crush
title: "[Feature]: "
labels: ["enhancement"]
body:
  - type: textarea
    id: problem
    attributes:
      label: Problem Statement
      description: What problem does this feature solve?
      placeholder: I'm frustrated when...
    validations:
      required: true

  - type: textarea
    id: solution
    attributes:
      label: Proposed Solution
      description: Describe the solution you'd like
    validations:
      required: true

  - type: textarea
    id: alternatives
    attributes:
      label: Alternatives Considered
      description: Have you considered any alternative solutions?
    validations:
      required: false
```

### Alternatives Considered

- **Markdown templates**: Rejected - less structured, users often skip sections
- **Single template for all issues**: Rejected - conflates bugs and features, harder to triage
- **No templates**: Rejected - leads to incomplete and hard-to-process issues

## 3. Pull Request Template

### Decision

Single PR template at `.github/pull_request_template.md` using Markdown with checklists.

### Rationale

- Markdown format is flexible for PR descriptions
- Checklists guide contributors through required steps
- Auto-populated when PR is created
- Can reference constitution requirements (branching, testing)
- Single location (not in PULL_REQUEST_TEMPLATE directory)

### Template Structure

```markdown
## Description

<!-- Provide a clear and concise description of your changes -->

## Related Issues

<!-- Link related issues using keywords: Fixes #123, Closes #456, Relates to #789 -->

## Type of Change

- [ ] Bug fix (non-breaking change fixing an issue)
- [ ] New feature (non-breaking change adding functionality)
- [ ] Breaking change (fix or feature causing existing functionality to change)
- [ ] Documentation update
- [ ] Infrastructure/tooling change

## Testing Checklist

- [ ] I have followed the TDD approach (tests written first, approved, then implementation)
- [ ] All existing tests pass locally
- [ ] I have added tests covering my changes
- [ ] New and existing tests pass in CI

## Constitution Compliance

- [ ] Code follows Rust style guidelines (rustfmt, clippy)
- [ ] No `.unwrap()` or `.expect()` in production code
- [ ] All new public APIs are documented with examples
- [ ] Changes comply with applicable constitution principles

## Branching

- [ ] This PR targets the correct branch (develop for features, main for releases)
- [ ] I have pulled latest changes from the target branch

## Additional Context

<!-- Add any other context about the PR here -->
```

### Alternatives Considered

- **No template**: Rejected - inconsistent PR descriptions
- **Multiple templates**: Rejected - overkill for this project, single template suffices

## 4. CODEOWNERS Format

### Decision

Use `.github/CODEOWNERS` (preferred location) with glob patterns.

**Initial configuration**: Assign all files to repository owner until team grows.

### Syntax

```
# Default owner for everything
* @john-agentic-ai-tools

# Documentation (can be overridden by docs team later)
*.md @john-agentic-ai-tools

# GitHub templates and configuration
/.github/ @john-agentic-ai-tools

# Constitution changes require maintainer approval
/.specify/memory/constitution.md @john-agentic-ai-tools
```

### Rationale

- `.github/CODEOWNERS` is GitHub's preferred location (also supports root or `docs/`)
- Automatically requests reviews from owners when files are modified
- Glob patterns allow flexible ownership rules
- Can be expanded as team grows without restructuring

### Location Options Evaluated

- `.github/CODEOWNERS`: ✅ Chosen - modern standard, consistent with templates
- `CODEOWNERS` (root): Alternative - works but less organized
- `docs/CODEOWNERS`: Rejected - uncommon, unexpected location

### Alternatives Considered

- **No CODEOWNERS**: Rejected - manual reviewer assignment is error-prone
- **Fine-grained per-directory**: Rejected - premature for single maintainer project

## 5. Code of Conduct

### Decision

Create a **stub** file referencing the Contributor Covenant standard with placeholder for manual completion.

**File**: `CODE_OF_CONDUCT.md` (repository root)
**Content**: Placeholder indicating details will be provided manually

### Stub Content

```markdown
# Code of Conduct

This project follows the [Contributor Covenant](https://www.contributor-covenant.org/) Code of Conduct.

**Status**: Details to be provided manually by project maintainers.

## Placeholder

The full Code of Conduct will include:
- Our pledge to maintain a welcoming and inclusive community
- Standards for acceptable behavior
- Responsibilities of maintainers
- Reporting process for violations
- Enforcement guidelines and consequences

For now, contributors are expected to:
- Be respectful and professional in all interactions
- Welcome diverse perspectives and experiences
- Provide constructive feedback
- Focus on what is best for the community

**Note**: This is a temporary stub. Full Code of Conduct to be completed per Contributor Covenant template.
```

### Rationale

- Contributor Covenant is the industry standard (used by 100k+ projects)
- Stub acknowledges importance while allowing manual customization
- Provides minimal guidance until full version is completed
- GitHub recognizes CODE_OF_CONDUCT.md and links it in repository header

### Alternatives Considered

- **Complete Code of Conduct now**: Rejected - spec specifies manual completion for customization
- **No Code of Conduct**: Rejected - GitHub community standards expect this file
- **Different standard (Mozilla, Rust)**: Rejected - Contributor Covenant is most widely adopted

## 6. CONTRIBUTING.md Structure

### Decision

Comprehensive contributor guide covering the full development lifecycle per constitution requirements.

### Required Sections

1. **Welcome & Quick Start**
   - Project overview
   - Ways to contribute (code, docs, issues)

2. **Development Setup**
   - Rust toolchain installation (reference constitution's rust-toolchain.toml)
   - Clone and build instructions
   - Pre-commit hook setup (cargo-husky)

3. **Branching Model**
   - Git Flow explanation (main, develop, feature branches)
   - Branch naming conventions
   - Reference constitution's branching governance

4. **Commit Conventions**
   - Semantic commit messages (feat, fix, docs, etc.)
   - Co-authored-by for AI assistance
   - Examples

5. **Testing Requirements**
   - TDD approach (tests first)
   - Running tests (`cargo test`)
   - Coverage expectations (80%+)
   - Reference constitution's Test-First principle

6. **Pull Request Process**
   - Feature branch → develop (not main)
   - PR template usage
   - Code review expectations
   - CI must pass before merge

7. **Code Style**
   - `rustfmt` and `clippy` requirements
   - No `.unwrap()` in production
   - Documentation standards

### Rationale

- Single source of truth for contributors
- References constitution without duplicating it
- Aligns with SpecKit workflow
- Reduces onboarding friction

## 7. GitHub Workflows Directory

### Decision

Create empty `.github/workflows/` directory with README explaining future usage.

**Content**: Small README.md explaining directory purpose

```markdown
# GitHub Actions Workflows

This directory will contain CI/CD workflow definitions for the Crush project.

## Planned Workflows

- **CI**: Build, test, lint, and security checks on pull requests
- **Release**: Automated release builds and publishing
- **Benchmarks**: Performance regression testing

Workflow definitions will be added in future specifications.
```

### Rationale

- Establishes the location for future CI work
- GitHub recognizes empty workflows directory
- README prevents confusion about empty directory
- FR-012 specifies directory creation, not workflow implementation

## Implementation Notes

All formats and standards researched above are current as of 2026-01-17. GitHub's issue forms, CODEOWNERS, and template locations have been stable since 2021.

**Next Phase**: Use these standards to generate actual file content in Phase 1 (contracts and quickstart).
