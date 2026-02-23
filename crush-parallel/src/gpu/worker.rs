//! GPU worker using wgpu for compute-based block compression.

use crush_core::error::{CrushError, Result};
use wgpu::util::DeviceExt;

/// A handle to a GPU device capable of running the compression compute shader.
///
/// Created via [`GpuWorker::new()`], which returns `None` when no compatible
/// adapter is present (automatic CPU fallback).
pub struct GpuWorker {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl GpuWorker {
    /// Attempt to initialise a GPU worker.
    ///
    /// Returns `None` when no compatible GPU adapter is found, allowing the
    /// engine to fall back to CPU compression transparently.
    #[must_use]
    pub fn new() -> Option<Self> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Option<Self> {
        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await?;

        // Request device and queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Crush GPU Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .ok()?;

        // Load shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Crush DEFLATE Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/deflate.wgsl").into()),
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Crush Bind Group Layout"),
            entries: &[
                // Input buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Output buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Metadata buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Crush Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create compute pipeline
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Crush Compression Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: "compress_block",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Some(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
        })
    }

    /// Compress a single block on the GPU.
    ///
    /// # Errors
    ///
    /// Returns an error if the GPU compute dispatch fails.
    #[allow(clippy::too_many_lines)] // GPU buffer setup is inherently verbose
    pub fn compress_block(&self, input: &[u8]) -> Result<Vec<u8>> {
        if input.is_empty() {
            return Ok(Vec::new());
        }

        let input_size = u32::try_from(input.len())
            .map_err(|_| CrushError::InvalidConfig("input too large for GPU".to_owned()))?;

        // Pad input to u32 alignment
        let padded_size = input_size.div_ceil(4) * 4;
        let mut padded_input = input.to_vec();
        padded_input.resize(
            usize::try_from(padded_size)
                .map_err(|_| CrushError::InvalidConfig("padded size overflow".to_owned()))?,
            0,
        );

        // Create input buffer
        let input_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Input Buffer"),
                contents: &padded_input,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        // Create output buffer (same size as input, will be trimmed later)
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: u64::from(padded_size),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Create metadata buffer [input_size, output_size, compression_level]
        let metadata = [input_size, 0u32, 6u32]; // level 6 default
        let metadata_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Metadata Buffer"),
                contents: bytemuck::cast_slice(&metadata),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });

        // Create staging buffer for readback
        let staging_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: u64::from(padded_size),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let metadata_staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Metadata Staging"),
            size: 12, // 3 * u32
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create bind group
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Crush Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: metadata_buffer.as_entire_binding(),
                },
            ],
        });

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Crush Encoder"),
            });

        // Dispatch compute shader
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Crush Compute Pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);

            // Dispatch workgroups (256 threads per workgroup)
            let workgroup_count = (padded_size / 4).div_ceil(256);
            compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
        }

        // Copy output to staging buffer
        encoder.copy_buffer_to_buffer(
            &output_buffer,
            0,
            &staging_buffer,
            0,
            u64::from(padded_size),
        );
        encoder.copy_buffer_to_buffer(&metadata_buffer, 0, &metadata_staging, 0, 12);

        // Submit commands
        self.queue.submit(Some(encoder.finish()));

        // Wait for GPU to finish
        self.device.poll(wgpu::MaintainBase::Wait);

        // Read back metadata to get actual output size
        let metadata_slice = metadata_staging.slice(..);
        metadata_slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::MaintainBase::Wait);

        let metadata_view = metadata_slice.get_mapped_range();
        let output_size = u32::from_le_bytes([
            metadata_view[4],
            metadata_view[5],
            metadata_view[6],
            metadata_view[7],
        ]);
        drop(metadata_view);
        metadata_staging.unmap();

        // Read back output data
        let output_slice = staging_buffer.slice(..);
        output_slice.map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::MaintainBase::Wait);

        let data = output_slice.get_mapped_range();
        let result = data[..output_size as usize].to_vec();
        drop(data);
        staging_buffer.unmap();

        Ok(result)
    }
}
