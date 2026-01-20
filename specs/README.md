# space-recorder

A Rust TUI app that renders your webcam as ASCII art in a modal overlay while hosting your shell. Screen share the terminal for instant coding streams.

## Vision

A single terminal window that contains:
- Your actual shell (zsh, bash, fish) running in a nested PTY
- An ASCII-rendered camera feed as a floating modal
- Hotkeys to toggle, resize, and reposition the camera

No FFmpeg. No external windows. No complexity. Just run `space-recorder`, do your thing, and screen share.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        space-recorder                            │
├─────────────────────────────────────────────────────────────────┤
│  Layer 4: TUI                                                    │
│  ├── ratatui layout and rendering                               │
│  ├── Modal positioning (corner selection)                       │
│  └── Hotkey handling                                            │
├─────────────────────────────────────────────────────────────────┤
│  Layer 3: ASCII Renderer                                         │
│  ├── Frame to ASCII/Unicode conversion                          │
│  ├── Character set selection (density ramps)                    │
│  └── Optional edge detection for sharper features               │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: Camera Capture                                         │
│  ├── Webcam access (nokhwa or v4l2)                             │
│  ├── Frame buffer management                                    │
│  └── Resolution and FPS control                                 │
├─────────────────────────────────────────────────────────────────┤
│  Layer 1: PTY Host                                               │
│  ├── Spawn user's shell                                         │
│  ├── Bidirectional I/O relay                                    │
│  └── Signal forwarding (SIGWINCH, etc.)                         │
└─────────────────────────────────────────────────────────────────┘
```

## Visual Layout

```
┌────────────────────────────────────────────────────────────────┐
│ $ cargo build                                                   │
│    Compiling space-recorder v0.1.0                             │
│    Finished dev [unoptimized + debuginfo]                      │
│ $ _                                                            │
│                                                                │
│                                                                │
│                                          ┌───────────────────┐ │
│                                          │ @@@@@@@@@@@@@@@@@ │ │
│                                          │ @%#*+:.    .:+*#@ │ │
│                                          │ @    ○    ○    @ │ │
│                                          │ @      ︵      @ │ │
│                                          │ @    ╲____╱    @ │ │
│                                          │ @@@@@@@@@@@@@@@@@ │ │
│                                          └───────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

1. **Rust + ratatui** - Fast, safe, great TUI ecosystem
2. **ASCII rendering** - Works in ANY terminal, no Sixel/Kitty needed
3. **PTY hosting** - Your real shell, not a fake prompt
4. **No recording** - Screen share or cmd+shift+5, keep it simple
5. **Modal overlay** - Camera floats over terminal, togglable

## Platform

**macOS primary** - Uses AVFoundation for camera via nokhwa crate
**Linux possible** - v4l2 camera support in nokhwa

## Quick Start

```bash
# Start with default shell and camera
space-recorder

# Specify shell
space-recorder --shell /bin/zsh

# Camera in different corner
space-recorder --position top-left

# Disable camera on start (toggle with hotkey)
space-recorder --no-camera
```

## Hotkeys

| Key | Action |
|-----|--------|
| `Ctrl+\` | Toggle camera visibility |
| `Ctrl+]` | Cycle camera position (corners) |
| `Ctrl+[` | Cycle camera size (S/M/L) |
| `Ctrl+/` | Cycle ASCII style |

## Specification Documents

| Document | Description |
|----------|-------------|
| [01-pty-host.md](./01-pty-host.md) | PTY spawning, I/O relay, signal handling |
| [02-camera-capture.md](./02-camera-capture.md) | Webcam access, frame buffering |
| [03-ascii-renderer.md](./03-ascii-renderer.md) | Frame to ASCII conversion algorithms |
| [04-tui.md](./04-tui.md) | Layout, modal rendering, hotkeys |
| [05-cli.md](./05-cli.md) | Arguments, config, shell integration |

## ASCII Rendering Styles

```
# Dense (default) - good contrast
 .:-=+*#%@

# Blocks - higher resolution feel
 ░▒▓█

# Minimal - clean look
 .:#

# Braille - highest resolution (if terminal supports)
⠀⠁⠂⠃⠄⠅⠆⠇⡀⡁...⣿
```

## Why ASCII?

1. **Universal** - Works in Terminal.app, iTerm2, Kitty, Linux console, SSH
2. **Aesthetic** - Retro/hacker vibe that fits coding streams
3. **Lightweight** - No GPU, no complex encoding
4. **Readable** - Doesn't obscure terminal text underneath
