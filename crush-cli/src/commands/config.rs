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
