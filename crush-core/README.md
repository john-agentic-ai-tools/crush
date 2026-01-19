# crush-core

High-performance parallel compression library core.

## Overview

`crush-core` is the core library for the Crush compression toolkit. It provides high-performance, parallel compression algorithms designed to match or exceed the performance of tools like `pigz`.

## Features (Placeholder)

This is the initial workspace structure setup. Actual compression features will be implemented in subsequent development phases:

- DEFLATE compression (planned)
- Multi-threaded parallel processing (planned)
- Plugin architecture for multiple compression algorithms (planned)
- Memory-efficient streaming I/O (planned)

## Usage

```rust
use crush_core::hello;

fn main() {
    let message = hello();
    println!("{}", message);
}
```

## Architecture

The library follows a modular design with clean separation of concerns:

- **Core engine**: Compression/decompression algorithms
- **Stream handling**: Efficient I/O for large files
- **Plugin system**: Extensible architecture for new formats

## Development

This crate is part of a Cargo workspace. Build from the repository root:

```bash
cargo build
cargo test
cargo doc --no-deps
```

## License

MIT

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) in the repository root.
