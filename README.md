# space-recorder

ASCII camera overlay for terminal streaming. A TUI app that renders your webcam as ASCII art while hosting a fully functional shell.

## Features

- Real-time webcam to ASCII art conversion
- Multiple character sets: standard, blocks, minimal, braille
- Adjustable overlay position, size, and transparency
- Full shell passthrough with proper PTY handling
- Hotkey controls for live adjustments
- Color support with 24-bit true color
- macOS native camera support via AVFoundation

## Installation

### Homebrew (macOS)

```bash
brew install PepijnSenders/tap/space-recorder
```

### From source

```bash
cargo install --path .
```

## Usage

```bash
# Start with defaults
space-recorder

# List available cameras
space-recorder list-cameras

# Customize position and size
space-recorder --position top-right --size medium

# Use braille characters for higher resolution
space-recorder --charset braille

# Mirror mode (selfie view)
space-recorder --mirror

# Start with camera hidden
space-recorder --no-camera
```

## Hotkeys

| Key | Action |
|-----|--------|
| `Alt+C` | Toggle camera visibility |
| `Alt+P` | Cycle position (corners + center) |
| `Alt+S` | Cycle size (small → medium → large → xlarge → huge) |
| `Alt+A` | Cycle ASCII charset |
| `Alt+T` | Cycle transparency level |

All other keys pass through to the shell.

## Options

```
-s, --shell <SHELL>      Shell to spawn (default: $SHELL or /bin/zsh)
    --camera <INDEX>     Camera device index [default: 0]
    --no-camera          Disable camera on start
-p, --position <POS>     Position: top-left, top-right, bottom-left, bottom-right, center [default: bottom-right]
    --size <SIZE>        Size: small, medium, large, xlarge, huge [default: small]
    --charset <CHARSET>  Character set: standard, blocks, minimal, braille [default: standard]
    --mirror             Mirror camera horizontally
    --invert             Invert brightness (for light terminals)
    --no-status          Hide status bar
-c, --config <PATH>      Config file path
```

## Configuration

Create a config file with defaults:

```bash
space-recorder config init
```

Config location: `~/.config/space-recorder/config.toml`

## Requirements

- macOS (AVFoundation for camera access)
- Camera permissions granted in System Settings > Privacy & Security > Camera
- A terminal with true color support for best results

## License

MIT
