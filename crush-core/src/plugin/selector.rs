//! Plugin selection and scoring logic
//!
//! Implements intelligent plugin selection based on performance metadata.
//! Uses configurable scoring weights (default 70% throughput, 30% compression ratio)
//! with logarithmic throughput scaling and min-max normalization.

use crate::error::{PluginError, Result, ValidationError};
use crate::plugin::registry::{get_plugin_by_magic, list_plugins_with_awareness};
use crate::plugin::{PluginMetadata, list_plugins};

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

/// Throughput weight at or above which speed is preferred: only then may a
/// content-aware plugin be chosen automatically (`--level fast` uses 0.9).
pub const SPEED_PREFERRED_THROUGHPUT: f64 = 0.8;

/// Smallest estimated ratio (`input_len / compressed_len`) for which a
/// content-aware plugin is chosen automatically.
pub const MIN_ESTIMATED_RATIO: f64 = 1.10;

/// Plugins that take part in weight-based scoring: everything that isn't
/// content-aware (see `CompressionAlgorithm::content_aware`).
fn scoring_candidates(all: &[(PluginMetadata, bool)]) -> Vec<PluginMetadata> {
    all.iter()
        .filter(|(_, aware)| !aware)
        .map(|(m, _)| *m)
        .collect()
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
        let candidates = list_plugins_with_awareness();
        if candidates.is_empty() {
            return Err(PluginError::NotFound(
                "No plugins available. Call init_plugins() first.".to_string(),
            )
            .into());
        }
        self.best_by_score(&scoring_candidates(&candidates))
    }

    /// Select a plugin for compressing `input`.
    ///
    /// When speed is preferred (`throughput` weight of at least
    /// [`SPEED_PREFERRED_THROUGHPUT`]), content-aware plugins estimate how
    /// well `input` compresses with them, and the one with the highest
    /// estimate of at least [`MIN_ESTIMATED_RATIO`] is chosen. Otherwise, or
    /// when none qualifies, the choice is the same as [`Self::select`]'s.
    ///
    /// # Errors
    ///
    /// Returns an error if no plugins are available.
    pub fn select_for_input(&self, input: &[u8]) -> Result<PluginMetadata> {
        let candidates = list_plugins_with_awareness();
        if candidates.is_empty() {
            return Err(PluginError::NotFound(
                "No plugins available. Call init_plugins() first.".to_string(),
            )
            .into());
        }
        self.choose_for_input(&candidates, |m| {
            get_plugin_by_magic(m.magic_number).and_then(|p| p.estimate_ratio(input))
        })
    }

    /// The decision behind [`Self::select_for_input`], with the estimate
    /// supplied by the caller. `estimate` is only called for content-aware
    /// plugins, and only when speed is preferred.
    fn choose_for_input(
        &self,
        all: &[(PluginMetadata, bool)],
        estimate: impl Fn(&PluginMetadata) -> Option<f64>,
    ) -> Result<PluginMetadata> {
        if self.weights.throughput >= SPEED_PREFERRED_THROUGHPUT {
            let best = all
                .iter()
                .filter(|(_, aware)| *aware)
                .filter_map(|(m, _)| {
                    estimate(m)
                        .filter(|r| r.is_finite() && *r >= MIN_ESTIMATED_RATIO)
                        .map(|r| (r, m))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.name.cmp(a.1.name)));
            if let Some((_, m)) = best {
                return Ok(*m);
            }
        }
        self.best_by_score(&scoring_candidates(all))
    }

    /// Highest-scoring plugin among `plugins` (ties broken by name).
    fn best_by_score(&self, plugins: &[PluginMetadata]) -> Result<PluginMetadata> {
        let mut scored: Vec<(f64, &PluginMetadata)> = plugins
            .iter()
            .map(|plugin| {
                (
                    calculate_plugin_score(plugin, plugins, &self.weights),
                    plugin,
                )
            })
            .collect();

        // Sort by score (descending), then by name (ascending) for ties
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.name.cmp(b.1.name))
        });

        scored.first().map(|(_, p)| **p).ok_or_else(|| {
            PluginError::NotFound("No plugins eligible for automatic selection".to_string()).into()
        })
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

    fn meta(name: &'static str, magic: u8, throughput: f64, ratio: f64) -> PluginMetadata {
        PluginMetadata {
            name,
            version: "1.0.0",
            magic_number: [0x43, 0x52, 0x01, magic],
            throughput,
            compression_ratio: ratio,
            description: "test",
        }
    }

    /// Content-aware plugins are left out of weight-based scoring, so adding
    /// one never changes what `select()` picks (FR-010).
    #[test]
    #[allow(clippy::expect_used)]
    fn content_aware_plugins_are_not_scored() {
        let deflate = meta("deflate", 0x00, 200.0, 0.35);
        let gpu = meta("gpu", 0x03, 2000.0, 0.65);
        // Would win at balanced weights if it were scored.
        let fast = meta("fast", 0x04, 900.0, 0.44);
        let candidates = [(deflate, false), (gpu, false), (fast, true)];

        let scored = scoring_candidates(&candidates);
        assert_eq!(scored.len(), 2);
        assert!(scored.iter().all(|p| p.name != "fast"));

        let balanced = ScoringWeights::new(0.5, 0.5).expect("weights");
        let pick = PluginSelector::new(balanced)
            .best_by_score(&scored)
            .expect("pick");
        let without = PluginSelector::new(balanced)
            .best_by_score(&[deflate, gpu])
            .expect("pick");
        assert_eq!(pick.name, without.name);
    }

    /// Candidates for the content-aware tests: two scored plugins plus a
    /// content-aware one that would win balanced scoring if it were scored.
    fn aware_candidates() -> [(PluginMetadata, bool); 3] {
        [
            (meta("deflate", 0x00, 200.0, 0.35), false),
            (meta("gpu", 0x03, 2000.0, 0.65), false),
            (meta("fast", 0x04, 900.0, 0.44), true),
        ]
    }

    #[allow(clippy::expect_used)]
    fn pick(throughput: f64, estimate: Option<f64>) -> &'static str {
        let weights = ScoringWeights::new(throughput, 1.0 - throughput).expect("weights");
        let all = aware_candidates();
        PluginSelector::new(weights)
            .choose_for_input(&all, |m| if m.name == "fast" { estimate } else { None })
            .expect("pick")
            .name
    }

    #[allow(clippy::expect_used)]
    fn pick_without_aware(throughput: f64) -> &'static str {
        let weights = ScoringWeights::new(throughput, 1.0 - throughput).expect("weights");
        let all = aware_candidates();
        PluginSelector::new(weights)
            .best_by_score(&scoring_candidates(&all))
            .expect("pick")
            .name
    }

    /// Below the speed preference, content-aware plugins are never chosen
    /// and the choice is exactly the one without them (FR-010).
    #[test]
    #[allow(clippy::expect_used)]
    fn input_selection_without_speed_preference_ignores_estimates() {
        for throughput in [0.0, 0.3, 0.5, 0.7, 0.79] {
            for estimate in [None, Some(0.9), Some(1.5), Some(10.0)] {
                assert_eq!(
                    pick(throughput, estimate),
                    pick_without_aware(throughput),
                    "throughput {throughput}, estimate {estimate:?}"
                );
            }
        }
    }

    /// With speed preferred, a content-aware plugin is chosen when its
    /// estimate is at least `MIN_ESTIMATED_RATIO`, otherwise normal scoring.
    #[test]
    #[allow(clippy::expect_used)]
    fn input_selection_with_speed_preference_uses_the_estimate() {
        for throughput in [0.8, 0.9, 1.0] {
            assert_eq!(pick(throughput, Some(MIN_ESTIMATED_RATIO)), "fast");
            assert_eq!(pick(throughput, Some(2.3)), "fast");
            for estimate in [None, Some(1.09), Some(1.0), Some(0.5), Some(f64::NAN)] {
                assert_eq!(
                    pick(throughput, estimate),
                    pick_without_aware(throughput),
                    "throughput {throughput}, estimate {estimate:?}"
                );
            }
        }
    }

    /// Estimates are only computed when they can change the choice.
    #[test]
    #[allow(clippy::expect_used)]
    fn estimates_only_computed_when_speed_preferred() {
        let all = aware_candidates();
        let calls = std::cell::Cell::new(0);
        let estimate = |_: &PluginMetadata| {
            calls.set(calls.get() + 1);
            Some(2.0)
        };
        let balanced = ScoringWeights::new(0.5, 0.5).expect("weights");
        PluginSelector::new(balanced)
            .choose_for_input(&all, estimate)
            .expect("pick");
        assert_eq!(calls.get(), 0);
        let fast = ScoringWeights::new(0.9, 0.1).expect("weights");
        PluginSelector::new(fast)
            .choose_for_input(&all, estimate)
            .expect("pick");
        assert_eq!(calls.get(), 1, "only the content-aware plugin is estimated");
    }

    /// Several eligible content-aware plugins: the highest estimate wins.
    #[test]
    #[allow(clippy::expect_used)]
    fn highest_estimate_wins() {
        let all = [
            (meta("deflate", 0x00, 200.0, 0.35), false),
            (meta("lz-a", 0x05, 900.0, 0.44), true),
            (meta("lz-b", 0x06, 800.0, 0.40), true),
        ];
        let fast = ScoringWeights::new(1.0, 0.0).expect("weights");
        let chosen = PluginSelector::new(fast)
            .choose_for_input(&all, |m| Some(if m.name == "lz-a" { 1.5 } else { 2.5 }))
            .expect("pick");
        assert_eq!(chosen.name, "lz-b");
    }

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
