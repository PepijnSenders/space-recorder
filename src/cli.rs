//! Command-line interface definitions and helpers.
//!
//! This module contains all CLI argument parsing, enums, and subcommand handlers.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::ascii;
use crate::camera;
use crate::config::default_path as get_config_path;
use crate::terminal::{ModalPosition, ModalSize};

// ==================== CLI Enums ====================

/// Camera modal position on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    Center,
}

impl From<Position> for ModalPosition {
    fn from(p: Position) -> Self {
        match p {
            Position::TopLeft => ModalPosition::TopLeft,
            Position::TopRight => ModalPosition::TopRight,
            Position::BottomLeft => ModalPosition::BottomLeft,
            Position::BottomRight => ModalPosition::BottomRight,
            Position::Center => ModalPosition::Center,
        }
    }
}

/// Camera modal size preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Size {
    #[default]
    Small,
    Medium,
    Large,
    Xlarge,
    Huge,
}

impl From<Size> for ModalSize {
    fn from(s: Size) -> Self {
        match s {
            Size::Small => ModalSize::Small,
            Size::Medium => ModalSize::Medium,
            Size::Large => ModalSize::Large,
            Size::Xlarge => ModalSize::XLarge,
            Size::Huge => ModalSize::Huge,
        }
    }
}

/// ASCII character set for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CharacterSet {
    #[default]
    Standard,
    Blocks,
    Minimal,
    Braille,
}

impl From<CharacterSet> for ascii::CharSet {
    fn from(c: CharacterSet) -> Self {
        match c {
            CharacterSet::Standard => ascii::CharSet::Standard,
            CharacterSet::Blocks => ascii::CharSet::Blocks,
            CharacterSet::Minimal => ascii::CharSet::Minimal,
            CharacterSet::Braille => ascii::CharSet::Braille,
        }
    }
}

// ==================== CLI Arguments ====================

/// TUI app that renders webcam as ASCII art overlay while hosting a shell
#[derive(Parser, Debug)]
#[command(name = "space-recorder")]
#[command(version, about = "ASCII camera overlay for terminal streaming", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Shell to spawn (default: $SHELL or /bin/zsh)
    #[arg(short, long)]
    pub shell: Option<String>,

    /// Camera device index (from list-cameras)
    #[arg(long, default_value = "0")]
    pub camera: u32,

    /// Disable camera on start
    #[arg(long)]
    pub no_camera: bool,

    /// Camera position
    #[arg(long, short, default_value = "bottom-right")]
    pub position: Position,

    /// Camera size
    #[arg(long, default_value = "small")]
    pub size: Size,

    /// ASCII character set
    #[arg(long, default_value = "standard")]
    pub charset: CharacterSet,

    /// Mirror camera horizontally
    #[arg(long)]
    pub mirror: bool,

    /// Invert brightness (for light terminals)
    #[arg(long)]
    pub invert: bool,

    /// Hide status bar
    #[arg(long)]
    pub no_status: bool,

    /// Config file path
    #[arg(long, short)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List available cameras
    ListCameras,
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Create default config file
    Init,
}

// ==================== Subcommand Handlers ====================

/// List available cameras and print them to stdout.
pub fn list_cameras() {
    match camera::list_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                println!("No cameras found.");
                println!();
                println!("Make sure your camera is connected and permissions are granted.");
                println!("On macOS, grant access in System Settings > Privacy & Security > Camera.");
            } else {
                println!("Available cameras:");
                for device in devices {
                    println!("  {}", device);
                }
                println!();
                println!("Use --camera <index> to select a camera.");
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Handle config subcommand actions.
pub fn handle_config_action(action: ConfigAction) {
    match action {
        ConfigAction::Show => {
            println!("Current configuration:");
            println!("  Shell: {}", std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()));
            println!("  Camera: 0");
            println!("  Position: bottom-right");
            println!("  Size: small");
            println!("  Charset: standard");
            println!("  Mirror: no");
            println!("  Status bar: yes");
            println!();

            let config_path = get_config_path();
            if config_path.exists() {
                println!("Config file: {} (exists)", config_path.display());
            } else {
                println!("Config file: {} (not found)", config_path.display());
            }
        }
        ConfigAction::Init => {
            let config_path = get_config_path();

            if config_path.exists() {
                eprintln!("Config file already exists: {}", config_path.display());
                eprintln!("Use 'space-recorder config show' to view current settings.");
                std::process::exit(1);
            }

            // Create parent directories if needed
            if let Some(parent) = config_path.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                eprintln!("Error creating config directory: {}", e);
                std::process::exit(1);
            }

            // Write default config
            let default_config = r#"# space-recorder configuration

[shell]
# Shell to spawn (default: $SHELL)
# command = "/bin/zsh"

[camera]
# Camera device index
device = 0
# Mirror horizontally (selfie mode)
mirror = true
# Capture resolution (lower = faster)
resolution = "640x480"

[modal]
# Start with camera visible
visible = true
# Position: top-left, top-right, bottom-left, bottom-right, center
position = "bottom-right"
# Size: small, medium, large
size = "small"
# Show border around modal
border = true

[ascii]
# Character set: standard, blocks, minimal, braille
charset = "standard"
# Invert brightness (for light themes)
invert = false
# Enable edge detection for sharper features
edge_detection = false

[ui]
# Show status bar
status_bar = true

[hotkeys]
# Key bindings (Alt + key)
toggle_camera = "c"
cycle_position = "p"
cycle_size = "s"
cycle_charset = "a"
"#;

            if let Err(e) = std::fs::write(&config_path, default_config) {
                eprintln!("Error writing config file: {}", e);
                std::process::exit(1);
            }

            println!("Created config file: {}", config_path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== CLI Default Values Tests ====================

    #[test]
    fn test_args_defaults() {
        let args = Args::parse_from(["space-recorder"]);
        assert!(args.shell.is_none());
        assert_eq!(args.camera, 0);
        assert!(!args.no_camera);
        assert_eq!(args.position, Position::BottomRight);
        assert_eq!(args.size, Size::Small);
        assert_eq!(args.charset, CharacterSet::Standard);
        assert!(!args.mirror);
        assert!(!args.invert);
        assert!(!args.no_status);
        assert!(args.config.is_none());
        assert!(args.command.is_none());
    }

    #[test]
    fn test_args_mirror_flag() {
        let args = Args::parse_from(["space-recorder", "--mirror"]);
        assert!(args.mirror);
    }

    #[test]
    fn test_args_invert_flag() {
        let args = Args::parse_from(["space-recorder", "--invert"]);
        assert!(args.invert);
    }

    #[test]
    fn test_args_no_status_flag() {
        let args = Args::parse_from(["space-recorder", "--no-status"]);
        assert!(args.no_status);
    }

    #[test]
    fn test_args_no_camera_flag() {
        let args = Args::parse_from(["space-recorder", "--no-camera"]);
        assert!(args.no_camera);
    }

    #[test]
    fn test_args_camera_index() {
        let args = Args::parse_from(["space-recorder", "--camera", "2"]);
        assert_eq!(args.camera, 2);
    }

    #[test]
    fn test_args_position_values() {
        let args = Args::parse_from(["space-recorder", "--position", "top-left"]);
        assert_eq!(args.position, Position::TopLeft);

        let args = Args::parse_from(["space-recorder", "-p", "top-right"]);
        assert_eq!(args.position, Position::TopRight);

        let args = Args::parse_from(["space-recorder", "--position", "bottom-left"]);
        assert_eq!(args.position, Position::BottomLeft);

        let args = Args::parse_from(["space-recorder", "--position", "bottom-right"]);
        assert_eq!(args.position, Position::BottomRight);

        let args = Args::parse_from(["space-recorder", "--position", "center"]);
        assert_eq!(args.position, Position::Center);
    }

    #[test]
    fn test_args_size_values() {
        let args = Args::parse_from(["space-recorder", "--size", "small"]);
        assert_eq!(args.size, Size::Small);

        let args = Args::parse_from(["space-recorder", "--size", "medium"]);
        assert_eq!(args.size, Size::Medium);

        let args = Args::parse_from(["space-recorder", "--size", "large"]);
        assert_eq!(args.size, Size::Large);
    }

    #[test]
    fn test_args_charset_values() {
        let args = Args::parse_from(["space-recorder", "--charset", "standard"]);
        assert_eq!(args.charset, CharacterSet::Standard);

        let args = Args::parse_from(["space-recorder", "--charset", "blocks"]);
        assert_eq!(args.charset, CharacterSet::Blocks);

        let args = Args::parse_from(["space-recorder", "--charset", "minimal"]);
        assert_eq!(args.charset, CharacterSet::Minimal);

        let args = Args::parse_from(["space-recorder", "--charset", "braille"]);
        assert_eq!(args.charset, CharacterSet::Braille);
    }

    #[test]
    fn test_args_shell_option() {
        let args = Args::parse_from(["space-recorder", "--shell", "/bin/bash"]);
        assert_eq!(args.shell, Some("/bin/bash".to_string()));

        let args = Args::parse_from(["space-recorder", "-s", "/bin/fish"]);
        assert_eq!(args.shell, Some("/bin/fish".to_string()));
    }

    #[test]
    fn test_args_config_option() {
        let args = Args::parse_from(["space-recorder", "--config", "/tmp/config.toml"]);
        assert_eq!(args.config, Some(PathBuf::from("/tmp/config.toml")));

        let args = Args::parse_from(["space-recorder", "-c", "/tmp/test.toml"]);
        assert_eq!(args.config, Some(PathBuf::from("/tmp/test.toml")));
    }

    #[test]
    fn test_args_list_cameras_subcommand() {
        let args = Args::parse_from(["space-recorder", "list-cameras"]);
        assert!(matches!(args.command, Some(Command::ListCameras)));
    }

    #[test]
    fn test_args_config_show_subcommand() {
        let args = Args::parse_from(["space-recorder", "config", "show"]);
        match args.command {
            Some(Command::Config { action: ConfigAction::Show }) => (),
            _ => panic!("Expected Config Show subcommand"),
        }
    }

    #[test]
    fn test_args_config_init_subcommand() {
        let args = Args::parse_from(["space-recorder", "config", "init"]);
        match args.command {
            Some(Command::Config { action: ConfigAction::Init }) => (),
            _ => panic!("Expected Config Init subcommand"),
        }
    }

    #[test]
    fn test_args_combined_options() {
        let args = Args::parse_from([
            "space-recorder",
            "--shell", "/bin/zsh",
            "--camera", "1",
            "--position", "top-left",
            "--size", "large",
            "--charset", "braille",
            "--mirror",
            "--invert",
            "--no-status",
        ]);
        assert_eq!(args.shell, Some("/bin/zsh".to_string()));
        assert_eq!(args.camera, 1);
        assert_eq!(args.position, Position::TopLeft);
        assert_eq!(args.size, Size::Large);
        assert_eq!(args.charset, CharacterSet::Braille);
        assert!(args.mirror);
        assert!(args.invert);
        assert!(args.no_status);
    }

    // ==================== CLI Enum Conversion Tests ====================

    #[test]
    fn test_position_to_modal_position() {
        assert_eq!(ModalPosition::from(Position::TopLeft), ModalPosition::TopLeft);
        assert_eq!(ModalPosition::from(Position::TopRight), ModalPosition::TopRight);
        assert_eq!(ModalPosition::from(Position::BottomLeft), ModalPosition::BottomLeft);
        assert_eq!(ModalPosition::from(Position::BottomRight), ModalPosition::BottomRight);
        assert_eq!(ModalPosition::from(Position::Center), ModalPosition::Center);
    }

    #[test]
    fn test_size_to_modal_size() {
        assert_eq!(ModalSize::from(Size::Small), ModalSize::Small);
        assert_eq!(ModalSize::from(Size::Medium), ModalSize::Medium);
        assert_eq!(ModalSize::from(Size::Large), ModalSize::Large);
    }

    #[test]
    fn test_charset_to_ascii_charset() {
        assert_eq!(ascii::CharSet::from(CharacterSet::Standard), ascii::CharSet::Standard);
        assert_eq!(ascii::CharSet::from(CharacterSet::Blocks), ascii::CharSet::Blocks);
        assert_eq!(ascii::CharSet::from(CharacterSet::Minimal), ascii::CharSet::Minimal);
        assert_eq!(ascii::CharSet::from(CharacterSet::Braille), ascii::CharSet::Braille);
    }
}
