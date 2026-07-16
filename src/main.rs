slint::include_modules!();

mod app;
mod config;
mod terminal;

use anyhow::{Context, Result};
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    match command_from_args(std::env::args().skip(1))? {
        Command::Run => {
            initialize_logging();
            info!(version = env!("CARGO_PKG_VERSION"), "starting Termi");
            let config = config::AppConfig::load_or_create()
                .context("failed to load Termi configuration")?;
            app::run(config)
        }
        Command::Version => {
            println!("termi {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::PrintConfigPath => {
            println!("{}", config::config_path()?.display());
            Ok(())
        }
        Command::CheckConfig => {
            let path = config::config_path()?;
            config::AppConfig::load_or_create()
                .with_context(|| format!("configuration check failed for {}", path.display()))?;
            println!("configuration is valid: {}", path.display());
            Ok(())
        }
        Command::Help => {
            print!(
                "Termi {version}\n\nUSAGE:\n    termi [OPTION]\n\nOPTIONS:\n    --version             Print the version\n    --print-config-path   Print the configuration file path\n    --check-config        Validate the configuration without opening a window\n    -h, --help            Print this help\n",
                version = env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Command {
    Run,
    Version,
    PrintConfigPath,
    CheckConfig,
    Help,
}

fn command_from_args(arguments: impl IntoIterator<Item = String>) -> Result<Command> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    anyhow::ensure!(
        arguments.len() <= 1,
        "Termi accepts at most one option; use --help for usage"
    );
    match arguments.first().map(String::as_str) {
        None => Ok(Command::Run),
        Some("--version" | "-V") => Ok(Command::Version),
        Some("--print-config-path") => Ok(Command::PrintConfigPath),
        Some("--check-config") => Ok(Command::CheckConfig),
        Some("--help" | "-h") => Ok(Command::Help),
        Some(option) => anyhow::bail!("unknown option {option:?}; use --help for usage"),
    }
}

fn initialize_logging() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("termi=info,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::{Command, command_from_args};

    #[test]
    fn parses_non_ui_commands() {
        assert_eq!(
            command_from_args(["--check-config".to_owned()]).unwrap(),
            Command::CheckConfig
        );
        assert!(command_from_args(["--unknown".to_owned()]).is_err());
    }
}
