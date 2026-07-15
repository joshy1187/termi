slint::include_modules!();

mod app;
mod config;
mod terminal;

use anyhow::{Context, Result};
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    initialize_logging();
    info!(version = env!("CARGO_PKG_VERSION"), "starting Termi");

    let config =
        config::AppConfig::load_or_create().context("failed to load Termi configuration")?;
    app::run(config)
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
