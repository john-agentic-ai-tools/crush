# Known Issues and Limitations

This document tracks known limitations and issues in the Crush project.

## Active Limitations

### 1. Ctrl+C Cannot Interrupt Active Compression

**Issue**: When a user presses Ctrl+C during a large file compression, the operation does not interrupt until the compression completes.

**Severity**: Medium
**Status**: Deferred (not blocking v0.1.0 release)
**Discovered**: Phase 12 manual testing (T173)

**Technical Details**:

The CLI properly captures Ctrl+C signals via `ctrlc::set_handler()` and sets an `Arc<AtomicBool>` flag. However, this flag is checked only:
- Before compression starts
- After compression completes
- Before/after file I/O operations

The flag is **not checked during compression** because:
1. `CompressionOptions` does not have a `cancel_flag` field
2. The CLI's `interrupted` flag is never passed to `crush_core::compress_with_options()`
3. The core library creates its own cancellation flag only for timeout handling

**Impact**:
- Users cannot cancel long-running compressions (e.g., 10GB+ files)
- Ctrl+C works immediately for decompression and small files
- No data corruption - partial output files are cleaned up after compression completes

**Workaround**: None available. Users must wait for compression to complete.

**Proposed Solution**:

Add `cancel_flag` field to `CompressionOptions`:

```rust
// crush-core/src/compression.rs
pub struct CompressionOptions {
    plugin_name: Option<String>,
    weights: ScoringWeights,
    timeout: Duration,
    file_metadata: Option<FileMetadata>,
    cancel_flag: Option<Arc<AtomicBool>>,  // NEW
}

impl CompressionOptions {
    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(flag);
        self
    }
}
```

Then pass CLI's `interrupted` flag through:

```rust
// crush-cli/src/commands/compress.rs
let options = CompressionOptions::default()
    .with_weights(args.level.to_weights())
    .with_cancel_flag(interrupted.clone());  // NEW
```

**Estimated Effort**: 1-2 hours
**Files Affected**: 4 files (~50 lines changed)
- `crush-core/src/compression.rs` (add cancel_flag, wire through to plugin)
- `crush-core/src/decompression.rs` (mirror changes)
- `crush-cli/src/commands/compress.rs` (pass interrupt flag)
- `crush-cli/src/commands/decompress.rs` (pass interrupt flag)

**Related Code**:
- Plugin contract already supports cancellation: `contract.rs:100, 118`
- CLI signal handler: `signal.rs:9`
- Interrupt checks: `compress.rs:174, 243, 268`

**Tracking**: See Phase 12, Task T173 in `specs/005-cli-implementation/tasks.md`

---

## Future Enhancements

### 2. Code Coverage for CLI Binary

**Issue**: CLI binary shows 0% code coverage due to subprocess testing approach.

**Severity**: Low (not a real testing gap)
**Status**: Accepted limitation

**Details**:

The CLI uses `assert_cmd` to spawn the binary as a subprocess for integration testing. This approach:
- ✅ Tests the CLI exactly as users invoke it (real-world accuracy)
- ✅ Validates complete process lifecycle
- ❌ Prevents `cargo-llvm-cov` from measuring coverage (process boundary limitation)

The CLI **is** comprehensively tested via 47 integration tests covering all commands and scenarios, but coverage tools cannot measure execution across process boundaries.

**Impact**: Misleading coverage metrics (30% workspace total vs 68-92% for crush-core library)

**Workaround**: Document limitation and rely on integration test count as quality metric.

**Alternative Solution**: Refactor CLI into a library crate + thin binary wrapper to enable unit testing. **Not recommended** - adds complexity without meaningful benefit.

**Documentation**: See `COVERAGE.md` for detailed analysis.

---

## Resolved Issues

None yet (v0.1.0 is first release).

---

## Reporting Issues

To report a new issue:

1. Check this document to see if it's already known
2. Search existing [GitHub Issues](https://github.com/yourusername/crush/issues)
3. Create a new issue with:
   - Steps to reproduce
   - Expected vs actual behavior
   - System information (OS, Rust version)
   - Output of `crush --version`

---

**Last Updated**: 2026-01-29
**Version**: 0.1.0
