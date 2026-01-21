//! CLI argument parsing with clap.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use super::enums::{CharacterSet, Position, Size};

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
    #[arg(long, default_value = "blocks")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_defaults() {
        let args = Args::parse_from(["space-recorder"]);
        assert!(args.shell.is_none());
        assert_eq!(args.camera, 0);
        assert!(!args.no_camera);
        assert_eq!(args.position, Position::BottomRight);
        assert_eq!(args.size, Size::Small);
        assert_eq!(args.charset, CharacterSet::Blocks);
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
            Some(Command::Config {
                action: ConfigAction::Show,
            }) => (),
            _ => panic!("Expected Config Show subcommand"),
        }
    }

    #[test]
    fn test_args_config_init_subcommand() {
        let args = Args::parse_from(["space-recorder", "config", "init"]);
        match args.command {
            Some(Command::Config {
                action: ConfigAction::Init,
            }) => (),
            _ => panic!("Expected Config Init subcommand"),
        }
    }

    #[test]
    fn test_args_combined_options() {
        let args = Args::parse_from([
            "space-recorder",
            "--shell",
            "/bin/zsh",
            "--camera",
            "1",
            "--position",
            "top-left",
            "--size",
            "large",
            "--charset",
            "braille",
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
}
