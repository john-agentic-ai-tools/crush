//! Error types for Crush compression library
//!
//! This module defines all error types using `thiserror` for ergonomic error handling.
//! All public APIs return `Result<T, CrushError>` to enforce proper error propagation.

use thiserror::Error;

/// Main error type for Crush library operations
#[derive(Error, Debug)]
pub enum CrushError {
    /// Plugin-related errors (registration, discovery, execution)
    #[error("Plugin error: {0}")]
    Plugin(#[from] PluginError),

    /// Timeout-related errors (plugin operations exceeding limits)
    #[error("Timeout error: {0}")]
    Timeout(#[from] TimeoutError),

    /// Validation errors (invalid input, corrupted data, malformed headers)
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    /// I/O errors (file operations, network, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Plugin-specific errors
#[derive(Error, Debug)]
pub enum PluginError {
    /// Plugin not found for the specified identifier
    #[error("Plugin not found: {0}")]
    NotFound(String),

    /// Plugin with duplicate magic number or name
    #[error("Duplicate plugin magic number: {0:?}")]
    DuplicateMagic([u8; 4]),

    /// Plugin metadata is invalid
    #[error("Invalid plugin metadata: {0}")]
    InvalidMetadata(String),

    /// Plugin operation failed
    #[error("Plugin operation failed: {0}")]
    OperationFailed(String),

    /// Plugin was cancelled due to timeout or user request
    #[error("Plugin operation was cancelled")]
    Cancelled,
}

/// Timeout-related errors
#[derive(Error, Debug)]
pub enum TimeoutError {
    /// Operation exceeded configured timeout
    #[error("Operation timed out after {0:?}")]
    Timeout(std::time::Duration),

    /// Plugin thread panicked during execution
    #[error("Plugin panicked during execution")]
    PluginPanic,
}

/// Validation errors
#[derive(Error, Debug)]
pub enum ValidationError {
    /// Invalid magic number in file header
    #[error("Invalid magic number: expected Crush format, got {0:?}")]
    InvalidMagic([u8; 4]),

    /// CRC32 checksum mismatch
    #[error("CRC32 mismatch: expected {expected:08x}, got {actual:08x}")]
    CrcMismatch { expected: u32, actual: u32 },

    /// Invalid header format
    #[error("Invalid header format: {0}")]
    InvalidHeader(String),

    /// Corrupted compressed data
    #[error("Corrupted data: {0}")]
    CorruptedData(String),

    /// Invalid plugin scoring weights
    #[error("Invalid scoring weights: {0}")]
    InvalidWeights(String),
}

/// Type alias for Results using `CrushError`
pub type Result<T> = std::result::Result<T, CrushError>;
