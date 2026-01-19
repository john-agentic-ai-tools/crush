# crush-cli

Command-line interface for the Crush high-performance compression library.

## Overview

`crush-cli` is a command-line wrapper around the `crush-core` compression library. It provides a user-friendly CLI for compression and decompression operations.

## Installation (Future)

Once published to crates.io:

```bash
cargo install crush-cli
```

## Usage (Placeholder)

Current placeholder binary demonstrates successful compilation:

```bash
# From repository root
cargo run --bin crush

# Or after building
./target/debug/crush
```

## Planned Features

- Compress files with multiple algorithms (gzip, zstd, lz4, etc.)
- Parallel compression for improved performance
- Stream processing for large files
- Progress reporting and verbose modes
- Integration with system compression tools

## Development

Build and run from the workspace root:

```bash
cargo build --bin crush
cargo run --bin crush
cargo test -p crush-cli
```

## Architecture

The CLI is a thin wrapper that:
- Parses command-line arguments (via clap - to be added)
- Calls `crush-core` library functions
- Handles I/O and user interaction
- Provides progress feedback

## License

MIT

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) in the repository root.
