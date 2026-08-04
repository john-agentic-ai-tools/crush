# Crush

Crush is a high-performance parallel compression toolkit written in Rust. It combines multi-threaded CPU compression with GPU-accelerated decompression through a pluggable architecture, delivering throughput that scales from single-core workloads to discrete GPUs.

Crush is designed for high-throughput data pipelines, particularly in AI and ML workflows where large datasets are frequently compressed, transferred over the network, and decompressed. By pairing near-linear CPU scaling with GDeflate-based GPU decompression, Crush minimizes end-to-end data ingestion time at every stage of the pipeline.

## Why Crush?

Modern data pipelines move massive amounts of data — training sets, model checkpoints, log archives — often across networks and under tight latency budgets. Traditional compression tools force a choice between speed and flexibility. Crush eliminates that tradeoff.

Crush is built to:

- **Scale with hardware** — parallel CPU compression across all cores, GPU-accelerated decompression via CUDA or Vulkan/Metal/DX12
- **Adapt to data** — plugin architecture enables format-aware compression strategies; automatic entropy analysis routes data to the best algorithm
- **Stay out of the way** — automatic GPU detection with CPU fallback, cooperative cancellation, and pipeline-friendly stdin/stdout
- **Be safe and predictable** — memory-safe Rust, CRC32 integrity checks, RAII cleanup of partial files, no `.unwrap()` in production code

## Features

### Core Capabilities

- **Parallel CPU Compression**: Multi-threaded compression scaling near-linearly to all available cores (~1.1 GiB/s at default workers, post-011 hot-path optimizations)
- **GPU-Accelerated Decompression**: GDeflate-based GPU decompression via CUDA (560 MiB/s) or wgpu Vulkan/Metal/DX12 (386 MiB/s) with automatic CPU fallback
- **Plugin Architecture**: Extensible system supporting multiple compression algorithms with automatic scoring and selection
- **Random Access**: O(1) tile-based decompression for seeking into large archives without full decompression
- **Metadata Preservation**: Automatically preserves file modification times and Unix permissions
- **Pipeline Integration**: Full stdin/stdout support for seamless Unix pipeline integration
- **Configuration Management**: Per-user TOML configuration with environment variable overrides

### Graceful Cancellation

Crush supports graceful cancellation of long-running operations via **Ctrl+C** (SIGINT):

- **Instant Response**: Cancellation detection within 100μs for immediate feedback
- **Automatic Cleanup**: Incomplete output files are automatically deleted on cancellation
- **User Hints**: Large file operations (>1MB) display helpful hints about cancellation
- **Proper Exit Codes**: Exit code 130 (Unix) / 2 (Windows) indicates cancelled operations
- **Clear Messaging**: Displays "Operation cancelled" message for user feedback

```bash
# Start compression of a large file
crush compress large_dataset.bin
# ℹ️  Press Ctrl+C to cancel this operation

# Press Ctrl+C at any time
^C
# Output: Operation cancelled
# Exit code: 130
# Result: Incomplete large_dataset.bin.crush automatically deleted
```

**Benefits**:
- No more waiting for operations you didn't want to complete
- No manual cleanup of incomplete files
- Safe interruption without corruption or partial files
- Ideal for interactive use and long-running batch jobs

## Performance

Crush delivers exceptional throughput for both compression and decompression operations.

### Benchmark Results

**Test Environment:**
- Rust: 1.93.1 (stable)
- CPU: Multi-core (benchmarks use all available cores by default)
- GPU: NVIDIA GeForce RTX 3060 Ti (Vulkan)
- Build: Release mode with optimizations
- Feature 011 hot-path optimizations applied: pooled libdeflater state, pre-allocated output with parallel assembly, direct-write decompress, scoped-borrow input on `crush-core::compress`, and a cumulative-offset `BlockIndex`

#### CPU Compression (crush-parallel, 128 MB corpus)

**Compression Scaling (64 KB blocks):**

| Workers | Throughput  | Scaling |
|---------|-------------|---------|
| 1       | 90 MiB/s    | 1.0×    |
| 2       | 182 MiB/s   | 2.02×   |
| 4       | 361 MiB/s   | 4.01×   |
| 8       | 601 MiB/s   | 6.68×   |
| default (all cores) | 1.10 GiB/s | — |

Compression scales near-linearly with thread count. `workers=0` (default) uses all available logical CPUs.

**Compression Performance by Block Size (default workers):**

| Block Size | Throughput  |
|-----------|-------------|
| 64 KB     | 1.10 GiB/s  |
| 512 KB    | 1.09 GiB/s  |
| 1024 KB   | 1.11 GiB/s  |

#### CPU Decompression (crush-parallel, 128 MB corpus, 1 MB blocks)

Post-011 Slice C (direct-write decompress) eliminates the intermediate `Vec<Option<Vec<u8>>>` + flatten-copy phase; workers decompress DEFLATE output straight into disjoint slices of a single pre-allocated output buffer.

| Workers | Throughput  |
|---------|-------------|
| 1       | 842 MiB/s   |
| 2       | 1.54 GiB/s  |
| 4       | 2.72 GiB/s  |
| 8       | 4.05 GiB/s  |
| default (all cores) | 4.63 GiB/s |

#### GPU Decompression (crush-gpu, GDeflate)

GPU decompression uses multi-block kernel dispatch — all tiles in a batch launch as parallel CUDA blocks or wgpu workgroups, fully utilizing GPU hardware.

**Benchmark Results (Criterion, wgpu Vulkan):**

| Corpus | GPU | CPU | Winner |
|--------|-----|-----|--------|
| log-1MB | 141 MiB/s | 319 MiB/s | CPU |
| binary-1MB | 85 MiB/s | 126 MiB/s | CPU |
| mixed-1MB | 87 MiB/s | 176 MiB/s | CPU |
| mixed-10MB | **355 MiB/s** | 179 MiB/s | **GPU 1.98x** |

GPU decompression outperforms CPU at larger data sizes (>2-4 MB) where the fixed dispatch overhead is amortized across many tiles. For smaller data, CPU is faster.

**Real-World Large File (1.8 GB compressed, ~62K tiles, RTX 3060 Ti):**

| Backend | Throughput | vs CPU |
|---------|-----------|--------|
| **CUDA** | **560 MiB/s** | **3.1x** |
| wgpu (Vulkan) | 386 MiB/s | 2.2x |
| CPU (all cores) | 179 MiB/s | baseline |

**GPU Compression Throughput (GDeflate, CPU-side):**

| Corpus | Throughput |
|--------|-----------|
| log-text-1MB | 179 MiB/s |
| binary-1MB | 69 MiB/s |
| mixed-1MB | 98 MiB/s |
| mixed-10MB | 100 MiB/s |

**Random Access Performance** (64 MB corpus, 1 MB blocks):

| Operation              | Latency  |
|------------------------|----------|
| Decompress last block  | ~1.14 ms |
| Decompress first block | ~1.14 ms |

**Index Lookup Performance** (10 240-block CRSH file, 64 KB blocks, post-011 cumulative-offset index):

| Operation                                               | Latency | Per-call |
|---------------------------------------------------------|---------|----------|
| 10 000 `block_for_offset` + `uncompressed_offset` calls | ~120 µs | ~12 ns   |

Feature 011 replaced the per-lookup O(N) cumulative sum with an O(1) indexed read for `uncompressed_offset` and an O(log N) `partition_point` binary search for `block_for_offset`, delivering the SC-004 ≥100× reduction target.

### Key Performance Features

- **Near-linear compression scaling**: throughput doubles with each doubling of worker count up to CPU core count
- **SIMD-accelerated DEFLATE**: powered by libdeflater for maximum per-thread throughput (~4x faster than pure-Rust backends)
- **GPU-accelerated decompression**: multi-block GDeflate dispatch via CUDA (560 MiB/s) or wgpu (Vulkan/Metal/DX12) with automatic CPU fallback
- **Configurable parallelism**: `workers(n)` limits CPU usage for background tasks; `workers(0)` (default) uses all cores
- **Consistent Random Access**: O(1) block decompression via seekable block index (~1 ms per block)
- **Memory Efficient**: Exact worst-case buffer sizing eliminates over-allocation

### Running Benchmarks

```bash
# Run crush-parallel compression/decompression benchmarks
cargo bench -p crush-parallel --bench throughput

# Run random access benchmarks
cargo bench -p crush-parallel --bench random_access

# Run GPU decompression throughput benchmarks (requires compatible GPU)
cargo bench -p crush-gpu --bench throughput

# Run GPU compression ratio benchmarks
cargo bench -p crush-gpu --bench ratio
```

### Comparing Against gzip/pigz

To compare crush against standard compression tools:

```bash
# Build crush first
cargo build --release

# Run comparison script (Linux/Mac/WSL)
bash benchmark-compare.sh

# Manual comparison example
echo "Test data..." > test.txt

# Crush
time ./target/release/crush compress test.txt
ls -lh test.txt.crush

# gzip --fast (comparable compression ratio to crush)
time gzip --fast --keep test.txt
ls -lh test.txt.gz

# pigz (parallel gzip)
time pigz --keep test.txt
ls -lh test.txt.gz
```

**Note**: Crush currently optimizes for speed over compression ratio. For fairest comparisons, compare against `gzip --fast` rather than default gzip, as they have similar compression ratios while crush provides significantly higher throughput.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/john-agentic-ai-tools/crush.git
cd crush

# Build release binary
cargo build --release

# Install to ~/.cargo/bin
cargo install --path crush-cli
```

### Binary Release

Download the latest release from [GitHub Releases](https://github.com/john-agentic-ai-tools/crush/releases).

## Quick Start

### Basic Compression

```bash
# Compress a file (creates data.txt.crush)
crush compress data.txt

# Compress with specific output name
crush compress data.txt -o compressed.crush

# Compress and keep the original file
crush compress data.txt --keep
```

### Basic Decompression

```bash
# Decompress a file (creates data.txt)
crush decompress data.txt.crush

# Decompress to specific output name
crush decompress data.txt.crush -o output.txt

# Decompress and keep compressed file
crush decompress data.txt.crush --keep
```

### Pipeline Usage

Crush supports stdin/stdout for seamless integration with Unix pipelines:

```bash
# Compress from stdin to file
cat data.txt | crush compress -o data.txt.crush

# Compress from stdin to stdout
cat data.txt | crush compress | ssh remote 'cat > data.txt.crush'

# Decompress from file to stdout
crush decompress --stdout data.txt.crush | grep "pattern"

# Full pipeline: compress and decompress
echo "Hello, World!" | crush compress | crush decompress
```

## Usage Examples

### File Compression

#### Compress a Single File

```bash
# Basic compression
crush compress data.txt
# Output: data.txt.crush

# With progress bar (shown for files > 1MB)
crush compress large_file.bin
# Compressing large_file.bin [=========>     ] 64% 450 MB/s
```

#### Compress Multiple Files

```bash
# Compress each file individually
for file in *.txt; do
    crush compress "$file"
done

# Or use shell globbing (requires bash/zsh)
crush compress *.txt  # Note: processes first file only, use loop for multiple
```

#### Force Overwrite

```bash
# Overwrite existing compressed file
crush compress data.txt --force
```

### File Decompression

#### Decompress a Single File

```bash
# Basic decompression
crush decompress data.txt.crush
# Output: data.txt

# Decompress to stdout
crush decompress --stdout data.txt.crush > output.txt
```

#### Verify Decompression

```bash
# Decompress and verify file integrity (CRC32 check is automatic)
crush decompress data.txt.crush
# ✓ Decompressed: data.txt.crush → data.txt (CRC32 verified)
```

### File Inspection

Inspect compressed files without decompressing:

```bash
# Basic inspection
crush inspect data.txt.crush
# File: data.txt.crush
# Size: 1.2 MB → 450 KB (62.5% reduction)
# Plugin: deflate
# Original name: data.txt
# Compressed: 2026-01-27 14:32:15
# CRC32: a3f5c2d1

# JSON output for scripting
crush inspect data.txt.crush --json
# {"file":"data.txt.crush","input_size":1200000,"output_size":450000,...}

# CSV output for batch processing
crush inspect *.crush --csv
# file,input_size,output_size,ratio,plugin,crc32
# data1.txt.crush,1200000,450000,62.5,deflate,a3f5c2d1
# data2.txt.crush,2400000,800000,66.7,deflate,b4e6d3f2
```

### Plugin Management

#### List Available Plugins

```bash
# List all plugins
crush plugins list
# Available plugins:
#   deflate - DEFLATE compression (default)

# JSON output
crush plugins list --json
```

#### Get Plugin Information

```bash
# Get detailed plugin info
crush plugins info deflate
# Plugin: deflate
# Compression: DEFLATE (RFC 1951)
# Magic: 0x1F8B
# Features: fast, widely-supported
# Format: GZIP-compatible
```

#### Test Plugin Performance

```bash
# Benchmark a plugin
crush plugins test deflate
# Testing deflate plugin...
# Compression: 12.5 MB/s
# Decompression: 48.3 MB/s
# Ratio: 3.2x (average)
```

### Configuration

Crush supports persistent configuration for default settings:

#### Set Configuration

```bash
# Set default compression level
crush config set compression.level fast

# Set default plugin
crush config set compression.plugin deflate

# Set default verbosity
crush config set general.verbose 1
```

#### Get Configuration

```bash
# Get specific setting
crush config get compression.level
# fast

# List all settings
crush config list
# compression.level = fast
# compression.plugin = auto
# general.verbose = 0
```

#### Reset Configuration

```bash
# Reset specific setting to default
crush config reset compression.level

# Reset all settings
crush config reset --all
```

### Verbose Output

Control output verbosity with `-v` flags:

```bash
# Level 1: Show compression details
crush compress data.txt -v
# Compressing: data.txt
# Plugin: deflate (auto-selected)
# Compressed: 1.2 MB → 450 KB (62.5% reduction)
# Throughput: 125 MB/s

# Level 2: Show performance metrics
crush compress data.txt -vv
# Compressing: data.txt
# Plugin selection:
#   deflate: score 85.5 (compression: 90, speed: 75, ratio: 92)
# Compressed: 1.2 MB → 450 KB (62.5% reduction)
# Time: 9.6ms | Throughput: 125 MB/s
# CRC32: a3f5c2d1

# Level 3: Show debug information
crush compress data.txt -vvv
# [DEBUG] Reading input file: data.txt
# [DEBUG] File size: 1,200,000 bytes
# [DEBUG] Initializing plugin: deflate
# [DEBUG] Compression started
# ...
```

### Logging

Enable structured logging for debugging and auditing:

```bash
# Log to file (JSON format)
crush compress data.txt --log compress.log

# Log with specific level
RUST_LOG=debug crush compress data.txt --log debug.log

# View logs
cat compress.log | jq
# {
#   "timestamp": "2026-01-27T14:32:15Z",
#   "level": "INFO",
#   "message": "Compression started",
#   "input_file": "data.txt",
#   "plugin": "deflate"
# }
```

### Advanced Examples

#### Compress with Metadata Preservation

```bash
# File timestamps and permissions are preserved automatically
crush compress data.txt
crush decompress data.txt.crush
ls -l data.txt  # Original timestamp preserved
```

#### Batch Compression with Error Handling

```bash
# Compress all files, continue on error
for file in *.txt; do
    crush compress "$file" --force 2>> errors.log || echo "Failed: $file"
done
```

#### Pipeline Integration with Progress

```bash
# Download and compress on-the-fly
curl https://example.com/large-dataset.json | crush compress -o dataset.json.crush

# Note: Progress bar is hidden for stdin/stdout mode
```

#### Measure Compression Ratio

```bash
# Compare sizes
ls -lh data.txt data.txt.crush
# -rw-r--r-- 1 user user 1.2M Jan 27 14:30 data.txt
# -rw-r--r-- 1 user user 450K Jan 27 14:32 data.txt.crush

# Or use inspect command
crush inspect data.txt.crush | grep "reduction"
# Size: 1.2 MB → 450 KB (62.5% reduction)
```

## Command Reference

```bash
Usage: crush [OPTIONS] <COMMAND>

Commands:
  compress    Compress files
  decompress  Decompress files
  inspect     Inspect compressed file metadata
  config      Manage configuration
  plugins     Manage compression plugins
  help        Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...
          Enable verbose output (repeat for more verbosity: -v, -vv)

  -q, --quiet
          Suppress all output except errors

      --log-format <FORMAT>
          Log format: human or json

          [default: human]
          [possible values: human, json]

      --log-file <FILE>
          Log output file (default: stderr)

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

### crush compress

Compress files or stdin.

```bash
crush compress [OPTIONS] [INPUT]

Arguments:
  [INPUT]  Input file (omit for stdin)

Options:
  -o, --output <FILE>    Output file (default: INPUT.crush or stdout)
  -f, --force            Overwrite existing output file
  -p, --plugin <NAME>    Force specific compression plugin (default: auto)
  -v, --verbose          Increase verbosity (-v, -vv, -vvv)
      --log <FILE>       Log operations to file
  -h, --help             Print help
  --stout                Write output to stdout (for piping)
  --block <N>            Decompress only a specific block (random access, 0-indexed)
  --force-cpu            Force CPU-only decompression (skip GPU even for GPU-compressed files)
  --gpu-device <INDEX>   GPU device index to use for GPU-accelerated decompression
  --gpu-backend <BACKEND> GPU compute backend to use (default: auto)
                          Possible values:
                          - auto: Auto-select: try CUDA first, then wgpu
                          - cuda: Force CUDA backend (requires NVIDIA GPU + CUDA toolkit)
                          - wgpu: Force wgpu backend (Vulkan / Metal / DX12)
```

### crush decompress

Decompress files or stdin.

```bash
Usage: crush decompress [OPTIONS] [FILE]...

Arguments:
  [FILE]...
          Compressed files to decompress (reads from stdin if not provided with --stdout)

Options:
  -o, --output <PATH>
          Output file or directory (default: strip .crush extension)

  -f, --force
          Force overwrite of existing files

      --stdout
          Write output to stdout (for piping)

      --block <N>
          Decompress only a specific block (random access, 0-indexed)

      --force-cpu
          Force CPU-only decompression (skip GPU even for GPU-compressed files)

      --gpu-device <INDEX>
          GPU device index to use for GPU-accelerated decompression

      --gpu-backend <BACKEND>
          GPU compute backend to use (default: auto)

          Possible values:
          - auto: Auto-select: try CUDA first, then wgpu
          - cuda: Force CUDA backend (requires NVIDIA GPU + CUDA toolkit)
          - wgpu: Force wgpu backend (Vulkan / Metal / DX12)

          [default: auto]

  -h, --help
          Print help (see a summary with '-h')
```

### crush inspect

Inspect compressed files.

```bash
crush inspect [OPTIONS] <FILES>...

Arguments:
  <FILES>...  Compressed files to inspect

Options:
      --json    Output as JSON
      --csv     Output as CSV
  -v, --verbose Increase verbosity
  -h, --help    Print help
```

### crush config

Manage configuration.

```bash
Usage: crush config <COMMAND>

Commands:
  set    Set configuration value
  get    Get configuration value
  list   List all configuration
  reset  Reset to default configuration
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help

EXAMPLES:
    # Set compression level to fast
    crush config set compression.level fast

    # Get current compression level
    crush config get compression.level

    # List all configuration
    crush config list

    # Reset configuration to defaults
    crush config reset --yes

AVAILABLE KEYS:
    compression.default-plugin    Default plugin name (auto)
    compression.level             fast | balanced | best
    compression.timeout-seconds   Timeout in seconds (0 = no timeout)
    output.progress-bars          true | false
    output.color                  auto | always | never
    output.quiet                  true | false
    logging.format                human | json
    logging.level                 error | warn | info | debug | trace
    logging.file                  Log file path (empty = stderr)
    gpu.enabled                   true | false (auto-select gpu-deflate)
    gpu.device                    GPU device index or auto
    gpu.force-cpu                 true | false (force CPU decompression)
```

### `crush plugins`

Manage compression plugins.

```bash
crush plugins <COMMAND>

Commands:
  list    List available plugins
  info    Show plugin information
  test    Test plugin performance
  help    Print help
```

## Performance Tips

### Maximize Throughput

1. **Use pipelines**: Compress/decompress in-memory without writing intermediate files
2. **Parallel processing**: Crush automatically uses multiple threads for large files
3. **GPU decompression**: For datasets larger than ~2-4 MB, GPU decompression (auto-detected) outperforms CPU
4. **SIMD acceleration**: CPU compression uses libdeflater's SIMD-optimized DEFLATE implementation

### Optimize Compression Ratio vs Speed

```bash
# Fast compression (lower ratio, higher speed)
crush config set compression.level fast
crush compress large_file.bin

# Balanced (default)
crush config set compression.level balanced

# Best compression (higher ratio, slower)
crush config set compression.level best
```

### Benchmarking CLI

```bash
# Measure compression performance
time crush compress large_file.bin

# Measure with verbose output
crush compress large_file.bin -vv
# Shows: throughput, time, ratio
```

## Troubleshooting

### Common Errors

#### "File already exists"

```bash
# Use --force to overwrite
crush compress data.txt --force
```

#### "Invalid compressed file"

```bash
# File may be corrupted or not a Crush file
crush inspect data.txt.crush
# Verify: Is this a .crush file? Check the magic number.
```

#### "Plugin not found"

```bash
# List available plugins
crush plugins list

# Use auto-selection instead of forcing specific plugin
crush compress data.txt  # Uses --plugin auto by default
```

### Debug Mode

```bash
# Enable debug logging
RUST_LOG=debug crush compress data.txt --log debug.log

# View logs
cat debug.log
```

### Getting Help

```bash
# General help
crush --help

# Command-specific help
crush compress --help
crush decompress --help
crush inspect --help

# Version information
crush --version
```

## Integration Examples

### Shell Scripts

```bash
#!/bin/bash
# backup.sh - Compress and archive logs

LOG_DIR="/var/log/app"
BACKUP_DIR="/backup/logs"

for log in "$LOG_DIR"/*.log; do
    if [ -f "$log" ]; then
        crush compress "$log" --keep -o "$BACKUP_DIR/$(basename "$log").crush"
    fi
done
```

### CI/CD Pipeline

```yaml
# .github/workflows/compress-artifacts.yml
name: Compress Build Artifacts

jobs:
  compress:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Install Crush
        run: cargo install --path crush-cli

      - name: Build artifacts
        run: make build

      - name: Compress artifacts
        run: |
          crush compress target/release/app -o app.crush
          crush inspect app.crush --json > artifact-info.json

      - name: Upload
        uses: actions/upload-artifact@v3
        with:
          name: compressed-build
          path: |
            app.crush
            artifact-info.json
```

### Python Integration

```python
import subprocess
import json

def compress_file(input_path, output_path):
    """Compress a file using Crush."""
    result = subprocess.run(
        ["crush", "compress", input_path, "-o", output_path],
        capture_output=True,
        text=True
    )
    return result.returncode == 0

def inspect_file(compressed_path):
    """Inspect a compressed file and return metadata."""
    result = subprocess.run(
        ["crush", "inspect", compressed_path, "--json"],
        capture_output=True,
        text=True
    )
    if result.returncode == 0:
        return json.loads(result.stdout)
    return None

# Usage
compress_file("data.txt", "data.txt.crush")
info = inspect_file("data.txt.crush")
print(f"Compression ratio: {info['compression_ratio']:.1%}")
```

## Project Structure

```text
crush/
├── crush-core/          # Core compression library
│   ├── src/
│   │   ├── lib.rs
│   │   ├── compression.rs
│   │   ├── decompression.rs
│   │   ├── inspection.rs
│   │   └── plugin/      # Plugin system
│   └── Cargo.toml
├── crush-gpu/           # GPU-accelerated compression engine
│   ├── src/
│   │   ├── lib.rs       # Plugin registration
│   │   ├── engine.rs    # Compress/decompress API
│   │   ├── backend/     # GPU backends (wgpu, CUDA)
│   │   ├── shader/      # WGSL compute shaders
│   │   ├── format.rs    # CGPU binary file format
│   │   ├── gdeflate.rs  # GDeflate algorithm
│   │   ├── scorer.rs    # GPU eligibility scoring
│   │   └── entropy.rs   # Shannon entropy analysis
│   ├── benches/         # Criterion benchmarks
│   └── Cargo.toml
├── crush-cli/           # CLI application
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/    # Command implementations
│   │   ├── config.rs    # Configuration management
│   │   └── output.rs    # Output formatting
│   └── Cargo.toml
└── Cargo.toml           # Workspace manifest
```

## Development

### Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Build specific crate
cargo build -p crush-core
cargo build -p crush-gpu
cargo build -p crush-cli
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test -p crush-core
cargo test -p crush-gpu
cargo test -p crush-cli

# Run integration tests only
cargo test --test '*'
```

### Benchmarking

```bash
# Run all benchmarks
cargo bench

# CPU compression/decompression throughput
cargo bench -p crush-core --bench throughput

# GPU decompression throughput (requires compatible GPU)
cargo bench -p crush-gpu --bench throughput

# CLI startup benchmarks
cargo bench -p crush-cli --bench cli_startup
```

### Code Quality

```bash
# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings

# Generate documentation
cargo doc --no-deps --open
```

## Contributing

See the [Contributing Guide](CONTRIBUTING.md) for details on how to contribute to Crush.

## License

This project is licensed under [LICENSE] - see the LICENSE file for details.

## Acknowledgments

- Inspired by [pigz](https://zlib.net/pigz/) by Mark Adler
- Built with [Rust](https://www.rust-lang.org/)
- DEFLATE implementation via [libdeflater](https://github.com/ebiggers/libdeflate) (SIMD-optimized)
- GPU compute via [wgpu](https://wgpu.rs/) (cross-platform Vulkan/Metal/DX12)
- GDeflate format based on [Microsoft DirectStorage GDeflate](https://github.com/microsoft/DirectStorage)
