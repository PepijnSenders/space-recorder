//! Configuration file handling for space-recorder.
//!
//! Loads configuration from `~/.config/space-recorder/config.toml` or a custom path.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Configuration file structure for space-recorder.
/// Loaded from ~/.config/space-recorder/config.toml (or custom path via --config).
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub shell: ShellConfig,
    #[serde(default)]
    pub camera: CameraConfig,
    #[serde(default)]
    pub modal: ModalConfig,
    #[serde(default)]
    pub ascii: AsciiConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct ShellConfig {
    pub command: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CameraConfig {
    #[serde(default)]
    pub device: u32,
    #[serde(default = "default_true")]
    pub mirror: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct ModalConfig {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct AsciiConfig {
    #[serde(default)]
    pub charset: Option<String>,
    #[serde(default)]
    pub invert: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default = "default_true")]
    pub status_bar: bool,
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from a file path.
    /// Returns default config if the file doesn't exist.
    /// Returns an error if the file exists but cannot be parsed.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let path = path.map(PathBuf::from).unwrap_or_else(default_path);

        if path.exists() {
            let content = std::fs::read_to_string(&path).map_err(|e| ConfigError::IoError {
                path: path.clone(),
                source: e,
            })?;
            let config: Config = toml::from_str(&content).map_err(|e| ConfigError::ParseError {
                path: path.clone(),
                source: e,
            })?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }
}

/// Errors that can occur when loading configuration.
#[derive(Debug)]
pub enum ConfigError {
    IoError {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseError {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError { path, source } => {
                write!(
                    f,
                    "Failed to read config file '{}': {}",
                    path.display(),
                    source
                )
            }
            ConfigError::ParseError { path, source } => {
                write!(
                    f,
                    "Failed to parse config file '{}': {}",
                    path.display(),
                    source
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::IoError { source, .. } => Some(source),
            ConfigError::ParseError { source, .. } => Some(source),
        }
    }
}

/// Get the default config file path.
pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("com", "space-recorder", "space-recorder")
        .map(|d| d.config_dir().to_path_buf().join("config.toml"))
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config/space-recorder/config.toml")
        })
}
