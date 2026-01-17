# Tasks: Project Structure & Open Source Foundation

**Input**: Design documents from `/specs/001-project-structure/`
**Prerequisites**: plan.md, spec.md, contracts/file-locations.md, research.md, quickstart.md

**Tests**: This feature does not require automated tests. Validation is performed through manual verification using quickstart.md.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

## Path Conventions

- Repository root for documentation files
- `.github/` directory for GitHub-specific configuration
- `.github/ISSUE_TEMPLATE/` for issue templates
- `.github/workflows/` for future CI/CD workflows

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create directory structure for GitHub templates

- [ ] T001 Create .github directory at repository root
- [ ] T002 Create .github/ISSUE_TEMPLATE directory
- [ ] T003 Create .github/workflows directory

---

## Phase 2: User Story 1 - Legal Clarity (Priority: P1) 🎯 MVP

**Goal**: Provide MIT License for legal clarity on usage and distribution

**Independent Test**: Verify LICENSE file exists at root, contains valid MIT License text with "Crush Contributors" and 2026, and GitHub displays "MIT License" badge in repository header

### Implementation for User Story 1

- [ ] T004 [US1] Create LICENSE file at repository root with standard MIT License text (Copyright (c) 2026 Crush Contributors)

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently. Verify LICENSE file exists and GitHub recognizes it.

---

## Phase 3: User Story 2 - Contributor Onboarding (Priority: P2)

**Goal**: Provide comprehensive contribution guidelines and GitHub templates for seamless contributor experience

**Independent Test**: New contributor can find CONTRIBUTING.md in < 30s, issue templates appear when creating issues, PR template auto-populates when creating PRs

### Implementation for User Story 2

- [ ] T005 [P] [US2] Create CONTRIBUTING.md at repository root with all 7 required sections (Welcome, Development Setup, Branching Model, Commit Conventions, Testing Requirements, PR Process, Code Style) and constitution references
- [ ] T006 [P] [US2] Create .github/ISSUE_TEMPLATE/bug_report.yml with GitHub Issue Form format including required fields (What happened, Steps to Reproduce, Expected Behavior, Environment)
- [ ] T007 [P] [US2] Create .github/ISSUE_TEMPLATE/feature_request.yml with GitHub Issue Form format including required fields (Problem Statement, Proposed Solution, Alternatives Considered)
- [ ] T008 [P] [US2] Create .github/pull_request_template.md with all 7 required sections (Description, Related Issues, Type of Change, Testing Checklist, Constitution Compliance, Branching, Additional Context)

**Checkpoint**: At this point, User Story 2 should be fully functional. Verify CONTRIBUTING.md exists, issue templates render in GitHub UI, and PR template auto-populates.

---

## Phase 4: User Story 3 - Community Standards (Priority: P3)

**Goal**: Establish community behavioral standards and code ownership for automated reviewer assignment

**Independent Test**: CODE_OF_CONDUCT.md stub exists and is clearly marked as placeholder, CODEOWNERS file exists with valid syntax and GitHub auto-assigns reviewers on PRs

### Implementation for User Story 3

- [ ] T009 [P] [US3] Create CODE_OF_CONDUCT.md stub at repository root with placeholder text referencing Contributor Covenant and indicating manual completion required
- [ ] T010 [P] [US3] Create .github/CODEOWNERS file with initial ownership rules (all files to @john-agentic-ai-tools, .github/ to @john-agentic-ai-tools, constitution.md to @john-agentic-ai-tools)

**Checkpoint**: At this point, User Story 3 should be fully functional. Verify CODE_OF_CONDUCT.md is clearly a stub and CODEOWNERS file has valid syntax.

---

## Phase 5: User Story 4 - CI/CD Infrastructure (Priority: P4)

**Goal**: Prepare directory structure for future GitHub Actions workflows

**Independent Test**: .github/workflows/ directory exists and is recognized by GitHub Actions tab

### Implementation for User Story 4

- [ ] T011 [US4] Create .github/workflows/README.md explaining the directory purpose and listing planned workflows (CI, Release, Benchmarks) with note that workflow definitions will be added in future specs

**Checkpoint**: At this point, User Story 4 should be fully functional. Verify workflows directory exists and README explains future usage.

---

## Phase 6: Polish & Verification

**Purpose**: Validate all files and test GitHub recognition

- [ ] T012 [P] Verify all files exist at correct paths per contracts/file-locations.md
- [ ] T013 [P] Verify all Markdown files render correctly on GitHub
- [ ] T014 [P] Verify all YAML files have valid syntax (use YAML validator or GitHub's template preview)
- [ ] T015 Test LICENSE file: Check GitHub displays "MIT License" badge in repository header
- [ ] T016 Test CONTRIBUTING.md: Verify new contributors can find it in < 30 seconds from landing on repository
- [ ] T017 Test issue templates: Create test bug report issue, verify structured form renders, delete test issue
- [ ] T018 Test issue templates: Create test feature request issue, verify structured form renders, delete test issue
- [ ] T019 Test PR template: Create test PR from temporary branch, verify template auto-populates, close without merging, delete test branch
- [ ] T020 Test CODEOWNERS: Create test PR modifying a file, verify @john-agentic-ai-tools is auto-requested as reviewer, close test PR
- [ ] T021 Test workflows directory: Verify .github/workflows/ is recognized by GitHub Actions tab
- [ ] T022 Run GitHub Community Standards check: Navigate to Insights → Community, verify all items have checkmarks (License, Code of Conduct, Contributing, Issue templates, PR template)
- [ ] T023 Run complete quickstart.md verification guide to ensure all success criteria are met

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **User Story 1 (Phase 2)**: Depends on Setup completion - Create LICENSE (MVP!)
- **User Story 2 (Phase 3)**: Depends on Setup completion - Can run in parallel with US1 after Setup
- **User Story 3 (Phase 4)**: Depends on Setup completion - Can run in parallel with US1/US2 after Setup
- **User Story 4 (Phase 5)**: Depends on Setup completion - Can run in parallel with US1/US2/US3 after Setup
- **Polish (Phase 6)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Setup - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Setup - No dependencies on other stories (independent)
- **User Story 3 (P3)**: Can start after Setup - No dependencies on other stories (independent)
- **User Story 4 (P4)**: Can start after Setup - No dependencies on other stories (independent)

### Within Each User Story

- **US1**: Single task (T004) - create LICENSE file
- **US2**: All four tasks (T005-T008) marked [P] can run in parallel (different files)
- **US3**: Both tasks (T009-T010) marked [P] can run in parallel (different files)
- **US4**: Single task (T011) - create workflows README
- **Polish**: Verification tasks T012-T014 marked [P] can run in parallel, then T015-T023 run sequentially for testing

### Parallel Opportunities

- After Setup (T001-T003) completes, all user stories can start in parallel
- Within US2: All 4 tasks can run in parallel (T005, T006, T007, T008)
- Within US3: Both tasks can run in parallel (T009, T010)
- Within Polish: T012, T013, T014 can run in parallel

---

## Parallel Example: After Setup

```bash
# Once Setup completes, launch all user stories in parallel:
Task T004 [US1]: Create LICENSE
Task T005 [US2]: Create CONTRIBUTING.md
Task T006 [US2]: Create bug_report.yml
Task T007 [US2]: Create feature_request.yml
Task T008 [US2]: Create pull_request_template.md
Task T009 [US3]: Create CODE_OF_CONDUCT.md
Task T010 [US3]: Create CODEOWNERS
Task T011 [US4]: Create workflows/README.md

# All 8 tasks can execute simultaneously (different files, no dependencies)
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T003)
2. Complete Phase 2: User Story 1 (T004 - LICENSE file)
3. **STOP and VALIDATE**: Push to GitHub, verify MIT License badge appears
4. Deploy/demo if ready - repository now has legal clarity

### Incremental Delivery

1. Complete Setup → Foundation ready
2. Add User Story 1 → Test independently → Push to GitHub (MVP - Legal clarity!)
3. Add User Story 2 → Test independently → Push to GitHub (Contributor onboarding complete)
4. Add User Story 3 → Test independently → Push to GitHub (Community standards in place)
5. Add User Story 4 → Test independently → Push to GitHub (CI/CD infrastructure ready)
6. Run Polish phase → Comprehensive validation → Create PR to develop
7. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers (or parallel agent execution):

1. Team completes Setup together (T001-T003)
2. Once Setup is done:
   - Developer A: User Story 1 (T004)
   - Developer B: User Story 2 (T005-T008)
   - Developer C: User Story 3 (T009-T010)
   - Developer D: User Story 4 (T011)
3. Stories complete and integrate independently
4. Team runs Polish phase together (T012-T023)

---

## Notes

- [P] tasks = different files, no dependencies - safe for parallel execution
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable per quickstart.md
- No automated tests required - validation through manual verification steps
- Commit after each task or logical group (e.g., all US2 tasks together)
- Stop at any checkpoint to validate story independently
- All file paths are relative to repository root
- Use research.md for exact file content standards
- Use contracts/file-locations.md for validation criteria
- Use quickstart.md for comprehensive testing procedures
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence

---

## Task Count Summary

- **Total Tasks**: 23
- **Setup Phase**: 3 tasks
- **User Story 1 (P1)**: 1 task
- **User Story 2 (P2)**: 4 tasks (all parallelizable)
- **User Story 3 (P3)**: 2 tasks (both parallelizable)
- **User Story 4 (P4)**: 1 task
- **Polish Phase**: 12 tasks (3 parallel, 9 sequential)
- **Parallel Opportunities**: 10 tasks can run simultaneously after Setup (T004-T011)

---

## Validation Checklist

After implementation, verify using quickstart.md:

- [ ] All 10+ files created at correct paths
- [ ] LICENSE recognized by GitHub (badge appears)
- [ ] CONTRIBUTING.md has all 7 sections with constitution references
- [ ] Bug report and feature request templates render as structured forms
- [ ] PR template auto-populates
- [ ] CODE_OF_CONDUCT.md clearly marked as stub
- [ ] CODEOWNERS has valid syntax and auto-assigns reviewers
- [ ] workflows directory exists and is recognized by GitHub
- [ ] GitHub Community Standards shows 100% completion
- [ ] All success criteria from spec.md are met

This completes the task breakdown for the Project Structure & Open Source Foundation feature.
