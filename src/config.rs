//! Configuration path utilities for space-recorder.

use std::path::PathBuf;

/// Get the default config file path.
///
/// Returns the platform-appropriate configuration directory:
/// - Linux: `~/.config/space-recorder/config.toml`
/// - macOS: `~/Library/Application Support/com.space-recorder.space-recorder/config.toml`
/// - Windows: `{FOLDERID_RoamingAppData}\space-recorder\space-recorder\config.toml`
pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("com", "space-recorder", "space-recorder")
        .map(|d| d.config_dir().to_path_buf().join("config.toml"))
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config/space-recorder/config.toml")
        })
}
