# Crush CLI Benchmarks

This directory contains performance benchmarks for the Crush CLI.

## Available Benchmarks

### 1. CLI Startup Time (`cli_startup.rs`)
**Target: <50ms**

Measures the time it takes for the CLI to start, parse arguments, and exit.

**Scenarios:**
- `version_flag` - Time to display version (`crush --version`)
- `invalid_command` - Argument parsing overhead with invalid commands
- `no_args` - Startup with no arguments (shows usage)
- `verbose` - Overhead of verbose flags (`-v`, `-vv`, `-vvv`)

### 2. Help Command (`help_command.rs`)
**Target: <100ms**

Measures the time to generate and display help text.

**Scenarios:**
- `root_help` - Root help command (`crush --help`)
- `subcommand/<cmd>` - Subcommand help (`crush compress --help`, etc.)
- `help_cmd/<cmd>` - Help subcommand (`crush help compress`)
- `short_flag` vs `long_flag` - Comparison of `-h` vs `--help`

## Running Benchmarks

### Run All Benchmarks
```bash
cargo bench
```

### Run Specific Benchmark
```bash
# CLI startup benchmarks
cargo bench --bench cli_startup

# Help command benchmarks
cargo bench --bench help_command
```

### Quick Mode (Faster, Less Precise)
```bash
cargo bench --bench cli_startup -- --quick
```

### Run Specific Test
```bash
cargo bench --bench cli_startup -- version_flag
```

## Understanding Results

Benchmark results show:
- **time**: Mean execution time with confidence interval
- **thrpt**: Throughput (for throughput benchmarks)

Example output:
```
cli_startup/version_flag
                        time:   [7.6238 ms 7.9132 ms 7.9856 ms]
```

This means:
- Lower bound: 7.62ms
- **Mean: 7.91ms** ← Most important value
- Upper bound: 7.99ms

### Performance Targets

| Benchmark | Target | Current | Status |
|-----------|--------|---------|--------|
| CLI Startup | <50ms | ~8ms | ✅ Excellent |
| Help Command | <100ms | ~9ms | ✅ Excellent |

## Benchmark Configuration

Benchmarks use [Criterion](https://github.com/bheisler/criterion.rs) with the following settings:

- **Measurement time**: 10 seconds per benchmark
- **Sample size**: 100 iterations
- **Warmup**: 3 seconds
- **Profile**: Release mode with optimizations

## Interpreting Performance

### What's Being Measured?

These benchmarks measure **complete process lifecycle**:
1. Process spawning
2. Binary loading
3. Argument parsing
4. Command execution
5. Process exit

This includes OS overhead for process creation, which typically dominates the timing.

### Why Use Process-Based Benchmarks?

We benchmark by spawning the binary (not library calls) because:
- **Real-world accuracy**: Matches how users actually invoke the CLI
- **Complete picture**: Includes all startup costs (binary loading, initialization)
- **Regression detection**: Catches issues in startup time, dependencies, binary size

### Expected Performance Characteristics

- **Startup time ~7-9ms**: Excellent for a Rust CLI
- **Help commands ~9ms**: Similar to startup (process spawn dominates)
- **Consistency**: Low variance indicates predictable performance

## Regression Detection

Criterion automatically detects performance regressions:

- **Green**: Performance improved
- **Yellow**: No significant change
- **Red**: Performance regression detected

To compare against baseline:
```bash
# Save current results as baseline
cargo bench --bench cli_startup --save-baseline main

# Compare against baseline after changes
cargo bench --bench cli_startup --baseline main
```

## CI Integration

Benchmarks can be run in CI to detect regressions:

```yaml
- name: Run benchmarks
  run: cargo bench --no-fail-fast

- name: Check performance targets
  run: |
    # Parse criterion output and verify targets are met
    cargo bench --bench cli_startup -- --output-format bencher
```

## Optimizing Performance

If benchmarks show regressions:

1. **Binary size**: Check if dependencies increased binary size
   ```bash
   cargo bloat --release --bin crush
   ```

2. **Startup time**: Profile with `cargo flamegraph`
   ```bash
   cargo install flamegraph
   cargo flamegraph --bin crush -- --version
   ```

3. **Dependencies**: Review dependency tree
   ```bash
   cargo tree --depth 2
   ```

## Local vs CI Performance

**Note**: Absolute times will vary between machines. Focus on:
- **Relative changes** (before/after)
- **Meeting targets** (<50ms, <100ms)
- **Consistency** (low variance)

CI systems may show slower absolute times due to:
- Shared resources
- Virtualization overhead
- Different CPU architecture

## Troubleshooting

### "Binary not found" Error

The benchmark tries to build the release binary. If it fails:

```bash
# Manually build first
cargo build --release --bin crush

# Then run benchmarks
cargo bench
```

### Inconsistent Results

If results vary widely:
- Close background applications
- Disable CPU throttling
- Run with `--sample-size 200` for more samples

### Gnuplot Warning

```
Gnuplot not found, using plotters backend
```

This is harmless. To enable gnuplot graphs:
```bash
# Ubuntu/Debian
sudo apt install gnuplot

# macOS
brew install gnuplot

# Windows
# Download from http://www.gnuplot.info/
```

## Further Reading

- [Criterion User Guide](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Benchmarking Best Practices](https://easyperf.net/blog/2018/08/26/Benchmarking-Best-Practices)
