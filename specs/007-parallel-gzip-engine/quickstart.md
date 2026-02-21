# Quickstart: crush-parallel

**Branch**: `007-parallel-gzip-engine`
**Date**: 2026-02-21

---

## Add to Workspace

In `Cargo.toml` (workspace root):

```toml
[workspace]
members = [
    "crush-core",
    "crush-cli",
    "crush-parallel",   # ← add this
]
```

In your crate's `Cargo.toml`:

```toml
[dependencies]
crush-parallel = { path = "../crush-parallel" }

# Optional: GPU acceleration (requires wgpu + compatible GPU driver)
# crush-parallel = { path = "../crush-parallel", features = ["gpu"] }
```

---

## Basic Compression & Decompression

```rust
use crush_parallel::{compress, decompress, EngineConfiguration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = b"Hello, parallel world!".repeat(100_000);

    // Compress with defaults (1 MB blocks, level 6, all CPU cores)
    let config = EngineConfiguration::default();
    let compressed = compress(&input, &config)?;

    println!("Compressed {} → {} bytes", input.len(), compressed.len());

    // Decompress
    let decompressed = decompress(&compressed, &config)?;
    assert_eq!(input.as_slice(), decompressed.as_slice());

    Ok(())
}
```

---

## Custom Configuration

```rust
use crush_parallel::{compress, EngineConfiguration};

let config = EngineConfiguration::builder()
    .workers(4)                    // use 4 threads
    .block_size(512 * 1024)        // 512 KB blocks (finer random-access granularity)
    .compression_level(9)          // maximum compression
    .max_decompression_ratio(64.0) // refuse to decompress if output > 64× compressed size
    .checksums(true)               // verify CRC32 per block on decompress
    .build()?;

let compressed = compress(&large_data, &config)?;
```

---

## File Compression with Progress Bar

```rust
use crush_parallel::{compress_to_writer, EngineConfiguration, ProgressCallback, ProgressEvent};
use indicatif::{ProgressBar, ProgressStyle};
use std::{fs::File, sync::{Arc, Mutex}};

fn compress_file(src: &str, dst: &str) -> Result<(), Box<dyn std::error::Error>> {
    let input = std::fs::read(src)?;
    let total_bytes = input.len() as u64;

    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40} {bytes}/{total_bytes} ETA {eta}")?,
    );

    let pb2 = pb.clone();
    let callback: ProgressCallback = Box::new(move |event: ProgressEvent| {
        pb2.set_position(event.bytes_processed);
        true  // return false to cancel
    });

    let config = EngineConfiguration::builder()
        .progress(Arc::new(Mutex::new(callback)))
        .build()?;

    let out = File::create(dst)?;
    compress_to_writer(&input, out, &config)?;

    pb.finish_with_message("done");
    Ok(())
}
```

---

## Cancellation via Ctrl+C

```rust
use crush_parallel::{compress, EngineConfiguration, ProgressCallback};
use crush_core::cancel::AtomicCancellationToken;
use std::sync::{Arc, Mutex};

// Set up a shared cancel token wired to Ctrl+C
let token = Arc::new(AtomicCancellationToken::new());
let token2 = Arc::clone(&token);

ctrlc::set_handler(move || {
    token2.cancel();
})?;

let callback: ProgressCallback = Box::new(move |_event| {
    !token.is_cancelled()  // return false = abort when Ctrl+C pressed
});

let config = EngineConfiguration::builder()
    .progress(Arc::new(Mutex::new(callback)))
    .build()?;

match compress(&large_data, &config) {
    Ok(output) => println!("Compressed {} bytes", output.len()),
    Err(e) if e.is_cancelled() => println!("Cancelled by user."),
    Err(e) => return Err(e.into()),
}
```

---

## Random Access (Single Block Decompression)

```rust
use crush_parallel::{load_index, decompress_block, EngineConfiguration};
use std::{fs::File, io::BufReader};

fn read_block(path: &str, block_n: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Load the index once (reads only the last 24 bytes + index region)
    let index = load_index(&mut reader)?;

    println!("File has {} blocks", index.len());

    // Decompress only block N — O(1) seek, no other blocks touched
    let config = EngineConfiguration::default();
    let block_data = decompress_block(&mut reader, &index, block_n, &config)?;

    Ok(block_data)
}
```

---

## GPU Acceleration (Optional Feature)

```toml
# Cargo.toml
[dependencies]
crush-parallel = { path = "../crush-parallel", features = ["gpu"] }
```

```rust
let config = EngineConfiguration::builder()
    .gpu(true)   // attempt GPU; silently falls back to CPU if no adapter found
    .build()?;

let compressed = compress(&large_data, &config)?;
// Output is identical whether compressed by CPU or GPU.
```

---

## CLI Integration Pattern

In `crush-cli`, the compression command wires `crush-parallel` as a plugin — no manual registration needed. The parallel DEFLATE plugin is auto-discovered via the `linkme` distributed slice. The CLI invokes it when the user selects the `parallel-deflate` algorithm or it is the default.

```text
$ crush compress --algorithm parallel-deflate input.dat output.crsh
$ crush decompress output.crsh restored.dat
$ crush decompress --block 47 output.crsh block47.dat   # single block random access
```

---

## Version Mismatch Error

If you attempt to decompress a file produced by a different engine version:

```
Error: version mismatch: file was produced by engine 0.1.0, current engine is 0.2.0
Use engine version 0.1.0 to decompress this file, or recompress with the current version.
```

This is by design (see spec clarification Q4). No automatic migration is provided.
