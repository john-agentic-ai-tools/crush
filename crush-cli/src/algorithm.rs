//! Algorithm selection logic for crush-cli (FR-016).
//!
//! This module is the single authoritative location for deciding which compression
//! plugin to use. It is intentionally decoupled from `crush-parallel` so that the
//! selection policy can change without recompiling the compression library.

/// Default input-size threshold above which the parallel engine is preferred (25 MB).
pub const DEFAULT_PARALLEL_THRESHOLD_BYTES: u64 = 25 * 1024 * 1024;

/// Select the name of the compression plugin to use.
///
/// # Arguments
///
/// - `input_size`: Known input byte count, or `None` for streaming inputs (unknown size).
/// - `explicit`: Value of the `--algorithm` CLI flag, if provided.
/// - `threshold`: Size threshold in bytes; inputs at or above this use `parallel-deflate`.
/// - `gpu_enabled`: If `true`, prefer `gpu-deflate` over `parallel-deflate` for large files.
///
/// # Returns
///
/// The plugin name string to look up in the crush-core plugin registry.
///
/// # Behaviour
///
/// 1. If `explicit` is `Some`, that value is returned unconditionally.
/// 2. If `input_size` is `None` (streaming), `"parallel-deflate"` is returned
///    (streaming favours parallel to avoid buffering the full input).
/// 3. If `input_size >= threshold` and `gpu_enabled`, `"gpu-deflate"` is returned.
/// 4. If `input_size >= threshold`, `"parallel-deflate"` is returned.
/// 5. Otherwise `"default"` is returned.
#[must_use]
pub fn select_algorithm(
    input_size: Option<u64>,
    explicit: Option<&str>,
    threshold: u64,
    gpu_enabled: bool,
) -> &'static str {
    if let Some(name) = explicit {
        // Leak the string to get &'static str — safe here because this is called
        // at most once per CLI invocation and the process exits afterwards.
        return Box::leak(name.to_owned().into_boxed_str());
    }
    match input_size {
        None => "parallel-deflate",
        Some(size) if size >= threshold && gpu_enabled => "gpu-deflate",
        Some(size) if size >= threshold => "parallel-deflate",
        _ => "default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_algorithm_auto_selects_parallel_above_threshold() {
        let name = select_algorithm(
            Some(DEFAULT_PARALLEL_THRESHOLD_BYTES),
            None,
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            false,
        );
        assert_eq!(name, "parallel-deflate");

        let name_over = select_algorithm(
            Some(DEFAULT_PARALLEL_THRESHOLD_BYTES + 1),
            None,
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            false,
        );
        assert_eq!(name_over, "parallel-deflate");
    }

    #[test]
    fn test_select_algorithm_uses_default_below_threshold() {
        let name = select_algorithm(
            Some(DEFAULT_PARALLEL_THRESHOLD_BYTES - 1),
            None,
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            false,
        );
        assert_eq!(name, "default");
    }

    #[test]
    fn test_select_algorithm_explicit_override() {
        let name = select_algorithm(
            Some(1024),
            Some("my-algo"),
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            false,
        );
        assert_eq!(name, "my-algo");

        // Explicit override even for large inputs
        let name_large = select_algorithm(
            Some(u64::MAX),
            Some("default"),
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            false,
        );
        assert_eq!(name_large, "default");
    }

    #[test]
    fn test_select_algorithm_streaming_uses_parallel() {
        let name = select_algorithm(None, None, DEFAULT_PARALLEL_THRESHOLD_BYTES, false);
        assert_eq!(name, "parallel-deflate");
    }

    #[test]
    fn test_select_algorithm_gpu_enabled_prefers_gpu_above_threshold() {
        let name = select_algorithm(
            Some(DEFAULT_PARALLEL_THRESHOLD_BYTES),
            None,
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            true,
        );
        assert_eq!(name, "gpu-deflate");
    }

    #[test]
    fn test_select_algorithm_gpu_enabled_still_default_below_threshold() {
        let name = select_algorithm(
            Some(DEFAULT_PARALLEL_THRESHOLD_BYTES - 1),
            None,
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            true,
        );
        assert_eq!(name, "default");
    }

    #[test]
    fn test_select_algorithm_gpu_enabled_explicit_override_wins() {
        let name = select_algorithm(
            Some(DEFAULT_PARALLEL_THRESHOLD_BYTES),
            Some("parallel-deflate"),
            DEFAULT_PARALLEL_THRESHOLD_BYTES,
            true,
        );
        assert_eq!(name, "parallel-deflate");
    }
}
