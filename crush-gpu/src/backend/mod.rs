//! GPU compute backend trait and discovery
//!
//! Defines the [`ComputeBackend`] trait implemented by each GPU vendor
//! backend (wgpu, CUDA) and the types needed for backend auto-selection.

#[cfg(feature = "cuda")]
pub mod cuda;
pub mod wgpu_backend;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

use crush_core::error::Result;

// ---------------------------------------------------------------------------
// GpuVendor
// ---------------------------------------------------------------------------

/// Known GPU hardware vendors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Other,
}

impl std::fmt::Display for GpuVendor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nvidia => write!(f, "NVIDIA"),
            Self::Amd => write!(f, "AMD"),
            Self::Intel => write!(f, "Intel"),
            Self::Apple => write!(f, "Apple"),
            Self::Other => write!(f, "Other"),
        }
    }
}

// ---------------------------------------------------------------------------
// GpuInfo
// ---------------------------------------------------------------------------

/// Runtime information about a discovered GPU.
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// Human-readable adapter name (e.g. "NVIDIA `GeForce` RTX 4090").
    pub name: String,
    /// Hardware vendor.
    pub vendor: GpuVendor,
    /// Estimated VRAM in bytes.
    pub vram_bytes: u64,
    /// Graphics API backend in use (e.g. "Vulkan", "Metal", "CUDA").
    pub api_backend: String,
}

// ---------------------------------------------------------------------------
// CompressedTile
// ---------------------------------------------------------------------------

/// A single compressed tile ready for GPU decompression dispatch.
#[derive(Debug, Clone)]
pub struct CompressedTile {
    /// Compressed payload bytes (excluding `TileHeader`).
    pub data: Vec<u8>,
    /// Expected uncompressed size.
    pub uncompressed_size: u32,
    /// Sub-stream count within this tile.
    pub sub_stream_count: u8,
    /// CRC32 of the uncompressed data (0 if checksums disabled).
    pub checksum: u32,
}

// ---------------------------------------------------------------------------
// ComputeBackend trait
// ---------------------------------------------------------------------------

/// Abstraction over GPU compute backends (wgpu, CUDA).
///
/// All methods that can fail return [`crush_core::error::Result`] so the
/// engine can decide whether to fall back to CPU.
pub trait ComputeBackend: Send + Sync {
    /// Backend display name (e.g. "wgpu-Vulkan", "CUDA").
    fn name(&self) -> &str;

    /// Information about the GPU selected by this backend.
    fn gpu_info(&self) -> &GpuInfo;

    /// Decompress a batch of compressed tiles on the GPU.
    ///
    /// Returns one `Vec<u8>` per input tile in the same order.
    ///
    /// # Cancellation
    ///
    /// Implementations **should** check `cancel` between tile batches
    /// and return `CrushError::Cancelled` when set.
    ///
    /// # Errors
    ///
    /// May return any GPU error variant wrapped in a `CrushError`.
    fn decompress_tiles(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>>;

    /// Decompress a batch of `GDeflate`-encoded tiles on the GPU.
    ///
    /// Returns one `Vec<u8>` per input tile in the same order.
    /// The output is already in the correct byte order (no de-interleaving).
    ///
    /// # Cancellation
    ///
    /// Implementations **should** check `cancel` between tile dispatches
    /// and return `CrushError::Cancelled` when set.
    ///
    /// # Errors
    ///
    /// May return any GPU error variant wrapped in a `CrushError`.
    fn decompress_tiles_gdeflate(
        &self,
        tiles: &[CompressedTile],
        cancel: &AtomicBool,
    ) -> Result<Vec<Vec<u8>>>;

    /// Release GPU resources held by this backend.
    fn release(&self);
}

/// Minimum VRAM requirement in bytes (2 GB).
pub const MIN_VRAM_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// GPU memory budget for decompression dispatch in bytes (256 MB).
pub const GPU_MEMORY_BUDGET: u64 = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Backend auto-discovery (cached)
// ---------------------------------------------------------------------------

/// Cached GPU backend singleton.
///
/// GPU device creation is expensive (50-500 ms) and rapid creation/destruction
/// destabilizes Windows DX12 drivers, causing `DXGI_ERROR_DEVICE_REMOVED` and
/// `device.lose()` in wgpu. By caching the backend for the process lifetime
/// we avoid these issues and match how games and other GPU applications work.
static CACHED_BACKEND: OnceLock<Option<Arc<dyn ComputeBackend>>> = OnceLock::new();

/// Discover the best available GPU backend, caching the result.
///
/// The backend is created once and reused for the process lifetime.
/// If the GPU device becomes lost during use, the engine's `catch_unwind`
/// safety net converts the error and falls back to CPU decompression.
///
/// Selection priority:
/// 1. CUDA (if `cuda` feature enabled and NVIDIA GPU present)
/// 2. wgpu with Vulkan/Metal/DX12
///
/// Returns `Ok(None)` if no compatible GPU is found.
///
/// # Errors
///
/// This function always returns `Ok`. GPU initialization errors are
/// handled internally and result in `Ok(None)` (no GPU available).
pub fn discover_gpu() -> Result<Option<Arc<dyn ComputeBackend>>> {
    let cached = CACHED_BACKEND.get_or_init(|| {
        // 1. Try CUDA first (if feature enabled) — fastest path for NVIDIA GPUs.
        #[cfg(feature = "cuda")]
        {
            if let Ok(Some(backend)) = cuda::CudaBackend::try_new() {
                return Some(Arc::new(backend) as Arc<dyn ComputeBackend>);
            }
        }

        // 2. Try wgpu (Vulkan / Metal / DX12)
        match wgpu_backend::WgpuBackend::try_new() {
            Ok(Some(backend)) => Some(Arc::new(backend) as Arc<dyn ComputeBackend>),
            Ok(None) => None,
            Err(e) => {
                eprintln!("crush-gpu: GPU backend init failed: {e}");
                None
            }
        }
    });

    Ok(cached.clone())
}
