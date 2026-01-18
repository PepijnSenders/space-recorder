# 06: CLI

**Layer 6** - Command-line interface, arguments, and interactive mode.

## Overview

The CLI layer handles:
- Command structure and parsing
- Argument and flag definitions
- Interactive mode for positioning
- Status display and logging
- Error reporting and help

## Dependencies

- **All layers**: CLI orchestrates and configures all layers

## Command Structure

```
space-recorder <COMMAND> [OPTIONS]

Commands:
  start         Start the compositor and preview
  list-devices  List available capture devices
  init          Generate default config file
  help          Show help information

Global Options:
  -c, --config <PATH>    Use custom config file
  -v, --verbose          Enable verbose logging
  -q, --quiet            Suppress non-error output
  --version              Show version
  --help                 Show help
```

## Commands

### start

Main command to start the compositor.

```bash
space-recorder start [OPTIONS]
```

**Options:**

| Flag | Short | Type | Default | Description |
|------|-------|------|---------|-------------|
| `--window` | `-w` | string | - | Capture specific window by app name |
| `--screen` | `-s` | int | 0 | Screen index to capture |
| `--webcam` | | string | auto | Webcam device name or index |
| `--no-webcam` | | flag | false | Disable webcam |
| `--opacity` | `-o` | float | 0.3 | Ghost overlay opacity (0.0-1.0) |
| `--effect` | `-e` | string | cyberpunk | Effect preset name |
| `--no-effects` | | flag | false | Disable all effects |
| `--profile` | `-p` | string | - | Use named profile |
| `--output` | | string | - | Record to file instead of preview |
| `--resolution` | `-r` | string | 1280x720 | Output resolution |
| `--framerate` | `-f` | int | 30 | Output framerate |

**Examples:**

```bash
# Basic start with Terminal window
space-recorder start --window Terminal

# Custom opacity and effect
space-recorder start -w Terminal -o 0.4 -e dark_mode

# Record to file
space-recorder start -w Terminal --output recording.mp4

# Use a profile
space-recorder start --profile coding

# Full screen, no webcam
space-recorder start --screen 0 --no-webcam
```

### list-devices

List available capture devices.

```bash
space-recorder list-devices [OPTIONS]
```

**Options:**

| Flag | Type | Description |
|------|------|-------------|
| `--video` | flag | Show only video devices |
| `--audio` | flag | Show only audio devices |
| `--json` | flag | Output as JSON |

**Example Output:**

```
Video Devices:
  [0] FaceTime HD Camera
  [1] Capture screen 0
  [2] Capture screen 1

Audio Devices:
  [0] MacBook Pro Microphone
  [1] BlackHole 2ch
```

### init

Generate default configuration file.

```bash
space-recorder init [OPTIONS]
```

**Options:**

| Flag | Type | Description |
|------|------|-------------|
| `--path` | string | Custom output path |
| `--force` | flag | Overwrite existing file |

## Interactive Mode (Post-MVP)

```bash
space-recorder start --interactive
```

Features:
- Arrow keys to adjust PIP position
- +/- to adjust opacity
- Number keys for effect presets
- q to quit, Enter to confirm

## Status Display

### During Streaming

```
space-recorder v0.1.0

Status: Streaming
Window: Terminal
Webcam: FaceTime HD Camera
Effect: cyberpunk
Opacity: 30%

Output: mpv preview (share this window)
Resolution: 1280x720 @ 30fps

Hotkeys:
  +/-  Adjust opacity
  q    Quit
```

## Error Handling

### User-Friendly Errors

```
Error: Window 'Terminal' not found.
Available windows: iTerm2, Code, Safari

Error: Webcam 'USB Camera' not found.
Run 'space-recorder list-devices' to see available devices.

Error: FFmpeg not found.
Please install FFmpeg: brew install ffmpeg

Error: Permission denied: Screen Recording
Grant permission in System Preferences > Privacy > Screen Recording
```

## Help Output

```
space-recorder - Video compositor for coding streams

Usage: space-recorder <COMMAND> [OPTIONS]

Commands:
  start         Start the compositor and preview
  list-devices  List available capture devices
  init          Generate default config file
  help          Show help for a command

Options:
  -c, --config <PATH>  Use custom config file
  -v, --verbose        Enable verbose logging
  -q, --quiet          Suppress non-error output
  --version            Show version
  -h, --help           Show this help

Examples:
  space-recorder start --window Terminal
  space-recorder start -w Terminal -o 0.4 -e dark_mode
  space-recorder list-devices
```

## Acceptance Criteria

### MVP Requirements

1. [ ] `start` command with basic options (--window, --opacity, --effect)
2. [ ] `list-devices` command
3. [ ] Clear error messages with suggested fixes
4. [ ] Help text for all commands
5. [ ] Status display while running

### Post-MVP Requirements

1. [ ] `init` command for config generation
2. [ ] Interactive TUI mode
3. [ ] --json output for list-devices
4. [ ] Shell completion scripts
5. [ ] Verbose/quiet logging modes

## Example Session

```bash
$ space-recorder list-devices
Video Devices:
  [0] FaceTime HD Camera
  [1] Capture screen 0

Audio Devices:
  [0] MacBook Pro Microphone

$ space-recorder start --window Terminal --opacity 0.3 --effect cyberpunk

space-recorder v0.1.0

Found window: Terminal (800x600 at 100,50)
Webcam: FaceTime HD Camera
Starting FFmpeg pipeline...
Preview window opened (mpv)

Status: Streaming
Hotkeys: +/- opacity, q quit

^C
Stopping...
Done.
```
