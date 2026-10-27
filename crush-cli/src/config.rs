use crate::cli::{Cli, LogFormat};
use crate::error::{CliError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub compression: CompressionConfig,

    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub logging: LoggingConfig,

    #[serde(default)]
    pub gpu: GpuConfig,
}

impl Config {
    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate compression level
        if !["fast", "balanced", "best"].contains(&self.compression.level.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid compression level: '{}' (must be fast, balanced, or best)",
                self.compression.level
            )));
        }

        // Validate color setting
        if !["auto", "always", "never"].contains(&self.output.color.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid color setting: '{}' (must be auto, always, or never)",
                self.output.color
            )));
        }

        // Validate log format
        if !["human", "json"].contains(&self.logging.format.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid log format: '{}' (must be human or json)",
                self.logging.format
            )));
        }

        // Validate log level
        if !["error", "warn", "info", "debug", "trace"].contains(&self.logging.level.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid log level: '{}' (must be error, warn, info, debug, or trace)",
                self.logging.level
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Default plugin ("auto" for automatic selection)
    #[serde(default = "default_plugin")]
    pub default_plugin: String,

    /// Default compression level
    #[serde(default = "default_level")]
    pub level: String, // "fast" | "balanced" | "best"

    /// Default timeout in seconds (0 = no timeout)
    #[serde(default)]
    pub timeout_seconds: u64,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            default_plugin: "auto".to_string(),
            level: "balanced".to_string(),
            timeout_seconds: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Show progress bars for long operations
    #[serde(default = "default_true")]
    pub progress_bars: bool,

    /// Use colored output ("auto" | "always" | "never")
    #[serde(default = "default_auto")]
    pub color: String,

    /// Suppress non-error output
    #[serde(default)]
    pub quiet: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            progress_bars: true,
            color: "auto".to_string(),
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log format ("human" | "json")
    #[serde(default = "default_human")]
    pub format: String,

    /// Log level ("error" | "warn" | "info" | "debug" | "trace")
    #[serde(default = "default_info")]
    pub level: String,

    /// Log output file (empty = stderr)
    #[serde(default)]
    pub file: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: "human".to_string(),
            level: "info".to_string(),
            file: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuConfig {
    /// Enable GPU-accelerated compression for auto-selection
    #[serde(default)]
    pub enabled: bool,

    /// Preferred GPU device index (None = auto-select)
    #[serde(default)]
    pub device: Option<u32>,

    /// Force CPU-only decompression
    #[serde(default, rename = "force-cpu")]
    pub force_cpu: bool,
}

// Default value helpers for serde
fn default_plugin() -> String {
    "auto".to_string()
}
fn default_level() -> String {
    "balanced".to_string()
}
fn default_true() -> bool {
    true
}
fn default_auto() -> String {
    "auto".to_string()
}
fn default_human() -> String {
    "human".to_string()
}
fn default_info() -> String {
    "info".to_string()
}

/// Merge environment variables into config
pub fn merge_env_vars(config: Config) -> Result<Config> {
    merge_env_vars_from(config, std::env::vars())
}

/// Merge an explicit set of `CRUSH_*` variables into config.
///
/// Split out from [`merge_env_vars`] so the parsing rules can be tested without
/// mutating the process environment. Since Rust 2024 `std::env::set_var` is
/// `unsafe` — it is undefined behaviour if another thread reads the environment
/// concurrently, which is exactly what `cargo test`'s thread pool does. Passing
/// the variables in sidesteps that entirely.
pub fn merge_env_vars_from<I>(mut config: Config, vars: I) -> Result<Config>
where
    I: IntoIterator<Item = (String, String)>,
{
    for (key, value) in vars {
        if !key.starts_with("CRUSH_") {
            continue;
        }

        // Convert CRUSH_COMPRESSION_DEFAULT_PLUGIN to compression.default.plugin
        let config_key = key[6..] // Remove CRUSH_ prefix
            .to_lowercase()
            .replace('_', ".");

        match config_key.as_str() {
            "compression.default.plugin" | "compression.defaultplugin" => {
                config.compression.default_plugin = value;
            }
            "compression.level" => {
                config.compression.level = value;
            }
            "compression.timeout.seconds" | "compression.timeoutseconds" => {
                config.compression.timeout_seconds = value
                    .parse()
                    .map_err(|_| CliError::Config(format!("Invalid timeout value: {}", value)))?;
            }
            "output.progress.bars" | "output.progressbars" => {
                config.output.progress_bars = value
                    .parse()
                    .map_err(|_| CliError::Config(format!("Invalid boolean value: {}", value)))?;
            }
            "output.color" => {
                config.output.color = value;
            }
            "output.quiet" => {
                config.output.quiet = value
                    .parse()
                    .map_err(|_| CliError::Config(format!("Invalid boolean value: {}", value)))?;
            }
            "logging.format" => {
                config.logging.format = value;
            }
            "logging.level" => {
                config.logging.level = value;
            }
            "logging.file" => {
                config.logging.file = value;
            }
            "gpu.enabled" => {
                config.gpu.enabled = value
                    .parse()
                    .map_err(|_| CliError::Config(format!("Invalid boolean value: {}", value)))?;
            }
            "gpu.device" => {
                config.gpu.device = Some(value.parse().map_err(|_| {
                    CliError::Config(format!("Invalid GPU device index: {}", value))
                })?);
            }
            "gpu.force.cpu" | "gpu.forcecpu" | "gpu.force-cpu" => {
                config.gpu.force_cpu = value
                    .parse()
                    .map_err(|_| CliError::Config(format!("Invalid boolean value: {}", value)))?;
            }
            _ => {} // Ignore unknown env vars
        }
    }

    Ok(config)
}

/// Merge CLI arguments into config (highest priority)
pub fn merge_cli_args(mut config: Config, args: &Cli) -> Result<Config> {
    // Verbose flag overrides log level
    if args.verbose > 0 {
        config.logging.level = match args.verbose {
            1 => "debug".to_string(),
            _ => "trace".to_string(), // 2 or more = trace
        };
    }

    // Quiet flag overrides output setting
    if args.quiet {
        config.output.quiet = true;
    }

    // Log format
    config.logging.format = match args.log_format {
        LogFormat::Human => "human".to_string(),
        LogFormat::Json => "json".to_string(),
    };

    // Log file
    if let Some(ref log_file) = args.log_file {
        config.logging.file = log_file.to_string_lossy().to_string();
    }

    // GPU flags from subcommands override config
    match &args.command {
        crate::cli::Commands::Compress(compress_args) => {
            if let Some(device) = compress_args.gpu_device {
                config.gpu.device = Some(device);
            }
        }
        crate::cli::Commands::Decompress(decompress_args) => {
            if decompress_args.force_cpu {
                config.gpu.force_cpu = true;
            }
            if let Some(device) = decompress_args.gpu_device {
                config.gpu.device = Some(device);
            }
        }
        _ => {}
    }

    Ok(config)
}

// In-process config path override used by this crate's unit tests.
//
// Unit tests run as threads inside one process, so pointing them at a temp file
// via `CRUSH_TEST_CONFIG_FILE` would mean calling `std::env::set_var`, which is
// `unsafe` in Rust 2024 and unsound while sibling test threads read the
// environment. This thread-local carries the override instead: it is private to
// the calling thread, so tests need no locking and can run in parallel.
//
// Out-of-process integration tests are unaffected — they pass
// `CRUSH_TEST_CONFIG_FILE` to a child process via `Command::env`, which is safe
// and still honoured below.
#[cfg(test)]
thread_local! {
    static TEST_CONFIG_PATH: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Point [`config_file_path`] at `path` for the current thread only.
///
/// Returns a guard that restores the previous value on drop, so a failing test
/// cannot leak its override into whatever runs next on the same thread.
#[cfg(test)]
#[must_use]
pub(crate) fn set_test_config_path(path: PathBuf) -> TestConfigPathGuard {
    let previous = TEST_CONFIG_PATH.with(|p| p.borrow_mut().replace(path));
    TestConfigPathGuard { previous }
}

/// Restores the previous thread-local config path override on drop.
#[cfg(test)]
pub(crate) struct TestConfigPathGuard {
    previous: Option<PathBuf>,
}

#[cfg(test)]
impl Drop for TestConfigPathGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        TEST_CONFIG_PATH.with(|p| *p.borrow_mut() = previous);
    }
}

/// Resolve the current thread's test override, if one is installed.
#[cfg(test)]
fn test_config_path_override() -> Option<PathBuf> {
    TEST_CONFIG_PATH.with(|p| p.borrow().clone())
}

#[cfg(not(test))]
fn test_config_path_override() -> Option<PathBuf> {
    None
}

/// Get the config file path for the current OS
///
/// For testing, set `CRUSH_TEST_CONFIG_FILE` environment variable to use a custom path.
/// This allows tests to run in isolation without interfering with each other.
pub fn config_file_path() -> Result<PathBuf> {
    // In-process unit tests install a thread-local override (see
    // `set_test_config_path`); out-of-process tests still use the env var.
    let override_path = test_config_path_override()
        .map(|p| Ok(p.to_string_lossy().into_owned()))
        .unwrap_or_else(|| std::env::var("CRUSH_TEST_CONFIG_FILE"));

    // Allow tests to override config path via environment variable
    if let Ok(test_path) = override_path {
        let path = PathBuf::from(test_path);
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CliError::Config(format!("Could not create test config directory: {}", e))
            })?;
        }
        return Ok(path);
    }

    let config_dir = dirs::config_dir()
        .ok_or_else(|| CliError::Config("Could not determine config directory".to_string()))?;

    let crush_dir = config_dir.join("crush");
    fs::create_dir_all(&crush_dir)
        .map_err(|e| CliError::Config(format!("Could not create config directory: {}", e)))?;

    Ok(crush_dir.join("config.toml"))
}

/// Load configuration from file, or return defaults if file doesn't exist
pub fn load_config() -> Result<Config> {
    let path = config_file_path()?;

    if !path.exists() {
        return Ok(Config::default());
    }

    let contents = fs::read_to_string(&path)
        .map_err(|e| CliError::Config(format!("Could not read config file: {}", e)))?;

    toml::from_str(&contents)
        .map_err(|e| CliError::Config(format!("Invalid config file format: {}", e)))
}

/// Save configuration to file
pub fn save_config(config: &Config) -> Result<()> {
    let path = config_file_path()?;

    let toml_string = toml::to_string_pretty(config)
        .map_err(|e| CliError::Config(format!("Could not serialize config: {}", e)))?;

    fs::write(&path, toml_string)
        .map_err(|e| CliError::Config(format!("Could not write config file: {}", e)))?;

    Ok(())
}

/// Get a config value by key path (e.g., "compression.level")
pub fn get_config_value(config: &Config, key: &str) -> Result<String> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() != 2 {
        return Err(CliError::Config(format!(
            "Invalid config key: '{}' (must be section.key format)",
            key
        )));
    }

    let section = parts[0];
    let field = parts[1];

    match (section, field) {
        ("compression", "default-plugin") | ("compression", "default_plugin") => {
            Ok(config.compression.default_plugin.clone())
        }
        ("compression", "level") => Ok(config.compression.level.clone()),
        ("compression", "timeout-seconds") | ("compression", "timeout_seconds") => {
            Ok(config.compression.timeout_seconds.to_string())
        }
        ("output", "progress-bars") | ("output", "progress_bars") => {
            Ok(config.output.progress_bars.to_string())
        }
        ("output", "color") => Ok(config.output.color.clone()),
        ("output", "quiet") => Ok(config.output.quiet.to_string()),
        ("logging", "format") => Ok(config.logging.format.clone()),
        ("logging", "level") => Ok(config.logging.level.clone()),
        ("logging", "file") => Ok(config.logging.file.clone()),
        ("gpu", "enabled") => Ok(config.gpu.enabled.to_string()),
        ("gpu", "device") => Ok(config
            .gpu
            .device
            .map_or_else(|| "auto".to_string(), |d| d.to_string())),
        ("gpu", "force-cpu") | ("gpu", "force_cpu") => Ok(config.gpu.force_cpu.to_string()),
        _ => Err(CliError::Config(format!(
            "Invalid config key: '{}.{}' (unknown key)",
            section, field
        ))),
    }
}

/// Set a config value by key path
pub fn set_config_value(config: &mut Config, key: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.len() != 2 {
        return Err(CliError::Config(format!(
            "Invalid config key: '{}' (must be section.key format)",
            key
        )));
    }

    let section = parts[0];
    let field = parts[1];

    match (section, field) {
        ("compression", "default-plugin") | ("compression", "default_plugin") => {
            config.compression.default_plugin = value.to_string();
        }
        ("compression", "level") => {
            if !["fast", "balanced", "best"].contains(&value) {
                return Err(CliError::Config(format!(
                    "Invalid compression level: '{}' (must be fast, balanced, or best)",
                    value
                )));
            }
            config.compression.level = value.to_string();
        }
        ("compression", "timeout-seconds") | ("compression", "timeout_seconds") => {
            config.compression.timeout_seconds = value.parse().map_err(|_| {
                CliError::Config(format!(
                    "Invalid timeout value: '{}' (must be a number)",
                    value
                ))
            })?;
        }
        ("output", "progress-bars") | ("output", "progress_bars") => {
            config.output.progress_bars = value.parse().map_err(|_| {
                CliError::Config(format!(
                    "Invalid boolean value: '{}' (must be true or false)",
                    value
                ))
            })?;
        }
        ("output", "color") => {
            if !["auto", "always", "never"].contains(&value) {
                return Err(CliError::Config(format!(
                    "Invalid color setting: '{}' (must be auto, always, or never)",
                    value
                )));
            }
            config.output.color = value.to_string();
        }
        ("output", "quiet") => {
            config.output.quiet = value.parse().map_err(|_| {
                CliError::Config(format!(
                    "Invalid boolean value: '{}' (must be true or false)",
                    value
                ))
            })?;
        }
        ("logging", "format") => {
            if !["human", "json"].contains(&value) {
                return Err(CliError::Config(format!(
                    "Invalid log format: '{}' (must be human or json)",
                    value
                )));
            }
            config.logging.format = value.to_string();
        }
        ("logging", "level") => {
            if !["error", "warn", "info", "debug", "trace"].contains(&value) {
                return Err(CliError::Config(format!(
                    "Invalid log level: '{}' (must be error, warn, info, debug, or trace)",
                    value
                )));
            }
            config.logging.level = value.to_string();
        }
        ("logging", "file") => {
            config.logging.file = value.to_string();
        }
        ("gpu", "enabled") => {
            config.gpu.enabled = value.parse().map_err(|_| {
                CliError::Config(format!(
                    "Invalid boolean value: '{}' (must be true or false)",
                    value
                ))
            })?;
        }
        ("gpu", "device") => {
            if value == "auto" || value.is_empty() {
                config.gpu.device = None;
            } else {
                config.gpu.device = Some(value.parse().map_err(|_| {
                    CliError::Config(format!(
                        "Invalid GPU device index: '{}' (must be a number or 'auto')",
                        value
                    ))
                })?);
            }
        }
        ("gpu", "force-cpu") | ("gpu", "force_cpu") => {
            config.gpu.force_cpu = value.parse().map_err(|_| {
                CliError::Config(format!(
                    "Invalid boolean value: '{}' (must be true or false)",
                    value
                ))
            })?;
        }
        _ => {
            return Err(CliError::Config(format!(
                "Invalid config key: '{}.{}' (unknown key)",
                section, field
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the `(key, value)` pairs for [`merge_env_vars_from`] without
    /// touching the process environment.
    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn test_config_validate_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());

        let custom_config = Config {
            compression: CompressionConfig {
                default_plugin: "deflate".to_string(),
                level: "fast".to_string(),
                timeout_seconds: 30,
            },
            output: OutputConfig {
                progress_bars: false,
                color: "always".to_string(),
                quiet: true,
            },
            logging: LoggingConfig {
                format: "json".to_string(),
                level: "debug".to_string(),
                file: "/tmp/crush.log".to_string(),
            },
            gpu: GpuConfig::default(),
        };
        assert!(custom_config.validate().is_ok());
    }

    #[test]
    fn test_config_validate_invalid_compression_level() {
        let config = Config {
            compression: CompressionConfig {
                default_plugin: "auto".to_string(),
                level: "invalid".to_string(),
                timeout_seconds: 0,
            },
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid compression level"));
        assert!(err_msg.contains("invalid"));
    }

    #[test]
    fn test_config_validate_invalid_color() {
        let config = Config {
            output: OutputConfig {
                progress_bars: true,
                color: "invalid".to_string(),
                quiet: false,
            },
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid color setting"));
        assert!(err_msg.contains("invalid"));
    }

    #[test]
    fn test_config_validate_invalid_log_format() {
        let config = Config {
            logging: LoggingConfig {
                format: "invalid".to_string(),
                level: "info".to_string(),
                file: String::new(),
            },
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid log format"));
        assert!(err_msg.contains("invalid"));
    }

    #[test]
    fn test_config_validate_invalid_log_level() {
        let config = Config {
            logging: LoggingConfig {
                format: "human".to_string(),
                level: "invalid".to_string(),
                file: String::new(),
            },
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid log level"));
        assert!(err_msg.contains("invalid"));
    }

    #[test]
    fn test_config_defaults() {
        let compression = CompressionConfig::default();
        assert_eq!(compression.default_plugin, "auto");
        assert_eq!(compression.level, "balanced");
        assert_eq!(compression.timeout_seconds, 0);

        let output = OutputConfig::default();
        assert!(output.progress_bars);
        assert_eq!(output.color, "auto");
        assert!(!output.quiet);

        let logging = LoggingConfig::default();
        assert_eq!(logging.format, "human");
        assert_eq!(logging.level, "info");
        assert_eq!(logging.file, "");
    }

    #[test]
    fn test_compression_level_values() {
        for level in &["fast", "balanced", "best"] {
            let config = Config {
                compression: CompressionConfig {
                    default_plugin: "auto".to_string(),
                    level: level.to_string(),
                    timeout_seconds: 0,
                },
                ..Default::default()
            };
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_color_values() {
        for color in &["auto", "always", "never"] {
            let config = Config {
                output: OutputConfig {
                    progress_bars: true,
                    color: color.to_string(),
                    quiet: false,
                },
                ..Default::default()
            };
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_log_format_values() {
        for format in &["human", "json"] {
            let config = Config {
                logging: LoggingConfig {
                    format: format.to_string(),
                    level: "info".to_string(),
                    file: String::new(),
                },
                ..Default::default()
            };
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_log_level_values() {
        for level in &["error", "warn", "info", "debug", "trace"] {
            let config = Config {
                logging: LoggingConfig {
                    format: "human".to_string(),
                    level: level.to_string(),
                    file: String::new(),
                },
                ..Default::default()
            };
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_get_config_value() {
        let config = Config::default();

        assert_eq!(
            get_config_value(&config, "compression.level").unwrap(),
            "balanced"
        );
        assert_eq!(
            get_config_value(&config, "compression.default-plugin").unwrap(),
            "auto"
        );
        assert_eq!(get_config_value(&config, "output.color").unwrap(), "auto");
        assert_eq!(
            get_config_value(&config, "logging.format").unwrap(),
            "human"
        );
        assert_eq!(get_config_value(&config, "logging.level").unwrap(), "info");
    }

    #[test]
    fn test_get_config_value_invalid_key() {
        let config = Config::default();

        assert!(get_config_value(&config, "invalid").is_err());
        assert!(get_config_value(&config, "invalid.key.too.long").is_err());
        assert!(get_config_value(&config, "compression.invalid_field").is_err());
    }

    #[test]
    fn test_set_config_value() {
        let mut config = Config::default();

        assert!(set_config_value(&mut config, "compression.level", "fast").is_ok());
        assert_eq!(config.compression.level, "fast");

        assert!(set_config_value(&mut config, "output.color", "always").is_ok());
        assert_eq!(config.output.color, "always");

        assert!(set_config_value(&mut config, "logging.level", "debug").is_ok());
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    fn test_set_config_value_invalid() {
        let mut config = Config::default();

        assert!(set_config_value(&mut config, "compression.level", "invalid").is_err());
        assert!(set_config_value(&mut config, "output.color", "invalid").is_err());
        assert!(set_config_value(&mut config, "logging.format", "invalid").is_err());
        assert!(set_config_value(&mut config, "logging.level", "invalid").is_err());
    }

    #[test]
    fn test_set_config_value_invalid_key() {
        let mut config = Config::default();

        assert!(set_config_value(&mut config, "invalid", "value").is_err());
        assert!(set_config_value(&mut config, "invalid.key.long", "value").is_err());
        assert!(set_config_value(&mut config, "compression.unknown", "value").is_err());
    }

    #[test]
    fn test_default_helpers() {
        assert_eq!(default_plugin(), "auto");
        assert_eq!(default_level(), "balanced");
        assert!(default_true());
        assert_eq!(default_auto(), "auto");
        assert_eq!(default_human(), "human");
        assert_eq!(default_info(), "info");
    }

    // -----------------------------------------------------------------------
    // load_config / save_config (filesystem integration via tempfile)
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_config_returns_defaults_when_no_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        // Point config at a non-existent file
        let _guard = set_test_config_path(path);
        let config = load_config().expect("load_config");
        assert_eq!(config.compression.level, "balanced");
        assert_eq!(config.output.color, "auto");
    }

    #[test]
    fn test_save_and_load_config_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        let _guard = set_test_config_path(path);

        let mut config = Config::default();
        config.compression.level = "fast".to_string();
        config.gpu.enabled = true;
        config.gpu.device = Some(2);
        config.logging.level = "debug".to_string();

        save_config(&config).expect("save_config");
        let loaded = load_config().expect("load_config");

        assert_eq!(loaded.compression.level, "fast");
        assert!(loaded.gpu.enabled);
        assert_eq!(loaded.gpu.device, Some(2));
        assert_eq!(loaded.logging.level, "debug");
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml {{{{").expect("write");
        let _guard = set_test_config_path(path);
        let result = load_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_path_uses_env_override() {
        let dir = tempfile::tempdir().expect("tempdir");
        let expected = dir.path().join("sub").join("config.toml");
        let _guard = set_test_config_path(expected.clone());
        let got = config_file_path().expect("config_file_path");
        assert_eq!(got, expected);
        // Parent directory should have been created
        assert!(expected.parent().expect("parent").exists());
    }

    // -----------------------------------------------------------------------
    // get/set for GPU config keys
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_set_gpu_config_values() {
        let mut config = Config::default();

        // gpu.enabled
        assert_eq!(
            get_config_value(&config, "gpu.enabled").expect("get"),
            "false"
        );
        set_config_value(&mut config, "gpu.enabled", "true").expect("set");
        assert!(config.gpu.enabled);

        // gpu.device
        assert_eq!(
            get_config_value(&config, "gpu.device").expect("get"),
            "auto"
        );
        set_config_value(&mut config, "gpu.device", "1").expect("set");
        assert_eq!(config.gpu.device, Some(1));
        set_config_value(&mut config, "gpu.device", "auto").expect("set auto");
        assert_eq!(config.gpu.device, None);

        // gpu.force-cpu
        assert_eq!(
            get_config_value(&config, "gpu.force-cpu").expect("get"),
            "false"
        );
        set_config_value(&mut config, "gpu.force-cpu", "true").expect("set");
        assert!(config.gpu.force_cpu);
    }

    #[test]
    fn test_set_gpu_device_invalid() {
        let mut config = Config::default();
        assert!(set_config_value(&mut config, "gpu.device", "notanumber").is_err());
    }

    #[test]
    fn test_set_gpu_enabled_invalid() {
        let mut config = Config::default();
        assert!(set_config_value(&mut config, "gpu.enabled", "notabool").is_err());
    }

    #[test]
    fn test_set_gpu_force_cpu_invalid() {
        let mut config = Config::default();
        assert!(set_config_value(&mut config, "gpu.force-cpu", "maybe").is_err());
    }

    // -----------------------------------------------------------------------
    // get/set for remaining keys
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_set_timeout_seconds() {
        let mut config = Config::default();
        assert_eq!(
            get_config_value(&config, "compression.timeout-seconds").expect("get"),
            "0"
        );
        set_config_value(&mut config, "compression.timeout-seconds", "60").expect("set");
        assert_eq!(config.compression.timeout_seconds, 60);
    }

    #[test]
    fn test_set_timeout_seconds_invalid() {
        let mut config = Config::default();
        assert!(set_config_value(&mut config, "compression.timeout-seconds", "abc").is_err());
    }

    #[test]
    fn test_get_set_progress_bars() {
        let mut config = Config::default();
        assert_eq!(
            get_config_value(&config, "output.progress-bars").expect("get"),
            "true"
        );
        set_config_value(&mut config, "output.progress-bars", "false").expect("set");
        assert!(!config.output.progress_bars);
    }

    #[test]
    fn test_set_progress_bars_invalid() {
        let mut config = Config::default();
        assert!(set_config_value(&mut config, "output.progress-bars", "yes").is_err());
    }

    #[test]
    fn test_get_set_quiet() {
        let mut config = Config::default();
        assert_eq!(
            get_config_value(&config, "output.quiet").expect("get"),
            "false"
        );
        set_config_value(&mut config, "output.quiet", "true").expect("set");
        assert!(config.output.quiet);
    }

    #[test]
    fn test_set_quiet_invalid() {
        let mut config = Config::default();
        assert!(set_config_value(&mut config, "output.quiet", "nah").is_err());
    }

    #[test]
    fn test_get_set_logging_file() {
        let mut config = Config::default();
        assert_eq!(get_config_value(&config, "logging.file").expect("get"), "");
        set_config_value(&mut config, "logging.file", "/tmp/crush.log").expect("set");
        assert_eq!(config.logging.file, "/tmp/crush.log");
    }

    #[test]
    fn test_get_set_default_plugin() {
        let mut config = Config::default();
        assert_eq!(
            get_config_value(&config, "compression.default-plugin").expect("get"),
            "auto"
        );
        set_config_value(&mut config, "compression.default-plugin", "deflate").expect("set");
        assert_eq!(config.compression.default_plugin, "deflate");
        // Also works with underscore variant
        assert_eq!(
            get_config_value(&config, "compression.default_plugin").expect("get"),
            "deflate"
        );
    }

    // -----------------------------------------------------------------------
    // merge_cli_args
    // -----------------------------------------------------------------------

    #[test]
    fn test_merge_cli_args_verbose() {
        use crate::cli::{Cli, Commands, CompressArgs, CompressionLevel, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Compress(CompressArgs {
                input: vec![],
                output: None,
                stdout: false,
                plugin: None,
                level: CompressionLevel::Balanced,
                force: false,
                timeout: None,
                gpu_device: None,
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 1,
            quiet: false,
            log_format: LogFormat::Human,
            log_file: None,
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert_eq!(merged.logging.level, "debug");
    }

    #[test]
    fn test_merge_cli_args_very_verbose() {
        use crate::cli::{Cli, Commands, CompressArgs, CompressionLevel, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Compress(CompressArgs {
                input: vec![],
                output: None,
                stdout: false,
                plugin: None,
                level: CompressionLevel::Balanced,
                force: false,
                timeout: None,
                gpu_device: None,
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 2,
            quiet: false,
            log_format: LogFormat::Human,
            log_file: None,
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert_eq!(merged.logging.level, "trace");
    }

    #[test]
    fn test_merge_cli_args_quiet() {
        use crate::cli::{Cli, Commands, CompressArgs, CompressionLevel, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Compress(CompressArgs {
                input: vec![],
                output: None,
                stdout: false,
                plugin: None,
                level: CompressionLevel::Balanced,
                force: false,
                timeout: None,
                gpu_device: None,
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 0,
            quiet: true,
            log_format: LogFormat::Human,
            log_file: None,
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert!(merged.output.quiet);
    }

    #[test]
    fn test_merge_cli_args_log_format_json() {
        use crate::cli::{Cli, Commands, CompressArgs, CompressionLevel, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Compress(CompressArgs {
                input: vec![],
                output: None,
                stdout: false,
                plugin: None,
                level: CompressionLevel::Balanced,
                force: false,
                timeout: None,
                gpu_device: None,
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 0,
            quiet: false,
            log_format: LogFormat::Json,
            log_file: None,
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert_eq!(merged.logging.format, "json");
    }

    #[test]
    fn test_merge_cli_args_log_file() {
        use crate::cli::{Cli, Commands, CompressArgs, CompressionLevel, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Compress(CompressArgs {
                input: vec![],
                output: None,
                stdout: false,
                plugin: None,
                level: CompressionLevel::Balanced,
                force: false,
                timeout: None,
                gpu_device: None,
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 0,
            quiet: false,
            log_format: LogFormat::Human,
            log_file: Some(std::path::PathBuf::from("/tmp/test.log")),
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert_eq!(merged.logging.file, "/tmp/test.log");
    }

    #[test]
    fn test_merge_cli_args_compress_gpu_device() {
        use crate::cli::{Cli, Commands, CompressArgs, CompressionLevel, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Compress(CompressArgs {
                input: vec![],
                output: None,
                stdout: false,
                plugin: None,
                level: CompressionLevel::Balanced,
                force: false,
                timeout: None,
                gpu_device: Some(3),
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 0,
            quiet: false,
            log_format: LogFormat::Human,
            log_file: None,
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert_eq!(merged.gpu.device, Some(3));
    }

    #[test]
    fn test_merge_cli_args_decompress_force_cpu_and_device() {
        use crate::cli::{Cli, Commands, DecompressArgs, GpuBackend, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Decompress(DecompressArgs {
                input: vec![],
                output: None,
                force: false,
                stdout: false,
                block: None,
                force_cpu: true,
                gpu_device: Some(5),
                gpu_backend: GpuBackend::Auto,
            }),
            verbose: 0,
            quiet: false,
            log_format: LogFormat::Human,
            log_file: None,
        };

        let merged = merge_cli_args(config, &cli).expect("merge");
        assert!(merged.gpu.force_cpu);
        assert_eq!(merged.gpu.device, Some(5));
    }

    #[test]
    fn test_merge_cli_args_non_compress_decompress_command() {
        use crate::cli::{Cli, Commands, ConfigAction, ConfigArgs, LogFormat};

        let config = Config::default();
        let cli = Cli {
            command: Commands::Config(ConfigArgs {
                action: ConfigAction::List,
            }),
            verbose: 0,
            quiet: false,
            log_format: LogFormat::Human,
            log_file: None,
        };

        // Should not error, GPU fields left at defaults
        let merged = merge_cli_args(config, &cli).expect("merge");
        assert!(!merged.gpu.force_cpu);
        assert_eq!(merged.gpu.device, None);
    }

    // -----------------------------------------------------------------------
    // merge_env_vars
    // -----------------------------------------------------------------------

    #[test]
    fn test_merge_env_vars_compression_level() {
        let config = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_COMPRESSION_LEVEL", "fast")]),
        )
        .expect("merge");
        assert_eq!(config.compression.level, "fast");
    }

    #[test]
    fn test_merge_env_vars_output_color() {
        let config =
            merge_env_vars_from(Config::default(), vars(&[("CRUSH_OUTPUT_COLOR", "never")]))
                .expect("merge");
        assert_eq!(config.output.color, "never");
    }

    #[test]
    fn test_merge_env_vars_gpu_enabled() {
        let config = merge_env_vars_from(Config::default(), vars(&[("CRUSH_GPU_ENABLED", "true")]))
            .expect("merge");
        assert!(config.gpu.enabled);
    }

    #[test]
    fn test_merge_env_vars_gpu_device() {
        let config = merge_env_vars_from(Config::default(), vars(&[("CRUSH_GPU_DEVICE", "7")]))
            .expect("merge");
        assert_eq!(config.gpu.device, Some(7));
    }

    #[test]
    fn test_merge_env_vars_invalid_timeout() {
        let result = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_COMPRESSION_TIMEOUT_SECONDS", "not_a_number")]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_env_vars_invalid_bool() {
        let result = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_OUTPUT_QUIET", "yes_please")]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_env_vars_logging_file() {
        let config = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_LOGGING_FILE", "/var/log/crush.log")]),
        )
        .expect("merge");
        assert_eq!(config.logging.file, "/var/log/crush.log");
    }

    #[test]
    fn test_merge_env_vars_logging_format() {
        let config =
            merge_env_vars_from(Config::default(), vars(&[("CRUSH_LOGGING_FORMAT", "json")]))
                .expect("merge");
        assert_eq!(config.logging.format, "json");
    }

    #[test]
    fn test_merge_env_vars_logging_level() {
        let config =
            merge_env_vars_from(Config::default(), vars(&[("CRUSH_LOGGING_LEVEL", "trace")]))
                .expect("merge");
        assert_eq!(config.logging.level, "trace");
    }

    #[test]
    fn test_merge_env_vars_progress_bars() {
        let config = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_OUTPUT_PROGRESS_BARS", "false")]),
        )
        .expect("merge");
        assert!(!config.output.progress_bars);
    }

    #[test]
    fn test_merge_env_vars_gpu_force_cpu() {
        let config =
            merge_env_vars_from(Config::default(), vars(&[("CRUSH_GPU_FORCE_CPU", "true")]))
                .expect("merge");
        assert!(config.gpu.force_cpu);
    }

    #[test]
    fn test_merge_env_vars_default_plugin() {
        let config = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_COMPRESSION_DEFAULT_PLUGIN", "deflate")]),
        )
        .expect("merge");
        assert_eq!(config.compression.default_plugin, "deflate");
    }

    #[test]
    fn test_merge_env_vars_unknown_key_ignored() {
        let result = merge_env_vars_from(
            Config::default(),
            vars(&[("CRUSH_UNKNOWN_KEY_XYZ", "whatever")]),
        );
        assert!(result.is_ok()); // unknown keys are silently ignored
    }

    // -----------------------------------------------------------------------
    // GpuConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn test_gpu_config_defaults() {
        let gpu = GpuConfig::default();
        assert!(!gpu.enabled);
        assert_eq!(gpu.device, None);
        assert!(!gpu.force_cpu);
    }

    // -----------------------------------------------------------------------
    // Config serialization roundtrip via TOML
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_toml_roundtrip() {
        let config = Config {
            compression: CompressionConfig {
                default_plugin: "deflate".to_string(),
                level: "best".to_string(),
                timeout_seconds: 120,
            },
            output: OutputConfig {
                progress_bars: false,
                color: "never".to_string(),
                quiet: true,
            },
            logging: LoggingConfig {
                format: "json".to_string(),
                level: "trace".to_string(),
                file: "/tmp/log".to_string(),
            },
            gpu: GpuConfig {
                enabled: true,
                device: Some(4),
                force_cpu: true,
            },
        };

        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        let deserialized: Config = toml::from_str(&toml_str).expect("deserialize");

        assert_eq!(deserialized.compression.default_plugin, "deflate");
        assert_eq!(deserialized.compression.level, "best");
        assert_eq!(deserialized.compression.timeout_seconds, 120);
        assert!(!deserialized.output.progress_bars);
        assert_eq!(deserialized.output.color, "never");
        assert!(deserialized.output.quiet);
        assert_eq!(deserialized.logging.format, "json");
        assert_eq!(deserialized.logging.level, "trace");
        assert_eq!(deserialized.logging.file, "/tmp/log");
        assert!(deserialized.gpu.enabled);
        assert_eq!(deserialized.gpu.device, Some(4));
        assert!(deserialized.gpu.force_cpu);
    }
}
