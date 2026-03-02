//! GPU-specific error types for the crush-gpu crate

use thiserror::Error;

/// Errors originating from GPU backend operations.
#[derive(Error, Debug)]
pub enum GpuError {
    /// No compatible GPU was found during backend discovery.
    #[error("no compatible GPU found: {0}")]
    NoGpuAvailable(String),

    /// GPU backend failed to initialize (driver/API issue).
    #[error("GPU backend initialization failed: {0}")]
    BackendInit(String),

    /// A compute shader failed to compile.
    #[error("shader compilation failed: {0}")]
    ShaderCompilation(String),

    /// GPU ran out of memory during decompression dispatch.
    #[error(
        "GPU memory exceeded: requested {requested_bytes} bytes, available {available_bytes} bytes"
    )]
    MemoryExceeded {
        requested_bytes: u64,
        available_bytes: u64,
    },

    /// Tile version in the archive is not supported by this engine.
    #[error("unsupported tile version {version}; expected {expected}")]
    TileVersionMismatch { version: u8, expected: u8 },

    /// Generic GPU dispatch failure.
    #[error("GPU dispatch failed: {0}")]
    DispatchFailed(String),

    /// Data transfer between host and device failed.
    #[error("host ↔ device transfer failed: {0}")]
    TransferFailed(String),
}
