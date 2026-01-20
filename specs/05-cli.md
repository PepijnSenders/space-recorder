# 05 - CLI

Command-line interface and configuration for space-recorder.

## Crate Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
directories = "5"
toml = "0.8"
serde = { version = "1", features = ["derive"] }
```

## Commands

### Main Command (default)

```bash
# Start with defaults
space-recorder

# With options
space-recorder --shell /bin/zsh --position bottom-right --size medium
```

### Subcommands

```bash
# List available cameras
space-recorder list-cameras

# Show current config
space-recorder config show

# Generate default config file
space-recorder config init
```

## CLI Structure

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "space-recorder")]
#[command(about = "ASCII camera overlay for terminal streaming")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Shell to spawn (default: $SHELL or /bin/zsh)
    #[arg(long, short)]
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
    #[arg(long, short, default_value = "medium")]
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

#[derive(Subcommand)]
pub enum Command {
    /// List available cameras
    ListCameras,
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Create default config file
    Init,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Size {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum CharacterSet {
    Standard,
    Blocks,
    Minimal,
    Braille,
}
```

## Configuration File

### Location

```
~/.config/space-recorder/config.toml
```

Use `directories` crate:

```rust
fn config_path() -> PathBuf {
    directories::ProjectDirs::from("com", "space-recorder", "space-recorder")
        .map(|d| d.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("~/.config/space-recorder/config.toml"))
}
```

### Schema

```toml
# space-recorder configuration

[shell]
# Shell to spawn (default: $SHELL)
command = "/bin/zsh"

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
size = "medium"
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
```

### Config Struct

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Default)]
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
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ShellConfig {
    pub command: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CameraConfig {
    #[serde(default)]
    pub device: u32,
    #[serde(default = "default_true")]
    pub mirror: bool,
    #[serde(default = "default_resolution")]
    pub resolution: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModalConfig {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_size")]
    pub size: String,
    #[serde(default = "default_true")]
    pub border: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AsciiConfig {
    #[serde(default = "default_charset")]
    pub charset: String,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub edge_detection: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UiConfig {
    #[serde(default = "default_true")]
    pub status_bar: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_toggle_camera")]
    pub toggle_camera: String,
    #[serde(default = "default_cycle_position")]
    pub cycle_position: String,
    #[serde(default = "default_cycle_size")]
    pub cycle_size: String,
    #[serde(default = "default_cycle_charset")]
    pub cycle_charset: String,
}

fn default_true() -> bool { true }
fn default_resolution() -> String { "640x480".to_string() }
fn default_position() -> String { "bottom-right".to_string() }
fn default_size() -> String { "medium".to_string() }
fn default_charset() -> String { "standard".to_string() }
fn default_toggle_camera() -> String { "c".to_string() }
fn default_cycle_position() -> String { "p".to_string() }
fn default_cycle_size() -> String { "s".to_string() }
fn default_cycle_charset() -> String { "a".to_string() }
```

### Loading Config

```rust
impl Config {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path
            .map(PathBuf::from)
            .unwrap_or_else(config_path);

        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self, path: Option<&Path>) -> Result<()> {
        let path = path
            .map(PathBuf::from)
            .unwrap_or_else(config_path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
```

## Priority Order

Settings are resolved in this order (later overrides earlier):

1. **Defaults** - Built-in default values
2. **Config file** - `~/.config/space-recorder/config.toml`
3. **Environment variables** - `SPACE_RECORDER_*`
4. **CLI arguments** - `--position`, `--size`, etc.

```rust
fn resolve_settings(cli: &Cli, config: &Config) -> Settings {
    Settings {
        shell: cli.shell.clone()
            .or_else(|| std::env::var("SPACE_RECORDER_SHELL").ok())
            .or_else(|| config.shell.command.clone())
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/zsh".to_string()),

        position: cli.position
            // CLI takes precedence
            .unwrap_or_else(|| {
                // Then env var
                std::env::var("SPACE_RECORDER_POSITION")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    // Then config file
                    .unwrap_or_else(|| config.modal.position.parse().unwrap_or_default())
            }),

        // ... etc
    }
}
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SPACE_RECORDER_SHELL` | Shell command |
| `SPACE_RECORDER_CAMERA` | Camera device index |
| `SPACE_RECORDER_POSITION` | Modal position |
| `SPACE_RECORDER_SIZE` | Modal size |
| `SPACE_RECORDER_CHARSET` | ASCII character set |

## Help Output

```
$ space-recorder --help

ASCII camera overlay for terminal streaming

Usage: space-recorder [OPTIONS] [COMMAND]

Commands:
  list-cameras  List available cameras
  config        Configuration management
  help          Print this message or the help of the given subcommand(s)

Options:
  -s, --shell <SHELL>        Shell to spawn (default: $SHELL or /bin/zsh)
      --camera <CAMERA>      Camera device index [default: 0]
      --no-camera            Disable camera on start
  -p, --position <POSITION>  Camera position [default: bottom-right]
                             [possible values: top-left, top-right, bottom-left, bottom-right, center]
      --size <SIZE>          Camera size [default: medium]
                             [possible values: small, medium, large]
      --charset <CHARSET>    ASCII character set [default: standard]
                             [possible values: standard, blocks, minimal, braille]
      --mirror               Mirror camera horizontally
      --invert               Invert brightness (for light terminals)
      --no-status            Hide status bar
  -c, --config <CONFIG>      Config file path
  -h, --help                 Print help
  -V, --version              Print version
```

## List Cameras Command

```
$ space-recorder list-cameras

Available cameras:
  [0] FaceTime HD Camera (Built-in)
  [1] USB Webcam

Use --camera <index> to select a camera.
```

## Config Commands

```
$ space-recorder config show

Current configuration:
  Shell: /bin/zsh
  Camera: 0 (FaceTime HD Camera)
  Position: bottom-right
  Size: medium
  Charset: standard
  Mirror: yes
  Status bar: yes

Config file: ~/.config/space-recorder/config.toml (exists)
```

```
$ space-recorder config init

Created config file: ~/.config/space-recorder/config.toml
```

## Error Messages

```rust
fn print_error(e: &Error) {
    match e {
        Error::CameraNotFound(idx) => {
            eprintln!("Error: Camera {} not found.", idx);
            eprintln!("Run 'space-recorder list-cameras' to see available cameras.");
        }
        Error::ShellNotFound(shell) => {
            eprintln!("Error: Shell '{}' not found.", shell);
            eprintln!("Check that the shell exists or set $SHELL.");
        }
        Error::ConfigParseFailed(path, e) => {
            eprintln!("Error: Failed to parse config file: {}", path.display());
            eprintln!("Details: {}", e);
        }
        Error::PermissionDenied(resource) => {
            eprintln!("Error: Permission denied for {}.", resource);
            eprintln!("On macOS, grant access in System Settings > Privacy & Security.");
        }
    }
}
```

## Implementation Checklist

- [ ] CLI argument parsing with clap
- [ ] Subcommands (list-cameras, config)
- [ ] Config file loading/saving
- [ ] TOML serialization
- [ ] Environment variable support
- [ ] Settings resolution (priority order)
- [ ] Help text and examples
- [ ] Error messages with suggestions
- [ ] Config init command
- [ ] Config show command
