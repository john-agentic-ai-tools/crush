//! Optional CUDA backend for NVIDIA GPUs (feature-gated)
//!
//! Provides GPU decompression via CUDA compute kernels for NVIDIA GPUs.
//! Requires CUDA toolkit and an NVIDIA GPU with compute capability 7.0+.
//!
//! This module is only compiled when the `cuda` feature is enabled.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::compile_ptx;
use tracing::{debug, info, trace, warn};

use crush_core::error::{CrushError, PluginError, Result};

use super::{
    CompressedTile, ComputeBackend, GpuInfo, GpuVendor, MAX_TILES_PER_BATCH, MIN_VRAM_BYTES,
};

/// CUDA C source for the LZ77 (v1) decompression kernel.
const LZ77_KERNEL_SRC: &str = include_str!("../shader/decompress.cu");

/// CUDA C source for the `GDeflate` (v2) decompression kernel.
const GDEFLATE_KERNEL_SRC: &str = include_str!("../shader/gdeflate_decompress.cu");

/// CUDA-backed GPU compute backend for NVIDIA GPUs.
pub struct CudaBackend {
    info: GpuInfo,
    /// Kept alive to prevent the CUDA context from being destroyed.
    #[allow(dead_code)]
    ctx: Arc<CudaContext>,
    stream: Arc<CudaStream>,
    lz77_function: CudaFunction,
    gdeflate_function: CudaFunction,
}

/// Map a cudarc `DriverError` to a crush `PluginError`.
fn driver_err(context: &str, e: cudarc::driver::DriverError) -> CrushError {
    CrushError::from(PluginError::OperationFailed(format!("{context}: {e}")))
}

impl CudaBackend {
    /// Attempt to create a CUDA backend by discovering a suitable NVIDIA GPU.
    ///
    /// Compiles both CUDA decompression kernels via nvrtc on first init.
    /// Returns `None` if no compatible NVIDIA GPU is found or if CUDA
    /// initialization fails.
    ///
    /// # Errors
    ///
    /// Returns an error if CUDA probing encounters an unexpected failure.
    pub fn try_new() -> Result<Option<Self>> {
        // Attempt to initialize CUDA context via cudarc (v0.19+ API).
        debug!("Creating CUDA context on device 0...");
        let Ok(ctx) = CudaContext::new(0) else {
            debug!("CUDA context creation failed, no NVIDIA GPU available");
            return Ok(None);
        };

        // Query device properties.
        let name = ctx.name().unwrap_or_else(|_| "NVIDIA GPU".to_owned());
        debug!(gpu = %name, "CUDA context created: {name}");

        // `total_mem` is an unsafe free function in cudarc v0.19.
        let vram_bytes = unsafe { cudarc::driver::result::device::total_mem(ctx.cu_device()) }
            .map_err(|e| PluginError::OperationFailed(format!("CUDA total_mem failed: {e}")))?;

        let vram_bytes_u64 =
            u64::try_from(vram_bytes).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

        let vram_mb = vram_bytes_u64 / 1024 / 1024;
        if vram_bytes_u64 < MIN_VRAM_BYTES {
            debug!(
                vram_mb,
                "Insufficient VRAM ({vram_mb} MB), need {} MB",
                MIN_VRAM_BYTES / 1024 / 1024
            );
            return Ok(None);
        }
        debug!(vram_mb, "VRAM: {vram_mb} MB");

        // Compile CUDA kernels via nvrtc.
        debug!("Compiling LZ77 kernel via nvrtc...");
        let lz77_ptx = compile_ptx(LZ77_KERNEL_SRC).map_err(|e| {
            PluginError::OperationFailed(format!("CUDA LZ77 kernel compilation failed: {e}"))
        })?;
        debug!("LZ77 kernel compiled OK");

        debug!("Compiling GDeflate kernel via nvrtc...");
        let gdeflate_ptx = compile_ptx(GDEFLATE_KERNEL_SRC).map_err(|e| {
            PluginError::OperationFailed(format!("CUDA GDeflate kernel compilation failed: {e}"))
        })?;
        debug!("GDeflate kernel compiled OK");

        // Load modules and extract kernel functions.
        debug!("Loading LZ77 module...");
        let lz77_module = ctx
            .load_module(lz77_ptx)
            .map_err(|e| driver_err("CUDA load LZ77 module", e))?;
        let lz77_function = lz77_module
            .load_function("lz77_decompress_tile")
            .map_err(|e| driver_err("CUDA load LZ77 function", e))?;
        debug!("LZ77 module loaded OK");

        debug!("Loading GDeflate module...");
        let gdeflate_module = ctx
            .load_module(gdeflate_ptx)
            .map_err(|e| driver_err("CUDA load GDeflate module", e))?;
        let gdeflate_function = gdeflate_module
            .load_function("gdeflate_decompress_tile")
            .map_err(|e| driver_err("CUDA load GDeflate function", e))?;
        debug!("GDeflate module loaded OK");

        let stream = ctx.default_stream();
        info!(gpu = %name, vram_mb, "CUDA backend initialized: {name} ({vram_mb} MB)");

        let info = GpuInfo {
            name,
            vendor: GpuVendor::Nvidia,
            vram_bytes: vram_bytes_u64,
            api_backend: "CUDA".to_owned(),
        };

        Ok(Some(Self {
            info,
            ctx,
            stream,
            lz77_function,
            gdeflate_function,
        }))
    }

    /// Dispatch a single LZ77 tile on the GPU and return raw sub-stream
    /// outputs and per-sub-stream lengths.
    fn dispatch_lz77_tile(
        &self,
        tile: &CompressedTile,
        tile_index: u32,
    ) -> Result<(Vec<u8>, Vec<u32>)> {
        let n = u32::from(tile.sub_stream_count);
        if n == 0 {
            return Err(CrushError::InvalidFormat(
                "tile has zero sub-stream count".to_owned(),
            ));
        }

        let max_tile_size: u32 = crate::format::DEFAULT_TILE_SIZE;
        if tile.uncompressed_size > max_tile_size.saturating_mul(2) {
            return Err(CrushError::InvalidFormat(format!(
                "tile uncompressed_size {} exceeds maximum {}",
                tile.uncompressed_size,
                max_tile_size * 2,
            )));
        }

        let max_per_ss = tile.uncompressed_size.div_ceil(n);
        let output_buf_size = n.checked_mul(max_per_ss).ok_or_else(|| {
            CrushError::InvalidFormat(format!("output buffer size overflow: {n} * {max_per_ss}"))
        })?;

        // Pad compressed data to u32 alignment.
        let mut comp_data = tile.data.clone();
        while !comp_data.len().is_multiple_of(4) {
            comp_data.push(0);
        }

        // Build tile metadata matching the CUDA struct layout.
        let meta: [u32; 8] = [
            0, // compressed_offset
            u32::try_from(tile.data.len())
                .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
            tile.uncompressed_size, // uncompressed_size
            n,                      // sub_stream_count
            0,                      // output_offset
            tile_index,             // tile_index
            0,                      // _pad0
            0,                      // _pad1
        ];

        // Reinterpret compressed bytes as u32 slice for GPU upload.
        let comp_words: &[u32] = bytemuck::cast_slice(&comp_data);
        let output_words = (output_buf_size as usize).div_ceil(4);

        // Upload to GPU.
        let d_meta = self
            .stream
            .clone_htod(&meta)
            .map_err(|e| driver_err("CUDA upload meta", e))?;
        let d_compressed = self
            .stream
            .clone_htod(comp_words)
            .map_err(|e| driver_err("CUDA upload compressed", e))?;
        let mut d_output = self
            .stream
            .alloc_zeros::<u32>(output_words)
            .map_err(|e| driver_err("CUDA alloc output", e))?;
        let mut d_lengths = self
            .stream
            .alloc_zeros::<u32>(n as usize)
            .map_err(|e| driver_err("CUDA alloc lengths", e))?;

        // Launch kernel: 1 block of 32 threads.
        let cfg = LaunchConfig {
            grid_dim: (1, 1, 1),
            block_dim: (32, 1, 1),
            shared_mem_bytes: 0,
        };
        unsafe {
            self.stream
                .launch_builder(&self.lz77_function)
                .arg(&d_meta)
                .arg(&d_compressed)
                .arg(&mut d_output)
                .arg(&mut d_lengths)
                .launch(cfg)
                .map_err(|e| driver_err("CUDA launch LZ77 kernel", e))?;
        }

        // Synchronize and read back.
        self.stream
            .synchronize()
            .map_err(|e| driver_err("CUDA sync after LZ77", e))?;

        let output_words_host: Vec<u32> = self
            .stream
            .clone_dtoh(&d_output)
            .map_err(|e| driver_err("CUDA readback output", e))?;
        let lengths_host: Vec<u32> = self
            .stream
            .clone_dtoh(&d_lengths)
            .map_err(|e| driver_err("CUDA readback lengths", e))?;

        let output_bytes: Vec<u8> = bytemuck::cast_slice(&output_words_host).to_vec();

        Ok((output_bytes, lengths_host))
    }

    /// Dispatch a batch of `GDeflate` tiles as a **single multi-block kernel
    /// launch** — one CUDA block per tile, all tiles execute in parallel across
    /// all SMs.
    ///
    /// Data layout on the GPU:
    /// - `tile_metas`:     `[GDeflateMeta; N]` — per-tile payload/uncompressed sizes
    /// - `compressed_buf`: concatenated u32 words from all tiles
    /// - `output_buf`:     concatenated output space for all tiles (u32-aligned)
    /// - `comp_offsets`:   `[u32; N]` — word offset into `compressed_buf` for each tile
    /// - `out_offsets`:    `[u32; N]` — word offset into `output_buf` for each tile
    ///
    /// Launch: `grid_dim = (N, 1, 1), block_dim = (32, 1, 1)`.
    #[allow(clippy::too_many_lines)]
    fn dispatch_gdeflate_batch(&self, tiles: &[CompressedTile]) -> Result<Vec<Vec<u8>>> {
        let max_tile_size: u32 = crate::format::DEFAULT_TILE_SIZE;
        let num_tiles = tiles.len();

        // Phase 1: Build concatenated host buffers.
        // GDeflateMeta struct: { payload_size: u32, uncompressed_size: u32, _pad0: u32, _pad1: u32 }
        let mut h_metas: Vec<u32> = Vec::with_capacity(num_tiles * 4);
        let mut h_compressed: Vec<u32> = Vec::new();
        let mut h_comp_offsets: Vec<u32> = Vec::with_capacity(num_tiles);
        let mut h_out_offsets: Vec<u32> = Vec::with_capacity(num_tiles);
        let mut uncomp_sizes: Vec<u32> = Vec::with_capacity(num_tiles);
        let mut total_output_words: u32 = 0;

        for tile in tiles {
            if tile.uncompressed_size > max_tile_size.saturating_mul(2) {
                return Err(CrushError::InvalidFormat(format!(
                    "tile uncompressed_size {} exceeds maximum {}",
                    tile.uncompressed_size,
                    max_tile_size * 2,
                )));
            }

            // Pad compressed data to u32 alignment.
            let mut padded_data = tile.data.clone();
            while !padded_data.len().is_multiple_of(4) {
                padded_data.push(0);
            }

            let payload_size = u32::try_from(padded_data.len())
                .map_err(|e| PluginError::OperationFailed(e.to_string()))?;

            // GDeflateMeta for this tile.
            h_metas.push(payload_size);
            h_metas.push(tile.uncompressed_size);
            h_metas.push(0); // _pad0
            h_metas.push(0); // _pad1

            // Compressed data offset (in u32 words).
            let comp_offset = u32::try_from(h_compressed.len())
                .map_err(|e| PluginError::OperationFailed(e.to_string()))?;
            h_comp_offsets.push(comp_offset);

            // Append compressed words.
            let comp_words: &[u32] = bytemuck::cast_slice(&padded_data);
            h_compressed.extend_from_slice(comp_words);

            // Output offset (in u32 words).
            h_out_offsets.push(total_output_words);
            let output_words_for_tile =
                u32::try_from((tile.uncompressed_size as usize).div_ceil(4).max(1))
                    .map_err(|e| PluginError::OperationFailed(e.to_string()))?;
            total_output_words = total_output_words
                .checked_add(output_words_for_tile)
                .ok_or_else(|| {
                    CrushError::InvalidFormat(
                        "total output buffer size overflow in GDeflate batch".to_owned(),
                    )
                })?;

            uncomp_sizes.push(tile.uncompressed_size);
        }

        // Phase 2: Upload all buffers to GPU.
        let d_metas = self
            .stream
            .clone_htod(&h_metas)
            .map_err(|e| driver_err("CUDA upload GDeflate metas", e))?;
        let d_compressed = self
            .stream
            .clone_htod(&h_compressed)
            .map_err(|e| driver_err("CUDA upload GDeflate compressed", e))?;
        let mut d_output = self
            .stream
            .alloc_zeros::<u32>(total_output_words as usize)
            .map_err(|e| driver_err("CUDA alloc GDeflate output", e))?;
        let d_comp_offsets = self
            .stream
            .clone_htod(&h_comp_offsets)
            .map_err(|e| driver_err("CUDA upload GDeflate comp_offsets", e))?;
        let d_out_offsets = self
            .stream
            .clone_htod(&h_out_offsets)
            .map_err(|e| driver_err("CUDA upload GDeflate out_offsets", e))?;

        // Phase 3: Single kernel launch — one block per tile.
        let num_tiles_u32 =
            u32::try_from(num_tiles).map_err(|e| PluginError::OperationFailed(e.to_string()))?;
        let cfg = LaunchConfig {
            grid_dim: (num_tiles_u32, 1, 1),
            block_dim: (32, 1, 1),
            shared_mem_bytes: 0,
        };
        trace!(
            num_tiles,
            total_compressed_words = h_compressed.len(),
            total_output_words,
            "CUDA GDeflate: launching {num_tiles} blocks"
        );

        unsafe {
            self.stream
                .launch_builder(&self.gdeflate_function)
                .arg(&d_metas)
                .arg(&d_compressed)
                .arg(&mut d_output)
                .arg(&d_comp_offsets)
                .arg(&d_out_offsets)
                .launch(cfg)
                .map_err(|e| driver_err("CUDA launch GDeflate kernel", e))?;
        }

        // Phase 4: Single synchronize.
        self.stream
            .synchronize()
            .map_err(|e| driver_err("CUDA sync after GDeflate batch", e))?;

        // Phase 5: Read back the single large output buffer and split per tile.
        let all_output_words: Vec<u32> = self
            .stream
            .clone_dtoh(&d_output)
            .map_err(|e| driver_err("CUDA readback GDeflate output", e))?;
        let all_output_bytes: &[u8] = bytemuck::cast_slice(&all_output_words);

        let mut results = Vec::with_capacity(num_tiles);
        for (i, &size) in uncomp_sizes.iter().enumerate() {
            let word_offset = h_out_offsets[i] as usize;
            let byte_offset = word_offset * 4;
            let sz = size as usize;
            let end = (byte_offset + sz).min(all_output_bytes.len());
            let start = byte_offset.min(all_output_bytes.len());
            results.push(all_output_bytes[start..end].to_vec());
        }

        Ok(results)
    }
}

impl ComputeBackend for CudaBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "CUDA"
    }

    fn gpu_info(&self) -> &GpuInfo {
        &self.info
    }

    fn decompress_tiles(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        // Wrap the entire dispatch loop in catch_unwind so that CUDA panics
        // are converted to errors instead of crashing the process.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut results = Vec::with_capacity(tiles.len());
            for (i, tile) in tiles.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CrushError::Cancelled);
                }
                let tile_index =
                    u32::try_from(i).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

                let (raw_output, ss_lengths) = self.dispatch_lz77_tile(tile, tile_index)?;

                let decompressed = super::deinterleave(
                    &raw_output,
                    &ss_lengths,
                    u32::from(tile.sub_stream_count),
                    tile.uncompressed_size,
                );
                results.push(decompressed);
            }
            Ok(results)
        }))
        .unwrap_or_else(|e| {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("unknown CUDA panic");
            Err(CrushError::from(PluginError::OperationFailed(format!(
                "CUDA panic caught (falling back to CPU): {msg}"
            ))))
        })
    }

    fn decompress_tiles_gdeflate(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        let total_tiles = tiles.len();
        let num_batches = total_tiles.div_ceil(MAX_TILES_PER_BATCH);
        info!(
            total_tiles,
            num_batches,
            batch_size = MAX_TILES_PER_BATCH,
            "CUDA GDeflate: decompressing {total_tiles} tiles in {num_batches} batches"
        );
        let start = std::time::Instant::now();

        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut results = Vec::with_capacity(tiles.len());
            for (batch_idx, batch) in tiles.chunks(MAX_TILES_PER_BATCH).enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    warn!("CUDA decompression cancelled at batch {batch_idx}");
                    return Err(CrushError::Cancelled);
                }
                trace!(
                    batch = batch_idx,
                    batch_tiles = batch.len(),
                    tiles_done = results.len(),
                    "CUDA batch {batch_idx}/{num_batches} ({} tiles in batch)",
                    batch.len()
                );

                // Dispatch entire batch with single sync.
                let batch_results = self.dispatch_gdeflate_batch(batch)?;
                results.extend(batch_results);

                if batch_idx % 10 == 0 || batch_idx + 1 == num_batches {
                    let elapsed = start.elapsed().as_secs_f64();
                    let done = results.len();
                    #[allow(clippy::cast_precision_loss)]
                    let pct = done as f64 / total_tiles as f64 * 100.0;
                    debug!(
                        batch = batch_idx,
                        tiles_done = done,
                        elapsed_secs = elapsed,
                        "CUDA progress: {done}/{total_tiles} tiles ({pct:.1}%) in {elapsed:.1}s"
                    );
                }
            }
            let elapsed = start.elapsed().as_secs_f64();
            info!(
                total_tiles,
                elapsed_secs = elapsed,
                "CUDA GDeflate: all {total_tiles} tiles decompressed in {elapsed:.1}s"
            );
            Ok(results)
        }))
        .unwrap_or_else(|e| {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("unknown CUDA panic");
            Err(CrushError::from(PluginError::OperationFailed(format!(
                "CUDA GDeflate panic caught (falling back to CPU): {msg}"
            ))))
        })
    }

    fn release(&self) {
        // cudarc resources are dropped automatically via RAII.
    }
}
