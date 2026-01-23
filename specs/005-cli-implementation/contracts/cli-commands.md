# CLI Command Contracts

**Feature**: CLI Implementation | **Branch**: `005-cli-implementation` | **Date**: 2026-01-22

This document defines the behavioral contracts for all CLI commands, including inputs, outputs, error conditions, and exit codes.

## Contract Format

Each contract specifies:
- **Command**: Full command syntax
- **Inputs**: Required and optional arguments
- **Preconditions**: Requirements that must be met before execution
- **Postconditions**: Guaranteed state after successful execution
- **Outputs**: What is written to stdout/stderr
- **Exit Codes**: Return values indicating success/failure
- **Error Conditions**: Specific failure scenarios and handling

---

## Global Flags

These flags apply to all commands:

```
--verbose, -v        Enable verbose output (repeat for more: -vv)
--quiet, -q          Suppress all output except errors
--log-format FORMAT  Set log format: human | json
--log-file FILE      Write logs to file (default: stderr)
--help, -h           Show help message
--version, -V        Show version information
```

**Contracts**:
- `--verbose` and `--quiet` are mutually exclusive
- `--verbose` can be repeated: `-v` (debug), `-vv` (trace)
- `--help` exits immediately with code 0 after printing help
- `--version` exits immediately with code 0 after printing version

---

## Command: `crush compress`

### Syntax

```
crush compress [OPTIONS] <FILE>...
```

### Inputs

**Required**:
- `<FILE>...`: One or more input files to compress

**Optional**:
- `-o, --output <PATH>`: Output file or directory
- `-p, --plugin <PLUGIN>`: Force specific compression plugin
- `-l, --level <LEVEL>`: Compression level (fast | balanced | best)
- `-f, --force`: Overwrite existing output files
- `-k, --keep`: Keep input files after compression
- `--timeout <SECONDS>`: Compression timeout

### Preconditions

1. All input files MUST exist
2. All input files MUST be readable
3. All input files MUST be regular files (not directories or special files)
4. Output path MUST be writable (parent directory exists)
5. If output file exists, `--force` MUST be specified or operation fails

### Postconditions (Success)

1. For each input file, a compressed output file is created
2. Output file has `.crush` extension (unless custom output specified)
3. Output file contains valid Crush header + compressed data + CRC32
4. If `--keep` not specified, input files are deleted
5. Exit code is 0

### Outputs

**Stdout** (unless `--quiet`):
```
Compressing file1.txt... [========>   ] 75% (3.2s remaining)
Compressed file1.txt -> file1.txt.crush (50.2% compression, 234.5 MB/s)

Summary:
  Files: 1
  Total input: 100.0 MB
  Total output: 50.2 MB
  Compression ratio: 50.2%
  Total time: 4.2s
  Average throughput: 234.5 MB/s
```

**Stderr** (verbose mode `-v`):
```
[DEBUG] Selected plugin: deflate (score: 0.85)
[DEBUG] Thread pool: 8 threads
[DEBUG] Hardware acceleration: AVX2 enabled
[DEBUG] Block size: 1048576 bytes
[DEBUG] Timeout: none
```

**Stderr** (verbose mode `-vv`):
```
[TRACE] Reading block 1 (1048576 bytes)
[TRACE] Compressing block 1 with deflate
[TRACE] Compressed block 1: 524288 bytes (50.0% ratio)
[TRACE] Writing compressed block 1
...
```

### Exit Codes

- `0`: All files compressed successfully
- `1`: One or more files failed to compress
- `2`: Invalid arguments or missing files
- `130`: Interrupted by Ctrl+C

### Error Conditions

| Condition | Error Message | Exit Code |
|-----------|--------------|-----------|
| Input file not found | `Error: File does not exist: <path>` | 2 |
| Input file not readable | `Error: Permission denied: <path>` | 2 |
| Input is directory | `Error: Cannot compress directory: <path>` | 2 |
| Output exists, no `--force` | `Error: Output file already exists (use --force to overwrite): <path>` | 2 |
| Output not writable | `Error: Cannot write to output path: <path>` | 2 |
| Plugin not found | `Error: Plugin '<name>' not found. Run 'crush plugins list' to see available plugins.` | 2 |
| Compression timeout | `Error: Compression timeout after <N>s: <path>` | 1 |
| Disk full | `Error: No space left on device: <path>` | 1 |
| Ctrl+C pressed | `Interrupted. Cleaning up partial files...` | 130 |

### Examples

```bash
# Compress single file
crush compress input.txt
# Output: input.txt.crush

# Compress multiple files
crush compress file1.txt file2.txt file3.txt

# Compress with custom output
crush compress input.txt -o compressed/output.crush

# Fast compression, keep input
crush compress -l fast -k input.txt

# Force specific plugin
crush compress -p deflate input.txt

# Verbose output
crush compress -v input.txt
```

---

## Command: `crush decompress`

### Syntax

```
crush decompress [OPTIONS] <FILE>...
```

### Inputs

**Required**:
- `<FILE>...`: One or more compressed files to decompress

**Optional**:
- `-o, --output <PATH>`: Output file or directory
- `-f, --force`: Overwrite existing output files
- `-k, --keep`: Keep compressed files after decompression
- `--stdout`: Write output to stdout (pipeline mode)

### Preconditions

1. All input files MUST exist and be readable
2. All input files MUST have valid Crush header
3. Output path MUST be writable
4. If output exists, `--force` MUST be specified

### Postconditions (Success)

1. For each input, a decompressed output file is created
2. Output file content matches original (pre-compression) data
3. CRC32 validation passes
4. If `--keep` not specified, compressed files are deleted
5. File timestamps/permissions restored (best effort)
6. Exit code is 0

### Outputs

**Stdout** (unless `--quiet`):
```
Decompressing file1.txt.crush... [=========>  ] 80% (0.8s remaining)
Decompressed file1.txt.crush -> file1.txt (523.4 MB/s)

Summary:
  Files: 1
  Total input: 50.2 MB
  Total output: 100.0 MB
  Total time: 0.19s
  Average throughput: 523.4 MB/s
```

### Exit Codes

- `0`: All files decompressed successfully
- `1`: One or more files failed to decompress
- `2`: Invalid arguments or missing files
- `130`: Interrupted by Ctrl+C

### Error Conditions

| Condition | Error Message | Exit Code |
|-----------|--------------|-----------|
| File not found | `Error: File does not exist: <path>` | 2 |
| Invalid header | `Error: Not a valid Crush archive: <path>` | 1 |
| CRC32 mismatch | `Error: File corrupted (CRC32 mismatch): <path>. The compressed file may be damaged.` | 1 |
| Plugin missing | `Error: Required plugin '<name>' not found. The file was compressed with a plugin not installed.` | 1 |
| Output exists | `Error: Output file already exists (use --force to overwrite): <path>` | 2 |
| Disk full | `Error: No space left on device: <path>` | 1 |

### Examples

```bash
# Decompress single file
crush decompress input.txt.crush
# Output: input.txt

# Decompress to stdout (pipeline)
crush decompress data.crush --stdout | less

# Decompress multiple files
crush decompress file1.crush file2.crush

# Keep compressed files
crush decompress -k backup.crush
```

---

## Command: `crush inspect`

### Syntax

```
crush inspect [OPTIONS] <FILE>...
```

### Inputs

**Required**:
- `<FILE>...`: One or more compressed files to inspect

**Optional**:
- `-f, --format <FORMAT>`: Output format (human | json | csv)
- `-s, --summary`: Show summary statistics for multiple files

### Preconditions

1. All input files MUST exist and be readable

### Postconditions (Success)

1. Metadata is extracted from all files
2. CRC32 validation is performed
3. Results are displayed in requested format
4. Exit code is 0

### Outputs

**Stdout** (human format, single file):
```
File: data.txt.crush
Original size: 104857600 bytes (100.0 MB)
Compressed size: 52428800 bytes (50.0 MB)
Compression ratio: 50.0%
Plugin: deflate (v1.0.0)
CRC32: VALID
Created: 2026-01-22 10:30:45
```

**Stdout** (human format, multiple files with `--summary`):
```
File: file1.crush
  Original: 100.0 MB | Compressed: 50.0 MB | Ratio: 50.0% | Plugin: deflate

File: file2.crush
  Original: 200.0 MB | Compressed: 120.0 MB | Ratio: 60.0% | Plugin: lz4

Summary:
  Files: 2
  Total original: 300.0 MB
  Total compressed: 170.0 MB
  Average compression ratio: 56.7%
```

**Stdout** (JSON format):
```json
[
  {
    "file_path": "data.txt.crush",
    "original_size": 104857600,
    "compressed_size": 52428800,
    "compression_ratio": 50.0,
    "plugin_name": "deflate",
    "plugin_version": "1.0.0",
    "crc_valid": true,
    "created_at": "2026-01-22T10:30:45Z"
  }
]
```

**Stdout** (CSV format):
```csv
file_path,original_size,compressed_size,compression_ratio,plugin,crc_valid
data.txt.crush,104857600,52428800,50.0,deflate,true
```

### Exit Codes

- `0`: All files inspected successfully
- `1`: One or more files could not be inspected
- `2`: Invalid arguments or missing files

### Error Conditions

| Condition | Error Message | Exit Code |
|-----------|--------------|-----------|
| File not found | `Error: File does not exist: <path>` | 2 |
| Invalid header | `Error: Not a valid Crush archive: <path>` | 1 |
| Corrupt file | `Warning: CRC32 validation failed: <path>` (continues) | 0 (warning only) |

### Examples

```bash
# Inspect single file
crush inspect data.crush

# Inspect multiple files with summary
crush inspect -s *.crush

# JSON output for scripting
crush inspect -f json data.crush | jq '.compression_ratio'

# CSV for spreadsheet import
crush inspect -f csv *.crush > report.csv
```

---

## Command: `crush config set`

### Syntax

```
crush config set <KEY> <VALUE>
```

### Inputs

**Required**:
- `<KEY>`: Configuration key (dot-notation: `section.key`)
- `<VALUE>`: Configuration value

### Preconditions

1. Key MUST be a valid configuration path
2. Value MUST match expected type for key

### Postconditions (Success)

1. Configuration file is updated with new value
2. File is written to disk
3. Success message is displayed
4. Exit code is 0

### Outputs

**Stdout**:
```
Configuration updated: compression.default-plugin = deflate
```

### Exit Codes

- `0`: Configuration updated successfully
- `2`: Invalid key or value

### Error Conditions

| Condition | Error Message | Exit Code |
|-----------|--------------|-----------|
| Invalid key | `Error: Unknown configuration key: <key>` | 2 |
| Invalid value | `Error: Invalid value for <key>: <value>` | 2 |
| Config file not writable | `Error: Cannot write configuration file: <path>` | 1 |

### Valid Keys

```
compression.default-plugin
compression.level
compression.timeout-seconds
output.progress-bars
output.color
output.quiet
logging.format
logging.level
logging.file
```

### Examples

```bash
# Set default plugin
crush config set compression.default-plugin deflate

# Set compression level
crush config set compression.level fast

# Disable progress bars
crush config set output.progress-bars false

# Enable JSON logging
crush config set logging.format json
```

---

## Command: `crush config get`

### Syntax

```
crush config get <KEY>
```

### Inputs

**Required**:
- `<KEY>`: Configuration key to retrieve

### Preconditions

1. Key MUST be a valid configuration path

### Postconditions (Success)

1. Current value is displayed
2. Exit code is 0

### Outputs

**Stdout**:
```
balanced
```

### Exit Codes

- `0`: Value retrieved successfully
- `2`: Invalid key

### Examples

```bash
# Get current compression level
crush config get compression.level
# Output: balanced

# Use in scripts
LEVEL=$(crush config get compression.level)
```

---

## Command: `crush config list`

### Syntax

```
crush config list
```

### Preconditions

None (always succeeds)

### Outputs

**Stdout**:
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

### Exit Codes

- `0`: Always

### Examples

```bash
# Show all configuration
crush config list

# Save to file
crush config list > my-config-backup.toml
```

---

## Command: `crush config reset`

### Syntax

```
crush config reset [OPTIONS]
```

### Inputs

**Optional**:
- `-y, --yes`: Skip confirmation prompt

### Preconditions

None

### Postconditions (Success)

1. Configuration file is reset to defaults
2. Confirmation is shown (unless `--yes`)
3. Exit code is 0

### Outputs

**Stdout** (interactive):
```
Reset configuration to defaults? [y/N]: y
Configuration reset to defaults.
```

**Stdout** (with `--yes`):
```
Configuration reset to defaults.
```

### Exit Codes

- `0`: Reset completed or user cancelled
- `1`: Failed to write configuration file

### Examples

```bash
# Interactive reset
crush config reset

# Non-interactive reset
crush config reset --yes
```

---

## Command: `crush plugins list`

### Syntax

```
crush plugins list [OPTIONS]
```

### Inputs

**Optional**:
- `-f, --format <FORMAT>`: Output format (human | json)

### Preconditions

1. Plugin registry MUST be initialized (automatic)

### Postconditions (Success)

1. All registered plugins are listed
2. Exit code is 0

### Outputs

**Stdout** (human format):
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

**Stdout** (JSON format):
```json
[
  {
    "name": "deflate",
    "version": "1.0.0",
    "description": "DEFLATE compression (RFC 1951)",
    "throughput": 250.0,
    "compression_ratio": 0.55,
    "features": ["general-purpose"]
  },
  {
    "name": "lz4",
    "version": "1.0.0",
    "description": "LZ4 fast compression",
    "throughput": 800.0,
    "compression_ratio": 0.70,
    "features": ["speed"]
  }
]
```

### Exit Codes

- `0`: Always (even if no plugins found)

### Examples

```bash
# List plugins (human-readable)
crush plugins list

# JSON output for scripting
crush plugins list -f json | jq '.[0].name'
```

---

## Command: `crush plugins info`

### Syntax

```
crush plugins info <PLUGIN>
```

### Inputs

**Required**:
- `<PLUGIN>`: Plugin name

### Preconditions

1. Plugin MUST exist in registry

### Postconditions (Success)

1. Detailed plugin information is displayed
2. Exit code is 0

### Outputs

**Stdout**:
```
Plugin: deflate
Version: 1.0.0
Description: DEFLATE compression (RFC 1951)

Performance:
  Throughput: 250 MB/s
  Compression ratio: 0.55 (45% size reduction)

Characteristics:
  Best for: General-purpose data, text files
  Algorithm: LZ77 + Huffman coding
  Block size: 32 KB - 1 MB
  Memory usage: ~270 KB per thread

Benchmarks:
  Calgary corpus: 43.2% compression
  Canterbury corpus: 46.8% compression
  Silesia corpus: 52.1% compression
```

### Exit Codes

- `0`: Plugin found and displayed
- `2`: Plugin not found

### Error Conditions

| Condition | Error Message | Exit Code |
|-----------|--------------|-----------|
| Plugin not found | `Error: Plugin '<name>' not found. Run 'crush plugins list' to see available plugins.` | 2 |

### Examples

```bash
# Show plugin details
crush plugins info deflate

# Compare plugins
crush plugins info deflate > deflate.txt
crush plugins info lz4 > lz4.txt
diff deflate.txt lz4.txt
```

---

## Command: `crush plugins test`

### Syntax

```
crush plugins test <PLUGIN>
```

### Inputs

**Required**:
- `<PLUGIN>`: Plugin name to test

### Preconditions

1. Plugin MUST exist in registry

### Postconditions (Success)

1. Plugin self-test is executed
2. Roundtrip compress/decompress validation passes
3. Exit code is 0

### Outputs

**Stdout**:
```
Testing plugin: deflate

[1/3] Compressing test data... OK (0.002s)
[2/3] Decompressing result... OK (0.001s)
[3/3] Validating roundtrip... OK

Test passed: deflate is working correctly.
```

### Exit Codes

- `0`: Test passed
- `1`: Test failed
- `2`: Plugin not found

### Error Conditions

| Condition | Error Message | Exit Code |
|-----------|--------------|-----------|
| Plugin not found | `Error: Plugin '<name>' not found.` | 2 |
| Compression failed | `Error: Plugin test failed during compression.` | 1 |
| Decompression failed | `Error: Plugin test failed during decompression.` | 1 |
| Roundtrip mismatch | `Error: Plugin test failed: decompressed data does not match original.` | 1 |

### Examples

```bash
# Test single plugin
crush plugins test deflate

# Test all plugins
for plugin in $(crush plugins list -f json | jq -r '.[].name'); do
  crush plugins test $plugin
done
```

---

## Exit Code Summary

| Code | Meaning | When Used |
|------|---------|-----------|
| 0 | Success | All operations completed successfully |
| 1 | Operational error | Compression/decompression failed, corruption, etc. |
| 2 | Usage error | Invalid arguments, missing files, bad configuration |
| 130 | Interrupted | User pressed Ctrl+C (128 + SIGINT) |

---

## Contract Testing

All contracts are validated by integration tests in `crush-cli/tests/integration/`:

```rust
#[test]
fn compress_creates_output_file() {
    // Postcondition: Output file is created with .crush extension
}

#[test]
fn compress_without_force_fails_if_output_exists() {
    // Error condition: Output exists, no --force
}

#[test]
fn compress_respects_keep_flag() {
    // Postcondition: Input file kept when --keep specified
}

// ... one test per contract clause
```

**Coverage Target**: >90% of contract clauses have explicit tests.
