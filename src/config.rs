//! Configuration file loading and parsing for space-recorder.
//!
//! Loads configuration from `~/.config/space-recorder/config.toml` with
//! support for CLI argument overrides.

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Errors that can occur when loading configuration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    Read(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Invalid config value: {0}")]
    Validation(String),
}

/// Main configuration structure loaded from TOML
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Compositor settings (opacity, blend mode)
    pub compositor: CompositorConfig,
    /// Effects settings (preset, overlays)
    pub effects: EffectsConfig,
    /// Capture settings (window, webcam)
    pub capture: CaptureConfig,
    /// Audio settings
    pub audio: AudioConfig,
    /// Output settings
    pub output: OutputConfig,
    /// fal.ai settings
    pub fal: FalConfig,
}

/// fal.ai configuration settings
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FalConfig {
    /// Whether fal.ai features are enabled
    pub enabled: Option<bool>,
    /// Default model to use for video generation
    pub default_model: Option<String>,
    /// Overlay settings for AI-generated videos
    pub overlay: FalOverlayConfig,
    /// Cache settings for generated videos
    pub cache: FalCacheConfig,
}

/// fal.ai overlay configuration settings
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct FalOverlayConfig {
    /// Overlay opacity (0.0-1.0)
    pub opacity: Option<f32>,
    /// Crossfade duration in milliseconds (default 500)
    pub crossfade_duration_ms: Option<u32>,
    /// Whether to loop the overlay video (default true)
    pub r#loop: Option<bool>,
}

impl Default for FalOverlayConfig {
    fn default() -> Self {
        Self {
            opacity: None,
            crossfade_duration_ms: Some(500),
            r#loop: Some(true),
        }
    }
}

/// fal.ai cache configuration settings
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct FalCacheConfig {
    /// Whether caching is enabled (default true)
    pub enabled: Option<bool>,
    /// Cache directory path (default ~/.cache/space-recorder/fal-videos)
    pub directory: Option<String>,
    /// Maximum cache size in megabytes (default 1000)
    pub max_size_mb: Option<u64>,
}

impl Default for FalCacheConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            directory: None, // Will use default ~/.cache/space-recorder/fal-videos
            max_size_mb: Some(1000),
        }
    }
}

/// Compositor configuration (ghost overlay settings)
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CompositorConfig {
    /// Ghost overlay opacity (0.0-1.0)
    pub opacity: Option<f32>,
}

/// Effects configuration (presets and overlays)
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct EffectsConfig {
    /// Video effect preset (none, cyberpunk, dark_mode)
    pub preset: Option<String>,
    /// Vignette effect settings
    pub vignette: Option<bool>,
    /// Film grain effect
    pub grain: Option<bool>,
    /// Overlay settings
    pub overlays: OverlayConfig,
}

/// Overlay configuration (LIVE badge, timestamp)
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    /// Show LIVE badge
    pub live_badge: Option<bool>,
    /// Show timestamp
    pub timestamp: Option<bool>,
}

/// Capture configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    /// Window capture settings
    pub window: WindowCaptureConfig,
    /// Webcam capture settings
    pub webcam: WebcamCaptureConfig,
    /// Screen capture settings
    pub screen: ScreenCaptureConfig,
}

/// Window capture configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WindowCaptureConfig {
    /// Application name to capture (e.g., "Terminal")
    pub app_name: Option<String>,
}

/// Webcam capture configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WebcamCaptureConfig {
    /// Whether webcam is enabled
    pub enabled: Option<bool>,
    /// Webcam device name or index
    pub device: Option<String>,
    /// Mirror (horizontally flip) the webcam
    pub mirror: Option<bool>,
}

/// Screen capture configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ScreenCaptureConfig {
    /// Screen device index
    pub device: Option<usize>,
}

/// Audio configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Whether audio is enabled
    pub enabled: Option<bool>,
    /// Audio volume (0.0-2.0)
    pub volume: Option<f32>,
    /// Audio processing settings
    pub processing: AudioProcessingConfig,
}

/// Audio processing configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AudioProcessingConfig {
    /// Enable noise gate
    pub noise_gate: Option<bool>,
    /// Noise gate threshold (0.0-1.0)
    pub noise_gate_threshold: Option<f32>,
    /// Enable compressor
    pub compressor: Option<bool>,
}

/// Output configuration
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Output resolution [width, height]
    pub resolution: Option<[u32; 2]>,
    /// Output framerate
    pub framerate: Option<u32>,
}

impl Config {
    /// Get the default configuration file path
    pub fn default_path() -> PathBuf {
        dirs_home()
            .map(|h| h.join(".config").join("space-recorder").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }

    /// Load configuration from the default path, returning default config if file doesn't exist
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(Self::default_path())
    }

    /// Load configuration from a specific path (returns default if file doesn't exist)
    pub fn load_from(path: PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from an explicit path (errors if file doesn't exist)
    /// Used when --config flag is explicitly provided by the user
    pub fn load_from_explicit(path: PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::Read(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Config file not found: {}", path.display()),
            )));
        }

        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate opacity range
        if let Some(opacity) = self.compositor.opacity {
            if !(0.0..=1.0).contains(&opacity) {
                return Err(ConfigError::Validation(format!(
                    "opacity must be between 0.0 and 1.0, got {}",
                    opacity
                )));
            }
        }

        // Validate volume range
        if let Some(volume) = self.audio.volume {
            if !(0.0..=2.0).contains(&volume) {
                return Err(ConfigError::Validation(format!(
                    "volume must be between 0.0 and 2.0, got {}",
                    volume
                )));
            }
        }

        // Validate noise gate threshold
        if let Some(threshold) = self.audio.processing.noise_gate_threshold {
            if !(0.0..=1.0).contains(&threshold) {
                return Err(ConfigError::Validation(format!(
                    "noise_gate_threshold must be between 0.0 and 1.0, got {}",
                    threshold
                )));
            }
        }

        // Validate effect preset
        if let Some(ref preset) = self.effects.preset {
            let valid_presets = ["none", "cyberpunk", "dark_mode"];
            if !valid_presets.contains(&preset.as_str()) {
                return Err(ConfigError::Validation(format!(
                    "unknown effect preset '{}'. Valid presets: {}",
                    preset,
                    valid_presets.join(", ")
                )));
            }
        }

        // Validate resolution
        if let Some(res) = self.output.resolution {
            if res[0] == 0 || res[1] == 0 {
                return Err(ConfigError::Validation(
                    "resolution width and height must be greater than 0".to_string(),
                ));
            }
            if res[0] > 7680 || res[1] > 4320 {
                return Err(ConfigError::Validation(
                    "resolution exceeds maximum supported (7680x4320)".to_string(),
                ));
            }
        }

        // Validate framerate
        if let Some(fps) = self.output.framerate {
            if !(1..=120).contains(&fps) {
                return Err(ConfigError::Validation(format!(
                    "framerate must be between 1 and 120, got {}",
                    fps
                )));
            }
        }

        // Validate fal.ai default_model
        if let Some(ref model) = self.fal.default_model {
            if model.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "fal.default_model cannot be empty. Example: 'fal-ai/fast-svd-lcm'".to_string(),
                ));
            }
        }

        // Validate fal.overlay.opacity range
        if let Some(opacity) = self.fal.overlay.opacity {
            if !(0.0..=1.0).contains(&opacity) {
                return Err(ConfigError::Validation(format!(
                    "fal.overlay.opacity must be between 0.0 and 1.0, got {}",
                    opacity
                )));
            }
        }

        // Validate fal.overlay.crossfade_duration_ms (must be reasonable)
        if let Some(duration) = self.fal.overlay.crossfade_duration_ms {
            if duration > 10000 {
                return Err(ConfigError::Validation(format!(
                    "fal.overlay.crossfade_duration_ms must be at most 10000ms, got {}",
                    duration
                )));
            }
        }

        // Validate fal.cache.max_size_mb (must be positive)
        if let Some(max_size) = self.fal.cache.max_size_mb {
            if max_size == 0 {
                return Err(ConfigError::Validation(
                    "fal.cache.max_size_mb must be greater than 0".to_string(),
                ));
            }
        }

        // Validate fal.cache.directory (must not be empty if specified)
        if let Some(ref dir) = self.fal.cache.directory {
            if dir.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "fal.cache.directory cannot be empty. Example: '~/.cache/space-recorder/fal-videos'".to_string(),
                ));
            }
        }

        Ok(())
    }
}

/// Get the user's home directory
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.compositor.opacity.is_none());
        assert!(config.effects.preset.is_none());
        assert!(config.capture.window.app_name.is_none());
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [compositor]
            opacity = 0.4
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.compositor.opacity, Some(0.4));
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            [compositor]
            opacity = 0.5

            [effects]
            preset = "cyberpunk"
            vignette = true
            grain = false

            [effects.overlays]
            live_badge = true
            timestamp = false

            [capture.window]
            app_name = "Terminal"

            [capture.webcam]
            enabled = true
            mirror = true

            [audio]
            enabled = true
            volume = 1.0

            [audio.processing]
            noise_gate = true
            compressor = false

            [output]
            resolution = [1920, 1080]
            framerate = 60
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.compositor.opacity, Some(0.5));
        assert_eq!(config.effects.preset, Some("cyberpunk".to_string()));
        assert_eq!(config.effects.vignette, Some(true));
        assert_eq!(config.effects.grain, Some(false));
        assert_eq!(config.effects.overlays.live_badge, Some(true));
        assert_eq!(config.capture.window.app_name, Some("Terminal".to_string()));
        assert_eq!(config.capture.webcam.mirror, Some(true));
        assert_eq!(config.audio.volume, Some(1.0));
        assert_eq!(config.audio.processing.noise_gate, Some(true));
        assert_eq!(config.output.resolution, Some([1920, 1080]));
        assert_eq!(config.output.framerate, Some(60));
    }

    #[test]
    fn test_validate_opacity_range() {
        let mut config = Config::default();
        config.compositor.opacity = Some(1.5);
        assert!(config.validate().is_err());

        config.compositor.opacity = Some(-0.1);
        assert!(config.validate().is_err());

        config.compositor.opacity = Some(0.5);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_volume_range() {
        let mut config = Config::default();
        config.audio.volume = Some(2.5);
        assert!(config.validate().is_err());

        config.audio.volume = Some(1.5);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_effect_preset() {
        let mut config = Config::default();
        config.effects.preset = Some("invalid".to_string());
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown effect preset"));

        config.effects.preset = Some("cyberpunk".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_resolution() {
        let mut config = Config::default();
        config.output.resolution = Some([0, 720]);
        assert!(config.validate().is_err());

        config.output.resolution = Some([8000, 4000]);
        assert!(config.validate().is_err());

        config.output.resolution = Some([1920, 1080]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_framerate() {
        let mut config = Config::default();
        config.output.framerate = Some(0);
        assert!(config.validate().is_err());

        config.output.framerate = Some(121);
        assert!(config.validate().is_err());

        config.output.framerate = Some(60);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/path/config.toml");
        let config = Config::load_from(path).unwrap();
        // Should return default config
        assert!(config.compositor.opacity.is_none());
    }

    #[test]
    fn test_load_from_explicit_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/config.toml");
        let result = Config::load_from_explicit(path);
        // Should error for explicit paths
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Config file not found"));
    }

    #[test]
    fn test_default_path() {
        let path = Config::default_path();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".config"));
        assert!(path_str.contains("space-recorder"));
        assert!(path_str.ends_with("config.toml"));
    }

    #[test]
    fn test_invalid_toml_syntax_shows_helpful_error() {
        let invalid_toml = r#"
            [compositor
            opacity = 0.5
        "#;
        let result: Result<Config, _> = toml::from_str(invalid_toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Error should indicate TOML parsing issue with location info
        let err_str = err.to_string();
        assert!(
            err_str.contains("expected") || err_str.contains("line") || err_str.contains("invalid"),
            "Error message should be helpful: {}",
            err_str
        );
    }

    #[test]
    fn test_invalid_opacity_shows_helpful_error() {
        let mut config = Config::default();
        config.compositor.opacity = Some(2.0);
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("opacity"), "Error should mention 'opacity': {}", err);
        assert!(err.contains("0.0") && err.contains("1.0"), "Error should mention valid range: {}", err);
    }

    #[test]
    fn test_invalid_effect_preset_shows_helpful_error() {
        let mut config = Config::default();
        config.effects.preset = Some("neon_glow".to_string());
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("neon_glow"), "Error should mention the invalid value: {}", err);
        assert!(err.contains("none") && err.contains("cyberpunk") && err.contains("dark_mode"),
            "Error should list valid presets: {}", err);
    }

    #[test]
    fn test_window_app_name_parsing() {
        // Test that window app_name is correctly parsed from config
        let toml = r#"
            [capture.window]
            app_name = "Terminal"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.capture.window.app_name, Some("Terminal".to_string()));

        // Test with different app names
        let toml2 = r#"
            [capture.window]
            app_name = "Visual Studio Code"
        "#;
        let config2: Config = toml::from_str(toml2).unwrap();
        assert_eq!(config2.capture.window.app_name, Some("Visual Studio Code".to_string()));
    }

    #[test]
    fn test_fal_section_recognized() {
        // Test that [fal] section is parsed correctly
        let toml = r#"
            [fal]
            enabled = true
            default_model = "fal-ai/fast-svd-lcm"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.enabled, Some(true));
        assert_eq!(config.fal.default_model, Some("fal-ai/fast-svd-lcm".to_string()));
    }

    #[test]
    fn test_fal_enabled_false() {
        // Test that enabled = false works
        let toml = r#"
            [fal]
            enabled = false
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.enabled, Some(false));
        assert!(config.fal.default_model.is_none());
    }

    #[test]
    fn test_fal_default_values() {
        // Test that fal has None defaults when not specified
        let config = Config::default();
        assert!(config.fal.enabled.is_none());
        assert!(config.fal.default_model.is_none());
    }

    #[test]
    fn test_fal_empty_model_shows_helpful_error() {
        // Test that empty default_model produces helpful error
        let mut config = Config::default();
        config.fal.default_model = Some("".to_string());
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("fal.default_model"), "Error should mention fal.default_model: {}", err);
        assert!(err.contains("cannot be empty"), "Error should say cannot be empty: {}", err);
        assert!(err.contains("fal-ai/fast-svd-lcm"), "Error should include example model: {}", err);
    }

    #[test]
    fn test_fal_whitespace_only_model_shows_error() {
        // Test that whitespace-only default_model is rejected
        let mut config = Config::default();
        config.fal.default_model = Some("   ".to_string());
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_fal_valid_model_passes_validation() {
        // Test that a valid model name passes validation
        let mut config = Config::default();
        config.fal.default_model = Some("fal-ai/fast-svd-lcm".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_config_with_other_sections() {
        // Test that fal section works alongside other config sections
        let toml = r#"
            [compositor]
            opacity = 0.5

            [fal]
            enabled = true
            default_model = "fal-ai/some-model"

            [effects]
            preset = "cyberpunk"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.compositor.opacity, Some(0.5));
        assert_eq!(config.fal.enabled, Some(true));
        assert_eq!(config.fal.default_model, Some("fal-ai/some-model".to_string()));
        assert_eq!(config.effects.preset, Some("cyberpunk".to_string()));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_overlay_section_parsed() {
        // Test that [fal.overlay] section is parsed correctly
        let toml = r#"
            [fal.overlay]
            opacity = 0.5
            crossfade_duration_ms = 750
            loop = false
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.overlay.opacity, Some(0.5));
        assert_eq!(config.fal.overlay.crossfade_duration_ms, Some(750));
        assert_eq!(config.fal.overlay.r#loop, Some(false));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_overlay_default_values() {
        // Test that fal.overlay has correct defaults
        let config = Config::default();
        assert!(config.fal.overlay.opacity.is_none());
        assert_eq!(config.fal.overlay.crossfade_duration_ms, Some(500));
        assert_eq!(config.fal.overlay.r#loop, Some(true));
    }

    #[test]
    fn test_fal_overlay_opacity_validation() {
        // Test that fal.overlay.opacity validates range
        let mut config = Config::default();
        config.fal.overlay.opacity = Some(1.5);
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("fal.overlay.opacity"), "Error should mention fal.overlay.opacity: {}", err);
        assert!(err.contains("0.0") && err.contains("1.0"), "Error should mention valid range: {}", err);

        config.fal.overlay.opacity = Some(-0.1);
        assert!(config.validate().is_err());

        config.fal.overlay.opacity = Some(0.5);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_overlay_crossfade_duration_validation() {
        // Test that fal.overlay.crossfade_duration_ms validates maximum
        let mut config = Config::default();
        config.fal.overlay.crossfade_duration_ms = Some(15000);
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("fal.overlay.crossfade_duration_ms"), "Error should mention field: {}", err);
        assert!(err.contains("10000"), "Error should mention maximum: {}", err);

        config.fal.overlay.crossfade_duration_ms = Some(5000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_overlay_loop_setting() {
        // Test loop setting parsing
        let toml_true = r#"
            [fal.overlay]
            loop = true
        "#;
        let config: Config = toml::from_str(toml_true).unwrap();
        assert_eq!(config.fal.overlay.r#loop, Some(true));

        let toml_false = r#"
            [fal.overlay]
            loop = false
        "#;
        let config: Config = toml::from_str(toml_false).unwrap();
        assert_eq!(config.fal.overlay.r#loop, Some(false));
    }

    #[test]
    fn test_fal_overlay_with_parent_section() {
        // Test that fal.overlay works alongside fal parent settings
        let toml = r#"
            [fal]
            enabled = true
            default_model = "fal-ai/fast-svd-lcm"

            [fal.overlay]
            opacity = 0.7
            crossfade_duration_ms = 300
            loop = true
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.enabled, Some(true));
        assert_eq!(config.fal.default_model, Some("fal-ai/fast-svd-lcm".to_string()));
        assert_eq!(config.fal.overlay.opacity, Some(0.7));
        assert_eq!(config.fal.overlay.crossfade_duration_ms, Some(300));
        assert_eq!(config.fal.overlay.r#loop, Some(true));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_overlay_partial_config() {
        // Test that partial fal.overlay config uses defaults for missing values
        let toml = r#"
            [fal.overlay]
            opacity = 0.4
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.overlay.opacity, Some(0.4));
        // Should use defaults for missing fields
        assert_eq!(config.fal.overlay.crossfade_duration_ms, Some(500));
        assert_eq!(config.fal.overlay.r#loop, Some(true));
    }

    #[test]
    fn test_fal_cache_section_parsed() {
        // AC: [fal.cache] section for cache settings
        let toml = r#"
            [fal.cache]
            enabled = true
            directory = "/custom/cache/path"
            max_size_mb = 500
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.cache.enabled, Some(true));
        assert_eq!(config.fal.cache.directory, Some("/custom/cache/path".to_string()));
        assert_eq!(config.fal.cache.max_size_mb, Some(500));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_cache_default_values() {
        // AC: enabled default true, directory default None, max_size_mb default 1000
        let config = Config::default();
        assert_eq!(config.fal.cache.enabled, Some(true));
        assert!(config.fal.cache.directory.is_none()); // Uses ~/.cache/space-recorder/fal-videos
        assert_eq!(config.fal.cache.max_size_mb, Some(1000));
    }

    #[test]
    fn test_fal_cache_enabled_false() {
        // AC: enabled setting (default true)
        let toml = r#"
            [fal.cache]
            enabled = false
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.cache.enabled, Some(false));
        // Defaults for unspecified fields
        assert_eq!(config.fal.cache.max_size_mb, Some(1000));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_cache_max_size_validation() {
        // AC: max_size_mb setting (default 1000)
        let mut config = Config::default();
        config.fal.cache.max_size_mb = Some(0);
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("fal.cache.max_size_mb"), "Error should mention field: {}", err);
        assert!(err.contains("greater than 0"), "Error should mention constraint: {}", err);

        // Valid max_size should pass
        config.fal.cache.max_size_mb = Some(2000);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_cache_directory_validation() {
        // AC: directory setting (default ~/.cache/space-recorder/fal-videos)
        let mut config = Config::default();
        config.fal.cache.directory = Some("".to_string());
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("fal.cache.directory"), "Error should mention field: {}", err);
        assert!(err.contains("cannot be empty"), "Error should mention constraint: {}", err);

        // Whitespace-only should also fail
        config.fal.cache.directory = Some("   ".to_string());
        assert!(config.validate().is_err());

        // Valid directory should pass
        config.fal.cache.directory = Some("~/.cache/my-custom-cache".to_string());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_cache_with_all_fal_sections() {
        // Test that fal.cache works alongside fal and fal.overlay sections
        let toml = r#"
            [fal]
            enabled = true
            default_model = "fal-ai/fast-svd-lcm"

            [fal.overlay]
            opacity = 0.5
            crossfade_duration_ms = 600

            [fal.cache]
            enabled = true
            directory = "/tmp/fal-cache"
            max_size_mb = 750
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.fal.enabled, Some(true));
        assert_eq!(config.fal.default_model, Some("fal-ai/fast-svd-lcm".to_string()));
        assert_eq!(config.fal.overlay.opacity, Some(0.5));
        assert_eq!(config.fal.overlay.crossfade_duration_ms, Some(600));
        assert_eq!(config.fal.cache.enabled, Some(true));
        assert_eq!(config.fal.cache.directory, Some("/tmp/fal-cache".to_string()));
        assert_eq!(config.fal.cache.max_size_mb, Some(750));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_fal_cache_partial_config() {
        // Test that partial fal.cache config uses defaults for missing values
        let toml = r#"
            [fal.cache]
            max_size_mb = 2000
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        // Only max_size_mb was specified
        assert_eq!(config.fal.cache.max_size_mb, Some(2000));
        // Should use defaults for missing fields
        assert_eq!(config.fal.cache.enabled, Some(true));
        assert!(config.fal.cache.directory.is_none());
    }
}
