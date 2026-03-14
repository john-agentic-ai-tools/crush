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
use tracing::{debug, info, trace, warn};

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

/// Maximum number of tiles to batch into a single GPU submission.
/// 512 tiles × ~200KB GPU buffers ≈ 100MB, well within `GPU_MEMORY_BUDGET` (256MB).
pub const MAX_TILES_PER_BATCH: usize = 512;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// De-interleave sub-stream outputs back to the original tile byte order.
///
/// The LZ77 GPU kernel decompresses each sub-stream independently into a
/// separate region of the output buffer. This function reconstructs the
/// original byte order by round-robin reading from each sub-stream:
/// byte `i` of the original tile came from sub-stream `i % n`, position
/// `i / n` within that sub-stream.
#[must_use]
pub fn deinterleave(
    raw_output: &[u8],
    ss_lengths: &[u32],
    sub_stream_count: u32,
    uncompressed_size: u32,
) -> Vec<u8> {
    let n = sub_stream_count as usize;
    let max_per_ss = (uncompressed_size as usize).div_ceil(n);

    // Extract each sub-stream's decoded bytes.
    let sub_streams: Vec<&[u8]> = (0..n)
        .map(|i| {
            let start = i * max_per_ss;
            let len = ss_lengths[i] as usize;
            let end = (start + len).min(raw_output.len());
            let actual_start = start.min(raw_output.len());
            &raw_output[actual_start..end]
        })
        .collect();

    // De-interleave: byte i of the original tile came from sub-stream i%n,
    // position i/n within that sub-stream.
    let mut output = Vec::with_capacity(uncompressed_size as usize);
    let max_len = sub_streams.iter().map(|s| s.len()).max().unwrap_or(0);
    for j in 0..max_len {
        for ss in &sub_streams {
            if j < ss.len() {
                output.push(ss[j]);
            }
            if output.len() == uncompressed_size as usize {
                return output;
            }
        }
    }

    output
}

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

/// Attempt to create a CUDA backend.
///
/// Wrapped in `catch_unwind` because cudarc's nvrtc loading panics if the
/// NVIDIA Runtime Compiler library is not installed (e.g. `nvrtc.dll` on
/// Windows, `libnvrtc.so` on Linux).
///
/// Returns `Ok(backend)` on success, `Err(message)` explaining why CUDA
/// is unavailable on failure.
#[cfg(feature = "cuda")]
fn try_cuda() -> std::result::Result<Arc<dyn ComputeBackend>, String> {
    debug!("Probing CUDA backend...");

    // Temporarily silence the default panic hook so the user doesn't see
    // Rust's "thread panicked at ..." noise when nvrtc is missing.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let result = std::panic::catch_unwind(cuda::CudaBackend::try_new);

    // Restore the original panic hook.
    std::panic::set_hook(prev_hook);

    match result {
        Ok(Ok(Some(backend))) => {
            let gi = backend.gpu_info();
            info!(
                gpu = %gi.name,
                vram_mb = gi.vram_bytes / 1024 / 1024,
                "CUDA backend ready: {}",
                gi.name
            );
            Ok(Arc::new(backend) as Arc<dyn ComputeBackend>)
        }
        Ok(Ok(None)) => {
            debug!("CUDA probe: no compatible NVIDIA GPU found");
            Err("no compatible NVIDIA GPU found".to_owned())
        }
        Ok(Err(e)) => {
            debug!("CUDA probe failed: {e}");
            Err(format!("{e}"))
        }
        Err(panic_info) => {
            let msg = panic_info
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic_info.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            warn!("CUDA probe panicked (nvrtc likely missing): {msg}");
            Err(format!(
                "CUDA runtime compiler (nvrtc) not found. \
                 Install the CUDA Toolkit to enable CUDA decompression. \
                 Detail: {msg}"
            ))
        }
    }
}

/// Attempt to create a wgpu backend, returning `None` on failure.
fn try_wgpu() -> Option<Arc<dyn ComputeBackend>> {
    debug!("Probing wgpu backend...");
    match wgpu_backend::WgpuBackend::try_new() {
        Ok(Some(backend)) => {
            let gi = backend.gpu_info();
            info!(
                gpu = %gi.name,
                api = %gi.api_backend,
                vram_mb = gi.vram_bytes / 1024 / 1024,
                "wgpu backend ready: {} ({})",
                gi.name,
                gi.api_backend
            );
            Some(Arc::new(backend) as Arc<dyn ComputeBackend>)
        }
        Ok(None) => {
            debug!("wgpu probe: no compatible GPU found");
            None
        }
        Err(e) => {
            warn!("wgpu backend init failed: {e}");
            None
        }
    }
}

/// Discover the best available GPU backend, caching the result.
///
/// The backend is created once and reused for the process lifetime.
/// If the GPU device becomes lost during use, the engine's `catch_unwind`
/// safety net converts the error and falls back to CPU decompression.
///
/// Backend selection is controlled by [`crate::get_config()`]`.backend`:
/// - `Auto`: try CUDA first (if feature enabled), then wgpu
/// - `Cuda`: only try CUDA
/// - `Wgpu`: only try wgpu
///
/// Returns `Ok(None)` if no compatible GPU is found.
///
/// # Errors
///
/// This function always returns `Ok`. GPU initialization errors are
/// handled internally and result in `Ok(None)` (no GPU available).
pub fn discover_gpu() -> Result<Option<Arc<dyn ComputeBackend>>> {
    use crush_core::error::{CrushError, PluginError};

    use crate::BackendPreference;

    // We cannot return errors from inside `get_or_init`, so we do a two-step:
    // first check if the cached value already exists, then handle the init
    // with potential errors.
    if let Some(cached) = CACHED_BACKEND.get() {
        trace!("Using cached GPU backend");
        return Ok(cached.clone());
    }

    let pref = crate::get_config().backend;
    info!(preference = ?pref, "Discovering GPU backend (preference: {pref:?})");

    let backend: Option<Arc<dyn ComputeBackend>> = match pref {
        BackendPreference::Auto => {
            #[cfg(feature = "cuda")]
            {
                match try_cuda() {
                    Ok(backend) => Some(backend),
                    Err(msg) => {
                        info!("CUDA unavailable ({msg}), trying wgpu");
                        try_wgpu()
                    }
                }
            }
            #[cfg(not(feature = "cuda"))]
            {
                debug!("CUDA feature not compiled in, trying wgpu only");
                try_wgpu()
            }
        }
        BackendPreference::Cuda => {
            #[cfg(feature = "cuda")]
            {
                match try_cuda() {
                    Ok(backend) => Some(backend),
                    Err(msg) => {
                        return Err(CrushError::from(PluginError::OperationFailed(format!(
                            "CUDA backend requested but unavailable: {msg}"
                        ))));
                    }
                }
            }
            #[cfg(not(feature = "cuda"))]
            {
                return Err(CrushError::from(PluginError::OperationFailed(
                    "CUDA backend not included in this build. \
                     Reinstall with: cargo install crush-cli --features cuda"
                        .to_owned(),
                )));
            }
        }
        BackendPreference::Wgpu => try_wgpu(),
    };

    if let Some(ref b) = backend {
        info!(backend = b.name(), "GPU backend selected: {}", b.name());
    } else {
        info!("No GPU backend available, will use CPU fallback");
    }

    // Cache the result (race-safe: if another thread beat us, use their value).
    let _ = CACHED_BACKEND.set(backend.clone());
    Ok(CACHED_BACKEND.get().cloned().flatten())
}
