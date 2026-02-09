# crush-cli

Command-line interface for the [crush-core](https://github.com/john-agentic-ai-tools/crush) high-performance parallel compression library.

## Installation

From crates.io:

```bash
cargo install crush-cli
```

From source:

```bash
git clone https://github.com/john-agentic-ai-tools/crush.git
cd crush
cargo install --path crush-cli
```

## Usage

### Compress

```bash
# Compress a file
crush compress file.txt

# Compress multiple files
crush compress file1.txt file2.txt file3.txt

# Choose a compression level (fast, balanced, best)
crush compress --level fast largefile.dat

# Use a specific plugin
crush compress --plugin deflate data.bin

# Compress to a specific output path
crush compress input.txt --output /backup/input.txt.crush

# Force overwrite existing compressed files
crush compress --force document.txt

# Set a compression timeout (in seconds)
crush compress --timeout 60 largefile.dat

# Pipeline: stdin to file
cat file.txt | crush compress --output file.txt.crush

# Pipeline: stdin to stdout
cat file.txt | crush compress --stdout > file.txt.crush
```

### Decompress

```bash
# Decompress a file (outputs to <name> with .crush extension stripped)
crush decompress file.txt.crush

# Decompress multiple files
crush decompress file1.crush file2.crush

# Decompress to a specific output path
crush decompress archive.crush --output /tmp/restored.txt

# Decompress to stdout for piping
crush decompress data.crush --stdout | grep pattern

# Pipeline: stdin to stdout
cat data.crush | crush decompress --stdout

# Force overwrite existing files
crush decompress --force document.txt.crush
```

### Inspect

```bash
# Inspect compressed file metadata
crush inspect file.txt.crush

# Inspect multiple files with summary statistics
crush inspect --summary *.crush

# Output as JSON
crush inspect --format json archive.crush

# Output as CSV
crush inspect --format csv *.crush > report.csv
```

### Plugins

```bash
# List all available compression plugins
crush plugins list

# List plugins in JSON format
crush plugins list --format json

# Show detailed information about a plugin
crush plugins info deflate

# Test a plugin's functionality
crush plugins test deflate
```

### Configuration

Crush stores its configuration in a TOML file at the OS config directory (`~/.config/crush/config.toml` on Linux).

```bash
# Set a configuration value
crush config set compression.level fast

# Get a configuration value
crush config get compression.level

# List all configuration
crush config list

# Reset to defaults
crush config reset --yes
```

**Configuration keys:**

| Key | Values | Default |
| --- | --- | --- |
| `compression.default-plugin` | Plugin name or `auto` | `auto` |
| `compression.level` | `fast`, `balanced`, `best` | `balanced` |
| `compression.timeout-seconds` | Seconds (`0` = no timeout) | `0` |
| `output.progress-bars` | `true`, `false` | `true` |
| `output.color` | `auto`, `always`, `never` | `auto` |
| `output.quiet` | `true`, `false` | `false` |
| `logging.format` | `human`, `json` | `human` |
| `logging.level` | `error`, `warn`, `info`, `debug`, `trace` | `info` |
| `logging.file` | File path (empty = stderr) | _(empty)_ |

Configuration can also be set via environment variables with the `CRUSH_` prefix (e.g., `CRUSH_COMPRESSION_LEVEL=fast`).

### Global Flags

```bash
# Verbose output (-v for debug, -vv for trace)
crush -v compress file.txt
crush -vv compress file.txt

# Quiet mode (suppress all output except errors)
crush -q compress file.txt

# JSON log format
crush --log-format json compress file.txt

# Log to a file
crush --log-file /tmp/crush.log compress file.txt
```

## Exit Codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Operational error (I/O, compression failure) |
| `2` | Configuration or usage error |
| `130` | Operation cancelled (Ctrl+C) |

## Graceful Cancellation

Press Ctrl+C during any operation to cancel gracefully. Crush will clean up partial output and exit with code 130.

## Development

```bash
cargo build --bin crush
cargo test -p crush-cli
cargo bench -p crush-cli
```

## License

MIT
