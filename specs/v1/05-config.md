# 05: Config

**Layer 5** - Configuration system using TOML, profiles, and presets.

## Overview

The config layer handles:
- TOML configuration file parsing
- Profile system (coding, presenting, etc.)
- Effect and layout presets
- Default values and validation
- Hot-reload support (post-MVP)

## Dependencies

- **All layers**: Config values feed into every layer

## Configuration Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                    Configuration Priority                        │
├─────────────────────────────────────────────────────────────────┤
│  1. CLI Arguments          (highest priority)                    │
│           ▼                                                      │
│  2. Environment Variables                                        │
│           ▼                                                      │
│  3. Profile Settings       (from --profile flag)                │
│           ▼                                                      │
│  4. User Config File       (~/.config/space-recorder/config.toml)│
│           ▼                                                      │
│  5. Built-in Defaults      (lowest priority)                    │
└─────────────────────────────────────────────────────────────────┘
```

## Config File Location

### Default Paths

| Platform | Path |
|----------|------|
| macOS | `~/.config/space-recorder/config.toml` |
| Linux | `~/.config/space-recorder/config.toml` |

## TOML Schema

### Complete Configuration File

```toml
# space-recorder configuration

# Output settings
[output]
resolution = [1280, 720]
framerate = 30
format = "mp4"

# Screen capture settings
[capture.screen]
device = "Capture screen 0"
framerate = 30
capture_cursor = true

# Window capture
[capture.window]
app_name = "Terminal"

# Webcam settings
[capture.webcam]
device = "FaceTime HD Camera"
resolution = [1280, 720]
framerate = 30
mirror = false

# Audio settings
[audio]
enabled = true

[audio.microphone]
device = "MacBook Pro Microphone"
volume = 1.0

[audio.system]
enabled = false
device = "BlackHole 2ch"
volume = 0.5

[audio.processing]
noise_gate = true
compressor = false
normalize = false

# Compositor settings
[compositor]
mode = "ghost"

[compositor.ghost]
opacity = 0.3
blend_mode = "normal"

[compositor.pip]
position = "bottom_right"
size = 0.25
margin = 10

# Effects settings
[effects]
preset = "cyberpunk"

[effects.color_grade]
saturation = 1.3
contrast = 1.05

[effects.vignette]
enabled = true
strength = 0.4

[effects.overlays]
live_badge = true
timestamp = true
timestamp_format = "%H:%M:%S"

# Hotkey bindings
[hotkeys]
opacity_up = "ctrl+="
opacity_down = "ctrl+-"
toggle_effects = "ctrl+e"
toggle_webcam = "ctrl+w"

# Profiles
[profiles.coding]
capture.window.app_name = "Terminal"
compositor.ghost.opacity = 0.25
effects.preset = "cyberpunk"

[profiles.presenting]
capture.screen.device = "Capture screen 0"
compositor.mode = "pip"
compositor.pip.size = 0.3
effects.preset = "clean"

[profiles.minimal]
capture.webcam.enabled = false
effects.preset = "none"
effects.overlays.live_badge = false
```

## Profile System

### Built-in Profiles

| Profile | Description |
|---------|-------------|
| `coding` | Terminal + ghost webcam + cyberpunk effects |
| `presenting` | Full screen + PIP webcam + clean effects |
| `minimal` | Terminal only, no effects |

### Profile Activation

```bash
space-recorder start --profile coding
space-recorder start --profile coding --opacity 0.5
```

## Environment Variables

| Variable | Config Path | Example |
|----------|-------------|---------|
| `SPACE_RECORDER_OPACITY` | compositor.ghost.opacity | `0.4` |
| `SPACE_RECORDER_EFFECT` | effects.preset | `cyberpunk` |
| `SPACE_RECORDER_WEBCAM` | capture.webcam.device | `FaceTime HD Camera` |
| `SPACE_RECORDER_WINDOW` | capture.window.app_name | `Terminal` |

## Hot Reload (Post-MVP)

Watch config file for changes and apply without restart.
Only certain settings can be hot-reloaded:
- Opacity
- Effect presets
- Text overlays

Settings requiring restart:
- Capture devices
- Resolution
- Framerate

## Acceptance Criteria

### MVP Requirements

1. [ ] Load config from default path
2. [ ] Parse TOML with all sections
3. [ ] Apply CLI argument overrides
4. [ ] Basic validation (resolution, opacity range)
5. [ ] Default values for all settings

### Post-MVP Requirements

1. [ ] Profile system with inheritance
2. [ ] Effect and layout presets
3. [ ] Environment variable support
4. [ ] Hot reload on file change
5. [ ] Config generation command

## Example Commands

```bash
# Use default config
space-recorder start

# Use custom config file
space-recorder start --config ~/myconfig.toml

# Override config with CLI args
space-recorder start --opacity 0.4 --effect dark_mode

# Use a profile
space-recorder start --profile coding

# Generate default config
space-recorder init
```
