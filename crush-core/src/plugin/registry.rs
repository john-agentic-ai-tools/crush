//! Plugin registry for runtime plugin management
//!
//! Provides a thread-safe registry that wraps the compile-time `COMPRESSION_ALGORITHMS`
//! distributed slice with runtime validation and management capabilities.

use crate::error::{PluginError, Result};
use crate::plugin::{CompressionAlgorithm, PluginMetadata, COMPRESSION_ALGORITHMS};
use std::collections::HashMap;
use std::sync::RwLock;

/// Thread-safe plugin registry
///
/// Wraps the compile-time plugin slice with runtime validation and lookup capabilities.
/// Uses `RwLock` for concurrent read access with exclusive write access during initialization.
static PLUGIN_REGISTRY: RwLock<Option<PluginRegistry>> = RwLock::new(None);

/// Plugin registry state
struct PluginRegistry {
    /// Maps magic numbers to plugin references
    plugins: HashMap<[u8; 4], &'static dyn CompressionAlgorithm>,
    /// Whether the registry has been initialized
    initialized: bool,
}

impl PluginRegistry {
    /// Create a new empty registry
    fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            initialized: false,
        }
    }

    /// Clear the registry (used for re-initialization)
    fn clear(&mut self) {
        self.plugins.clear();
        self.initialized = false;
    }

    /// Register a plugin after validation
    fn register(&mut self, plugin: &'static dyn CompressionAlgorithm) -> Result<()> {
        let metadata = plugin.metadata();

        // Validate metadata
        if metadata.name.is_empty() {
            return Err(
                PluginError::InvalidMetadata("Plugin name cannot be empty".to_string()).into(),
            );
        }

        if metadata.version.is_empty() {
            return Err(
                PluginError::InvalidMetadata("Plugin version cannot be empty".to_string()).into(),
            );
        }

        if metadata.throughput <= 0.0 {
            return Err(PluginError::InvalidMetadata(format!(
                "Plugin {} has invalid throughput: {}",
                metadata.name, metadata.throughput
            ))
            .into());
        }

        if metadata.compression_ratio <= 0.0 || metadata.compression_ratio > 1.0 {
            return Err(PluginError::InvalidMetadata(format!(
                "Plugin {} has invalid compression ratio: {}",
                metadata.name, metadata.compression_ratio
            ))
            .into());
        }

        // Check for duplicate magic number
        if let Some(existing) = self.plugins.get(&metadata.magic_number) {
            let existing_metadata = existing.metadata();
            eprintln!(
                "Warning: Duplicate magic number {:02X?} detected. \
                 Plugin '{}' conflicts with '{}'. \
                 Using first-registered plugin '{}'.",
                metadata.magic_number,
                metadata.name,
                existing_metadata.name,
                existing_metadata.name
            );
            return Ok(()); // Skip registration, keep first-registered
        }

        // Register the plugin
        self.plugins.insert(metadata.magic_number, plugin);

        Ok(())
    }

    /// Get all registered plugins
    fn list(&self) -> Vec<PluginMetadata> {
        self.plugins
            .values()
            .map(|plugin| plugin.metadata())
            .collect()
    }

    /// Get plugin by magic number
    fn get(&self, magic: [u8; 4]) -> Option<&'static dyn CompressionAlgorithm> {
        self.plugins.get(&magic).copied()
    }
}

/// Initialize the plugin system
///
/// Scans all compile-time registered plugins from the `COMPRESSION_ALGORITHMS`
/// distributed slice, validates their metadata, and registers them in the runtime registry.
///
/// # Behavior
///
/// - Can be called multiple times (re-initialization clears and re-scans)
/// - Detects duplicate magic numbers and logs warnings
/// - Validates plugin metadata (non-empty names, valid performance metrics)
/// - Thread-safe via `RwLock`
///
/// # Errors
///
/// Returns an error if:
/// - A plugin has invalid metadata (empty name/version, invalid performance metrics)
/// - Lock acquisition fails (should never happen in single-threaded tests)
///
/// # Examples
///
/// ```
/// use crush_core::init_plugins;
///
/// init_plugins().expect("Failed to initialize plugins");
/// ```
pub fn init_plugins() -> Result<()> {
    let mut guard = PLUGIN_REGISTRY
        .write()
        .map_err(|_| PluginError::OperationFailed("Failed to acquire registry lock".to_string()))?;

    // Create or clear registry
    let registry = guard.get_or_insert_with(PluginRegistry::new);

    // Clear existing plugins (support re-initialization)
    if registry.initialized {
        registry.clear();
    }

    // Scan and register all compile-time plugins
    for &plugin in COMPRESSION_ALGORITHMS {
        // Validate and register (duplicates are logged and skipped)
        registry.register(plugin)?;
    }

    registry.initialized = true;

    Ok(())
}

/// List all registered plugins
///
/// Returns metadata for all plugins currently registered in the runtime registry.
///
/// # Returns
///
/// A vector of `PluginMetadata` for all registered plugins. If `init_plugins()`
/// has not been called yet, returns an empty vector.
///
/// # Examples
///
/// ```
/// use crush_core::{init_plugins, list_plugins};
///
/// init_plugins().expect("Failed to initialize plugins");
/// let plugins = list_plugins();
///
/// for plugin in plugins {
///     println!("Plugin: {} v{}", plugin.name, plugin.version);
/// }
/// ```
#[must_use]
pub fn list_plugins() -> Vec<PluginMetadata> {
    PLUGIN_REGISTRY
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().map(PluginRegistry::list))
        .unwrap_or_default()
}

/// Get a plugin by magic number (internal use)
///
/// Used by decompression to route to the correct plugin based on file header.
pub(crate) fn get_plugin_by_magic(magic: [u8; 4]) -> Option<&'static dyn CompressionAlgorithm> {
    PLUGIN_REGISTRY
        .read()
        .ok()
        .and_then(|guard| guard.as_ref().and_then(|registry| registry.get(magic)))
}

/// Get the default plugin (internal use)
///
/// Returns the DEFLATE plugin (magic 0x43525100) for use by the `compress()` API.
pub(crate) fn get_default_plugin() -> Option<&'static dyn CompressionAlgorithm> {
    get_plugin_by_magic([0x43, 0x52, 0x01, 0x00])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_init_plugins() {
        init_plugins().unwrap();

        let plugins = list_plugins();
        assert!(
            !plugins.is_empty(),
            "Should discover at least DEFLATE plugin"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_get_default_plugin() {
        init_plugins().unwrap();

        let plugin = get_default_plugin();
        assert!(
            plugin.is_some(),
            "Default DEFLATE plugin should be available"
        );

        let plugin = plugin.unwrap();
        assert_eq!(plugin.name(), "deflate");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_reinitialization() {
        init_plugins().unwrap();
        let count1 = list_plugins().len();

        init_plugins().unwrap();
        let count2 = list_plugins().len();

        assert_eq!(
            count1, count2,
            "Re-initialization should maintain plugin count"
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_registry_validation_empty_name() {
        use crate::plugin::PluginMetadata;

        struct InvalidPlugin;
        impl CompressionAlgorithm for InvalidPlugin {
            fn name(&self) -> &'static str {
                "test_invalid"
            }

            fn metadata(&self) -> PluginMetadata {
                PluginMetadata {
                    name: "", // Empty name
                    version: "1.0.0",
                    magic_number: [0x43, 0x52, 0x01, 0xFF],
                    throughput: 100.0,
                    compression_ratio: 0.5,
                    description: "Test",
                }
            }

            fn compress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn decompress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn detect(&self, _file_header: &[u8]) -> bool {
                false
            }
        }

        let mut registry = PluginRegistry::new();
        let plugin: &'static dyn CompressionAlgorithm = Box::leak(Box::new(InvalidPlugin));
        let result = registry.register(plugin);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("name cannot be empty"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_registry_validation_empty_version() {
        use crate::plugin::PluginMetadata;

        struct InvalidPlugin;
        impl CompressionAlgorithm for InvalidPlugin {
            fn name(&self) -> &'static str {
                "test_invalid"
            }

            fn metadata(&self) -> PluginMetadata {
                PluginMetadata {
                    name: "test",
                    version: "", // Empty version
                    magic_number: [0x43, 0x52, 0x01, 0xFE],
                    throughput: 100.0,
                    compression_ratio: 0.5,
                    description: "Test",
                }
            }

            fn compress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn decompress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn detect(&self, _file_header: &[u8]) -> bool {
                false
            }
        }

        let mut registry = PluginRegistry::new();
        let plugin: &'static dyn CompressionAlgorithm = Box::leak(Box::new(InvalidPlugin));
        let result = registry.register(plugin);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("version cannot be empty"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_registry_validation_invalid_throughput() {
        use crate::plugin::PluginMetadata;

        struct InvalidPlugin;
        impl CompressionAlgorithm for InvalidPlugin {
            fn name(&self) -> &'static str {
                "test_invalid"
            }

            fn metadata(&self) -> PluginMetadata {
                PluginMetadata {
                    name: "test",
                    version: "1.0.0",
                    magic_number: [0x43, 0x52, 0x01, 0xFD],
                    throughput: -10.0, // Invalid throughput
                    compression_ratio: 0.5,
                    description: "Test",
                }
            }

            fn compress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn decompress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn detect(&self, _file_header: &[u8]) -> bool {
                false
            }
        }

        let mut registry = PluginRegistry::new();
        let plugin: &'static dyn CompressionAlgorithm = Box::leak(Box::new(InvalidPlugin));
        let result = registry.register(plugin);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid throughput"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_registry_validation_invalid_compression_ratio() {
        use crate::plugin::PluginMetadata;

        struct InvalidPlugin;
        impl CompressionAlgorithm for InvalidPlugin {
            fn name(&self) -> &'static str {
                "test_invalid"
            }

            fn metadata(&self) -> PluginMetadata {
                PluginMetadata {
                    name: "test",
                    version: "1.0.0",
                    magic_number: [0x43, 0x52, 0x01, 0xFC],
                    throughput: 100.0,
                    compression_ratio: 1.5, // Invalid ratio > 1.0
                    description: "Test",
                }
            }

            fn compress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn decompress(
                &self,
                _input: &[u8],
                _cancel_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<Vec<u8>> {
                Ok(vec![])
            }

            fn detect(&self, _file_header: &[u8]) -> bool {
                false
            }
        }

        let mut registry = PluginRegistry::new();
        let plugin: &'static dyn CompressionAlgorithm = Box::leak(Box::new(InvalidPlugin));
        let result = registry.register(plugin);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid compression ratio"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_get_plugin_by_magic() {
        init_plugins().unwrap();

        // Test retrieving DEFLATE plugin by magic number
        let plugin = get_plugin_by_magic([0x43, 0x52, 0x01, 0x00]);
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().name(), "deflate");

        // Test non-existent magic number
        let plugin = get_plugin_by_magic([0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(plugin.is_none());
    }

    #[test]
    fn test_list_plugins_before_init() {
        // Clear registry by creating new one (this is a bit hacky for testing)
        // In reality, users should always call init_plugins() first
        let plugins = list_plugins();
        // Should return empty list or default based on registry state
        // Just verify it doesn't panic
        let _ = plugins.len();
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_registry_clear() {
        init_plugins().unwrap();
        let count1 = list_plugins().len();
        assert!(count1 > 0);

        // Re-initialize to trigger clear
        init_plugins().unwrap();
        let count2 = list_plugins().len();
        assert_eq!(count1, count2);
    }
}
