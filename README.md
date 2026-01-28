# Crush

Crush is a Rust-based port of pigz that adds support for hardware acceleration and a plugin architecture for file-format-specific compression extensions.

It is designed for high-throughput data pipelines, particularly in AI and ML workflows where large datasets are frequently ingested from internet sources. Fast compression enables files to be compressed quickly, transferred efficiently over the network, and decompressed with minimal overhead, reducing end-to-end data ingestion time.

## Why Crush?

Modern AI and ML pipelines move massive amounts of data, often across networks and from untrusted or bandwidth-constrained sources. In these environments, compression speed matters just as much as compression ratio.

Crush is built to:

- Maximize throughput using parallel compression and hardware acceleration
- Reduce network transfer time by compressing data as early as possible in the pipeline
- Adapt to data formats through a plugin model that enables format-aware compression strategies
- Integrate cleanly with Rust-based systems, offering safety, performance, and predictable behavior

By focusing on fast, extensible compression rather than one-size-fits-all algorithms, Crush helps data pipelines move faster without becoming a bottleneck.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/crush.git
cd crush

# Build release binary
cargo build --release

# Install to ~/.cargo/bin
cargo install --path crush-cli
```

### Binary Release

Download the latest release from [GitHub Releases](https://github.com/yourusername/crush/releases).

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

### `crush compress`

Compress files or stdin.

```bash
crush compress [OPTIONS] [INPUT]

Arguments:
  [INPUT]  Input file (omit for stdin)

Options:
  -o, --output <FILE>    Output file (default: INPUT.crush or stdout)
  -k, --keep             Keep input file after compression
  -f, --force            Overwrite existing output file
  -p, --plugin <NAME>    Force specific compression plugin (default: auto)
  -v, --verbose          Increase verbosity (-v, -vv, -vvv)
      --log <FILE>       Log operations to file
  -h, --help             Print help
```

### `crush decompress`

Decompress files or stdin.

```bash
crush decompress [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Input file or stdin (use '-' for stdin)

Options:
  -o, --output <FILE>    Output file (default: removes .crush extension or stdout)
  -k, --keep             Keep input file after decompression
  -f, --force            Overwrite existing output file
  -c, --stdout           Write to stdout
  -v, --verbose          Increase verbosity (-v, -vv, -vvv)
      --log <FILE>       Log operations to file
  -h, --help             Print help
```

### `crush inspect`

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

### `crush config`

Manage configuration.

```bash
crush config <COMMAND>

Commands:
  set     Set configuration value
  get     Get configuration value
  list    List all configuration
  reset   Reset configuration to defaults
  help    Print help
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
3. **Hardware acceleration**: Plugins can utilize CPU-specific instructions (when available)

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

### Benchmarking

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

```
crush/
├── crush-core/          # Core compression library
│   ├── src/
│   │   ├── lib.rs
│   │   ├── compression.rs
│   │   ├── decompression.rs
│   │   ├── inspection.rs
│   │   └── plugin/      # Plugin system
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
cargo build -p crush-cli
cargo build -p crush-core
```

### Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test -p crush-cli
cargo test -p crush-core

# Run integration tests only
cargo test --test '*'
```

### Benchmarking

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench cli_startup
cargo bench --bench help_command
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
- Compression algorithms from [flate2](https://github.com/rust-lang/flate2-rs)
