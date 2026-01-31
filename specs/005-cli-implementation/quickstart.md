# Quickstart Guide: Crush CLI

**Feature**: CLI Implementation | **Branch**: `005-cli-implementation` | **Date**: 2026-01-22

Get started with the Crush command-line compression tool in minutes.

## Installation

```bash
# Install from source (requires Rust 1.84+)
git clone https://github.com/your-org/crush.git
cd crush
cargo build --release

# Binary will be at: target/release/crush
# Copy to PATH:
sudo cp target/release/crush /usr/local/bin/

# Verify installation
crush --version
```

## Basic Usage

### Compress a File

Compress a single file:

```bash
crush compress myfile.txt
```

This creates `myfile.txt.crush` in the same directory.

### Decompress a File

Restore the original file:

```bash
crush decompress myfile.txt.crush
```

This creates `myfile.txt` (the original).

### Inspect a Compressed File

View metadata without decompressing:

```bash
crush inspect myfile.txt.crush
```

Output:
```
File: myfile.txt.crush
Original size: 1048576 bytes (1.0 MB)
Compressed size: 524288 bytes (512.0 KB)
Compression ratio: 50.0%
Plugin: deflate (v1.0.0)
CRC32: VALID
```

## Common Operations

### Compress Multiple Files

```bash
# Compress all .txt files
crush compress *.txt

# Compress specific files
crush compress file1.txt file2.txt file3.txt
```

### Batch Decompression

```bash
# Decompress all .crush files in current directory
crush decompress *.crush
```

### Custom Output Location

```bash
# Compress to specific output file
crush compress input.txt -o /backup/output.crush

# Decompress to specific location
crush decompress data.crush -o /restored/data.txt
```

### Keep Original Files

By default, Crush removes the input file after compression/decompression. To keep it:

```bash
# Compress and keep input
crush compress -k myfile.txt

# Now you have both myfile.txt and myfile.txt.crush

# Decompress and keep compressed file
crush decompress -k backup.crush
```

## Compression Levels

Choose the right balance between speed and compression ratio:

### Fast Compression (Prioritize Speed)

```bash
crush compress --level fast large-file.txt
```

Best for: Large files, time-sensitive operations, already-compressed data

### Balanced Compression (Default)

```bash
crush compress --level balanced document.txt
# or simply:
crush compress document.txt
```

Best for: General use, mixed workloads

### Best Compression (Prioritize Ratio)

```bash
crush compress --level best archive.txt
```

Best for: Long-term storage, bandwidth-constrained transfers

## Working with Pipelines

### Compress stdin to stdout

```bash
cat largefile.txt | crush compress -o - > compressed.crush
```

### Decompress to stdout

```bash
crush decompress data.crush --stdout | less
# View compressed file without decompressing to disk

crush decompress backup.crush --stdout | tar xf -
# Decompress and extract in one step
```

## Progress and Feedback

### Normal Mode (Default)

Shows progress bars for operations taking >2 seconds:

```bash
crush compress large-file.txt
```

Output:
```
Compressing large-file.txt... [========>   ] 75% (3.2s remaining)
Compressed large-file.txt -> large-file.txt.crush (50.2% compression, 234.5 MB/s)
```

### Quiet Mode

Suppress all output except errors:

```bash
crush compress -q myfile.txt
# No output unless there's an error
```

Useful for scripts where you only care about success/failure.

### Verbose Mode

See detailed diagnostics:

```bash
crush compress -v myfile.txt
```

Output:
```
[DEBUG] Selected plugin: deflate (score: 0.85)
[DEBUG] Thread pool: 8 threads
[DEBUG] Hardware acceleration: AVX2 enabled
[DEBUG] Block size: 1048576 bytes

Compressing myfile.txt... [==========] 100%
Compressed myfile.txt -> myfile.txt.crush

Details:
  Plugin: deflate
  Threads: 8
  Hardware acceleration: AVX2
  Input size: 100.0 MB
  Output size: 50.2 MB
  Compression ratio: 50.2%
  Duration: 4.2s
  Throughput: 234.5 MB/s
```

### Extra Verbose Mode

For low-level debugging:

```bash
crush compress -vv myfile.txt
```

Shows every block processed, compression decisions, and internal timings.

## Configuration

### Set Default Preferences

```bash
# Use fast compression by default
crush config set compression.level fast

# Always keep input files
crush config set compression.default-plugin deflate

# Disable progress bars
crush config set output.progress-bars false
```

### View Current Configuration

```bash
crush config list
```

Output:
```
[compression]
default-plugin = auto
level = balanced
timeout-seconds = 0

[output]
progress-bars = true
color = auto
quiet = false

[logging]
format = human
level = info
file =
```

### Get Specific Setting

```bash
crush config get compression.level
# Output: balanced
```

### Reset to Defaults

```bash
crush config reset
# Prompts for confirmation

crush config reset --yes
# No confirmation
```

## Plugin Management

### List Available Plugins

```bash
crush plugins list
```

Output:
```
Available compression plugins:

deflate (v1.0.0)
  DEFLATE compression (RFC 1951)
  Throughput: 250 MB/s
  Compression ratio: 0.55
  Best for: general-purpose data

lz4 (v1.0.0)
  LZ4 fast compression
  Throughput: 800 MB/s
  Compression ratio: 0.70
  Best for: speed-critical applications

Total: 2 plugins
```

### View Plugin Details

```bash
crush plugins info deflate
```

### Force Specific Plugin

```bash
# Use LZ4 instead of auto-selection
crush compress --plugin lz4 myfile.txt
```

### Test Plugin

```bash
# Verify plugin is working correctly
crush plugins test deflate
```

Output:
```
Testing plugin: deflate

[1/3] Compressing test data... OK (0.002s)
[2/3] Decompressing result... OK (0.001s)
[3/3] Validating roundtrip... OK

Test passed: deflate is working correctly.
```

## Advanced Features

### Timeout Protection

Prevent runaway compression operations:

```bash
# Abort if compression takes longer than 60 seconds
crush compress --timeout 60 problematic-file.txt
```

### Force Overwrite

By default, Crush won't overwrite existing files. To allow:

```bash
crush compress -f myfile.txt
# Overwrites myfile.txt.crush if it exists
```

### Production Logging

For monitoring in production environments:

```bash
# JSON-structured logs
crush compress --log-format json myfile.txt

# Log to file
crush compress --log-file /var/log/crush.log myfile.txt

# Both
crush compress --log-format json --log-file operations.log myfile.txt
```

JSON log example:
```json
{
  "timestamp": "2026-01-22T10:30:45.123Z",
  "level": "INFO",
  "target": "crush_cli::commands::compress",
  "fields": {
    "message": "Compression completed",
    "plugin": "deflate",
    "input_size": 104857600,
    "output_size": 52428800,
    "ratio": 50.0,
    "throughput_mbps": 234.5,
    "duration_ms": 4200
  }
}
```

## Environment Variables

Override settings without changing configuration:

```bash
# Use specific plugin
export CRUSH_COMPRESSION_DEFAULT_PLUGIN=lz4
crush compress myfile.txt

# Enable JSON logging
export CRUSH_LOGGING_FORMAT=json
crush compress myfile.txt

# Disable colors
export NO_COLOR=1
crush compress myfile.txt
```

**Precedence** (highest to lowest):
1. Command-line flags
2. Environment variables
3. Config file
4. Built-in defaults

## Getting Help

### General Help

```bash
crush --help
```

### Command-Specific Help

```bash
crush compress --help
crush decompress --help
crush inspect --help
crush config --help
crush plugins --help
```

### "Did You Mean" Suggestions

```bash
crush compres myfile.txt
# Error: 'compres' is not a valid command. Did you mean 'compress'?
```

## Tips and Best Practices

### 1. Use Appropriate Compression Levels

- **Fast**: Logs, temporary files, data that will be deleted soon
- **Balanced**: General use, most file types
- **Best**: Archives, long-term storage, bandwidth-constrained transfers

### 2. Let Auto-Selection Work

Unless you have specific requirements, let Crush automatically select the best plugin:

```bash
# Auto-selection is usually best
crush compress myfile.txt

# Only force plugin if you have a reason
crush compress --plugin deflate myfile.txt
```

### 3. Use Verbose Mode for Troubleshooting

If compression is slower than expected:

```bash
crush compress -v myfile.txt
# Check: Which plugin was selected? How many threads? Hardware acceleration?
```

### 4. Batch Processing

For many files, compress/decompress in a single command:

```bash
crush compress *.log
# More efficient than running crush 100 times
```

### 5. Verify with Inspect

Before decompressing large files, check integrity:

```bash
crush inspect backup.crush
# Verify CRC32 is VALID before decompressing
```

### 6. Keep Backups

When compressing important data:

```bash
crush compress -k important-data.txt
# Keep original until you've verified the compressed version
```

## Common Scenarios

### Scenario: Compress Log Files for Archival

```bash
# Compress all .log files with best compression, keep originals
crush compress --level best -k *.log

# Verify compression ratio
crush inspect *.crush -s
```

### Scenario: Fast Compression for CI/CD Artifacts

```bash
# Prioritize speed for build artifacts
crush compress --level fast build-output.tar

# Upload compressed artifact
aws s3 cp build-output.tar.crush s3://my-bucket/
```

### Scenario: Decompress and Pipe to Another Tool

```bash
# Decompress and extract without writing to disk
crush decompress backup.tar.crush --stdout | tar xf -

# Decompress and search without decompressing to disk
crush decompress logs.txt.crush --stdout | grep ERROR
```

### Scenario: Monitor Compression in Production

```bash
# JSON logging to file with timestamps
crush compress \
  --log-format json \
  --log-file /var/log/crush.log \
  /data/*.csv

# Parse logs later
jq '.fields.throughput_mbps' /var/log/crush.log
```

### Scenario: Verify All Compressed Files

```bash
# Test integrity of all .crush files
for file in *.crush; do
  crush inspect "$file" | grep -q "CRC32: VALID" || echo "CORRUPT: $file"
done
```

## Troubleshooting

### Problem: "Permission denied" error

**Solution**: Check file permissions and output directory:

```bash
# Check input file is readable
ls -l myfile.txt

# Check output directory is writable
touch test-write && rm test-write
```

### Problem: "Disk full" during compression

**Solution**: Ensure enough free space (at least input file size):

```bash
df -h .
# Check available space in current directory
```

### Problem: Compression seems slow

**Solution**: Use verbose mode to diagnose:

```bash
crush compress -v myfile.txt
# Check thread count and hardware acceleration
```

If threads = 1, verify multi-threading is available:
```bash
lscpu | grep "CPU(s)"
# How many cores do you have?
```

### Problem: "Plugin not found" error

**Solution**: List available plugins:

```bash
crush plugins list
# Verify plugin name is correct
```

### Problem: Decompression fails with CRC error

**Solution**: File may be corrupted. Verify with inspect:

```bash
crush inspect corrupted.crush
# If CRC32: INVALID, file is damaged
```

## Next Steps

- **Read the full manual**: `man crush` (if installed)
- **Explore advanced options**: `crush <command> --help`
- **Contribute**: https://github.com/your-org/crush
- **Report issues**: https://github.com/your-org/crush/issues

## Quick Reference

```bash
# Compress
crush compress file.txt                    # Basic compression
crush compress -l fast file.txt            # Fast compression
crush compress -l best -k file.txt         # Best compression, keep original
crush compress -p deflate file.txt         # Force plugin

# Decompress
crush decompress file.crush                # Basic decompression
crush decompress -k file.crush             # Keep compressed file
crush decompress --stdout file.crush       # Output to stdout

# Inspect
crush inspect file.crush                   # View metadata
crush inspect -f json file.crush           # JSON output
crush inspect -s *.crush                   # Summary of multiple files

# Configuration
crush config set <key> <value>             # Set config
crush config get <key>                     # Get config
crush config list                          # Show all config
crush config reset                         # Reset to defaults

# Plugins
crush plugins list                         # List plugins
crush plugins info <name>                  # Plugin details
crush plugins test <name>                  # Test plugin

# Help
crush --help                               # General help
crush <command> --help                     # Command help
```

## Exit Codes

```bash
echo $?    # Check exit code after running crush

# 0   = Success
# 1   = Operational error (compression failed, corruption, etc.)
# 2   = Usage error (invalid arguments, missing files)
# 130 = Interrupted (Ctrl+C)
```

---

**Version**: 1.0.0 | **Last Updated**: 2026-01-22
