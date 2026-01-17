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
- [ ] Performance improvement

## Testing Checklist

- [ ] I have followed the TDD approach (tests written first, then implementation)
- [ ] All existing tests pass locally (`cargo test`)
- [ ] I have added tests covering my changes
- [ ] New and existing tests pass in CI
- [ ] Code coverage remains above 80% (verified with `cargo tarpaulin`)
- [ ] Benchmarks show no performance regression (if applicable)

## Constitution Compliance

- [ ] Code follows Rust style guidelines (`rustfmt`, `clippy`)
- [ ] No `.unwrap()` or `.expect()` in production code (tests/examples OK)
- [ ] All new public APIs are documented with examples
- [ ] Changes comply with applicable constitution principles:
  - [ ] Performance First (benchmarked if performance-sensitive)
  - [ ] Correctness & Safety (memory-safe, proper error handling)
  - [ ] Modularity & Extensibility (trait-based, pluggable)
  - [ ] Test-First Development (TDD followed)

## Branching

- [ ] This PR targets `develop` (NOT `main` - per constitution)
- [ ] I have pulled the latest changes from `develop`
- [ ] My branch is up to date with the target branch

## Code Quality Gates

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] `cargo bench` completes (if benchmarks exist)
- [ ] No unsafe code introduced (or justified with safety comments)

## Additional Context

<!-- Add any other context about the PR here:
- Screenshots (for UI changes)
- Performance numbers (for optimizations)
- Migration guide (for breaking changes)
- Links to design docs or RFCs
-->
