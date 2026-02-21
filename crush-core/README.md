# crush-core

High-performance parallel compression library with a pluggable algorithm system.

## Overview

`crush-core` is the core library for the [Crush](https://github.com/john-agentic-ai-tools/crush) compression toolkit. It provides:

- **Plugin-based architecture** - extensible compression via the `CompressionAlgorithm` trait
- **Intelligent plugin selection** - automatic scoring based on throughput and compression ratio weights
- **Timeout protection** - configurable timeouts prevent runaway operations
- **Cooperative cancellation** - lock-free, async-signal-safe cancellation tokens
- **CRC32 integrity checks** - automatic checksum generation and validation
- **File metadata preservation** - mtime and Unix permissions stored in the archive
- **RAII resource cleanup** - partial output files automatically removed on failure

## Installation

```toml
[dependencies]
crush-core = "0.1.1"
```

## Quick Start

```rust
use crush_core::{init_plugins, compress, decompress};

// Initialize the plugin registry (call once at startup)
init_plugins().expect("Plugin initialization failed");

// Compress
let data = b"Hello, Crush!";
let compressed = compress(data).expect("Compression failed");

// Decompress
let result = decompress(&compressed).expect("Decompression failed");
assert_eq!(data.as_slice(), result.data.as_slice());
```

## Advanced Usage

### Compression Options

```rust
use crush_core::{init_plugins, compress_with_options, CompressionOptions, ScoringWeights};
use std::time::Duration;

init_plugins()?;

// Select a specific plugin
let options = CompressionOptions::default()
    .with_plugin("deflate")
    .with_timeout(Duration::from_secs(10));
let compressed = compress_with_options(b"data", &options)?;

// Automatic selection weighted toward speed
let weights = ScoringWeights::new(0.8, 0.2)?;
let options = CompressionOptions::default().with_weights(weights);
let compressed = compress_with_options(b"data", &options)?;
```

### Cancellation

```rust
use crush_core::{init_plugins, compress_with_options, CompressionOptions};
use crush_core::cancel::AtomicCancellationToken;
use std::sync::Arc;

init_plugins()?;

let token = Arc::new(AtomicCancellationToken::new());
let options = CompressionOptions::default()
    .with_cancel_token(token.clone());

// Cancel from another thread or signal handler
// token.cancel();

let result = compress_with_options(b"data", &options);
```

### Inspection

```rust
use crush_core::{init_plugins, compress, inspect};

init_plugins()?;

let compressed = compress(b"test data")?;
let info = inspect(&compressed)?;
println!("Original: {} bytes", info.original_size);
println!("Compressed: {} bytes", info.compressed_size);
println!("Plugin: {}, CRC valid: {}", info.plugin_name, info.crc_valid);
```

### Plugin Discovery

```rust
use crush_core::{init_plugins, list_plugins};

init_plugins()?;

for plugin in list_plugins() {
    println!("{} v{} - {} MB/s, ratio {:.2}",
        plugin.name, plugin.version,
        plugin.throughput, plugin.compression_ratio);
}
```

## Writing a Custom Plugin

Implement the `CompressionAlgorithm` trait and register with the `linkme` distributed slice:

```rust
use crush_core::plugin::{CompressionAlgorithm, PluginMetadata, COMPRESSION_ALGORITHMS};
use crush_core::Result;
use linkme::distributed_slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

struct MyPlugin;

impl CompressionAlgorithm for MyPlugin {
    fn name(&self) -> &'static str { "my-plugin" }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "my-plugin",
            version: "1.0.0",
            magic_number: [0x43, 0x52, 0x01, 0x10], // Must start with [0x43, 0x52, 0x01]
            throughput: 300.0,
            compression_ratio: 0.40,
            description: "My custom compression algorithm",
        }
    }

    fn compress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        // Check cancel_flag periodically in hot loops
        todo!()
    }

    fn decompress(&self, input: &[u8], cancel_flag: Arc<AtomicBool>) -> Result<Vec<u8>> {
        todo!()
    }

    fn detect(&self, _file_header: &[u8]) -> bool { false }
}

#[distributed_slice(COMPRESSION_ALGORITHMS)]
static MY_PLUGIN: &dyn CompressionAlgorithm = &MyPlugin;
```

## Built-in Plugins

| Plugin | Algorithm | Throughput | Ratio | Description |
| --- | --- | --- | --- | --- |
| `deflate` | DEFLATE (RFC 1951) | 200 MB/s | 0.35 | Standard DEFLATE via `flate2` |

## Archive Format

Crush archives use a 16-byte header followed by optional CRC32 and metadata:

```text
[Header 16B] [CRC32 4B] [Metadata (optional)] [Compressed payload]

Header layout (little-endian):
  Bytes 0-3:   Magic number ([0x43, 0x52, version, plugin_id])
  Bytes 4-11:  Original size (u64)
  Byte 12:     Flags (bit 0 = CRC32 present, bit 1 = metadata present)
  Bytes 13-15: Reserved
```

## Error Handling

All operations return `crush_core::Result<T>`, with errors organized by category:

- `CrushError::Plugin` - plugin not found, duplicate magic, operation failure
- `CrushError::Validation` - invalid header, CRC mismatch, corrupted data
- `CrushError::Timeout` - operation exceeded timeout, plugin panic
- `CrushError::Io` - I/O errors
- `CrushError::Cancelled` - operation cancelled via cancellation token

## Development

```bash
cargo test -p crush-core
cargo bench -p crush-core
cargo doc -p crush-core --no-deps
```

## License

MIT
