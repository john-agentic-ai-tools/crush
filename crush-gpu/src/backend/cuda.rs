//! Optional CUDA backend for NVIDIA GPUs (feature-gated)
//!
//! Provides GPU decompression via CUDA compute kernels for NVIDIA GPUs.
//! Requires CUDA toolkit and an NVIDIA GPU with compute capability 7.0+.
//!
//! This module is only compiled when the `cuda` feature is enabled.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::compile_ptx;

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
        let Ok(ctx) = CudaContext::new(0) else {
            return Ok(None);
        };

        // Query device properties.
        let name = ctx.name().unwrap_or_else(|_| "NVIDIA GPU".to_owned());

        // `total_mem` is an unsafe free function in cudarc v0.19.
        let vram_bytes = unsafe { cudarc::driver::result::device::total_mem(ctx.cu_device()) }
            .map_err(|e| PluginError::OperationFailed(format!("CUDA total_mem failed: {e}")))?;

        let vram_bytes_u64 =
            u64::try_from(vram_bytes).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

        if vram_bytes_u64 < MIN_VRAM_BYTES {
            return Ok(None);
        }

        // Compile CUDA kernels via nvrtc.
        let lz77_ptx = compile_ptx(LZ77_KERNEL_SRC).map_err(|e| {
            PluginError::OperationFailed(format!("CUDA LZ77 kernel compilation failed: {e}"))
        })?;

        let gdeflate_ptx = compile_ptx(GDEFLATE_KERNEL_SRC).map_err(|e| {
            PluginError::OperationFailed(format!("CUDA GDeflate kernel compilation failed: {e}"))
        })?;

        // Load modules and extract kernel functions.
        let lz77_module = ctx
            .load_module(lz77_ptx)
            .map_err(|e| driver_err("CUDA load LZ77 module", e))?;
        let lz77_function = lz77_module
            .load_function("lz77_decompress_tile")
            .map_err(|e| driver_err("CUDA load LZ77 function", e))?;

        let gdeflate_module = ctx
            .load_module(gdeflate_ptx)
            .map_err(|e| driver_err("CUDA load GDeflate module", e))?;
        let gdeflate_function = gdeflate_module
            .load_function("gdeflate_decompress_tile")
            .map_err(|e| driver_err("CUDA load GDeflate function", e))?;

        let stream = ctx.default_stream();

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

    /// Dispatch a single `GDeflate` tile on the GPU and return decompressed bytes.
    fn dispatch_gdeflate_tile(&self, tile: &CompressedTile) -> Result<Vec<u8>> {
        let max_tile_size: u32 = crate::format::DEFAULT_TILE_SIZE;
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

        // Build GDeflateMeta matching the CUDA struct layout.
        let meta: [u32; 4] = [
            u32::try_from(padded_data.len())
                .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
            tile.uncompressed_size,
            0, // _pad0
            0, // _pad1
        ];

        let comp_words: &[u32] = bytemuck::cast_slice(&padded_data);
        let output_words = (tile.uncompressed_size as usize).div_ceil(4);

        // Upload to GPU.
        let d_meta = self
            .stream
            .clone_htod(&meta)
            .map_err(|e| driver_err("CUDA upload GDeflate meta", e))?;
        let d_compressed = self
            .stream
            .clone_htod(comp_words)
            .map_err(|e| driver_err("CUDA upload GDeflate compressed", e))?;
        let mut d_output = self
            .stream
            .alloc_zeros::<u32>(output_words.max(1))
            .map_err(|e| driver_err("CUDA alloc GDeflate output", e))?;

        // Launch kernel: 1 block of 32 threads.
        let cfg = LaunchConfig {
            grid_dim: (1, 1, 1),
            block_dim: (32, 1, 1),
            shared_mem_bytes: 0,
        };
        unsafe {
            self.stream
                .launch_builder(&self.gdeflate_function)
                .arg(&d_meta)
                .arg(&d_compressed)
                .arg(&mut d_output)
                .launch(cfg)
                .map_err(|e| driver_err("CUDA launch GDeflate kernel", e))?;
        }

        // Synchronize and read back.
        self.stream
            .synchronize()
            .map_err(|e| driver_err("CUDA sync after GDeflate", e))?;

        let output_words_host: Vec<u32> = self
            .stream
            .clone_dtoh(&d_output)
            .map_err(|e| driver_err("CUDA readback GDeflate output", e))?;

        let output_bytes: Vec<u8> = bytemuck::cast_slice(&output_words_host).to_vec();
        let size = tile.uncompressed_size as usize;
        Ok(output_bytes[..size.min(output_bytes.len())].to_vec())
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
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut results = Vec::with_capacity(tiles.len());
            for batch in tiles.chunks(MAX_TILES_PER_BATCH) {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CrushError::Cancelled);
                }
                for tile in batch {
                    results.push(self.dispatch_gdeflate_tile(tile)?);
                }
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
                "CUDA GDeflate panic caught (falling back to CPU): {msg}"
            ))))
        })
    }

    fn release(&self) {
        // cudarc resources are dropped automatically via RAII.
    }
}
