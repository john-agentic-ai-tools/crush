use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// High-performance parallel compression
#[derive(Parser, Debug)]
#[command(name = "crush")]
#[command(version)]
#[command(about = "High-performance parallel compression")]
#[command(long_about = "Crush - High-performance parallel compression library

Crush provides fast, reliable file compression with automatic plugin selection,
metadata preservation, and comprehensive error handling. It supports multiple
compression algorithms through a plugin system and can be configured globally
or via environment variables.")]
#[command(after_help = "EXAMPLES:
    # Compress a file
    crush compress file.txt

    # Decompress a file
    crush decompress file.txt.crush

    # List available plugins
    crush plugins list

    # Configure default compression level
    crush config set compression.level fast

For more information about a specific command, run:
    crush <COMMAND> --help")]
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
#[command(after_help = "EXAMPLES:
    # Compress a single file
    crush compress document.txt

    # Compress multiple files
    crush compress file1.txt file2.txt file3.txt

    # Compress with specific plugin
    crush compress --plugin deflate data.bin

    # Compress with fast preset
    crush compress --level fast largefile.dat

    # Compress to specific output
    crush compress input.txt --output /backup/input.txt.crush

    # Force overwrite existing compressed file
    crush compress --force document.txt

    # Pipeline: read from stdin, write to file
    cat file.txt | crush compress --output file.txt.crush

    # Pipeline: read from stdin, write to stdout
    cat file.txt | crush compress --stdout > file.txt.crush")]
pub struct CompressArgs {
    /// Input files to compress (reads from stdin if not provided)
    #[arg(value_name = "FILE")]
    pub input: Vec<PathBuf>,

    /// Output file or directory (default: <input>.crush or stdout)
    #[arg(short, long, value_name = "PATH", conflicts_with = "stdout")]
    pub output: Option<PathBuf>,

    /// Write output to stdout (for piping)
    #[arg(long, conflicts_with = "output")]
    pub stdout: bool,

    /// Compression plugin to use (default: auto-select)
    #[arg(short, long, value_name = "PLUGIN")]
    pub plugin: Option<String>,

    /// Compression level preset
    #[arg(short, long, value_name = "LEVEL", default_value = "balanced")]
    pub level: CompressionLevel,

    /// Force overwrite of existing files
    #[arg(short, long)]
    pub force: bool,

    /// Compression timeout in seconds (0 = no timeout)
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,
}

/// Decompress command arguments
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:
    # Decompress a file
    crush decompress file.txt.crush

    # Decompress multiple files
    crush decompress file1.crush file2.crush

    # Decompress to specific output
    crush decompress archive.crush --output /tmp/restored.txt

    # Decompress to stdout for piping
    crush decompress data.crush --stdout | grep pattern

    # Decompress from stdin to stdout
    cat data.crush | crush decompress --stdout

    # Force overwrite existing file
    crush decompress --force document.txt.crush")]
pub struct DecompressArgs {
    /// Compressed files to decompress (reads from stdin if not provided with --stdout)
    #[arg(value_name = "FILE")]
    pub input: Vec<PathBuf>,

    /// Output file or directory (default: strip .crush extension)
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Force overwrite of existing files
    #[arg(short, long)]
    pub force: bool,

    /// Write output to stdout (for piping)
    #[arg(long, conflicts_with = "output")]
    pub stdout: bool,
}

/// Inspect command arguments
#[derive(Args, Debug)]
#[command(after_help = "EXAMPLES:
    # Inspect a compressed file
    crush inspect file.txt.crush

    # Inspect multiple files with summary
    crush inspect --summary *.crush

    # Inspect with JSON output
    crush inspect --format json archive.crush

    # Inspect with CSV output (for importing to spreadsheet)
    crush inspect --format csv *.crush > report.csv")]
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
#[command(after_help = "EXAMPLES:
    # Set compression level to fast
    crush config set compression.level fast

    # Get current compression level
    crush config get compression.level

    # List all configuration
    crush config list

    # Reset configuration to defaults
    crush config reset --yes

AVAILABLE KEYS:
    compression.default-plugin    Default plugin name (auto)
    compression.level             fast | balanced | best
    compression.timeout-seconds   Timeout in seconds (0 = no timeout)
    output.progress-bars          true | false
    output.color                  auto | always | never
    output.quiet                  true | false
    logging.format                human | json
    logging.level                 error | warn | info | debug | trace
    logging.file                  Log file path (empty = stderr)")]
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
#[command(after_help = "EXAMPLES:
    # List all available plugins
    crush plugins list

    # List plugins in JSON format
    crush plugins list --format json

    # Show detailed information about a plugin
    crush plugins info deflate

    # Test a plugin's functionality
    crush plugins test deflate")]
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
