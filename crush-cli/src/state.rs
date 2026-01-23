use crate::cli::Cli;
use crate::config::{self, Config};
use crate::error::Result;
use is_terminal::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
}

impl CliState {
    /// Initialize CLI state from arguments and environment
    pub fn new(args: &Cli, interrupted: Arc<AtomicBool>) -> Result<Self> {
        // Start with default config
        // TODO: Load config file when config file loading is implemented
        let config = Config::default();

        // Merge with environment variables
        let config = config::merge_env_vars(config)?;

        // Override with CLI arguments
        let config = config::merge_cli_args(config, args)?;

        // Validate final config
        config.validate()?;

        // Detect terminal
        let stdout_is_tty = std::io::stdout().is_terminal();
        let stderr_is_tty = std::io::stderr().is_terminal();

        Ok(Self {
            config,
            stdout_is_tty,
            stderr_is_tty,
            interrupted,
        })
    }

    /// Check if operation was interrupted (Ctrl+C)
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Should progress bars be shown?
    pub fn show_progress(&self) -> bool {
        self.config.output.progress_bars && self.stderr_is_tty && !self.config.output.quiet
    }

    /// Should colored output be used?
    pub fn use_colors(&self) -> bool {
        if self.config.output.quiet {
            return false;
        }
        match self.config.output.color.as_str() {
            "always" => true,
            "never" => false,
            _ => self.stdout_is_tty, // "auto"
        }
    }
}
