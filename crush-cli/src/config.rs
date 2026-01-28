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
pub fn merge_env_vars(mut config: Config) -> Result<Config> {
    use std::env;

    for (key, value) in env::vars() {
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

    Ok(config)
}

/// Get the config file path for the current OS
///
/// For testing, set `CRUSH_TEST_CONFIG_FILE` environment variable to use a custom path.
/// This allows tests to run in isolation without interfering with each other.
pub fn config_file_path() -> Result<PathBuf> {
    // Allow tests to override config path via environment variable
    if let Ok(test_path) = std::env::var("CRUSH_TEST_CONFIG_FILE") {
        let path = PathBuf::from(test_path);
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| CliError::Config(format!("Could not create test config directory: {}", e)))?;
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
        _ => {
            return Err(CliError::Config(format!(
                "Invalid config key: '{}.{}' (unknown key)",
                section, field
            )))
        }
    }

    Ok(())
}
