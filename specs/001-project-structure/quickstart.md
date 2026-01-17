# Quickstart: Project Structure Verification

**Feature**: Project Structure & Open Source Foundation
**Purpose**: Step-by-step guide to verify all files and test each user story

## Overview

This quickstart provides manual verification steps for the project structure feature. Use this after implementation to confirm all files are present, correctly formatted, and recognized by GitHub.

## Prerequisites

- Implementation complete (all tasks from tasks.md done)
- Feature branch pushed to GitHub
- Access to GitHub web interface

## Verification Steps

### User Story 1: Legal Clarity (P1)

**Goal**: Verify MIT License is present and recognized by GitHub

**Steps**:
1. Navigate to repository root on GitHub: `https://github.com/john-agentic-ai-tools/crush`
2. Look for `LICENSE` file in file list
3. Click on `LICENSE` file
4. Verify contents match MIT License text with:
   - Copyright (c) 2026 Crush Contributors
   - Full MIT License text from OSI
5. Return to repository main page
6. Check repository header (top-right area) for license badge
7. Verify badge displays "MIT License"

**Expected Results**:
- ✅ LICENSE file exists at root
- ✅ Contents are valid MIT License text
- ✅ GitHub displays "MIT License" badge
- ✅ Badge links to LICENSE file

**If Failed**:
- Check LICENSE file has exact OSI text (no modifications)
- Verify no extra whitespace or characters
- Ensure file is named `LICENSE` (all caps, no extension)

---

### User Story 2: Contributor Onboarding (P2)

**Goal**: Verify contribution guidelines and templates work correctly

#### Part A: CONTRIBUTING.md

**Steps**:
1. From repository root, locate `CONTRIBUTING.md`
2. Click to open the file
3. Verify all required sections are present:
   - [ ] Welcome & project overview
   - [ ] Development Setup
   - [ ] Branching Model (Git Flow)
   - [ ] Commit Conventions
   - [ ] Testing Requirements
   - [ ] Pull Request Process
   - [ ] Code Style
4. Check that constitution is referenced:
   - [ ] Git Flow branching mentioned
   - [ ] TDD requirement mentioned
   - [ ] rust-toolchain.toml mentioned
   - [ ] cargo-husky pre-commit hooks mentioned

**Expected Results**:
- ✅ File exists at root
- ✅ All 7 sections present
- ✅ Constitution governance referenced
- ✅ File renders correctly as Markdown

**Timing Test** (SC-001):
1. Have a new user (or clear browser cache) land on repository
2. Time how long it takes to locate CONTRIBUTING.md
3. Goal: < 30 seconds

#### Part B: Issue Templates

**Steps**:
1. Click "Issues" tab in repository
2. Click "New Issue" button
3. Verify template chooser appears with options:
   - [ ] Bug Report
   - [ ] Feature Request
4. Click "Get started" for Bug Report
5. Verify form has required fields:
   - [ ] What happened?
   - [ ] Steps to Reproduce
   - [ ] Expected Behavior
   - [ ] Environment
6. Verify fields are marked as required
7. Go back and test Feature Request template:
   - [ ] Problem Statement (required)
   - [ ] Proposed Solution (required)
   - [ ] Alternatives Considered (optional)

**Expected Results**:
- ✅ Template chooser displays both options
- ✅ Bug report has 4 required fields
- ✅ Feature request has 2 required, 1 optional field
- ✅ Forms render as structured input (not plain text)

**Create Test Issue**:
1. Fill out a test bug report
2. Submit issue
3. Verify issue is created with structured data
4. Delete test issue (or mark it as test and close)

#### Part C: Pull Request Template

**Steps**:
1. Create a test branch: `git checkout -b test-pr-template`
2. Make a small change (e.g., add comment to CLAUDE.md)
3. Commit and push: `git push -u origin test-pr-template`
4. Navigate to repository on GitHub
5. Click "Pull requests" tab
6. Click "New pull request"
7. Select `test-pr-template` as compare branch
8. Click "Create pull request"
9. Verify PR description is auto-populated with template
10. Check all sections are present:
    - [ ] Description
    - [ ] Related Issues
    - [ ] Type of Change (checkboxes)
    - [ ] Testing Checklist
    - [ ] Constitution Compliance
    - [ ] Branching
    - [ ] Additional Context

**Expected Results**:
- ✅ Template auto-fills PR description
- ✅ All sections and checklists present
- ✅ Checkboxes render correctly

**Cleanup**:
- Close test PR without merging
- Delete test branch: `git branch -D test-pr-template` and `git push origin --delete test-pr-template`

---

### User Story 3: Community Standards (P3)

**Goal**: Verify Code of Conduct stub and CODEOWNERS

#### Part A: Code of Conduct

**Steps**:
1. From repository root, locate `CODE_OF_CONDUCT.md`
2. Open the file
3. Verify it's clearly marked as a stub/placeholder
4. Check it references Contributor Covenant
5. Return to repository main page
6. Look for "Code of conduct" link in repository header (near About section)

**Expected Results**:
- ✅ File exists at root
- ✅ Marked as stub for manual completion
- ✅ References Contributor Covenant
- ✅ GitHub recognizes file and links it

#### Part B: CODEOWNERS

**Steps**:
1. Navigate to `.github/CODEOWNERS` file
2. Open and verify syntax:
   - `* @john-agentic-ai-tools` (all files)
   - `/.github/ @john-agentic-ai-tools`
   - `/.specify/memory/constitution.md @john-agentic-ai-tools`
3. Create test scenario:
   - Make a small change to any file in a fork or test branch
   - Create a PR
   - Verify that `@john-agentic-ai-tools` is automatically requested as reviewer

**Expected Results**:
- ✅ CODEOWNERS file exists at `.github/CODEOWNERS`
- ✅ Valid syntax (no errors)
- ✅ GitHub automatically requests reviews based on ownership rules

---

### User Story 4: CI/CD Infrastructure (P4)

**Goal**: Verify workflows directory exists and is ready

**Steps**:
1. Navigate to `.github/workflows/` directory
2. Verify directory exists (may be empty or contain README.md)
3. If README.md present, verify it explains future usage
4. Click "Actions" tab in repository
5. Verify GitHub recognizes workflows directory (even if empty)

**Expected Results**:
- ✅ `.github/workflows/` directory exists
- ✅ Optional README.md explains planned workflows
- ✅ GitHub Actions tab accessible
- ✅ Ready to receive workflow YAML files in future

---

## GitHub Community Standards Check

GitHub provides a built-in community standards checklist. Verify our repository passes.

**Steps**:
1. Navigate to repository "Insights" tab
2. Click "Community" in left sidebar
3. Review the checklist:
   - [ ] Description ✅ (set in repository settings)
   - [ ] README ✅ (CLAUDE.md serves this purpose)
   - [ ] Code of conduct ✅ (should show checkmark)
   - [ ] Contributing ✅ (should show checkmark)
   - [ ] License ✅ (should show checkmark)
   - [ ] Issue templates ✅ (should show checkmark)
   - [ ] Pull request template ✅ (should show checkmark)

**Expected Results**:
- ✅ All items have checkmarks (green ✓)
- ✅ "100% complete" or similar indicator

---

## Success Criteria Verification

Map each success criterion to verification steps above:

| ID | Success Criterion | Verification |
|----|-------------------|--------------|
| SC-001 | Contributors find guidelines < 30s | Timing test in User Story 2A |
| SC-002 | GitHub displays MIT License badge | User Story 1, step 6-7 |
| SC-003 | 100% issues use templates | User Story 2B - template chooser forces selection |
| SC-004 | 100% PRs use template | User Story 2C - template auto-populates |
| SC-005 | Workflows directory exists | User Story 4, steps 1-2 |
| SC-006 | CODEOWNERS requests reviews | User Story 3B, test PR scenario |
| SC-007 | Community standards pass | GitHub Community Standards Check |

---

## Troubleshooting

### License Badge Not Showing

**Problem**: GitHub doesn't display "MIT License" badge
**Solutions**:
- Verify LICENSE file is at root (not in subdirectory)
- Check exact filename: `LICENSE` (no `.txt` or `.md`)
- Ensure MIT License text is exact OSI standard
- Wait 5-10 minutes for GitHub to index changes
- Force refresh: push a small commit to trigger re-indexing

### Issue Templates Not Appearing

**Problem**: Template chooser doesn't show options
**Solutions**:
- Verify files are in `.github/ISSUE_TEMPLATE/` directory
- Check YAML syntax (use YAML validator)
- Ensure files have `.yml` extension (not `.yaml`)
- Verify `name:` and `description:` fields are present
- Push to default branch (templates may not work on feature branches)

### PR Template Not Auto-Populating

**Problem**: Creating PR doesn't show template
**Solutions**:
- Verify file is at `.github/pull_request_template.md`
- Check it's Markdown format
- Ensure file is on target branch (develop or main)
- Try closing and re-opening PR creation page
- Check for syntax errors in Markdown

### CODEOWNERS Not Requesting Review

**Problem**: PRs don't auto-assign reviewers
**Solutions**:
- Verify username in CODEOWNERS exists: `@john-agentic-ai-tools`
- Check file is at `.github/CODEOWNERS` (preferred location)
- Ensure glob patterns are correct (`*` for all files)
- CODEOWNERS must be on base branch (not feature branch)
- Test with a PR that actually modifies a covered file

---

## Final Verification Checklist

After completing all steps above:

- [ ] LICENSE file present and recognized by GitHub
- [ ] CONTRIBUTING.md exists with all 7 sections
- [ ] CODE_OF_CONDUCT.md stub exists
- [ ] CODEOWNERS file exists with valid syntax
- [ ] Bug report template renders as form
- [ ] Feature request template renders as form
- [ ] PR template auto-populates descriptions
- [ ] .github/workflows/ directory exists
- [ ] GitHub Community Standards shows 100% or all checkmarks
- [ ] All success criteria verified

---

## Next Steps

After successful verification:
1. Merge feature branch to develop via PR
2. Verify templates work on develop branch
3. Test full workflow: fork → change → issue → PR → review
4. Document any edge cases discovered during testing
5. Ready for next feature implementation

This completes the Project Structure & Open Source Foundation feature verification.
