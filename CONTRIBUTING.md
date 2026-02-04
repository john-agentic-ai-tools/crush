# Contributing to Crush

Welcome to Crush! We're excited that you're interested in contributing to a high-performance Rust compression library. This guide will help you get started with development, understand our workflows, and successfully contribute to the project.

## Ways to Contribute

- **Code**: Implement new features, fix bugs, optimize performance
- **Documentation**: Improve guides, add examples, fix typos
- **Issues**: Report bugs, suggest features, help triage existing issues
- **Testing**: Write tests, improve coverage, report edge cases
- **Performance**: Submit benchmarks, identify bottlenecks, optimize algorithms

## Development Setup

### Prerequisites

Crush uses a pinned Rust toolchain to ensure consistency across all development environments. The exact version is defined in `rust-toolchain.toml` at the repository root.

#### Required Tools

| Tool | Purpose | Installation |
|------|---------|--------------|
| Git | Version control + bash shell for hooks | [git-scm.com](https://git-scm.com/downloads) |
| Rust | Compilation | [rustup.rs](https://rustup.rs) |
| Docker | Local CI testing | [docker.com](https://docs.docker.com/get-docker/) |
| act-cli | Run GitHub Actions locally | `choco install act-cli` or [nektos/act](https://github.com/nektos/act) |

**Note**: On Windows, you must have a Unix shell environments such  Git Bash or Windows Subsystem for Linux. This is required for the quality gate hooks (`.claude/hooks/*.sh`).

### Installation Steps

1. **Install Rust** (if not already installed):

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone the repository**:

   ```bash
   git clone git@github.com:john-agentic-ai-tools/crush.git
   cd crush
   ```

3. **Build the project**:

   ```bash
   cargo build
   ```

   The pinned toolchain from `rust-toolchain.toml` will be automatically installed.

4. **Set up pre-commit hooks** (via cargo-husky):

   ```bash
   cargo install cargo-husky
   cargo husky install
   ```

   Pre-commit hooks will automatically run `cargo fmt` and `cargo clippy` before each commit.

### Verify Your Setup

```bash
# Run all tests
cargo test

# Run benchmarks
cargo bench

# Check formatting
cargo fmt --all -- --check

# Run linter
cargo clippy --all-targets --all-features -- -D warnings
```

## Branching Model

Crush follows the **Git Flow** branching model as defined in the project constitution:

- **`main`** - Release-only branch. Contains production-ready code. Direct commits prohibited.
- **`develop`** - Default integration branch. All feature branches merge here first.
- **`feature/*`** - Short-lived feature branches created from `develop`.

### Branch Naming Conventions

- Feature branches: `feature/001-description` or `001-feature-name` (matches spec number)
- Bug fixes: `bugfix/issue-123-description`
- Hotfixes: `hotfix/critical-issue`

### Creating a Feature Branch

```bash
# Start from develop
git checkout develop
git pull origin develop

# Create feature branch
git checkout -b feature/my-feature-name
```

**Important**: Always branch from `develop`, not `main`. See the constitution's Branching & Merge Governance for details.

## Commit Conventions

Use **semantic commit messages** with conventional commit format:

```text
<type>(<scope>): <subject>

<body>

<footer>
```

### Commit Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `refactor`: Code refactoring (no behavior change)
- `perf`: Performance improvements
- `test`: Adding or modifying tests
- `chore`: Build process, tooling, dependencies

### Examples

```text
feat(compression): add LZ4 algorithm support

Implement LZ4 compression as a pluggable algorithm per constitution
principle III (Modularity & Extensibility).

Benchmarks show 30% faster compression vs gzip on text workloads.
```

```text
fix(decompression): prevent buffer overflow on malformed input

Add bounds checking in decompression loop. Fixes #123.

Tests added to verify safe handling of truncated input.
```

### Signed Commits Required

All commits must be signed and verified. Please follow the GitHub documentation on how to set up a
GPG key and associate it with your GitHub account.

### AI-Assisted Contributions

If you use AI tools (like Claude, GitHub Copilot, ChatGPT) to write code, add a co-author line:

```text
feat(benchmark): add multi-threaded compression benchmark

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
```

### Spec Driven Development

This project uses the Spec Kit framework for AI-Assisted workflows. This approach ensures that
changes made by AI adhere the rules specified in the constitution and are backed up by
clear specifications. This helps to avoid AI hallucinations and unwanted features.

## Testing Requirements

Crush follows **Test-First Development (TDD)** as a NON-NEGOTIABLE constitution principle.

### TDD Workflow (Red-Green-Refactor)

1. **Red**: Write a failing test that defines desired behavior
2. **Green**: Write minimal code to make the test pass
3. **Refactor**: Improve code while keeping tests green

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for specific module
cargo test compression::lz4

# Run tests with output
cargo test -- --nocapture

# Run tests with coverage (requires tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### Coverage Expectations

- **Minimum**: 80% code coverage (enforced by CI)
- **Target**: 90%+ for critical paths (compression/decompression logic)
- All new features must include tests before implementation

### Test Categories

- **Unit tests**: Test individual functions/modules
- **Integration tests**: Test component interactions
- **Benchmark tests**: Verify performance characteristics
- **Fuzz tests**: Discover edge cases and vulnerabilities

## Pull Request Process

### Before Opening a PR

1. **Ensure all tests pass**:

   ```bash
   cargo test
   ```

2. **Run linter**:

   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

3. **Format code**:

   ```bash
   cargo fmt --all
   ```

4. **Update documentation** if adding public APIs

### Opening a Pull Request

1. **Push your feature branch**:

   ```bash
   git push origin feature/my-feature-name
   ```

2. **Open PR targeting `develop`** (NOT `main`)
   - PRs to `main` will be rejected per constitution

3. **Fill out the PR template** completely
   - Description of changes
   - Related issues (use `Fixes #123` to auto-close)
   - Testing checklist
   - Constitution compliance checklist

4. **Wait for CI to pass** before requesting review
   - All tests must pass
   - No clippy warnings
   - Coverage requirements met
   - Benchmarks show no regression

### Code Review Expectations

- **Be responsive**: Address review feedback promptly
- **Be open**: Constructive criticism improves code quality
- **Ask questions**: If feedback is unclear, ask for clarification
- **Update tests**: Test changes may be requested during review

### After Approval

- Maintainers will merge your PR into `develop`
- Your branch will be deleted automatically
- Changes will be included in the next release from `main`

## Code Style

### Rust Style Guidelines

Crush enforces strict code style via automated tools:

- **`rustfmt`**: Code formatting (enforced by pre-commit hooks)
- **`clippy`**: Linting and best practices (zero warnings required)

### Constitution-Mandated Rules

#### No `.unwrap()` or `.expect()` in Production Code

**Prohibited**:

```rust
let value = option.unwrap(); // ❌ Will fail CI
let result = result.expect("failed"); // ❌ Will fail CI
```

**Use instead**:

```rust
let value = option.ok_or(Error::MissingValue)?; // ✅
match result {
    Ok(val) => val,
    Err(e) => return Err(Error::from(e)), // ✅
}
```

Exception: `.unwrap()` is allowed in test code and examples where panics are acceptable.

### Documentation Standards

All public APIs must be documented with:

1. **Description**: What the function/type does
2. **Examples**: Working code demonstrating usage
3. **Errors**: What errors can be returned and why
4. **Panics**: If the function can panic (avoid in production code)

**Example**:

```rust
/// Compresses data using the specified algorithm.
///
/// # Examples
///
/// ```
/// use crush::{compress, Algorithm};
///
/// let data = b"Hello, world!";
/// let compressed = compress(data, Algorithm::Gzip)?;
/// assert!(compressed.len() < data.len());
/// # Ok::<(), crush::Error>(())
/// ```
///
/// # Errors
///
/// Returns `Error::CompressionFailed` if the algorithm encounters
/// an error during compression.
pub fn compress(data: &[u8], algo: Algorithm) -> Result<Vec<u8>, Error> {
    // Implementation
}
```

## Constitution Compliance

All contributions must comply with the Crush Constitution principles:

### I. Performance First (NON-NEGOTIABLE)

- Benchmark any performance-sensitive changes
- No regressions in compression speed or ratio
- Target: Match or exceed pigz performance

### II. Correctness & Safety (NON-NEGOTIABLE)

- 100% memory-safe Rust code
- No unsafe blocks without justification and review
- No `.unwrap()` in production code

### III. Modularity & Extensibility (NON-NEGOTIABLE)

- Use trait-based interfaces for algorithms
- Support plugin architecture
- Avoid tight coupling between components

### IV. Test-First Development (NON-NEGOTIABLE)

- Write tests before implementation (TDD)
- Maintain 80%+ code coverage
- Include benchmark tests for performance claims

### Quality Gates (CI Enforcement)

Before merge, all PRs must pass:

- ✅ All tests pass (`cargo test`)
- ✅ No clippy warnings (`cargo clippy`)
- ✅ Code coverage > 80% (`cargo tarpaulin`)
- ✅ Benchmarks show no regression (`cargo bench`)
- ✅ Fuzz tests clean (100k iterations minimum)
- ✅ Miri clean for unsafe code (`cargo miri test`)

See `.specify/memory/constitution.md` for complete governance rules.

## CI/CD Workflows

Crush uses GitHub Actions for continuous integration and deployment. Understanding these workflows helps you diagnose failures and test changes locally.

### Workflow Overview

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | PRs, pushes to develop/main | Quality gates, multi-platform builds, tests, coverage |
| `security-audit.yml` | PRs, pushes, daily schedule | Dependency vulnerability and license checks |
| `release.yml` | Pushes to release/** branches | Version validation, publishing, branch merging |

### CI Workflow Jobs

The main CI pipeline runs these jobs in sequence:

1. **format_check**: Verifies code formatting with `cargo fmt --all -- --check`
2. **lint**: Runs Clippy with strict warnings (`-D warnings`)
3. **build_matrix**: Builds on 3 platforms × 2 Rust versions (stable/beta)
4. **test**: Runs tests with cargo-nextest on all platforms
5. **coverage**: Enforces 90% code coverage threshold

### Testing Workflows Locally

Use `act` to run workflows locally before pushing:

```bash
# Test format check job
act pull_request -j format_check -W .github/workflows/ci.yml

# Test lint job
act pull_request -j lint -W .github/workflows/ci.yml

# Test security audit
act pull_request -W .github/workflows/security-audit.yml
```

**Note**: Matrix builds (multi-platform) require actual GitHub runners and cannot be fully tested locally.

### Troubleshooting CI Failures

#### Format Check Fails

```bash
# Diagnosis
cargo fmt --all -- --check

# Fix
cargo fmt --all
git add . && git commit -m "fix: apply formatting"
```

#### Lint Fails (Clippy Warnings)

```bash
# Diagnosis
cargo clippy --all-targets --all-features -- -D warnings

# Auto-fix where possible
cargo clippy --fix --all-targets --all-features
git add . && git commit -m "fix: resolve clippy warnings"
```

#### Tests Fail in CI But Pass Locally

Common causes:
- **Test pollution**: Tests depend on execution order. Use `cargo nextest run` locally to match CI.
- **Environment differences**: Use `env!("CARGO_MANIFEST_DIR")` for paths.
- **Timing issues**: Add proper synchronization or use nextest retries.

#### Coverage Below Threshold

```bash
# Generate coverage report
cargo install cargo-llvm-cov
cargo llvm-cov --html
# Open target/llvm-cov/html/index.html to find uncovered code
```

#### Security Audit Fails

```bash
# Check for vulnerabilities
cargo audit

# Check licenses and sources
cargo deny check
```

See `.github/workflows/README.md` for detailed workflow documentation.

## Getting Help

- **Questions**: Open a discussion in GitHub Discussions
- **Bugs**: Open an issue using the bug report template
- **Features**: Open an issue using the feature request template
- **Chat**: Join our community (links TBD)

## License

By contributing to Crush, you agree that your contributions will be licensed under the MIT License.

Thank you for contributing to Crush! 🚀
