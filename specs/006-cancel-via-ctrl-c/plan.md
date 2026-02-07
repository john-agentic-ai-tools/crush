# Implementation Plan: Graceful Cancellation Support

**Branch**: `006-cancel-via-ctrl-c` | **Date**: 2026-02-03 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `specs/006-cancel-via-ctrl-c/spec.md`

## Summary

Add graceful cancellation support for compress/decompress operations via Ctrl+C (SIGINT) signal handling. Users can interrupt long-running operations at any time, triggering immediate feedback, coordinated shutdown of parallel threads, cleanup of incomplete output and temporary files, and proper exit codes. The implementation uses Rust's signal handling with atomic flags for cross-platform thread coordination.

## Technical Context

**Language/Version**: Rust 1.75+ (stable toolchain)
**Primary Dependencies**:
- `ctrlc` crate (cross-platform signal handling)
- `std::sync::atomic` (atomic cancellation flag)
- `rayon` (existing - parallel thread coordination)

**Storage**: File system (output files, temporary files)
**Testing**: `cargo test` (unit + integration), property-based tests for cleanup verification
**Target Platform**: Linux, macOS, Windows (cross-platform signal handling via ctrlc crate)
**Project Type**: CLI library (crush-core + crush-cli workspace)
**Performance Goals**:
- Cancellation response within 100ms (user feedback)
- Full shutdown within 1 second
- Zero memory leaks or leaked file handles

**Constraints**:
- Must not corrupt output files during cancellation
- Must complete current block before terminating (data integrity)
- Thread-safe coordination across parallel workers

**Scale/Scope**:
- 2 user stories (P1: core cancellation, P2: progress feedback)
- ~5-8 source files modified (signal handler, engine coordination, CLI output)
- Cross-cutting feature affecting compression engine core

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Core Principles Compliance

- ✅ **Performance First**: Cancellation adds minimal overhead (<1% when not triggered), uses atomic flags for zero-cost checks
- ✅ **Correctness & Safety**: No unsafe code required, proper cleanup guarantees via RAII, fuzz testing for cancellation during critical sections
- ✅ **Modularity & Extensibility**: Signal handler is plugin-compatible, cancellation flag accessible via trait methods
- ✅ **Test-First Development**: TDD workflow - write cancellation tests first, verify they fail, then implement

### Dependency Management

**New Dependency Justification**:
- `ctrlc` (v3.4+): Cross-platform signal handling
  - **Why needed**: Standard library signal handling is Unix-only; Windows requires different approach
  - **Alternative rejected**: Platform-specific signal code increases complexity and maintenance burden
  - **Justification**: Mature, widely-used crate (>10M downloads), minimal dependencies, focused scope

### Quality Gates Checklist

- [ ] All tests pass (including new cancellation tests)
- [ ] No clippy warnings
- [ ] Code coverage > 80% (cancellation paths covered)
- [ ] Benchmarks show < 1% overhead when cancellation not triggered
- [ ] Documentation complete (public cancellation API)
- [ ] Fuzz testing with mid-operation cancellation (100k iterations)
- [ ] No memory leaks (valgrind/miri verification)
- [ ] SpecKit task checklist complete

### Gate Evaluation

**Status**: ✅ **PASS** - No constitution violations

All requirements align with constitution principles. The `ctrlc` dependency is justified for cross-platform compatibility. No complexity violations requiring justification.

## Project Structure

### Documentation (this feature)

```text
specs/006-cancel-via-ctrl-c/
├── spec.md              # Feature specification
├── plan.md              # This file
├── research.md          # Phase 0 output (signal handling patterns, cleanup strategies)
├── data-model.md        # Phase 1 output (cancellation state machine)
├── quickstart.md        # Phase 1 output (using cancellation API)
├── contracts/           # Phase 1 output (CancellationToken trait contract)
└── tasks.md             # Phase 2 output (created by /speckit.tasks command)
```

### Source Code (repository root)

```text
crush-core/
├── src/
│   ├── lib.rs
│   ├── engine.rs          # Modified: check cancellation between blocks
│   ├── stream.rs          # Modified: cleanup on cancellation
│   ├── block.rs           # Modified: respect cancellation flag
│   ├── pool.rs            # Modified: release resources on cancel
│   ├── cancel.rs          # NEW: CancellationToken trait + AtomicFlag impl
│   └── plugins/
│       └── cancel_handler.rs  # NEW: signal handler plugin

crush-cli/
├── src/
│   ├── main.rs            # Modified: register signal handler, show messages
│   ├── args.rs            # Unchanged
│   └── progress.rs        # NEW: progress display with cancellation hint

tests/
├── integration/
│   └── cancel_tests.rs    # NEW: integration tests for cancellation
└── unit/
    └── cancel_unit_tests.rs  # NEW: unit tests for CancellationToken
```

**Structure Decision**: Existing Rust workspace structure (crush-core library + crush-cli wrapper). Cancellation logic lives in core as a cross-cutting concern accessible to all compression algorithms. CLI registers signal handler and coordinates user feedback.

## Complexity Tracking

No constitution violations requiring justification.

---

## Phase 0: Research & Unknowns Resolution

**Goal**: Resolve all technical unknowns and establish implementation patterns

### Research Tasks

1. **Cross-platform signal handling patterns**
   - Research: How does `ctrlc` crate handle Windows vs Unix signals?
   - Research: Best practices for atomic flag patterns in multi-threaded Rust
   - Output: Signal handler registration pattern, atomic flag design

2. **Resource cleanup strategies**
   - Research: RAII patterns for guaranteed file handle cleanup
   - Research: Temporary file tracking and deletion strategies
   - Output: Cleanup architecture ensuring zero leaked resources

3. **Thread coordination for cancellation**
   - Research: How to broadcast cancellation to all rayon worker threads
   - Research: Block-level vs byte-level cancellation granularity
   - Output: Thread coordination pattern, cancellation check frequency

4. **Exit code conventions**
   - Research: Standard exit codes for SIGINT termination (verify 130)
   - Research: Platform differences (Windows vs Unix)
   - Output: Exit code strategy

5. **Time estimation for operations**
   - Research: How to estimate if operation will take >5 seconds (for hint display)
   - Research: File size heuristics for compression time
   - Output: Time estimation algorithm

### Research Output

All findings documented in `research.md` with:
- Decision made
- Rationale
- Alternatives considered
- Code examples where applicable

---

## Phase 1: Design & Contracts

**Prerequisites**: `research.md` complete, all unknowns resolved

### 1. Data Model (`data-model.md`)

**Entities**:

1. **CancellationToken**
   - Fields: `cancelled: AtomicBool`, `signal_received_at: Option<Instant>`
   - Methods: `is_cancelled() -> bool`, `cancel()`, `reset()`
   - Lifecycle: Created at operation start, checked between blocks, reset after operation

2. **OperationState**
   - States: `Running`, `Cancelling`, `Cancelled`, `Completed`
   - Transitions: Running → Cancelling (on SIGINT) → Cancelled (after cleanup) → reset to Running
   - Validation: Cannot transition from Cancelled to Running without reset

3. **ResourceTracker**
   - Fields: `output_files: Vec<PathBuf>`, `temp_files: Vec<PathBuf>`, `handles: Vec<FileHandle>`
   - Methods: `register_file()`, `cleanup_all()`, `mark_complete()`
   - Lifecycle: Tracks all created resources, RAII cleanup on drop or explicit cancel

### 2. API Contracts (`contracts/`)

**CancellationToken Trait** (`contracts/cancellation-token.md`):

```rust
/// Thread-safe cancellation signal for compression operations
pub trait CancellationToken: Send + Sync {
    /// Check if cancellation has been requested (non-blocking)
    fn is_cancelled(&self) -> bool;

    /// Request cancellation (idempotent)
    fn cancel(&self);

    /// Reset cancellation state for next operation
    fn reset(&self);
}
```

**CompressionEngine API Changes** (`contracts/engine-changes.md`):

```rust
// New parameter added to compress/decompress methods
pub fn compress_with_cancel<W: Write>(
    &self,
    input: &[u8],
    output: W,
    cancel_token: &dyn CancellationToken,
) -> Result<(), CrushError>;
```

**Signal Handler Contract** (`contracts/signal-handler.md`):

```rust
/// Register global signal handler
pub fn register_cancellation_handler(
    token: Arc<dyn CancellationToken>
) -> Result<(), CrushError>;
```

### 3. Quickstart Guide (`quickstart.md`)

**For Users (CLI)**:
- How to cancel operations (Ctrl+C)
- What to expect (messages, cleanup, exit codes)
- Edge cases (multiple Ctrl+C, cleanup time)

**For Developers (Library)**:
- How to integrate CancellationToken into custom workflows
- Example: Using cancellation in custom compression pipeline
- Thread safety guarantees

### 4. Agent Context Update

Run: `.specify/scripts/powershell/update-agent-context.ps1 -AgentType claude`

Updates: `.specify/memory/claude.md` with:
- New dependency: `ctrlc` crate
- New module: `cancel.rs` (CancellationToken implementation)
- Modified modules: `engine.rs`, `stream.rs`, `block.rs`
- API surface changes: cancellation parameter added

---

## Phase 2: Task Decomposition

**Created by**: `/speckit.tasks` command (NOT part of /speckit.plan)

**Expected output**: `tasks.md` with dependency-ordered tasks organized by:
- Phase 1: Setup (dependencies, scaffolding)
- Phase 2: Foundational (CancellationToken trait, atomic implementation)
- Phase 3: User Story 1 - P1 (core cancellation, cleanup, exit codes)
- Phase 4: User Story 2 - P2 (progress feedback, "Already cancelling..." message)
- Phase 5: Polish (fuzz tests, benchmarks, documentation)

---

## Post-Design Constitution Re-Check

**Re-evaluate after Phase 1 design completion**:

- ✅ **Performance**: Atomic flag checks are near-zero cost
- ✅ **Correctness**: RAII guarantees cleanup, no unsafe code
- ✅ **Modularity**: CancellationToken is trait-based, plugin-compatible
- ✅ **Testing**: TDD workflow with cancellation-specific tests

**Status**: ✅ **PASS** - Design complies with all constitution principles

---

## Implementation Notes

### Critical Considerations

1. **Block-level cancellation**: Must finish current block to avoid corrupting format structures (gzip headers/trailers)
2. **Thread coordination**: All rayon threads must check cancellation flag between blocks
3. **File cleanup order**: Close handles before deleting files to avoid Windows file-in-use errors
4. **Exit code timing**: Set exit code AFTER cleanup completes, not when signal received

### Risk Mitigation

- **Risk**: Cancellation during critical write (file header)
  - **Mitigation**: Block-level cancellation ensures headers/trailers complete

- **Risk**: Temporary files leaked if cleanup fails
  - **Mitigation**: RAII pattern with Drop impl guarantees cleanup even on panic

- **Risk**: Race condition between signal and operation completion
  - **Mitigation**: Atomic flag with proper memory ordering (SeqCst)

---

## Success Metrics

- All 9 functional requirements met (FR-001 through FR-009)
- All 6 success criteria achieved (SC-001 through SC-006)
- Both user stories independently testable and deliverable
- Zero constitution violations
- Quality gates passing (tests, clippy, coverage, benchmarks)

**Plan Status**: ✅ Ready for Phase 0 Research

**Next Command**: Begin research phase (internal to /speckit.plan execution)
