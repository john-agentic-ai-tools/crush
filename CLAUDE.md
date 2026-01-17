# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

This is a **Speckit** project - a structured workflow system for feature development that emphasizes specification-first design, incremental delivery, and independent user story implementation.

## Core Workflow

Speckit enforces a strict workflow for all feature development:

1. **Specification (`/speckit.specify`)**: Create detailed feature specs from natural language descriptions
2. **Planning (`/speckit.plan`)**: Generate technical plans with research, data models, and API contracts
3. **Task Breakdown (`/speckit.tasks`)**: Convert plans into dependency-ordered, executable tasks
4. **Implementation (`/speckit.implement`)**: Execute the task list phase by phase

### Available Commands

All commands are invoked via the Skill tool:

- `/speckit.specify <description>` - Create feature specification from natural language
- `/speckit.clarify` - Identify and resolve underspecified requirements
- `/speckit.plan` - Generate technical implementation plan
- `/speckit.tasks` - Break plan into actionable, dependency-ordered tasks
- `/speckit.implement` - Execute the implementation
- `/speckit.analyze` - Cross-artifact consistency analysis
- `/speckit.checklist` - Generate custom validation checklists
- `/speckit.constitution` - Create/update project principles
- `/speckit.taskstoissues` - Convert tasks to GitHub issues

## Feature Organization

Features are organized in numbered branches and spec directories:

```
Branch naming: [###-feature-name]
Spec location: specs/[###-feature-name]/

Example:
  Branch: 001-user-auth
  Specs: specs/001-user-auth/spec.md
                               /plan.md
                               /tasks.md
                               /research.md
                               /data-model.md
                               /contracts/
                               /checklists/
```

Branch numbers are auto-incremented per feature name. The system checks:
- Remote branches (`git ls-remote`)
- Local branches (`git branch`)
- Spec directories (`specs/[###-name]`)

## Key Scripts (PowerShell)

Located in `.specify/scripts/powershell/`:

- `create-new-feature.ps1 -Json <description> [-ShortName name] [-Number N]` - Initialize new feature
- `setup-plan.ps1 -Json` - Setup planning context
- `check-prerequisites.ps1 -Json [-RequireTasks] [-IncludeTasks]` - Validate feature prerequisites
- `update-agent-context.ps1 -AgentType claude` - Update AI agent context files

**Usage pattern**: All scripts support `-Json` flag for structured output. Parse the JSON to get paths like `BRANCH_NAME`, `SPEC_FILE`, `FEATURE_DIR`, etc.

**Escaping**: For single quotes in arguments (e.g., "I'm Groot"), use escape syntax: `'I'\''m Groot'` or use double quotes: `"I'm Groot"`

## Specification Principles

### What vs How Separation

- **Specifications (spec.md)**: WHAT users need, WHY it matters - written for business stakeholders, technology-agnostic
- **Plans (plan.md)**: HOW to implement - tech stack, architecture, file structure, decisions

### User Story Structure

User stories MUST be:
- **Prioritized** (P1, P2, P3...) by business value
- **Independently testable** - each story is a viable MVP increment
- **Independently implementable** - can be developed, tested, and deployed standalone

### Requirements Clarity

- Make informed assumptions rather than leaving gaps
- Document assumptions in the Assumptions section
- **LIMIT**: Maximum 3 `[NEEDS CLARIFICATION]` markers per spec
- Prioritize clarifications: scope > security/privacy > UX > technical details

### Success Criteria

Must be:
- **Measurable**: Include specific metrics (time, percentage, count)
- **Technology-agnostic**: No frameworks, languages, databases
- **User-focused**: Outcomes from user/business perspective
- **Verifiable**: Can be tested without implementation knowledge

**Good**: "Users complete checkout in under 3 minutes"
**Bad**: "API response time under 200ms"

## Task Structure

Tasks are organized to enable incremental delivery:

```
Phase 1: Setup - Project initialization
Phase 2: Foundational - Blocking prerequisites for ALL user stories
Phase 3+: User Stories - One phase per story (P1, P2, P3...)
Final Phase: Polish & Cross-cutting concerns
```

### Task Format

Every task follows strict checklist format:

```
- [ ] [TaskID] [P?] [Story?] Description with file path
```

- `TaskID`: Sequential (T001, T002...)
- `[P]`: Parallel marker - different files, no dependencies
- `[Story]`: User story label ([US1], [US2]...) - required for story phases
- File paths must be explicit

**Example**: `- [ ] T012 [P] [US1] Create User model in src/models/user.py`

### Execution Strategy

**MVP First**: Complete Setup → Foundational → User Story 1 → Validate → Deploy

**Incremental**: Each user story adds value without breaking previous stories

**Parallel Teams**: After Foundational phase, different developers can work on different user stories simultaneously

## Templates

Located in `.specify/templates/`:

- `spec-template.md` - Feature specification structure
- `plan-template.md` - Technical plan structure with phases
- `tasks-template.md` - Task breakdown structure
- `checklist-template.md` - Custom checklist format
- `constitution.md` - Project constitution template
- `agent-file-template.md` - Agent context file template

## Constitution System

The project constitution is defined in `.specify/memory/constitution.md` (version 1.0.0, ratified 2026-01-17).

### Crush Core Principles (All NON-NEGOTIABLE)

1. **Performance First**: Every decision MUST consider performance impact. Target: match/exceed pigz performance.
2. **Correctness & Safety**: 100% memory safe Rust. No `.unwrap()` in production. Fuzz testing mandatory.
3. **Modularity & Extensibility**: Plugin architecture from day one. Trait-based interfaces for algorithms/formats.
4. **Test-First Development**: TDD mandatory. Tests → Approval → Fail → Implement. Red-Green-Refactor strictly enforced.

### Quality Gates (Must Pass Before Merge)

- All tests pass (`cargo test`)
- No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- Code coverage > 80%
- Benchmarks show no regression (< 5% slowdown)
- Documentation builds without warnings (`cargo doc --no-deps`)
- Fuzz testing clean (100k iterations minimum)
- No memory leaks (miri/valgrind clean)
- SpecKit task checklist complete

### Allowed Core Dependencies

- `rayon` (parallel processing), `flate2` (DEFLATE - Phase 1 only), `crc32fast` (checksums)
- `memmap2` (memory-mapped files), `thiserror` (error handling)
- `clap` (CLI only), `criterion` (benchmarks only)
- **Prohibited**: async runtimes in core, heavy frameworks, deprecated crates

### Performance Targets (Phase 1)

- Single-threaded: Within 5% of gzip
- Multi-threaded: Match/exceed pigz on 4+ cores
- Memory: < 32MB per thread
- Throughput: > 500 MB/s compression (8-core CPU)

The constitution acts as a gate in the planning phase - violations must be justified or the plan fails.

## Implementation Execution

When running `/speckit.implement`:

1. **Checklist validation**: All checklists in `checklists/` must pass before proceeding
2. **Context loading**: Reads tasks.md, plan.md, data-model.md, contracts/, research.md
3. **Ignore file creation**: Auto-detects tech stack and creates/updates .gitignore, .dockerignore, etc.
4. **Phase-by-phase execution**: Respects dependencies, parallel markers [P], TDD approach
5. **Progress tracking**: Marks tasks as [X] when completed
6. **Error handling**: Halts on sequential task failures, reports parallel task failures

## Test Strategy

Tests are **OPTIONAL** unless explicitly requested. When required:

- **Test-First (TDD)**: Write tests → Get approval → Tests fail → Implement
- **Contract tests**: API/interface validation
- **Integration tests**: User journey validation
- **Unit tests**: Component-level validation (optional, in Polish phase)

Test tasks are marked with story labels and must be written before implementation tasks in each story phase.

## Agent Context Files

The system maintains AI agent-specific context in `.specify/memory/`:

- Stores technology stack learned during planning
- Preserves manual additions between markers
- Updated via `update-agent-context.ps1 -AgentType claude`

## Workflow Anti-Patterns to Avoid

- Creating specs without user stories
- Skipping the planning phase for non-trivial features
- Adding implementation details to specifications
- Creating user stories that aren't independently testable
- Generating tasks before plan.md exists
- Skipping checklist validation before implementation
- Using generic descriptions without file paths in tasks
- Creating cross-story dependencies that break independence

## Project Structure

This is a Rust workspace project for the **Crush** high-performance parallel compression library.

### Expected Workspace Structure (Post-Implementation)

```
crush/
├── crush-core/          # Core library crate
│   ├── src/
│   │   ├── lib.rs
│   │   ├── engine.rs    # Compression engine
│   │   ├── stream.rs    # Streaming I/O
│   │   ├── block.rs     # Block processing
│   │   ├── pool.rs      # Memory pool
│   │   └── plugins/     # Plugin system
│   └── Cargo.toml
├── crush-cli/           # CLI wrapper crate
│   ├── src/
│   │   ├── main.rs
│   │   ├── args.rs      # clap argument parsing
│   │   └── signal.rs    # Signal handling
│   └── Cargo.toml
├── benches/             # Criterion benchmarks
├── fuzz/                # Fuzz testing (cargo-fuzz)
├── specs/               # SpecKit feature specs
├── Cargo.toml           # Workspace manifest
└── CLAUDE.md
```

### Technology Stack

- **Language**: Rust (latest stable)
- **Build**: Cargo workspace
- **Testing**: `cargo test`, `cargo-fuzz`, property-based tests
- **Benchmarking**: `criterion`
- **Linting**: `clippy` with pedantic lints
- **Formatting**: `rustfmt`

The structure follows the constitution's **Modularity & Extensibility** principle: core library + thin CLI wrapper.
