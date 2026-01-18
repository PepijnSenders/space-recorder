# space-recorder

A Rust CLI tool for compositing video streams with ghost overlays and AI-generated effects. Built for coding streamers who want to share their terminal with a subtle webcam presence in video calls.

## Features

- **Ghost Overlay** - Full-screen webcam at low opacity, with your terminal showing through
- **AI Video Overlays** - Generate ambient video backgrounds from text prompts via [fal.ai](https://fal.ai)
- **Cyberpunk Effects** - Color grading and neon aesthetics
- **Live Indicators** - LIVE badge and timestamp overlays
- **Hotkey Controls** - Adjust opacity on the fly during your stream

## Requirements

- macOS (uses AVFoundation for capture)
- FFmpeg
- mpv (for preview window)
- Rust 1.70+

## Installation

```bash
git clone https://github.com/PepijnSenders/space-recorder.git
cd space-recorder
cargo build --release
```

## Quick Start

```bash
# Start with Terminal.app and webcam ghost overlay at 30% opacity
space-recorder start --window "Terminal" --opacity 0.3 --effect cyberpunk

# Adjust opacity during stream with +/- keys
# Share the mpv preview window in Google Meet, Zoom, etc.
```

## AI Video Overlays (fal.ai)

Generate AI video overlays from text prompts during your stream.

### Setup

```bash
export FAL_API_KEY="your-api-key-here"
```

### Usage

```bash
# Start with fal.ai overlay enabled
space-recorder start --window Terminal --fal

# Pre-generate videos to warm the cache
space-recorder fal-generate "cyberpunk cityscape"

# Manage cache
space-recorder fal-cache list
space-recorder fal-cache clear
```

### Prompt Commands

While streaming with `--fal` enabled:

| Command | Description |
|---------|-------------|
| `<text>` | Generate video from prompt |
| `/clear` | Remove the AI overlay |
| `/opacity <0.0-1.0>` | Adjust AI overlay opacity |

## Configuration

Create `~/.config/space-recorder/config.toml`:

```toml
[fal]
enabled = true
opacity = 0.5

[fal.cache]
directory = "~/.cache/space-recorder/fal-videos"
max_size_mb = 2048
```

## How It Works

The tool uses FFmpeg to composite video streams:

```
Terminal Window ──┐
                  ├── FFmpeg Compositor ── mpv Preview ── Share in Video Call
Webcam (ghost) ───┤
AI Overlay ───────┘
```

The ghost effect uses FFmpeg's `colorchannelmixer=aa=0.3` for alpha transparency.

## Documentation

See [specs/](./specs/) for detailed architecture and implementation specs.

## License

MIT
