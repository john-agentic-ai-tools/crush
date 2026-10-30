//! Compression functionality
//!
//! Provides the public `compress()` API that compresses data using the default
//! DEFLATE plugin and wraps it with a Crush header.

use crate::cancel::CancellationToken;
use crate::error::Result;
use crate::plugin::registry::{get_default_plugin, get_plugin_by_magic};
use crate::plugin::timeout::{run_with_timeout_and_cancel_scoped, run_with_timeout_scoped};
use crate::plugin::{
    CompressionAlgorithm, CrushHeader, FileMetadata, PluginSelector, ScoringWeights,
};
use crc32fast::Hasher;
use rayon::prelude::*;
use std::io::{Seek, SeekFrom, Write};
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

    let header = CrushHeader::new(default_magic, input.len() as u64).with_crc32();
    let prefix = output_prefix(header, &[]);

    // Slice D: borrow `input` directly via a scoped thread — avoids a full-input clone on the hot path.
    // The plugin appends its payload after the prefix (no assembly copy).
    let mut output = std::thread::scope(|s| {
        run_with_timeout_scoped(s, DEFAULT_TIMEOUT, |cancel_flag| {
            let mut out = prefix;
            plugin.compress_into(input, cancel_flag, &mut out)?;
            Ok(out)
        })
    })?;
    seal_output(&mut output);
    Ok(output)
}

/// Compress data with custom options (plugin selection, scoring weights)
///
/// Provides fine-grained control over plugin selection through either:
/// - Manual plugin override by name
/// - Automatic selection with custom scoring weights; when speed is
///   preferred, a content-aware plugin is used if it estimates that `input`
///   compresses well with it (see [`PluginSelector::select_for_input`])
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
    let (plugin, prefix) = select_and_prefix(input, options)?;
    compress_in_memory(plugin, prefix, input, options)
}

/// Compress `input` as [`compress_with_options`] does, writing the result to
/// `out` instead of returning it; returns the number of bytes written.
///
/// Plugins that stream their output (`fast-lz`) write it piece by piece:
/// the header goes first with a zero CRC, the CRC32 is computed as the
/// payload passes through, and the CRC field is then patched by seeking
/// back, so the whole output is never held in memory. Other plugins
/// compress in memory and the result is written once. The bytes are the
/// same either way, and `out` is left positioned after them.
///
/// # Errors
///
/// As for [`compress_with_options`], plus I/O errors from `out`. On error,
/// `out` may hold a partial output; callers writing a file should remove it.
pub fn compress_to_writer<W: Write + Seek + Send>(
    input: &[u8],
    options: &CompressionOptions,
    out: &mut W,
) -> Result<u64> {
    let (plugin, prefix) = select_and_prefix(input, options)?;
    if !plugin.streams_compressed_output() {
        let output = compress_in_memory(plugin, prefix, input, options)?;
        out.write_all(&output)?;
        return Ok(output.len() as u64);
    }

    let start = out.stream_position()?;
    out.write_all(&prefix)?;
    let mut hasher = Hasher::new();
    hasher.update(&prefix[CRC_START..]);
    let mut sink = CrcWriter {
        inner: &mut *out,
        hasher,
        written: 0,
    };
    let timeout = options.timeout;
    let cancel_token = options.cancel_token.clone();
    std::thread::scope(|s| {
        run_with_timeout_and_cancel_scoped(s, timeout, cancel_token, |cancel_flag| {
            plugin.compress_to(input, cancel_flag, &mut sink)
        })
    })?;
    let (payload_len, crc32) = (sink.written, sink.hasher.finalize());

    out.seek(SeekFrom::Start(start + CrushHeader::SIZE as u64))?;
    out.write_all(&crc32.to_le_bytes())?;
    let total = prefix.len() as u64 + payload_len;
    out.seek(SeekFrom::Start(start + total))?;
    Ok(total)
}

/// Pass-through writer that hashes (CRC32) and counts what it writes.
struct CrcWriter<'a, W: Write> {
    inner: &'a mut W,
    hasher: Hasher,
    written: u64,
}

impl<W: Write> Write for CrcWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Check cancellation, select the plugin (by name, or for this input), and
/// build the outer-format prefix (header, CRC placeholder, metadata).
fn select_and_prefix(
    input: &[u8],
    options: &CompressionOptions,
) -> Result<(&'static dyn CompressionAlgorithm, Vec<u8>)> {
    // Check if already cancelled before starting
    if let Some(ref token) = options.cancel_token
        && token.is_cancelled()
    {
        return Err(crate::error::CrushError::Cancelled);
    }

    // Select plugin based on options
    let selector = PluginSelector::new(options.weights);

    let selected_metadata = if let Some(ref plugin_name) = options.plugin_name {
        // Manual override
        selector.select_by_name(plugin_name)?
    } else {
        // Automatic selection; content-aware plugins may take part when
        // speed is preferred (they estimate how well `input` compresses).
        selector.select_for_input(input)?
    };

    // Get the actual plugin from registry
    let plugin = get_plugin_by_magic(selected_metadata.magic_number).ok_or_else(|| {
        crate::error::PluginError::NotFound(format!(
            "Plugin '{}' metadata found but not in registry",
            selected_metadata.name
        ))
    })?;

    // Handle file metadata
    let metadata_bytes = options
        .file_metadata
        .as_ref()
        .map_or(Vec::new(), super::plugin::metadata::FileMetadata::to_bytes);

    // Create header with original size and CRC32
    let mut header =
        CrushHeader::new(selected_metadata.magic_number, input.len() as u64).with_crc32();
    if !metadata_bytes.is_empty() {
        header = header.with_metadata();
    }
    Ok((plugin, output_prefix(header, &metadata_bytes)))
}

/// Run the plugin (with the options' timeout and cancellation) appending its
/// payload after `prefix`, then fill in the CRC.
fn compress_in_memory(
    plugin: &'static dyn CompressionAlgorithm,
    prefix: Vec<u8>,
    input: &[u8],
    options: &CompressionOptions,
) -> Result<Vec<u8>> {
    let timeout = options.timeout;
    let cancel_token = options.cancel_token.clone();

    // Slice D: borrow `input` directly via a scoped thread — avoids a full-input clone.
    // The plugin appends its payload after the prefix (no assembly copy).
    let mut output = std::thread::scope(|s| {
        run_with_timeout_and_cancel_scoped(s, timeout, cancel_token, |cancel_flag| {
            let mut out = prefix;
            plugin.compress_into(input, cancel_flag, &mut out)?;
            Ok(out)
        })
    })?;
    seal_output(&mut output);
    Ok(output)
}

/// Payloads at least this large get their CRC32 computed in parallel.
const CRC_PARALLEL_MIN: usize = 8 * 1024 * 1024;
/// Chunk size for the parallel CRC32.
const CRC_PARALLEL_CHUNK: usize = 4 * 1024 * 1024;

/// Offset of the CRC32 field; the CRC covers everything after it.
const CRC_START: usize = CrushHeader::SIZE + 4;

/// The outer format up to the payload:
/// `header | crc32 (zero until sealed) | [metadata_len: u16 | metadata]`.
/// `metadata` is empty when the header has no metadata flag.
fn output_prefix(header: CrushHeader, metadata: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(CRC_START + 2 + metadata.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&[0; 4]);
    if !metadata.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let metadata_len = metadata.len() as u16; // FileMetadata is always < 64KB
        output.extend_from_slice(&metadata_len.to_le_bytes());
        output.extend_from_slice(metadata);
    }
    output
}

/// Write the CRC32 of everything after the CRC field into it.
fn seal_output(output: &mut [u8]) {
    if let Some((head, rest)) = output.split_at_mut_checked(CRC_START) {
        let crc32 = parallel_crc32(rest);
        head[CrushHeader::SIZE..].copy_from_slice(&crc32.to_le_bytes());
    }
}

/// The whole outer format around an existing payload (the tests compare
/// the prefix/seal path with the reference layout through it).
#[cfg(test)]
fn assemble_output(header: CrushHeader, metadata: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut output = output_prefix(header, metadata);
    output.extend_from_slice(payload);
    seal_output(&mut output);
    output
}

/// CRC32 of `data`, split across the rayon pool for large inputs.
fn parallel_crc32(data: &[u8]) -> u32 {
    if data.len() < CRC_PARALLEL_MIN {
        return crc32fast::hash(data);
    }
    data.par_chunks(CRC_PARALLEL_CHUNK)
        .map(|chunk| {
            let mut h = Hasher::new();
            h.update(chunk);
            h
        })
        .reduce(Hasher::new, |mut a, b| {
            a.combine(&b);
            a
        })
        .finalize()
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

    // --- Outer-format assembly (perf: one exact-size buffer, parallel CRC) ---

    /// The assembly as `compress_with_options` did it before the single-buffer
    /// rewrite: concatenate metadata and payload, CRC that, copy again.
    fn reference_assemble(header: CrushHeader, metadata: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut payload_with_metadata = Vec::new();
        if !metadata.is_empty() {
            #[allow(clippy::cast_possible_truncation)]
            let metadata_len = metadata.len() as u16;
            payload_with_metadata.extend_from_slice(&metadata_len.to_le_bytes());
            payload_with_metadata.extend_from_slice(metadata);
        }
        payload_with_metadata.extend_from_slice(payload);
        let crc32 = crc32fast::hash(&payload_with_metadata);
        let mut output = Vec::new();
        output.extend_from_slice(&header.to_bytes());
        output.extend_from_slice(&crc32.to_le_bytes());
        output.extend_from_slice(&payload_with_metadata);
        output
    }

    fn pseudo_random(len: usize) -> Vec<u8> {
        let mut state: u64 = 0x1234_5678_9ABC_DEF1;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect()
    }

    /// Plugins that don't stream are compressed in memory and written once:
    /// same bytes as `compress_with_options`, writer left after them.
    #[test]
    #[allow(clippy::expect_used)]
    fn compress_to_writer_matches_in_memory_for_non_streaming_plugins() {
        use std::io::Cursor;
        init_plugins().expect("init");
        let data = pseudo_random(200_000);
        let options = CompressionOptions::default().with_plugin("deflate");
        let whole = compress_with_options(&data, &options).expect("compress");
        let mut sink = Cursor::new(Vec::new());
        let n = compress_to_writer(&data, &options, &mut sink).expect("stream");
        assert_eq!(n, whole.len() as u64);
        assert_eq!(sink.position(), n);
        assert_eq!(sink.into_inner(), whole);
    }

    #[test]
    fn assemble_output_is_byte_identical_to_reference() {
        let metadata = FileMetadata {
            mtime: Some(1_700_000_000),
            #[cfg(unix)]
            permissions: Some(0o644),
        }
        .to_bytes();
        for len in [0usize, 1, 5 * 1024 * 1024, 9 * 1024 * 1024 + 7] {
            let payload = pseudo_random(len);
            for meta in [&[][..], &metadata[..]] {
                let mut header = CrushHeader::new([0x43, 0x52, 0x01, 0x02], 42).with_crc32();
                if !meta.is_empty() {
                    header = header.with_metadata();
                }
                assert_eq!(
                    assemble_output(header, meta, &payload),
                    reference_assemble(header, meta, &payload),
                    "len {len}, metadata {}",
                    meta.len()
                );
            }
        }
    }

    #[test]
    fn parallel_crc32_matches_serial() {
        let data = pseudo_random(3 * CRC_PARALLEL_CHUNK + 5);
        for len in [
            0,
            1,
            CRC_PARALLEL_CHUNK - 1,
            CRC_PARALLEL_CHUNK,
            CRC_PARALLEL_CHUNK + 1,
            CRC_PARALLEL_MIN - 1,
            CRC_PARALLEL_MIN,
            data.len(),
        ] {
            assert_eq!(
                parallel_crc32(&data[..len]),
                crc32fast::hash(&data[..len]),
                "len {len}"
            );
        }
    }
}
