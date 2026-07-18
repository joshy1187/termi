use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::{fs::OpenOptionsExt, fs::PermissionsExt};

pub const CONFIG_VERSION: u32 = 1;
pub const MAX_CONFIG_BYTES: u64 = 64 * 1024;
pub const MAX_GRID_CELLS: usize = 131_072;
pub const MAX_COLUMNS: u16 = 512;
pub const MAX_ROWS: u16 = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub config_version: u32,
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
            config_version: CONFIG_VERSION,
            shell: default_shell(),
            font_family: "DejaVu Sans Mono".to_owned(),
            font_size: 16.0,
            cell_width: 9.64,
            cell_height: 20.0,
            scrollback_lines: 10_000,
            initial_columns: 100,
            initial_rows: 32,
            background_dim: 0.34,
            terminal_opacity: 0.72,
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Result<Self> {
        Self::load_or_create_at(&config_path()?)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.config_version == CONFIG_VERSION,
            "unsupported config_version {}; expected {CONFIG_VERSION}",
            self.config_version
        );

        let shell = Path::new(&self.shell);
        ensure!(!self.shell.trim().is_empty(), "shell must not be empty");
        ensure!(shell.is_absolute(), "shell must be an absolute path");
        let metadata = fs::metadata(shell)
            .with_context(|| format!("configured shell {} does not exist", shell.display()))?;
        ensure!(
            metadata.is_file(),
            "configured shell must be a regular file"
        );
        #[cfg(unix)]
        ensure!(
            metadata.permissions().mode() & 0o111 != 0,
            "configured shell {} is not executable",
            shell.display()
        );

        ensure!(
            !self.font_family.trim().is_empty(),
            "font_family must not be empty"
        );
        validate_f32("font_size", self.font_size, 8.0, 48.0)?;
        validate_f32("cell_width", self.cell_width, 4.0, 32.0)?;
        validate_f32("cell_height", self.cell_height, 8.0, 64.0)?;
        validate_f32("background_dim", self.background_dim, 0.0, 1.0)?;
        validate_f32("terminal_opacity", self.terminal_opacity, 0.0, 1.0)?;

        ensure!(
            self.scrollback_lines <= 100_000,
            "scrollback_lines must be between 0 and 100000"
        );
        ensure!(
            (20..=MAX_COLUMNS).contains(&self.initial_columns),
            "initial_columns must be between 20 and {MAX_COLUMNS}"
        );
        ensure!(
            (6..=MAX_ROWS).contains(&self.initial_rows),
            "initial_rows must be between 6 and {MAX_ROWS}"
        );
        ensure!(
            usize::from(self.initial_columns) * usize::from(self.initial_rows) <= MAX_GRID_CELLS,
            "initial terminal grid must not exceed {MAX_GRID_CELLS} cells"
        );
        Ok(())
    }

    fn load_or_create_at(path: &Path) -> Result<Self> {
        if !path.exists() {
            let config = Self::default();
            config.validate()?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            write_new_config(path, &config)?;
        }

        let config = read_config(path)?;
        config
            .validate()
            .with_context(|| format!("invalid configuration in {}", path.display()))?;
        Ok(config)
    }
}

fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| {
            let path = Path::new(shell);
            path.is_absolute() && executable_file(path)
        })
        .or_else(|| executable_file(Path::new("/bin/bash")).then(|| "/bin/bash".to_owned()))
        .unwrap_or_else(|| "/bin/sh".to_owned())
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    return metadata.permissions().mode() & 0o111 != 0;
    #[cfg(not(unix))]
    true
}

fn validate_f32(name: &str, value: f32, minimum: f32, maximum: f32) -> Result<()> {
    ensure!(value.is_finite(), "{name} must be finite");
    ensure!(
        (minimum..=maximum).contains(&value),
        "{name} must be between {minimum} and {maximum}"
    );
    Ok(())
}

fn read_config(path: &Path) -> Result<AppConfig> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut raw = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut raw)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if raw.len() as u64 > MAX_CONFIG_BYTES {
        bail!(
            "configuration {} exceeds the {} byte limit",
            path.display(),
            MAX_CONFIG_BYTES
        );
    }
    let raw = String::from_utf8(raw)
        .with_context(|| format!("configuration {} is not UTF-8", path.display()))?;
    toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_new_config(path: &Path, config: &AppConfig) -> Result<()> {
    let encoded = toml::to_string_pretty(config).context("failed to encode default config")?;
    let parent = path.parent().context("configuration path has no parent")?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("configuration path is not valid UTF-8")?;

    for attempt in 0..16_u8 {
        let temporary = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            attempt
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);

        let mut temporary_file = match options.open(&temporary) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to create {}", temporary.display()));
            }
        };
        temporary_file
            .write_all(encoded.as_bytes())
            .and_then(|()| temporary_file.sync_all())
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        drop(temporary_file);

        let link_result = fs::hard_link(&temporary, path);
        let cleanup_result = fs::remove_file(&temporary);
        if let Err(error) = cleanup_result {
            return Err(error).with_context(|| format!("failed to remove {}", temporary.display()));
        }

        return match link_result {
            Ok(()) => {
                File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .with_context(|| format!("failed to sync {}", parent.display()))?;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error).with_context(|| {
                format!(
                    "failed to atomically create configuration {}",
                    path.display()
                )
            }),
        };
    }

    bail!("could not allocate a temporary configuration file")
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("ai", "clairos", "termi")
        .context("could not determine the user configuration directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{AppConfig, CONFIG_VERSION, MAX_CONFIG_BYTES};

    fn temporary_config_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "termi-config-test-{}-{name}/config.toml",
            std::process::id()
        ))
    }

    #[test]
    fn accepts_unversioned_configuration_as_version_one() {
        let config: AppConfig = toml::from_str("shell = '/bin/sh'").unwrap();
        assert_eq!(config.config_version, CONFIG_VERSION);
        config.validate().unwrap();
    }

    #[test]
    fn rejects_unknown_fields() {
        let error = toml::from_str::<AppConfig>("shell = '/bin/sh'\ntyop = true").unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_non_finite_geometry() {
        let config = AppConfig {
            cell_width: f32::NAN,
            ..AppConfig::default()
        };
        assert!(
            config
                .validate()
                .unwrap_err()
                .to_string()
                .contains("finite")
        );
    }

    #[test]
    fn rejects_excessive_grid() {
        let config = AppConfig {
            initial_columns: 512,
            initial_rows: 256,
            ..AppConfig::default()
        };
        assert!(config.validate().is_ok());

        let config = AppConfig {
            initial_columns: 512,
            initial_rows: 257,
            ..AppConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_limit_is_large_enough_for_defaults() {
        let encoded = toml::to_string_pretty(&AppConfig::default()).unwrap();
        assert!((encoded.len() as u64) < MAX_CONFIG_BYTES);
    }

    #[test]
    fn creates_and_reloads_default_configuration() {
        let path = temporary_config_path("create");
        let _ = fs::remove_dir_all(path.parent().unwrap());
        let created = AppConfig::load_or_create_at(&path).unwrap();
        let reloaded = AppConfig::load_or_create_at(&path).unwrap();
        assert_eq!(created.config_version, reloaded.config_version);
        assert!(path.is_file());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_configuration_above_size_limit() {
        let path = temporary_config_path("oversized");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b' '; MAX_CONFIG_BYTES as usize + 1]).unwrap();
        let error = AppConfig::load_or_create_at(&path).unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
