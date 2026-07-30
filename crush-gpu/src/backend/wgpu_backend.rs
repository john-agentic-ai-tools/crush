//! wgpu compute shader backend (Vulkan/Metal/DX12)
//!
//! Provides GPU decompression via wgpu's cross-platform compute shader API.
//! Requires Vulkan 1.2 / Metal 2 / DX12 + 2 GB VRAM minimum.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crush_core::error::{CrushError, PluginError, Result};

use super::{CompressedTile, ComputeBackend, GpuInfo, GpuVendor, MIN_VRAM_BYTES};

/// WGSL compute shader source — LZ77 (v1 compat, embedded at compile time).
const DECOMPRESS_SHADER: &str = include_str!("../shader/decompress.wgsl");

/// WGSL compute shader source — `GDeflate` (v2, embedded at compile time).
const GDEFLATE_SHADER: &str = include_str!("../shader/gdeflate_decompress.wgsl");

/// Maximum time to wait for GPU work to complete before treating it as a hang.
/// 5 seconds is generous for a single tile (64KB) decompression dispatch.
/// On Windows, TDR typically resets the GPU after ~2s, so this catches hangs
/// that survive TDR as well.
const GPU_POLL_TIMEOUT: Duration = Duration::from_secs(5);

/// wgpu-backed GPU compute backend.
///
/// Holds two compute pipelines: one for LZ77 (v1) and one for `GDeflate` (v2).
pub struct WgpuBackend {
    info: GpuInfo,
    device: wgpu::Device,
    queue: wgpu::Queue,
    // LZ77 pipeline (v1)
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    // GDeflate pipeline (v2)
    gdeflate_pipeline: wgpu::ComputePipeline,
    gdeflate_bgl: wgpu::BindGroupLayout,
}

/// Uniform struct matching the `TileMeta` layout in the LZ77 WGSL shader.
/// 8 × u32 = 32 bytes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct TileMeta {
    compressed_offset: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    sub_stream_count: u32,
    output_offset: u32,
    tile_index: u32,
    _pad0: u32,
    _pad1: u32,
}

/// Metadata struct matching `GDeflateMeta` in the `GDeflate` WGSL shader.
/// 4 × u32 = 16 bytes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GDeflateMeta {
    payload_size: u32,
    uncompressed_size: u32,
    _pad0: u32,
    _pad1: u32,
}

/// GPU buffers needed for a single tile dispatch.
struct TileBuffers {
    meta: wgpu::Buffer,
    compressed: wgpu::Buffer,
    output: wgpu::Buffer,
    lengths: wgpu::Buffer,
    out_staging: wgpu::Buffer,
    len_staging: wgpu::Buffer,
}

/// Allocate all GPU buffers for a single tile dispatch.
///
/// Uses `catch_unwind` to prevent wgpu internal panics (e.g. on OOM)
/// from crashing the entire process.
fn create_tile_buffers(
    device: &wgpu::Device,
    comp_data: &[u8],
    out_aligned: u64,
    len_size: u64,
) -> Result<TileBuffers> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let buf = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let storage_dst = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let storage_src = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
        let map_read = wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ;

        TileBuffers {
            meta: buf(
                "tile_meta",
                std::mem::size_of::<TileMeta>() as u64,
                storage_dst,
            ),
            compressed: buf("compressed_data", comp_data.len() as u64, storage_dst),
            output: buf("decompressed_data", out_aligned, storage_src),
            lengths: buf("sub_stream_lengths", len_size, storage_src),
            out_staging: buf("out_staging", out_aligned, map_read),
            len_staging: buf("len_staging", len_size, map_read),
        }
    }))
    .map_err(|e| {
        let msg = e
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| e.downcast_ref::<&str>().copied())
            .unwrap_or("unknown GPU buffer allocation panic");
        CrushError::from(PluginError::OperationFailed(format!(
            "GPU buffer allocation failed: {msg}"
        )))
    })
}

/// Parse sub-stream length u32 values from raw bytes.
fn parse_ss_lengths(len_bytes: &[u8], n: u32) -> Vec<u32> {
    let mut ss_lengths = Vec::with_capacity(n as usize);
    for i in 0..n as usize {
        let off = i * 4;
        if off + 4 <= len_bytes.len() {
            ss_lengths.push(u32::from_le_bytes([
                len_bytes[off],
                len_bytes[off + 1],
                len_bytes[off + 2],
                len_bytes[off + 3],
            ]));
        } else {
            ss_lengths.push(0);
        }
    }
    ss_lengths
}

/// Create the bind group layout with 4 storage buffer bindings for the LZ77 shader.
fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let storage_entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("decompress_bgl"),
        entries: &[
            storage_entry(0, true),  // tile_meta
            storage_entry(1, true),  // compressed_data
            storage_entry(2, false), // decompressed_data
            storage_entry(3, false), // sub_stream_lengths
        ],
    })
}

/// Create the bind group layout with 3 storage buffer bindings for the `GDeflate` shader.
fn create_gdeflate_bgl(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let storage_entry = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("gdeflate_bgl"),
        entries: &[
            storage_entry(0, true),  // meta (GDeflateMeta)
            storage_entry(1, true),  // compressed (GDeflate payload)
            storage_entry(2, false), // output (decompressed bytes)
        ],
    })
}

impl WgpuBackend {
    /// Attempt to create a wgpu backend by discovering a suitable GPU adapter.
    ///
    /// Returns `None` if no compatible GPU is found.
    ///
    /// # Errors
    ///
    /// Returns an error if wgpu initialisation fails unexpectedly.
    pub fn try_new() -> Result<Option<Self>> {
        // wgpu 30 takes the descriptor by value, and `InstanceDescriptor` no
        // longer implements `Default` (it gained a non-`Default` boxed display
        // handle). We never present to a surface, so the handle-less
        // constructor is the right base.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            // Limit bucketing rounds reported limits down into coarse buckets
            // to reduce fingerprinting when exposing wgpu to untrusted content.
            // crush is trusted local code and uses `max_buffer_size` as its
            // VRAM proxy below, so keep the true device limits.
            apply_limit_buckets: false,
        }));

        let Ok(adapter) = adapter else {
            return Ok(None);
        };

        let adapter_info = adapter.get_info();

        // Reject software/CPU adapters — they won't provide GPU acceleration.
        if adapter_info.device_type == wgpu::DeviceType::Cpu {
            return Ok(None);
        }

        // Use max_buffer_size as a rough VRAM proxy. Note: this is the
        // driver-reported maximum single-buffer size, not total VRAM.
        // On discrete GPUs it's typically 2+ GB. We use a conservative
        // check and rely on catch_unwind + CPU fallback for OOM safety.
        let limits = adapter.limits();
        let estimated_vram = limits.max_buffer_size;
        if estimated_vram < MIN_VRAM_BYTES {
            return Ok(None);
        }

        let vendor = match adapter_info.vendor {
            0x10DE => GpuVendor::Nvidia,
            0x1002 => GpuVendor::Amd,
            0x8086 => GpuVendor::Intel,
            _ if adapter_info.driver.to_lowercase().contains("apple")
                || adapter_info.name.to_lowercase().contains("apple") =>
            {
                GpuVendor::Apple
            }
            _ => GpuVendor::Other,
        };

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("crush-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::default(),
        }))
        .map_err(|e| PluginError::OperationFailed(format!("wgpu device request failed: {e}")))?;

        // --- LZ77 pipeline (v1) ---
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("decompress.wgsl"),
            source: wgpu::ShaderSource::Wgsl(DECOMPRESS_SHADER.into()),
        });

        let bind_group_layout = create_bind_group_layout(&device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("decompress_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("decompress_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // --- GDeflate pipeline (v2) ---
        let gdeflate_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gdeflate_decompress.wgsl"),
            source: wgpu::ShaderSource::Wgsl(GDEFLATE_SHADER.into()),
        });

        let gdeflate_bgl = create_gdeflate_bgl(&device);

        let gdeflate_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gdeflate_pipeline_layout"),
            bind_group_layouts: &[Some(&gdeflate_bgl)],
            immediate_size: 0,
        });

        let gdeflate_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gdeflate_pipeline"),
            layout: Some(&gdeflate_pl),
            module: &gdeflate_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let info = GpuInfo {
            name: adapter_info.name.clone(),
            vendor,
            vram_bytes: estimated_vram,
            api_backend: format!("{:?}", adapter_info.backend),
        };

        Ok(Some(Self {
            info,
            device,
            queue,
            pipeline,
            bind_group_layout,
            gdeflate_pipeline,
            gdeflate_bgl,
        }))
    }

    /// Map two staging buffers, poll the device, and return their contents.
    fn readback_buffers(
        &self,
        out_staging: &wgpu::Buffer,
        lengths_staging: &wgpu::Buffer,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let out_slice = out_staging.slice(..);
        let lengths_slice = lengths_staging.slice(..);

        let (out_tx, out_rx) = std::sync::mpsc::channel();
        out_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = out_tx.send(result);
        });
        let (len_tx, len_rx) = std::sync::mpsc::channel();
        lengths_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = len_tx.send(result);
        });

        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(GPU_POLL_TIMEOUT),
            })
            .map_err(|e| {
                PluginError::OperationFailed(format!(
                    "GPU poll failed (timeout or device lost): {e}"
                ))
            })?;

        out_rx
            .recv()
            .map_err(|e| PluginError::OperationFailed(format!("GPU readback channel error: {e}")))?
            .map_err(|e| PluginError::OperationFailed(format!("GPU output map failed: {e}")))?;
        len_rx
            .recv()
            .map_err(|e| PluginError::OperationFailed(format!("GPU readback channel error: {e}")))?
            .map_err(|e| PluginError::OperationFailed(format!("GPU lengths map failed: {e}")))?;

        // wgpu 30 returns `Result` here instead of panicking on a bad range.
        let out_bytes = out_slice
            .get_mapped_range()
            .map_err(|e| PluginError::OperationFailed(format!("GPU output range map failed: {e}")))?
            .to_vec();
        let len_bytes = lengths_slice
            .get_mapped_range()
            .map_err(|e| {
                PluginError::OperationFailed(format!("GPU lengths range map failed: {e}"))
            })?
            .to_vec();
        Ok((out_bytes, len_bytes))
    }

    /// Decompress a single tile on the GPU and return the raw sub-stream outputs.
    fn dispatch_tile(&self, tile: &CompressedTile, tile_index: u32) -> Result<(Vec<u8>, Vec<u32>)> {
        let n = u32::from(tile.sub_stream_count);
        if n == 0 {
            return Err(CrushError::InvalidFormat(
                "tile has zero sub-stream count".to_owned(),
            ));
        }

        // Guard against crafted archives with absurd uncompressed_size that
        // would cause u32 overflow in `n * max_per_ss`.
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

        let mut comp_data = tile.data.clone();
        while !comp_data.len().is_multiple_of(4) {
            comp_data.push(0);
        }

        let meta = TileMeta {
            compressed_offset: 0,
            compressed_size: u32::try_from(tile.data.len())
                .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
            uncompressed_size: tile.uncompressed_size,
            sub_stream_count: n,
            output_offset: 0,
            tile_index,
            _pad0: 0,
            _pad1: 0,
        };

        let out_aligned = u64::from(output_buf_size.div_ceil(4) * 4).max(4);
        let len_size = (u64::from(n) * 4).max(4);
        let bufs = create_tile_buffers(&self.device, &comp_data, out_aligned, len_size)?;

        self.queue
            .write_buffer(&bufs.meta, 0, bytemuck::bytes_of(&meta));
        self.queue.write_buffer(&bufs.compressed, 0, &comp_data);

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("decompress_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: bufs.meta.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bufs.compressed.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bufs.output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: bufs.lengths.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("decompress_encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("decompress_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&bufs.output, 0, &bufs.out_staging, 0, out_aligned);
        encoder.copy_buffer_to_buffer(&bufs.lengths, 0, &bufs.len_staging, 0, len_size);
        self.queue.submit(std::iter::once(encoder.finish()));

        let (out_bytes, len_bytes) = self.readback_buffers(&bufs.out_staging, &bufs.len_staging)?;

        Ok((out_bytes, parse_ss_lengths(&len_bytes, n)))
    }
}

/// GPU buffers for a single `GDeflate` tile dispatch.
struct GDeflateBuffers {
    meta: wgpu::Buffer,
    compressed: wgpu::Buffer,
    output: wgpu::Buffer,
    out_staging: wgpu::Buffer,
}

/// Allocate GPU buffers for a `GDeflate` tile dispatch.
fn create_gdeflate_buffers(
    device: &wgpu::Device,
    comp_data: &[u8],
    out_aligned: u64,
) -> Result<GDeflateBuffers> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let buf = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let storage_dst = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let storage_src = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
        let map_read = wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ;

        GDeflateBuffers {
            meta: buf(
                "gdeflate_meta",
                std::mem::size_of::<GDeflateMeta>() as u64,
                storage_dst,
            ),
            compressed: buf("gdeflate_compressed", comp_data.len() as u64, storage_dst),
            output: buf("gdeflate_output", out_aligned, storage_src),
            out_staging: buf("gdeflate_staging", out_aligned, map_read),
        }
    }))
    .map_err(|e| {
        let msg = e
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| e.downcast_ref::<&str>().copied())
            .unwrap_or("unknown GPU buffer allocation panic");
        CrushError::from(PluginError::OperationFailed(format!(
            "GDeflate GPU buffer allocation failed: {msg}"
        )))
    })
}

impl ComputeBackend for WgpuBackend {
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "wgpu"
    }

    fn gpu_info(&self) -> &GpuInfo {
        &self.info
    }

    fn decompress_tiles(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        // Wrap the entire GPU dispatch loop in catch_unwind so that panics
        // inside wgpu (e.g. "device is lost", driver crashes, TDR) are
        // converted to errors instead of crashing the process.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.decompress_tiles_inner(tiles, cancel)
        }))
        .unwrap_or_else(|e| {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("unknown GPU panic");
            Err(CrushError::from(PluginError::OperationFailed(format!(
                "GPU panic caught (falling back to CPU): {msg}"
            ))))
        })
    }

    fn decompress_tiles_gdeflate(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.decompress_tiles_gdeflate_inner(tiles, cancel)
        }))
        .unwrap_or_else(|e| {
            let msg = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("unknown GPU panic");
            Err(CrushError::from(PluginError::OperationFailed(format!(
                "GDeflate GPU panic caught (falling back to CPU): {msg}"
            ))))
        })
    }

    fn release(&self) {
        // wgpu resources are dropped automatically via RAII.
    }
}

/// Validated and padded tile data ready for GPU upload.
struct PreparedTile {
    padded_data: Vec<u8>,
    meta: GDeflateMeta,
    out_aligned: u64,
}

/// Validate a tile and prepare its padded data + metadata for GPU dispatch.
fn prepare_gdeflate_tile(tile: &CompressedTile) -> Result<PreparedTile> {
    let max_tile_size: u32 = crate::format::DEFAULT_TILE_SIZE;
    if tile.uncompressed_size > max_tile_size.saturating_mul(2) {
        return Err(CrushError::InvalidFormat(format!(
            "tile uncompressed_size {} exceeds maximum {}",
            tile.uncompressed_size,
            max_tile_size * 2,
        )));
    }

    let mut padded_data = tile.data.clone();
    while !padded_data.len().is_multiple_of(4) {
        padded_data.push(0);
    }

    let meta = GDeflateMeta {
        payload_size: u32::try_from(padded_data.len())
            .map_err(|e| PluginError::OperationFailed(e.to_string()))?,
        uncompressed_size: tile.uncompressed_size,
        _pad0: 0,
        _pad1: 0,
    };

    let out_aligned = u64::from(tile.uncompressed_size.div_ceil(4) * 4).max(4);

    Ok(PreparedTile {
        padded_data,
        meta,
        out_aligned,
    })
}

/// Create a `GDeflate` bind group for a single tile's buffers.
fn create_gdeflate_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    bufs: &GDeflateBuffers,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gdeflate_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: bufs.meta.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: bufs.compressed.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: bufs.output.as_entire_binding(),
            },
        ],
    })
}

impl WgpuBackend {
    /// Dispatch a batch of `GDeflate` tiles in a single GPU submission.
    ///
    /// All tiles share one `CommandEncoder`, one `ComputePass`, one `queue.submit()`,
    /// and one `device.poll()`. Each tile gets its own buffers and bind group since
    /// buffer sizes vary per tile. This eliminates per-tile host-GPU synchronization
    /// overhead.
    fn dispatch_batch_gdeflate(&self, tiles: &[CompressedTile]) -> Result<Vec<Vec<u8>>> {
        // Prepare all tiles (validate, pad, build metadata).
        let prepared: Vec<PreparedTile> = tiles
            .iter()
            .map(prepare_gdeflate_tile)
            .collect::<Result<Vec<_>>>()?;

        // Allocate all GPU buffers upfront.
        let all_bufs: Vec<GDeflateBuffers> = prepared
            .iter()
            .map(|p| create_gdeflate_buffers(&self.device, &p.padded_data, p.out_aligned))
            .collect::<Result<Vec<_>>>()?;

        // Upload all metadata and compressed data.
        for (p, bufs) in prepared.iter().zip(all_bufs.iter()) {
            self.queue
                .write_buffer(&bufs.meta, 0, bytemuck::bytes_of(&p.meta));
            self.queue.write_buffer(&bufs.compressed, 0, &p.padded_data);
        }

        // Create all bind groups.
        let bind_groups: Vec<wgpu::BindGroup> = all_bufs
            .iter()
            .map(|bufs| create_gdeflate_bind_group(&self.device, &self.gdeflate_bgl, bufs))
            .collect();

        // One encoder, one compute pass, multiple dispatches.
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gdeflate_batch_encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gdeflate_batch_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.gdeflate_pipeline);
            for bg in &bind_groups {
                pass.set_bind_group(0, bg, &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
        }

        // Copy all output buffers to staging buffers.
        for (p, bufs) in prepared.iter().zip(all_bufs.iter()) {
            encoder.copy_buffer_to_buffer(&bufs.output, 0, &bufs.out_staging, 0, p.out_aligned);
        }

        // Single submit for all tiles.
        self.queue.submit(std::iter::once(encoder.finish()));

        // Map all staging buffers, single poll, collect results.
        self.readback_batch(&all_bufs, tiles)
    }

    /// Map all staging buffers, poll once, and collect decompressed results.
    fn readback_batch(
        &self,
        all_bufs: &[GDeflateBuffers],
        tiles: &[CompressedTile],
    ) -> Result<Vec<Vec<u8>>> {
        let receivers: Vec<_> = all_bufs
            .iter()
            .map(|bufs| {
                let slice = bufs.out_staging.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                rx
            })
            .collect();

        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(GPU_POLL_TIMEOUT),
            })
            .map_err(|e| {
                PluginError::OperationFailed(format!(
                    "GDeflate GPU poll failed (timeout or device lost): {e}"
                ))
            })?;

        let mut results = Vec::with_capacity(tiles.len());
        for (i, rx) in receivers.into_iter().enumerate() {
            rx.recv()
                .map_err(|e| {
                    PluginError::OperationFailed(format!(
                        "GDeflate GPU readback channel error: {e}"
                    ))
                })?
                .map_err(|e| {
                    PluginError::OperationFailed(format!("GDeflate GPU output map failed: {e}"))
                })?;

            let slice = all_bufs[i].out_staging.slice(..);
            let out_bytes = slice
                .get_mapped_range()
                .map_err(|e| {
                    PluginError::OperationFailed(format!(
                        "GDeflate GPU output range map failed: {e}"
                    ))
                })?
                .to_vec();
            let size = tiles[i].uncompressed_size as usize;
            results.push(out_bytes[..size.min(out_bytes.len())].to_vec());
        }

        Ok(results)
    }

    /// Inner dispatch loop for `GDeflate` tiles — batched for throughput.
    ///
    /// Processes tiles in chunks of `super::MAX_TILES_PER_BATCH`, checking for
    /// cancellation between batches. Each batch is dispatched as a single
    /// GPU submission to minimize host-GPU synchronization overhead.
    fn decompress_tiles_gdeflate_inner(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        let mut results = Vec::with_capacity(tiles.len());
        for batch in tiles.chunks(super::MAX_TILES_PER_BATCH) {
            if cancel.load(Ordering::Relaxed) {
                return Err(CrushError::Cancelled);
            }
            let batch_results = self.dispatch_batch_gdeflate(batch)?;
            results.extend(batch_results);
        }
        Ok(results)
    }

    /// Inner dispatch loop, separated so `decompress_tiles` can wrap it in
    /// `catch_unwind` to prevent wgpu panics from crashing the process.
    fn decompress_tiles_inner(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        let mut results = Vec::with_capacity(tiles.len());

        for (i, tile) in tiles.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(CrushError::Cancelled);
            }

            let tile_index =
                u32::try_from(i).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

            let (raw_output, ss_lengths) = self.dispatch_tile(tile, tile_index)?;

            let decompressed = super::deinterleave(
                &raw_output,
                &ss_lengths,
                u32::from(tile.sub_stream_count),
                tile.uncompressed_size,
            );

            results.push(decompressed);
        }

        Ok(results)
    }
}
