//! Subcommand handlers for list-cameras and config actions.

use super::args::ConfigAction;
use crate::camera;
use crate::config::default_path as get_config_path;

/// List available cameras and print them to stdout.
pub fn list_cameras() {
    match camera::list_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                println!("No cameras found.");
                println!();
                println!("Make sure your camera is connected and permissions are granted.");
                println!(
                    "On macOS, grant access in System Settings > Privacy & Security > Camera."
                );
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
            println!(
                "  Shell: {}",
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
            );
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
