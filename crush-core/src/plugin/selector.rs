//! Plugin selection and scoring logic
//!
//! Implements intelligent plugin selection based on performance metadata.
//! Uses configurable scoring weights (default 70% throughput, 30% compression ratio)
//! with logarithmic throughput scaling and min-max normalization.

use crate::error::{PluginError, Result, ValidationError};
use crate::plugin::{list_plugins, PluginMetadata};

/// Scoring weights for plugin selection
///
/// Weights determine the relative importance of throughput vs compression ratio
/// when selecting a plugin. Must sum to 1.0.
#[derive(Debug, Clone, Copy)]
pub struct ScoringWeights {
    /// Weight for throughput (MB/s) - default 0.7 (70%)
    pub throughput: f64,

    /// Weight for compression ratio - default 0.3 (30%)
    pub compression_ratio: f64,
}

impl ScoringWeights {
    /// Create new scoring weights
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Either weight is negative
    /// - Weights don't sum to 1.0 (within epsilon)
    ///
    /// # Examples
    ///
    /// ```
    /// use crush_core::ScoringWeights;
    ///
    /// // Default 70/30 weighting
    /// let weights = ScoringWeights::new(0.7, 0.3).expect("Valid weights");
    ///
    /// // Balanced weighting
    /// let balanced = ScoringWeights::new(0.5, 0.5).expect("Valid weights");
    /// ```
    pub fn new(throughput: f64, compression_ratio: f64) -> Result<Self> {
        if throughput < 0.0 || compression_ratio < 0.0 {
            return Err(
                ValidationError::InvalidWeights("Weights cannot be negative".to_string()).into(),
            );
        }

        let sum = throughput + compression_ratio;
        if (sum - 1.0).abs() > 1e-6 {
            return Err(ValidationError::InvalidWeights(format!(
                "Weights must sum to 1.0, got {sum}"
            ))
            .into());
        }

        Ok(Self {
            throughput,
            compression_ratio,
        })
    }
}

impl Default for ScoringWeights {
    /// Default weights: 70% throughput, 30% compression ratio
    fn default() -> Self {
        Self {
            throughput: 0.7,
            compression_ratio: 0.3,
        }
    }
}

/// Calculate plugin score using weighted metadata
///
/// Implements the scoring algorithm from research:
/// 1. Logarithmic throughput scaling (prevents throughput dominance)
/// 2. Min-max normalization (scales values to `[0,1]`)
/// 3. Weighted sum based on user preferences
///
/// # Arguments
///
/// * `plugin` - The plugin to score
/// * `all_plugins` - All available plugins (for min-max normalization)
/// * `weights` - Scoring weights (throughput vs compression ratio)
///
/// # Returns
///
/// Score in range [0.0, 1.0] where higher is better
///
/// # Examples
///
/// ```
/// use crush_core::{calculate_plugin_score, PluginMetadata, ScoringWeights};
///
/// let plugin = PluginMetadata {
///     name: "test",
///     version: "1.0.0",
///     magic_number: [0x43, 0x52, 0x01, 0x00],
///     throughput: 500.0,
///     compression_ratio: 0.35,
///     description: "Test plugin",
/// };
///
/// let plugins = vec![plugin];
/// let weights = ScoringWeights::default();
/// let score = calculate_plugin_score(&plugin, &plugins, &weights);
///
/// assert!(score >= 0.0 && score <= 1.0);
/// ```
pub fn calculate_plugin_score(
    plugin: &PluginMetadata,
    all_plugins: &[PluginMetadata],
    weights: &ScoringWeights,
) -> f64 {
    if all_plugins.is_empty() {
        return 0.0;
    }

    // Special case: single plugin always scores 1.0
    if all_plugins.len() == 1 {
        return 1.0;
    }

    // Apply logarithmic scaling to throughput
    let log_throughputs: Vec<f64> = all_plugins.iter().map(|p| p.throughput.ln()).collect();

    let plugin_log_throughput = plugin.throughput.ln();

    // Find min/max for normalization
    let min_log_throughput = log_throughputs
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let max_log_throughput = log_throughputs
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    let compression_ratios: Vec<f64> = all_plugins.iter().map(|p| p.compression_ratio).collect();

    let min_ratio = compression_ratios
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let max_ratio = compression_ratios
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    // Normalize throughput (higher is better)
    let norm_throughput = if (max_log_throughput - min_log_throughput).abs() < 1e-9 {
        1.0 // All same throughput
    } else {
        (plugin_log_throughput - min_log_throughput) / (max_log_throughput - min_log_throughput)
    };

    // Normalize compression ratio (lower is better, so invert)
    let norm_ratio = if (max_ratio - min_ratio).abs() < 1e-9 {
        1.0 // All same ratio
    } else {
        (max_ratio - plugin.compression_ratio) / (max_ratio - min_ratio)
    };

    // Weighted score
    weights.throughput * norm_throughput + weights.compression_ratio * norm_ratio
}

/// Plugin selector with scoring logic
pub struct PluginSelector {
    weights: ScoringWeights,
}

impl PluginSelector {
    /// Create a new plugin selector with custom weights
    #[must_use]
    pub fn new(weights: ScoringWeights) -> Self {
        Self { weights }
    }

    /// Select the best plugin based on scoring
    ///
    /// Returns the plugin with the highest score. In case of ties,
    /// selects alphabetically by name.
    ///
    /// # Errors
    ///
    /// Returns an error if no plugins are available.
    pub fn select(&self) -> Result<PluginMetadata> {
        let plugins = list_plugins();

        if plugins.is_empty() {
            return Err(PluginError::NotFound(
                "No plugins available. Call init_plugins() first.".to_string(),
            )
            .into());
        }

        // Calculate scores for all plugins
        let mut scored_plugins: Vec<(f64, &PluginMetadata)> = plugins
            .iter()
            .map(|plugin| {
                let score = calculate_plugin_score(plugin, &plugins, &self.weights);
                (score, plugin)
            })
            .collect();

        // Sort by score (descending), then by name (ascending) for ties
        scored_plugins.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.name.cmp(b.1.name))
        });

        // Return the highest-scoring plugin
        Ok(*scored_plugins[0].1)
    }

    /// Select a plugin by name (manual override)
    ///
    /// # Errors
    ///
    /// Returns an error if the specified plugin is not found.
    pub fn select_by_name(&self, name: &str) -> Result<PluginMetadata> {
        let plugins = list_plugins();

        plugins.into_iter().find(|p| p.name == name).ok_or_else(|| {
            PluginError::NotFound(format!(
                "Plugin '{}' not found. Available plugins: {}",
                name,
                list_plugins()
                    .iter()
                    .map(|p| p.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .into()
        })
    }
}

impl Default for PluginSelector {
    fn default() -> Self {
        Self::new(ScoringWeights::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scoring_weights_validation() {
        // Valid weights
        assert!(ScoringWeights::new(0.7, 0.3).is_ok());
        assert!(ScoringWeights::new(0.5, 0.5).is_ok());
        assert!(ScoringWeights::new(1.0, 0.0).is_ok());

        // Invalid: doesn't sum to 1.0
        assert!(ScoringWeights::new(0.6, 0.6).is_err());
        assert!(ScoringWeights::new(0.3, 0.3).is_err());

        // Invalid: negative
        assert!(ScoringWeights::new(-0.1, 1.1).is_err());
    }

    #[test]
    fn test_default_weights() {
        let weights = ScoringWeights::default();
        assert!((weights.throughput - 0.7).abs() < 1e-6);
        assert!((weights.compression_ratio - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_calculate_score_single_plugin() {
        let plugin = PluginMetadata {
            name: "test",
            version: "1.0.0",
            magic_number: [0x43, 0x52, 0x01, 0x00],
            throughput: 500.0,
            compression_ratio: 0.35,
            description: "Test",
        };

        let plugins = vec![plugin];
        let weights = ScoringWeights::default();
        let score = calculate_plugin_score(&plugin, &plugins, &weights);

        // Single plugin always scores 1.0
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_calculate_score_multiple_plugins() {
        let fast = PluginMetadata {
            name: "fast",
            version: "1.0.0",
            magic_number: [0x43, 0x52, 0x01, 0x10],
            throughput: 1000.0,
            compression_ratio: 0.8,
            description: "Fast but poor compression",
        };

        let slow = PluginMetadata {
            name: "slow",
            version: "1.0.0",
            magic_number: [0x43, 0x52, 0x01, 0x11],
            throughput: 100.0,
            compression_ratio: 0.3,
            description: "Slow but good compression",
        };

        let plugins = vec![fast, slow];
        let weights = ScoringWeights::default();

        let fast_score = calculate_plugin_score(&fast, &plugins, &weights);
        let slow_score = calculate_plugin_score(&slow, &plugins, &weights);

        // With 70% throughput weight, fast should win
        assert!(fast_score > slow_score);

        // With balanced weights, might be different
        let balanced = ScoringWeights::new(0.5, 0.5).unwrap();
        let fast_balanced = calculate_plugin_score(&fast, &plugins, &balanced);
        let slow_balanced = calculate_plugin_score(&slow, &plugins, &balanced);

        // Scores should change with different weights
        assert!((fast_balanced - fast_score).abs() > 1e-6);
        assert!((slow_balanced - slow_score).abs() > 1e-6);
    }
}
