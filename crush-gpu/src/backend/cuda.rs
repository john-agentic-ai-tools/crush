//! Optional CUDA backend for NVIDIA GPUs (feature-gated)
//!
//! Provides GPU decompression via CUDA compute kernels for NVIDIA GPUs.
//! Requires CUDA toolkit and an NVIDIA GPU with compute capability 7.0+.
//!
//! This module is only compiled when the `cuda` feature is enabled.

use std::sync::atomic::{AtomicBool, Ordering};

use crush_core::error::{CrushError, PluginError, Result};

use super::{CompressedTile, ComputeBackend, GpuInfo, GpuVendor, MIN_VRAM_BYTES};

/// CUDA-backed GPU compute backend for NVIDIA GPUs.
pub struct CudaBackend {
    info: GpuInfo,
}

impl CudaBackend {
    /// Attempt to create a CUDA backend by discovering a suitable NVIDIA GPU.
    ///
    /// Returns `None` if no compatible NVIDIA GPU is found or if CUDA
    /// initialization fails.
    ///
    /// # Errors
    ///
    /// Returns an error if CUDA probing encounters an unexpected failure.
    pub fn try_new() -> Result<Option<Self>> {
        // Attempt to initialize CUDA context via cudarc.
        let device = match cudarc::driver::CudaDevice::new(0) {
            Ok(d) => d,
            Err(_) => return Ok(None),
        };

        // Query device properties.
        let name = device.name().unwrap_or_else(|_| "NVIDIA GPU".to_owned());

        let vram_bytes = device
            .total_mem()
            .map_err(|e| PluginError::OperationFailed(format!("CUDA total_mem failed: {e}")))?;

        let vram_bytes_u64 =
            u64::try_from(vram_bytes).map_err(|e| PluginError::OperationFailed(e.to_string()))?;

        if vram_bytes_u64 < MIN_VRAM_BYTES {
            return Ok(None);
        }

        let info = GpuInfo {
            name,
            vendor: GpuVendor::Nvidia,
            vram_bytes: vram_bytes_u64,
            api_backend: "CUDA".to_owned(),
        };

        Ok(Some(Self { info }))
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
        _tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>> {
        if cancel.load(Ordering::Relaxed) {
            return Err(CrushError::Cancelled);
        }
        // TODO: Compile PTX decompression kernel at runtime via nvrtc and
        // dispatch tile decompression on the GPU.  For now, return an error
        // so the engine falls back to the CPU path.
        Err(CrushError::from(PluginError::OperationFailed(
            "CUDA GPU decompression not yet implemented – use CPU fallback".to_owned(),
        )))
    }

    fn release(&self) {
        // cudarc resources are dropped automatically.
    }
}
