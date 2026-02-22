//! GPU worker using wgpu for compute-based block compression.

/// A handle to a GPU device capable of running the compression compute shader.
///
/// Created via [`GpuWorker::new()`], which returns `None` when no compatible
/// adapter is present (automatic CPU fallback).
pub struct GpuWorker {
    // Phase 7 (T051): wgpu device, queue, and pipeline will be stored here.
    _private: (),
}

impl GpuWorker {
    /// Attempt to initialise a GPU worker.
    ///
    /// Returns `None` when no compatible GPU adapter is found, allowing the
    /// engine to fall back to CPU compression transparently.
    #[must_use]
    pub fn new() -> Option<Self> {
        // TODO (T051): implement via pollster::block_on(wgpu::Instance::request_adapter(...))
        // For now, always return None so the CPU path is used.
        None
    }

    /// Compress a single block on the GPU.
    ///
    /// # Errors
    ///
    /// Returns an error if the GPU compute dispatch fails.
    pub fn compress_block(&self, _input: &[u8]) -> crush_core::error::Result<Vec<u8>> {
        // TODO (T053): implement GPU compute dispatch
        Err(crush_core::error::CrushError::InvalidConfig(
            "GPU compression not yet implemented".to_owned(),
        ))
    }
}
