use clap::{Parser, Subcommand, Args, ValueEnum};
use std::path::PathBuf;

/// High-performance parallel compression
#[derive(Parser, Debug)]
#[command(name = "crush")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose output (repeat for more verbosity: -v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count)]
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
    pub fn to_weights(self) -> crush_core::ScoringWeights {
        match self {
            Self::Fast => crush_core::ScoringWeights {
                throughput: 0.9,
                compression_ratio: 0.1,
            },
            Self::Balanced => crush_core::ScoringWeights {
                throughput: 0.5,
                compression_ratio: 0.5,
            },
            Self::Best => crush_core::ScoringWeights {
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
