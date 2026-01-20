# space-recorder Implementation Plan

## Overview

Rust TUI app that renders webcam as ASCII art in a modal overlay while hosting your shell. Screen share the terminal for instant coding streams.

**Goal**: A working `space-recorder` binary you can run, use your shell normally, and have an ASCII camera overlay in the corner.

---

## Phase 1: Project Setup & PTY Host

> **Spec**: [01-pty-host.md](./specs/01-pty-host.md)

### 1.1 Rust Project Setup

- [x] **Initialize Cargo project**
  - AC: `cargo new space-recorder` with binary target
  - AC: `cargo build` succeeds
  - AC: Basic main.rs with placeholder

- [x] **Add core dependencies**
  - AC: `Cargo.toml` includes: ratatui, crossterm, portable-pty, tokio, clap
  - AC: `cargo build` succeeds with all dependencies

### 1.2 PTY Hosting

- [x] **Implement PtyHost struct**
  - AC: Can spawn a shell (zsh/bash) in a PTY
  - AC: Uses `portable-pty` crate
  - AC: Returns handle for read/write

- [x] **Implement shell selection**
  - AC: Uses `--shell` arg if provided
  - AC: Falls back to `$SHELL` env var
  - AC: Falls back to `/bin/zsh`

- [x] **Implement raw mode terminal**
  - AC: Enters raw mode on startup
  - AC: Restores terminal on exit (including panic)
  - AC: Uses scopeguard or similar for cleanup

- [x] **Implement stdin -> PTY forwarding**
  - AC: Keystrokes sent to shell
  - AC: Ctrl sequences work (Ctrl+C, Ctrl+D, etc.)
  - AC: Special characters (arrows, backspace) work

- [x] **Implement PTY -> stdout forwarding**
  - AC: Shell output displayed
  - AC: Colors and escape sequences pass through
  - AC: No visible lag

- [x] **Implement SIGWINCH handling**
  - AC: Terminal resize detected
  - AC: PTY resized to match
  - AC: Shell redraws correctly

### 1.3 Phase 1 Milestone

- [x] **End-to-end test: basic PTY**
  - AC: `cargo run` spawns shell
  - AC: Can type commands and see output
  - AC: Ctrl+C sends interrupt to shell (not app)
  - AC: Ctrl+D exits shell, app exits
  - AC: Terminal restored cleanly on exit

---

## Phase 2: Camera Capture

> **Spec**: [02-camera-capture.md](./specs/02-camera-capture.md)

### 2.1 Camera Access

- [x] **Add nokhwa dependency**
  - AC: `nokhwa` with `input-avfoundation` feature for macOS
  - AC: Compiles successfully

- [x] **Implement device enumeration**
  - AC: `list-cameras` command shows available cameras
  - AC: Shows device index and name
  - AC: Handles no cameras gracefully

- [x] **Implement camera opening**
  - AC: Can open camera by index
  - AC: Configurable resolution (default 640x480)
  - AC: Handles permission errors gracefully

### 2.2 Frame Capture

- [x] **Implement background capture thread**
  - AC: Captures frames continuously
  - AC: Stores latest frame in shared buffer
  - AC: Doesn't block main thread

- [x] **Implement frame format conversion**
  - AC: Converts camera output to RGB
  - AC: Handles various camera formats (MJPEG, YUYV)

- [x] **Implement horizontal mirroring**
  - AC: `--mirror` flag enables selfie-mode flip
  - AC: Default: mirror enabled

### 2.3 Phase 2 Milestone

- [x] **End-to-end test: camera capture**
  - AC: `space-recorder list-cameras` shows devices
  - AC: Camera opens without error
  - AC: Frames captured at reasonable rate (~15+ fps)
  - AC: App handles missing camera gracefully

---

## Phase 3: ASCII Renderer

> **Spec**: [03-ascii-renderer.md](./specs/03-ascii-renderer.md)

### 3.1 Core Rendering

- [x] **Implement grayscale conversion**
  - AC: RGB frame -> grayscale using luminance formula
  - AC: Efficient (no allocations in hot path if possible)

- [x] **Implement downsampling**
  - AC: Map image pixels to character grid
  - AC: Average brightness per cell
  - AC: Configurable output dimensions

- [x] **Implement character mapping**
  - AC: Standard charset: ` .:-=+*#%@`
  - AC: Brightness maps to character
  - AC: Invert option for light terminals

### 3.2 Character Sets

- [x] **Implement standard charset**
  - AC: 10-level density ramp
  - AC: Good contrast on dark terminals

- [x] **Implement blocks charset**
  - AC: Uses `░▒▓█` characters
  - AC: Higher perceived resolution

- [x] **Implement braille charset**
  - AC: Uses Unicode braille patterns
  - AC: 2x4 subpixel resolution per character
  - AC: Highest detail mode

### 3.3 Quality Improvements

- [x] **Implement aspect ratio correction**
  - AC: Accounts for terminal char aspect (~2:1)
  - AC: Face doesn't look stretched

- [x] **Implement edge detection (optional)**
  - AC: Sobel filter for sharper features
  - AC: `--edge-detection` flag to enable
  - AC: Default: off

### 3.4 Phase 3 Milestone

- [x] **End-to-end test: ASCII rendering**
  - AC: Camera frame converts to ASCII
  - AC: Face recognizable in output
  - AC: Different charsets produce different looks
  - AC: Performance: <10ms per frame

---

## Phase 4: TUI Integration

> **Spec**: [04-tui.md](./specs/04-tui.md)

### 4.1 Basic TUI Setup

- [x] **Implement Terminal wrapper**
  - AC: Uses ratatui with crossterm backend
  - AC: Enters alternate screen
  - AC: Restores on exit

- [x] **Implement main render loop**
  - AC: Uses tokio for async
  - AC: Handles PTY output, camera frames, and input concurrently
  - AC: Smooth ~30fps rendering

### 4.2 PTY Display

- [x] **Implement PTY buffer**
  - AC: Stores PTY output
  - AC: Renders to full screen

- [x] **Implement basic PTY rendering**
  - AC: Raw output displayed (pass-through mode)
  - AC: Shell usable (commands, output, colors)

### 4.3 Camera Modal

- [x] **Implement modal positioning**
  - AC: Four corners + center positions
  - AC: Respects terminal boundaries
  - AC: 1-char margin from edges

- [x] **Implement modal sizing**
  - AC: Small (20x10), Medium (40x20), Large (60x30)
  - AC: Optional border

- [x] **Implement modal rendering**
  - AC: ASCII frame displayed in modal area
  - AC: Clears area before drawing (overlay effect)
  - AC: Updates at ~15fps

### 4.4 Hotkeys

- [x] **Implement hotkey interception**
  - AC: Alt+C toggles camera visibility
  - AC: Alt+P cycles position
  - AC: Alt+S cycles size
  - AC: Alt+A cycles charset
  - AC: Other keys forwarded to PTY

### 4.5 Status Bar

- [x] **Implement status bar**
  - AC: Shows camera status, position, size, charset
  - AC: Bottom of screen
  - AC: Can be hidden with `--no-status`

### 4.6 Phase 4 Milestone

- [x] **End-to-end test: full TUI**
  - AC: `cargo run` shows shell with camera modal
  - AC: Can use shell normally
  - AC: Camera updates smoothly
  - AC: Hotkeys work
  - AC: Window resize handled
  - AC: Clean exit

---

## Phase 5: CLI & Polish

> **Spec**: [05-cli.md](./specs/05-cli.md)

### 5.1 CLI Arguments

- [x] **Implement clap argument parsing**
  - AC: All flags from spec work
  - AC: `--help` shows usage
  - AC: `--version` shows version

- [x] **Implement list-cameras subcommand**
  - AC: Lists cameras with indices
  - AC: Helpful output format

- [x] **Implement config subcommands**
  - AC: `config show` displays current settings
  - AC: `config init` creates default config file

### 5.2 Configuration

- [ ] **Implement config file loading**
  - AC: Reads `~/.config/space-recorder/config.toml`
  - AC: Falls back to defaults if missing
  - AC: CLI args override config

- [ ] **Implement config file writing**
  - AC: `config init` creates file with comments
  - AC: Doesn't overwrite existing without prompt

### 5.3 Error Handling

- [ ] **Implement user-friendly errors**
  - AC: Camera not found -> suggest list-cameras
  - AC: Permission denied -> suggest System Settings
  - AC: Config parse error -> show line number

### 5.4 Phase 5 Milestone

- [ ] **End-to-end test: complete app**
  - AC: Fresh install works out of box
  - AC: Config file customizes behavior
  - AC: All CLI options work
  - AC: Errors are helpful

---

## Final Verification

- [ ] **Real-world test: screen share in Google Meet**
  - AC: Start space-recorder
  - AC: Screen share the terminal window
  - AC: ASCII face visible to other participants
  - AC: Shell fully functional during call
  - AC: No significant lag or issues

---

## Quick Reference

```bash
# Development
cargo build
cargo run
cargo run -- --help
cargo run -- list-cameras
cargo run -- --position top-left --size large --charset braille

# Testing
cargo test
```

## Dependencies Summary

```toml
[dependencies]
ratatui = "0.28"
crossterm = "0.28"
portable-pty = "0.8"
nokhwa = { version = "0.10", features = ["input-avfoundation"] }
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
directories = "5"
```

## Architecture Summary

```
main.rs          - Entry point, CLI parsing, app bootstrap
pty.rs           - PTY hosting (spawn shell, I/O relay)
camera.rs        - Camera capture (nokhwa, frame buffer)
ascii.rs         - ASCII rendering (grayscale, downsample, charset)
tui.rs           - TUI (ratatui, modal, hotkeys)
config.rs        - Configuration (TOML, defaults, resolution)
```
