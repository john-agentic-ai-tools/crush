//! Compression functionality
//!
//! Provides the public `compress()` API that compresses data using the default
//! DEFLATE plugin and wraps it with a Crush header.

use crate::cancel::CancellationToken;
use crate::error::Result;
use crate::plugin::registry::{get_default_plugin, get_plugin_by_magic};
use crate::plugin::{
    run_with_timeout, run_with_timeout_and_cancel, CrushHeader, FileMetadata, PluginSelector,
    ScoringWeights,
};
use crc32fast::Hasher;
use std::sync::Arc;
use std::time::Duration;

/// Default timeout for compression operations (0 = no timeout)
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(0);

/// Compression options for plugin selection and scoring
#[derive(Clone)]
pub struct CompressionOptions {
    /// Optional plugin name for manual override
    plugin_name: Option<String>,

    /// Scoring weights for automatic selection
    weights: ScoringWeights,

    /// Timeout for compression operation
    timeout: Duration,

    /// Optional file metadata
    file_metadata: Option<FileMetadata>,

    /// Optional cancellation token for Ctrl+C support
    cancel_token: Option<Arc<dyn CancellationToken>>,
}

impl CompressionOptions {
    /// Create new compression options with default settings
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugin_name: None,
            weights: ScoringWeights::default(),
            timeout: DEFAULT_TIMEOUT,
            file_metadata: None,
            cancel_token: None,
        }
    }

    /// Specify a plugin by name (manual override)
    #[must_use]
    pub fn with_plugin(mut self, name: &str) -> Self {
        self.plugin_name = Some(name.to_string());
        self
    }

    /// Set custom scoring weights
    #[must_use]
    pub fn with_weights(mut self, weights: ScoringWeights) -> Self {
        self.weights = weights;
        self
    }

    /// Set timeout for compression operation
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set file metadata
    #[must_use]
    pub fn with_file_metadata(mut self, metadata: FileMetadata) -> Self {
        self.file_metadata = Some(metadata);
        self
    }

    /// Set cancellation token for Ctrl+C support
    #[must_use]
    pub fn with_cancel_token(mut self, token: Arc<dyn CancellationToken>) -> Self {
        self.cancel_token = Some(token);
        self
    }
}

impl std::fmt::Debug for CompressionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompressionOptions")
            .field("plugin_name", &self.plugin_name)
            .field("weights", &self.weights)
            .field("timeout", &self.timeout)
            .field("file_metadata", &self.file_metadata)
            .field(
                "cancel_token",
                &self.cancel_token.as_ref().map(|_| "Some(...)"),
            )
            .finish()
    }
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Compress data using the default compression algorithm
///
/// Uses the DEFLATE plugin (magic number `0x43525100`) to compress the input data.
/// The compressed output includes a 16-byte Crush header with:
/// - Magic number identifying the plugin
/// - Original uncompressed size
/// - CRC32 checksum of the compressed payload
///
/// # Errors
///
/// Returns an error if:
/// - No default plugin is available (should never happen - DEFLATE is always registered)
/// - Compression operation fails
///
/// # Examples
///
/// ```
/// use crush_core::{init_plugins, compress};
///
/// init_plugins().expect("Plugin initialization failed");
/// let data = b"Hello, world!";
/// let compressed = compress(data).expect("Compression failed");
/// assert!(!compressed.is_empty());
/// ```
pub fn compress(input: &[u8]) -> Result<Vec<u8>> {
    // Get the default DEFLATE plugin from registry
    let plugin = get_default_plugin().ok_or_else(|| {
        crate::error::PluginError::NotFound(
            "Default DEFLATE plugin not found. Call init_plugins() first.".to_string(),
        )
    })?;

    let default_magic = [0x43, 0x52, 0x01, 0x00];

    // Clone input for move into timeout closure
    let input_owned = input.to_vec();

    // Compress the data with timeout protection
    let compressed_payload = run_with_timeout(DEFAULT_TIMEOUT, move |cancel_flag| {
        plugin.compress(&input_owned, cancel_flag)
    })?;

    // Calculate CRC32 of compressed payload
    let mut hasher = Hasher::new();
    hasher.update(&compressed_payload);
    let crc32 = hasher.finalize();

    // Create header with original size and CRC32
    let header = CrushHeader::new(default_magic, input.len() as u64).with_crc32();

    // Build final output: header + compressed payload
    let mut output = Vec::with_capacity(CrushHeader::SIZE + 4 + compressed_payload.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&crc32.to_le_bytes());
    output.extend_from_slice(&compressed_payload);

    Ok(output)
}

/// Compress data with custom options (plugin selection, scoring weights)
///
/// Provides fine-grained control over plugin selection through either:
/// - Manual plugin override by name
/// - Automatic selection with custom scoring weights
///
/// # Errors
///
/// Returns an error if:
/// - Specified plugin is not found (manual override)
/// - No plugins are available (automatic selection)
/// - Compression operation fails
/// - Operation exceeds the specified timeout (0 = no timeout)
///
/// # Examples
///
/// ```
/// use crush_core::{init_plugins, compress_with_options, CompressionOptions};
///
/// init_plugins().expect("Plugin initialization failed");
/// let data = b"Hello, world!";
///
/// // Use default automatic selection
/// let options = CompressionOptions::default();
/// let compressed = compress_with_options(data, &options).expect("Compression failed");
///
/// // Manual override: specify plugin by name
/// let options = CompressionOptions::default().with_plugin("deflate");
/// let compressed = compress_with_options(data, &options).expect("Compression failed");
/// ```
pub fn compress_with_options(input: &[u8], options: &CompressionOptions) -> Result<Vec<u8>> {
    // Check if already cancelled before starting
    if let Some(ref token) = options.cancel_token {
        if token.is_cancelled() {
            return Err(crate::error::CrushError::Cancelled);
        }
    }

    // Select plugin based on options
    let selector = PluginSelector::new(options.weights);

    let selected_metadata = if let Some(ref plugin_name) = options.plugin_name {
        // Manual override
        selector.select_by_name(plugin_name)?
    } else {
        // Automatic selection
        selector.select()?
    };

    // Get the actual plugin from registry
    let plugin = get_plugin_by_magic(selected_metadata.magic_number).ok_or_else(|| {
        crate::error::PluginError::NotFound(format!(
            "Plugin '{}' metadata found but not in registry",
            selected_metadata.name
        ))
    })?;

    // Clone input for move into timeout closure
    let input_owned = input.to_vec();
    let timeout = options.timeout;
    let cancel_token = options.cancel_token.clone();

    // Compress the data with timeout and cancellation protection
    let compressed_payload =
        run_with_timeout_and_cancel(timeout, cancel_token, move |cancel_flag| {
            plugin.compress(&input_owned, cancel_flag)
        })?;

    // Handle file metadata
    let metadata_bytes = options
        .file_metadata
        .as_ref()
        .map_or(Vec::new(), super::plugin::metadata::FileMetadata::to_bytes);

    let mut payload_with_metadata = Vec::new();
    if !metadata_bytes.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let metadata_len = metadata_bytes.len() as u16; // FileMetadata is always < 64KB
        payload_with_metadata.extend_from_slice(&metadata_len.to_le_bytes());
        payload_with_metadata.extend_from_slice(&metadata_bytes);
    }
    payload_with_metadata.extend_from_slice(&compressed_payload);

    // Calculate CRC32 of compressed payload + metadata
    let mut hasher = Hasher::new();
    hasher.update(&payload_with_metadata);
    let crc32 = hasher.finalize();

    // Create header with original size and CRC32
    let mut header =
        CrushHeader::new(selected_metadata.magic_number, input.len() as u64).with_crc32();
    if !metadata_bytes.is_empty() {
        header = header.with_metadata();
    }

    // Build final output: header + CRC32 + payload_with_metadata
    let mut output = Vec::with_capacity(CrushHeader::SIZE + 4 + payload_with_metadata.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&crc32.to_le_bytes());
    output.extend_from_slice(&payload_with_metadata);

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_plugins;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_basic() {
        init_plugins().unwrap();
        let data = b"Hello, Crush!";
        let compressed = compress(data).unwrap();

        // Should have at least the header
        assert!(compressed.len() >= CrushHeader::SIZE);

        // Header should have correct magic number
        assert_eq!(&compressed[0..4], &[0x43, 0x52, 0x01, 0x00]);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_empty() {
        init_plugins().unwrap();
        let data = b"";
        let compressed = compress(data).unwrap();

        // Even empty data gets a header
        assert!(compressed.len() >= CrushHeader::SIZE);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_large() {
        init_plugins().unwrap();
        let data = vec![0x42; 1_000_000]; // 1MB of repeated bytes
        let compressed = compress(&data).unwrap();

        // Should compress well (repeated data)
        assert!(compressed.len() < data.len() / 2);
    }

    // CompressionOptions tests
    #[test]
    fn test_compression_options_default() {
        let options = CompressionOptions::default();
        assert_eq!(options.timeout, DEFAULT_TIMEOUT);
        assert!(options.plugin_name.is_none());
        assert!(options.file_metadata.is_none());
        assert!(options.cancel_token.is_none());
    }

    #[test]
    fn test_compression_options_new() {
        let options = CompressionOptions::new();
        assert_eq!(options.timeout, DEFAULT_TIMEOUT);
    }

    #[test]
    fn test_compression_options_with_plugin() {
        let options = CompressionOptions::new().with_plugin("deflate");
        assert_eq!(options.plugin_name, Some("deflate".to_string()));
    }

    #[test]
    fn test_compression_options_with_weights() {
        let weights = ScoringWeights {
            throughput: 0.5,
            compression_ratio: 0.5,
        };
        let options = CompressionOptions::new().with_weights(weights);
        assert!((options.weights.throughput - 0.5).abs() < f64::EPSILON);
        assert!((options.weights.compression_ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_compression_options_with_timeout() {
        let timeout = Duration::from_secs(10);
        let options = CompressionOptions::new().with_timeout(timeout);
        assert_eq!(options.timeout, timeout);
    }

    #[test]
    fn test_compression_options_with_file_metadata() {
        let metadata = FileMetadata {
            mtime: Some(1_234_567_890),
            #[cfg(unix)]
            permissions: Some(0o644),
        };
        let options = CompressionOptions::new().with_file_metadata(metadata);
        assert!(options.file_metadata.is_some());
    }

    #[test]
    fn test_compression_options_with_cancel_token() {
        use crate::cancel::AtomicCancellationToken;
        let token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());
        let options = CompressionOptions::new().with_cancel_token(token);
        assert!(options.cancel_token.is_some());
    }

    #[test]
    fn test_compression_options_debug() {
        let options = CompressionOptions::new().with_plugin("test");
        let debug_str = format!("{options:?}");
        assert!(debug_str.contains("CompressionOptions"));
        assert!(debug_str.contains("test"));
    }

    // compress_with_options tests
    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_default() {
        init_plugins().unwrap();
        let data = b"Test data for compression";
        let options = CompressionOptions::default();
        let compressed = compress_with_options(data, &options).unwrap();

        assert!(compressed.len() >= CrushHeader::SIZE);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_manual_plugin() {
        init_plugins().unwrap();
        let data = b"Test data";
        let options = CompressionOptions::default().with_plugin("deflate");
        let compressed = compress_with_options(data, &options).unwrap();

        assert!(compressed.len() >= CrushHeader::SIZE);
        assert_eq!(&compressed[0..4], &[0x43, 0x52, 0x01, 0x00]); // DEFLATE magic
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_invalid_plugin() {
        init_plugins().unwrap();
        let data = b"Test data";
        let options = CompressionOptions::default().with_plugin("nonexistent");
        let result = compress_with_options(data, &options);

        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_with_metadata() {
        init_plugins().unwrap();
        let data = b"Test data with metadata";
        let metadata = FileMetadata {
            mtime: Some(1_234_567_890),
            #[cfg(unix)]
            permissions: Some(0o644),
        };
        let options = CompressionOptions::default().with_file_metadata(metadata);
        let compressed = compress_with_options(data, &options).unwrap();

        // Should have header + metadata
        assert!(compressed.len() > CrushHeader::SIZE + 4); // +4 for metadata length
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_cancellation() {
        use crate::cancel::AtomicCancellationToken;

        init_plugins().unwrap();
        let data = b"Test data";
        let token = Arc::new(AtomicCancellationToken::new());
        token.cancel(); // Cancel before compression

        let options = CompressionOptions::default().with_cancel_token(token);
        let result = compress_with_options(data, &options);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::CrushError::Cancelled
        ));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_timeout() {
        init_plugins().unwrap();
        let data = vec![0u8; 10_000_000]; // Large data
        let options = CompressionOptions::default().with_timeout(Duration::from_nanos(1)); // Extremely short timeout

        let result = compress_with_options(&data, &options);
        // May succeed or timeout depending on system speed
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_with_options_zero_timeout() {
        init_plugins().unwrap();
        let data = b"Test with no timeout";
        let options = CompressionOptions::default().with_timeout(Duration::from_secs(0)); // No timeout

        let result = compress_with_options(data, &options);
        assert!(result.is_ok());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compress_roundtrip_with_options() {
        use crate::decompress;

        init_plugins().unwrap();
        let original = b"Roundtrip test data with options";
        let options = CompressionOptions::default();
        let compressed = compress_with_options(original, &options).unwrap();
        let result = decompress(&compressed).unwrap();

        assert_eq!(result.data, original);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_compression_options_builder_chain() {
        use crate::cancel::AtomicCancellationToken;

        let metadata = FileMetadata {
            mtime: Some(1_234_567_890),
            #[cfg(unix)]
            permissions: Some(0o644),
        };
        let weights = ScoringWeights {
            throughput: 0.7,
            compression_ratio: 0.3,
        };
        let token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());

        // Test method chaining
        let options = CompressionOptions::new()
            .with_plugin("deflate")
            .with_weights(weights)
            .with_timeout(Duration::from_secs(30))
            .with_file_metadata(metadata)
            .with_cancel_token(token);

        assert_eq!(options.plugin_name, Some("deflate".to_string()));
        assert!((options.weights.throughput - 0.7).abs() < f64::EPSILON);
        assert_eq!(options.timeout, Duration::from_secs(30));
        assert!(options.file_metadata.is_some());
        assert!(options.cancel_token.is_some());
    }
}
