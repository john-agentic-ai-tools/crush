//! Engine configuration and progress reporting types.

use std::sync::{Arc, Mutex};

/// Callback type invoked after each block completes.
///
/// Returning `false` signals cancellation. The engine halts at the next block
/// boundary, discards partial output, and returns `CrushError::Cancelled`.
pub type ProgressCallback = Box<dyn FnMut(ProgressEvent) -> bool + Send>;

/// Which direction the engine is operating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    /// Engine is compressing input blocks.
    Compressing,
    /// Engine is decompressing blocks.
    Decompressing,
}

/// Data payload delivered to the progress callback after each block completes.
#[derive(Debug, Clone)]
pub struct ProgressEvent {
    /// Cumulative uncompressed bytes processed so far.
    pub bytes_processed: u64,
    /// Number of blocks fully compressed/decompressed.
    pub blocks_completed: u64,
    /// Total blocks in the operation. `None` for streaming (unknown total).
    pub total_blocks: Option<u64>,
    /// Whether the engine is compressing or decompressing.
    pub phase: ProgressPhase,
}

/// Configuration for the parallel compression/decompression engine.
///
/// Construct via [`EngineConfiguration::builder()`] or use the [`Default`] impl.
#[derive(Clone)]
pub struct EngineConfiguration {
    /// Number of rayon worker threads. `0` = rayon default (logical CPU count).
    pub workers: usize,
    /// Dedicated rayon thread pool built from `workers`. `None` when `workers == 0`
    /// (falls back to the global rayon pool).
    pub(crate) thread_pool: Option<Arc<rayon::ThreadPool>>,
    /// Uncompressed block size in bytes. Range: 64 KB – 256 MB.
    pub block_size: u32,
    /// DEFLATE compression level 0–9.
    pub compression_level: u8,
    /// If `compressed / uncompressed > max_expansion_ratio`, store block raw.
    pub max_expansion_ratio: f64,
    /// During decompression, halt if total bytes would exceed
    /// `compressed_file_size * max_decompression_ratio`.
    pub max_decompression_ratio: f64,
    /// Enable per-block CRC32 checksums.
    pub checksums: bool,
    /// Attempt GPU-accelerated compression (no-op if `gpu` feature disabled or no adapter).
    pub gpu: bool,
    /// Optional progress callback.
    pub progress: Option<Arc<Mutex<ProgressCallback>>>,
}

impl Default for EngineConfiguration {
    fn default() -> Self {
        Self {
            workers: 0,
            thread_pool: None,
            block_size: 1_048_576, // 1 MB
            compression_level: 6,
            max_expansion_ratio: 1.0,
            max_decompression_ratio: 1024.0,
            checksums: true,
            gpu: false,
            progress: None,
        }
    }
}

impl EngineConfiguration {
    /// Create a new builder.
    #[must_use]
    pub fn builder() -> EngineConfigurationBuilder {
        EngineConfigurationBuilder::new()
    }
}

/// Builder for [`EngineConfiguration`].
#[derive(Default)]
pub struct EngineConfigurationBuilder {
    inner: EngineConfiguration,
}

impl EngineConfigurationBuilder {
    fn new() -> Self {
        Self {
            inner: EngineConfiguration::default(),
        }
    }

    /// Set the number of worker threads (`0` = rayon default).
    #[must_use]
    pub fn workers(mut self, n: usize) -> Self {
        self.inner.workers = n;
        self
    }

    /// Set the uncompressed block size in bytes.
    #[must_use]
    pub fn block_size(mut self, bytes: u32) -> Self {
        self.inner.block_size = bytes;
        self
    }

    /// Set the DEFLATE compression level (0–9).
    #[must_use]
    pub fn compression_level(mut self, level: u8) -> Self {
        self.inner.compression_level = level;
        self
    }

    /// Set the maximum expansion ratio for incompressible block detection.
    #[must_use]
    pub fn max_expansion_ratio(mut self, ratio: f64) -> Self {
        self.inner.max_expansion_ratio = ratio;
        self
    }

    /// Set the maximum decompression expansion ratio.
    #[must_use]
    pub fn max_decompression_ratio(mut self, ratio: f64) -> Self {
        self.inner.max_decompression_ratio = ratio;
        self
    }

    /// Enable or disable per-block CRC32 checksums.
    #[must_use]
    pub fn checksums(mut self, enabled: bool) -> Self {
        self.inner.checksums = enabled;
        self
    }

    /// Enable GPU-accelerated compression.
    #[must_use]
    pub fn gpu(mut self, enabled: bool) -> Self {
        self.inner.gpu = enabled;
        self
    }

    /// Attach a progress callback.
    #[must_use]
    pub fn progress(mut self, cb: Arc<Mutex<ProgressCallback>>) -> Self {
        self.inner.progress = Some(cb);
        self
    }

    /// Validate all fields and return the built configuration.
    ///
    /// # Errors
    ///
    /// Returns [`crush_core::error::CrushError::InvalidConfig`] if any field is out of range.
    pub fn build(self) -> crush_core::error::Result<EngineConfiguration> {
        use crush_core::error::CrushError;
        let mut cfg = self.inner;
        if cfg.block_size < 65_536 || cfg.block_size > 268_435_456 {
            return Err(CrushError::InvalidConfig(format!(
                "block_size {} is out of range [65536, 268435456]",
                cfg.block_size
            )));
        }
        if cfg.compression_level > 9 {
            return Err(CrushError::InvalidConfig(format!(
                "compression_level {} must be in [0, 9]",
                cfg.compression_level
            )));
        }
        if cfg.max_expansion_ratio <= 0.0 {
            return Err(CrushError::InvalidConfig(
                "max_expansion_ratio must be > 0.0".to_owned(),
            ));
        }
        if cfg.max_decompression_ratio <= 0.0 {
            return Err(CrushError::InvalidConfig(
                "max_decompression_ratio must be > 0.0".to_owned(),
            ));
        }
        if cfg.workers > 0 {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(cfg.workers)
                .build()
                .map_err(|e| CrushError::InvalidConfig(format!("thread pool: {e}")))?;
            cfg.thread_pool = Some(Arc::new(pool));
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_configuration_builder_validates_fields() {
        // block_size too small
        let err = EngineConfiguration::builder().block_size(1024).build();
        assert!(err.is_err());

        // block_size too large
        let err = EngineConfiguration::builder().block_size(u32::MAX).build();
        assert!(err.is_err());

        // invalid compression level
        let err = EngineConfiguration::builder().compression_level(10).build();
        assert!(err.is_err());

        // zero expansion ratio
        let err = EngineConfiguration::builder()
            .max_expansion_ratio(0.0)
            .build();
        assert!(err.is_err());

        // zero decompression ratio
        let err = EngineConfiguration::builder()
            .max_decompression_ratio(0.0)
            .build();
        assert!(err.is_err());

        // valid configuration
        let ok = EngineConfiguration::builder()
            .workers(4)
            .block_size(1_048_576)
            .compression_level(6)
            .build();
        assert!(ok.is_ok());
    }
}
