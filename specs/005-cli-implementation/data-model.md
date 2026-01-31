# Data Model: CLI Implementation

**Feature**: CLI Implementation | **Branch**: `005-cli-implementation` | **Date**: 2026-01-22

## Overview

This document defines the data structures, state management, and data flow for the CLI implementation. The CLI is primarily a thin presentation layer over crush-core, so most data models are configuration and runtime state rather than persistent domain models.

## Core Data Structures

### 1. CLI Arguments (clap-derived)

Defined using clap's derive API for type-safe argument parsing.

```rust
/// Root CLI command structure
#[derive(Parser, Debug)]
#[command(name = "crush")]
#[command(version, about = "High-performance parallel compression", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose output (repeat for more verbosity: -v, -vv)
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Log format: human or json
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub log_format: LogFormat,

    /// Log output file (default: stderr)
    #[arg(long, value_name = "FILE")]
    pub log_file: Option<PathBuf>,
}

/// All available subcommands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Compress files
    Compress(CompressArgs),
    /// Decompress files
    Decompress(DecompressArgs),
    /// Inspect compressed file metadata
    Inspect(InspectArgs),
    /// Manage configuration
    Config(ConfigArgs),
    /// Manage compression plugins
    Plugins(PluginsArgs),
}

/// Compress command arguments
#[derive(Args, Debug)]
pub struct CompressArgs {
    /// Input files to compress
    #[arg(required = true, value_name = "FILE")]
    pub input: Vec<PathBuf>,

    /// Output file or directory (default: <input>.crush)
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Compression plugin to use (default: auto-select)
    #[arg(short, long, value_name = "PLUGIN")]
    pub plugin: Option<String>,

    /// Compression level preset
    #[arg(short, long, value_name = "LEVEL", default_value = "balanced")]
    pub level: CompressionLevel,

    /// Force overwrite of existing files
    #[arg(short, long)]
    pub force: bool,

    /// Compression timeout in seconds
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,

    /// Keep input files after compression
    #[arg(short, long)]
    pub keep: bool,
}

/// Decompress command arguments
#[derive(Args, Debug)]
pub struct DecompressArgs {
    /// Compressed files to decompress
    #[arg(required = true, value_name = "FILE")]
    pub input: Vec<PathBuf>,

    /// Output file or directory (default: strip .crush extension)
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Force overwrite of existing files
    #[arg(short, long)]
    pub force: bool,

    /// Keep compressed files after decompression
    #[arg(short, long)]
    pub keep: bool,

    /// Write output to stdout (for piping)
    #[arg(long, conflicts_with = "output")]
    pub stdout: bool,
}

/// Inspect command arguments
#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Compressed files to inspect
    #[arg(required = true, value_name = "FILE")]
    pub input: Vec<PathBuf>,

    /// Output format: human, json, csv
    #[arg(short, long, value_name = "FORMAT", default_value = "human")]
    pub format: OutputFormat,

    /// Show summary statistics for multiple files
    #[arg(short, long)]
    pub summary: bool,
}

/// Config subcommand arguments
#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Set configuration value
    Set {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(value_name = "VALUE")]
        value: String,
    },
    /// Get configuration value
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// List all configuration
    List,
    /// Reset to default configuration
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

/// Plugins subcommand arguments
#[derive(Args, Debug)]
pub struct PluginsArgs {
    #[command(subcommand)]
    pub action: PluginsAction,
}

#[derive(Subcommand, Debug)]
pub enum PluginsAction {
    /// List all available plugins
    List {
        /// Output format: human, json
        #[arg(short, long, value_name = "FORMAT", default_value = "human")]
        format: OutputFormat,
    },
    /// Show detailed plugin information
    Info {
        #[arg(value_name = "PLUGIN")]
        name: String,
    },
    /// Test plugin functionality
    Test {
        #[arg(value_name = "PLUGIN")]
        name: String,
    },
}

/// Compression level presets
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CompressionLevel {
    /// Fast compression (prioritize speed)
    Fast,
    /// Balanced compression (default)
    Balanced,
    /// Best compression (prioritize ratio)
    Best,
}

impl CompressionLevel {
    /// Convert to ScoringWeights for crush-core
    pub fn to_weights(self) -> ScoringWeights {
        match self {
            Self::Fast => ScoringWeights {
                throughput: 0.9,
                compression_ratio: 0.1,
            },
            Self::Balanced => ScoringWeights {
                throughput: 0.5,
                compression_ratio: 0.5,
            },
            Self::Best => ScoringWeights {
                throughput: 0.1,
                compression_ratio: 0.9,
            },
        }
    }
}

/// Output format options
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Csv,
}

/// Log format options
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogFormat {
    Human,
    Json,
}
```

---

### 2. Configuration File Structure

Persisted in `~/.config/crush/config.toml` (Linux/macOS) or `%APPDATA%\Crush\config.toml` (Windows).

```rust
use serde::{Deserialize, Serialize};

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub compression: CompressionConfig,

    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            compression: CompressionConfig::default(),
            output: OutputConfig::default(),
            logging: LoggingConfig::default(),
        }
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
fn default_plugin() -> String { "auto".to_string() }
fn default_level() -> String { "balanced".to_string() }
fn default_true() -> bool { true }
fn default_auto() -> String { "auto".to_string() }
fn default_human() -> String { "human".to_string() }
fn default_info() -> String { "info".to_string() }
```

**Example config.toml**:
```toml
[compression]
default-plugin = "auto"
level = "balanced"
timeout-seconds = 0

[output]
progress-bars = true
color = "auto"
quiet = false

[logging]
format = "human"
level = "info"
file = ""
```

---

### 3. Runtime State

State managed during CLI execution (not persisted).

```rust
/// Global runtime state for the CLI
pub struct CliState {
    /// Configuration (merged from file, env vars, and CLI args)
    pub config: Config,

    /// Whether stdout is a terminal (affects colors/progress)
    pub stdout_is_tty: bool,

    /// Whether stderr is a terminal
    pub stderr_is_tty: bool,

    /// Interrupt signal handler (Ctrl+C detection)
    pub interrupted: Arc<AtomicBool>,

    /// Tracing subscriber guard (keeps logging active)
    pub _tracing_guard: Option<tracing::subscriber::DefaultGuard>,
}

impl CliState {
    /// Initialize CLI state from arguments and environment
    pub fn new(args: &Cli) -> Result<Self, CliError> {
        // Load config file
        let config = load_config()?;

        // Merge with environment variables
        let config = merge_env_vars(config)?;

        // Override with CLI arguments
        let config = merge_cli_args(config, args)?;

        // Set up interrupt handler
        let interrupted = Arc::new(AtomicBool::new(false));
        let r = interrupted.clone();
        ctrlc::set_handler(move || {
            r.store(true, Ordering::SeqCst);
        })?;

        // Detect terminal
        let stdout_is_tty = std::io::stdout().is_terminal();
        let stderr_is_tty = std::io::stderr().is_terminal();

        // Initialize logging
        let guard = init_logging(&config.logging, args.verbose, args.log_file.as_ref())?;

        Ok(Self {
            config,
            stdout_is_tty,
            stderr_is_tty,
            interrupted,
            _tracing_guard: Some(guard),
        })
    }

    /// Check if operation was interrupted (Ctrl+C)
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Should progress bars be shown?
    pub fn show_progress(&self) -> bool {
        self.config.output.progress_bars && self.stderr_is_tty
    }

    /// Should colored output be used?
    pub fn use_colors(&self) -> bool {
        match self.config.output.color.as_str() {
            "always" => true,
            "never" => false,
            _ => self.stdout_is_tty, // "auto"
        }
    }
}
```

---

### 4. Operation Results

Structures for reporting operation outcomes.

```rust
/// Result of a compression operation
#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_size: u64,
    pub output_size: u64,
    pub compression_ratio: f64, // percentage
    pub duration: Duration,
    pub throughput_mbps: f64,
    pub plugin_used: String,
    pub thread_count: usize,
    pub hardware_acceleration: Option<String>, // e.g., "AVX2", "NEON"
}

impl CompressionResult {
    pub fn format_summary(&self) -> String {
        format!(
            "{} -> {} ({:.1}% compression, {:.1} MB/s, {})",
            self.input_path.display(),
            self.output_path.display(),
            self.compression_ratio,
            self.throughput_mbps,
            self.plugin_used
        )
    }
}

/// Result of a decompression operation
#[derive(Debug, Clone)]
pub struct DecompressionResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_size: u64,
    pub output_size: u64,
    pub duration: Duration,
    pub throughput_mbps: f64,
    pub crc_valid: bool,
}

/// Result of an inspect operation
#[derive(Debug, Clone, Serialize)]
pub struct InspectResult {
    pub file_path: PathBuf,
    pub original_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
    pub plugin_name: String,
    pub plugin_version: String,
    pub crc_valid: bool,
    pub created_at: Option<SystemTime>,
}

impl InspectResult {
    pub fn format_human(&self) -> String {
        format!(
            "File: {}\n\
             Original size: {} bytes\n\
             Compressed size: {} bytes\n\
             Compression ratio: {:.1}%\n\
             Plugin: {} (v{})\n\
             CRC32: {}\n",
            self.file_path.display(),
            self.original_size,
            self.compressed_size,
            self.compression_ratio,
            self.plugin_name,
            self.plugin_version,
            if self.crc_valid { "VALID" } else { "INVALID" }
        )
    }
}
```

---

## Data Flow

### Compression Flow

```
User Input (CLI args)
    ↓
Parse arguments (clap) → Cli struct
    ↓
Load config file → Config struct
    ↓
Merge env vars + CLI overrides → final Config
    ↓
Initialize state → CliState
    ↓
For each input file:
    ├─ Check file exists and readable
    ├─ Determine output path
    ├─ Create progress bar (if TTY)
    ├─ Call crush_core::compress_with_options()
    ├─ Update progress (callback)
    ├─ Check interrupted flag periodically
    ├─ Collect result → CompressionResult
    └─ Display summary
    ↓
Aggregate results → Batch summary (if multiple files)
    ↓
Exit with appropriate code (0 = success, 1 = errors)
```

### Configuration Loading Priority

```
1. Built-in defaults (Config::default())
    ↓
2. Config file (~/.config/crush/config.toml)
    ↓ (merge)
3. Environment variables (CRUSH_*)
    ↓ (merge)
4. CLI arguments (highest priority)
    ↓
Final effective configuration
```

### Plugin Discovery Flow

```
CLI startup
    ↓
Call crush_core::init_plugins()
    ↓
Plugin registry populated
    ↓
User runs: crush plugins list
    ↓
Call crush_core::list_plugins()
    ↓
Format output (human/json)
    ↓
Display to stdout
```

---

## Environment Variables

All environment variables use the prefix `CRUSH_` and map to configuration keys:

| Environment Variable | Config Section | Config Key | Example |
|---------------------|----------------|------------|---------|
| `CRUSH_COMPRESSION_DEFAULT_PLUGIN` | compression | default-plugin | `deflate` |
| `CRUSH_COMPRESSION_LEVEL` | compression | level | `fast` |
| `CRUSH_COMPRESSION_TIMEOUT_SECONDS` | compression | timeout-seconds | `60` |
| `CRUSH_OUTPUT_PROGRESS_BARS` | output | progress-bars | `true` |
| `CRUSH_OUTPUT_COLOR` | output | color | `always` |
| `CRUSH_OUTPUT_QUIET` | output | quiet | `false` |
| `CRUSH_LOGGING_FORMAT` | logging | format | `json` |
| `CRUSH_LOGGING_LEVEL` | logging | level | `debug` |
| `CRUSH_LOGGING_FILE` | logging | file | `/var/log/crush.log` |

**Standard Environment Variables** (not prefixed):
- `NO_COLOR`: Disable colors (overrides `CRUSH_OUTPUT_COLOR`)
- `RUST_LOG`: Override tracing level (if `CRUSH_LOGGING_LEVEL` not set)

---

## State Persistence

### Configuration File Location

Determined using the `dirs` crate:

```rust
pub fn config_file_path() -> Result<PathBuf, CliError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| CliError::Config("Could not determine config directory".into()))?;

    let crush_dir = config_dir.join("crush");
    std::fs::create_dir_all(&crush_dir)?;

    Ok(crush_dir.join("config.toml"))
}
```

**Paths by OS**:
- Linux: `~/.config/crush/config.toml`
- macOS: `~/Library/Application Support/crush/config.toml`
- Windows: `C:\Users\<user>\AppData\Roaming\Crush\config.toml`

### No Other Persistent State

The CLI does not maintain:
- Caches
- Databases
- History files
- Temporary files (cleaned up on exit)

---

## Data Validation

### Input Validation

```rust
/// Validate file paths before processing
pub fn validate_input_file(path: &Path) -> Result<(), CliError> {
    if !path.exists() {
        return Err(CliError::InvalidInput(format!(
            "File does not exist: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(CliError::InvalidInput(format!(
            "Not a regular file: {}",
            path.display()
        )));
    }

    if path.metadata()?.len() == 0 {
        return Err(CliError::InvalidInput(format!(
            "File is empty: {}",
            path.display()
        )));
    }

    Ok(())
}

/// Validate output path is writable
pub fn validate_output_path(path: &Path, force: bool) -> Result<(), CliError> {
    if path.exists() && !force {
        return Err(CliError::InvalidInput(format!(
            "Output file already exists (use --force to overwrite): {}",
            path.display()
        )));
    }

    // Check parent directory is writable
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    Ok(())
}
```

### Configuration Validation

```rust
impl Config {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), CliError> {
        // Validate level
        if !["fast", "balanced", "best"].contains(&self.compression.level.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid compression level: {}",
                self.compression.level
            )));
        }

        // Validate color setting
        if !["auto", "always", "never"].contains(&self.output.color.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid color setting: {}",
                self.output.color
            )));
        }

        // Validate log format
        if !["human", "json"].contains(&self.logging.format.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid log format: {}",
                self.logging.format
            )));
        }

        // Validate log level
        if !["error", "warn", "info", "debug", "trace"].contains(&self.logging.level.as_str()) {
            return Err(CliError::Config(format!(
                "Invalid log level: {}",
                self.logging.level
            )));
        }

        Ok(())
    }
}
```

---

## Memory Management

### Zero-Copy Principles

- CLI passes file paths to crush-core (no buffering entire files)
- Progress callbacks use references, not clones
- Configuration loaded once at startup, then shared via references

### Bounded Memory Usage

- Progress bars: ~1KB per bar
- Configuration: <10KB
- Log buffer: Configurable, default 8KB
- No unbounded collections (file list processed sequentially)

**Total Baseline**: <1MB (excluding crush-core library overhead)

---

## Summary

The CLI data model is minimal by design:

1. **Input**: Clap-derived argument structures (type-safe parsing)
2. **Configuration**: TOML-based persistent config with env var overrides
3. **Runtime State**: Terminal detection, interrupt handling, logging
4. **Results**: Typed result structures for operations
5. **No Database**: Stateless CLI, no persistent caches or history

All compression/decompression logic and data structures are delegated to crush-core. The CLI is purely a presentation and orchestration layer.
