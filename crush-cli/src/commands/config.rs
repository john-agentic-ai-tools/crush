use crate::cli::{ConfigAction, ConfigArgs};
use crate::config::{self, Config};
use crate::error::Result;
use std::io::{self, Write};
use tracing::{debug, info};

pub fn run(args: &ConfigArgs) -> Result<()> {
    match &args.action {
        ConfigAction::Set { key, value } => {
            info!(key = %key, value = %value, "Setting config value");

            // Load current config
            let mut config = config::load_config()?;

            // Set the value
            config::set_config_value(&mut config, key, value)?;

            // Validate the updated config
            config.validate()?;

            // Save the config
            config::save_config(&config)?;

            debug!(key = %key, value = %value, "Config value saved successfully");
            println!("Set {key} = {value}");
            Ok(())
        }

        ConfigAction::Get { key } => {
            // Load current config
            let config = config::load_config()?;

            // Get the value
            let value = config::get_config_value(&config, key)?;

            println!("{value}");
            Ok(())
        }

        ConfigAction::List => {
            // Load current config
            let config = config::load_config()?;

            // Serialize to TOML and display
            let toml_string = toml::to_string_pretty(&config).map_err(|e| {
                crate::error::CliError::Config(format!("Could not format config: {}", e))
            })?;

            println!("{toml_string}");
            Ok(())
        }

        ConfigAction::Reset { yes } => {
            // Confirm reset unless --yes flag is provided
            if !yes {
                print!("This will reset all configuration to defaults. Continue? (y/N): ");
                io::stdout().flush()?;

                let mut response = String::new();
                io::stdin().read_line(&mut response)?;

                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("Reset cancelled");
                    return Ok(());
                }
            }

            // Create default config
            let config = Config::default();

            // Save it
            config::save_config(&config)?;

            println!("Configuration reset to defaults");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::set_test_config_path;

    /// Point config I/O at a fresh temp file for this thread and return the
    /// tempdir (kept alive by the caller) plus its RAII override guard.
    fn scratch_config() -> (tempfile::TempDir, crate::config::TestConfigPathGuard) {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = set_test_config_path(dir.path().join("config.toml"));
        (dir, guard)
    }

    fn args(action: ConfigAction) -> ConfigArgs {
        ConfigArgs { action }
    }

    #[test]
    fn set_then_get_persists_the_value() {
        let (_dir, _guard) = scratch_config();

        run(&args(ConfigAction::Set {
            key: "compression.level".to_string(),
            value: "fast".to_string(),
        }))
        .expect("set");

        // Round-trip through the file, not just in-memory state.
        let stored = config::load_config().expect("load");
        assert_eq!(stored.compression.level, "fast");

        run(&args(ConfigAction::Get {
            key: "compression.level".to_string(),
        }))
        .expect("get");
    }

    #[test]
    fn set_rejects_a_value_that_fails_validation() {
        let (_dir, _guard) = scratch_config();

        let result = run(&args(ConfigAction::Set {
            key: "compression.level".to_string(),
            value: "ludicrous".to_string(),
        }));

        assert!(result.is_err(), "invalid level should not be accepted");
        // And it must not have been written to disk.
        let stored = config::load_config().expect("load");
        assert_eq!(stored.compression.level, "balanced");
    }

    #[test]
    fn set_rejects_an_unknown_key() {
        let (_dir, _guard) = scratch_config();

        let result = run(&args(ConfigAction::Set {
            key: "no.such.key".to_string(),
            value: "1".to_string(),
        }));

        assert!(result.is_err(), "unknown key should be rejected");
    }

    #[test]
    fn get_rejects_an_unknown_key() {
        let (_dir, _guard) = scratch_config();

        let result = run(&args(ConfigAction::Get {
            key: "no.such.key".to_string(),
        }));

        assert!(result.is_err(), "unknown key should be rejected");
    }

    #[test]
    fn list_renders_the_current_config() {
        let (_dir, _guard) = scratch_config();

        run(&args(ConfigAction::Set {
            key: "logging.level".to_string(),
            value: "debug".to_string(),
        }))
        .expect("set");

        run(&args(ConfigAction::List)).expect("list");

        // `list` is display-only; verify it left the config untouched.
        let stored = config::load_config().expect("load");
        assert_eq!(stored.logging.level, "debug");
    }

    #[test]
    fn reset_with_yes_restores_defaults_without_prompting() {
        let (_dir, _guard) = scratch_config();

        run(&args(ConfigAction::Set {
            key: "compression.level".to_string(),
            value: "best".to_string(),
        }))
        .expect("set");
        assert_eq!(
            config::load_config().expect("load").compression.level,
            "best"
        );

        // `yes: true` must not touch stdin — an in-process read would hang here.
        run(&args(ConfigAction::Reset { yes: true })).expect("reset");

        let stored = config::load_config().expect("load");
        assert_eq!(stored.compression.level, "balanced");
        assert_eq!(stored.output.color, "auto");
    }
}
