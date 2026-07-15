use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub shell: String,
    pub font_family: String,
    pub font_size: f32,
    pub cell_width: f32,
    pub cell_height: f32,
    pub scrollback_lines: usize,
    pub initial_columns: u16,
    pub initial_rows: u16,
    pub background_dim: f32,
    pub terminal_opacity: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            shell: std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_owned()),
            font_family: "DejaVu Sans Mono".to_owned(),
            font_size: 16.0,
            cell_width: 9.64,
            cell_height: 20.0,
            scrollback_lines: 10_000,
            initial_columns: 100,
            initial_rows: 32,
            background_dim: 0.48,
            terminal_opacity: 0.78,
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            let config = Self::default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            let encoded =
                toml::to_string_pretty(&config).context("failed to encode default config")?;
            fs::write(&path, encoded)
                .with_context(|| format!("failed to write {}", path.display()))?;
            return Ok(config);
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("ai", "clairos", "termi")
        .context("could not determine the user configuration directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}
