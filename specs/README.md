# space-recorder

A Rust CLI tool for compositing video streams (terminal window + webcam + effects) for screen sharing in video calls.

## Vision

Create a lightweight, hackable tool for coding streamers who want to:
- Share their terminal with a ghostly webcam overlay
- Apply cyberpunk/neon aesthetics to their stream
- Add live indicators and timestamps
- Share the composited output directly in video calls (Google Meet, Zoom, etc.)

## Primary Use Case

**Coding streams**: Terminal.app + webcam for tutorials and demos. The webcam appears as a full-screen ghost overlay at low opacity, with the terminal showing through. This creates a subtle presence effect rather than a traditional corner PIP.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        space-recorder                            │
├─────────────────────────────────────────────────────────────────┤
│  Layer 6: CLI                                                    │
│  ├── Commands (start, list-devices, etc.)                       │
│  ├── Arguments and flags                                        │
│  └── Interactive mode                                           │
├─────────────────────────────────────────────────────────────────┤
│  Layer 5: Config                                                 │
│  ├── TOML configuration                                         │
│  ├── Profiles (coding, presenting)                              │
│  └── Presets                                                    │
├─────────────────────────────────────────────────────────────────┤
│  Layer 4: Effects                                                │
│  ├── Color grading (cyberpunk, dark mode)                       │
│  ├── Text overlays (LIVE badge, timestamp)                      │
│  └── Custom filters                                             │
├─────────────────────────────────────────────────────────────────┤
│  Layer 3: Compositor                                             │
│  ├── Layout engine                                              │
│  ├── Ghost overlay blending                                     │
│  └── Positioning and scaling                                    │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: Audio                                                  │
│  ├── Microphone capture                                         │
│  ├── System audio (optional)                                    │
│  └── Audio mixing                                               │
├─────────────────────────────────────────────────────────────────┤
│  Layer 1: Capture                                                │
│  ├── Screen/window capture (AVFoundation)                       │
│  ├── Webcam capture                                             │
│  └── Window detection (AppleScript)                             │
├─────────────────────────────────────────────────────────────────┤
│  Layer 0: Core Pipeline                                          │
│  ├── FFmpeg subprocess management                               │
│  ├── Video format handling                                      │
│  └── Output targets (mpv, file, virtual cam)                    │
└─────────────────────────────────────────────────────────────────┘
```

## Layer Dependency Map

```
CLI (6) ──────────────────────┐
    │                         │
    ▼                         │
Config (5) ◄──────────────────┤
    │                         │
    ▼                         │
Effects (4) ──────────────────┤
    │                         │
    ▼                         │
Compositor (3) ───────────────┤
    │                         │
    ├───────────┐             │
    ▼           ▼             │
Capture (1)   Audio (2) ──────┤
    │           │             │
    ▼           ▼             │
Core Pipeline (0) ◄───────────┘
```

## Key Design Decisions

1. **FFmpeg as subprocess** - Shell out to ffmpeg rather than linking libav
   - Simpler to debug (can test commands manually)
   - No complex C bindings
   - Easy to swap filters

2. **TOML for config** - Human-readable, Rust-native support via `serde`

3. **AppleScript for window bounds** - macOS-specific but reliable for window detection

4. **mpv for preview** - Pipe to mpv for low-latency display, then share mpv window in Meet

5. **Ghost overlay style** - Full-screen webcam at low opacity (not corner PIP)
   - Uses FFmpeg's `colorchannelmixer=aa=0.3` for alpha control
   - Adjustable via hotkey during stream

## Platform

**macOS only** - Uses AVFoundation for capture, AppleScript for window detection

## Development Phases

### Phase 1: MVP (Quick Hack)

**Day 1: Core Pipeline Working**
- Initialize Rust project
- FFmpeg subprocess spawning
- Basic capture (Terminal.app + webcam)
- Ghost overlay blending
- Pipe to mpv for preview

**Day 2: Ghost Overlay + Hotkeys**
- Opacity adjustment via hotkeys (+/- keys)
- Mic audio integration
- Basic cyberpunk color grading

**Day 3: Polish for Use**
- LIVE badge + timestamp overlays
- Window detection via AppleScript
- CLI arguments
- Test in Google Meet

### Phase 2: Post-MVP (Later)
- TOML config files
- Named profiles
- Interactive TUI mode
- Hot reload
- Alternative PIP modes

## Quick Start (MVP)

```bash
# Start with Terminal.app and webcam, 30% ghost opacity
space-recorder start --window "Terminal" --opacity 0.3 --effect cyberpunk

# Adjust opacity during stream with +/- keys
# Share the mpv preview window in your video call
```

## fal.ai Video Overlay (v2)

Generate AI video overlays from text prompts during your stream using [fal.ai](https://fal.ai).

### Setup

1. **Get a fal.ai API key** from [fal.ai/dashboard](https://fal.ai/dashboard)

2. **Set the environment variable:**
   ```bash
   # Add to your shell profile (~/.zshrc, ~/.bashrc, etc.)
   export FAL_API_KEY="your-api-key-here"
   ```

   Or create a `.env` file in your project directory:
   ```bash
   FAL_API_KEY=your-api-key-here
   ```

### Usage

Enable fal.ai overlay mode with the `--fal` flag:

```bash
# Start with fal.ai overlay enabled
space-recorder start --window Terminal --fal

# Custom AI overlay opacity (separate from webcam ghost opacity)
space-recorder start --window Terminal --fal --fal-opacity 0.5
```

### Prompt Commands

While streaming with `--fal` enabled, type at the `>` prompt:

| Command | Description |
|---------|-------------|
| `<text>` | Generate video from prompt (e.g., `cyberpunk cityscape`) |
| `/clear` | Remove the AI overlay |
| `/opacity <0.0-1.0>` | Adjust AI overlay opacity (e.g., `/opacity 0.5`) |

**Example session:**
```
> cyberpunk cityscape
Generating video... (prompt: "cyberpunk cityscape")
Video ready, crossfading in...

> /opacity 0.7
AI overlay opacity set to 70%

> abstract particles flowing
Found in cache: "abstract particles flowing"
Video ready, crossfading in...

> /clear
AI overlay cleared.
```

### Pre-generating Videos

Generate videos before streaming to warm the cache:

```bash
# Generate a single video
space-recorder fal-generate "cyberpunk cityscape"

# Batch generate from a file (one prompt per line)
space-recorder fal-generate --batch prompts.txt
```

### Cache Management

Generated videos are cached locally at `~/.cache/space-recorder/fal-videos/` to avoid re-generating the same prompts.

```bash
# List cached videos with prompts and sizes
space-recorder fal-cache list

# Clear all cached videos
space-recorder fal-cache clear

# Clear a specific cached video by hash
space-recorder fal-cache clear abc123def456
```

### Configuration

Add fal.ai settings to your config file (`~/.config/space-recorder/config.toml`):

```toml
[fal]
enabled = true
opacity = 0.5

[fal.cache]
# Cache directory (default: ~/.cache/space-recorder/fal-videos/)
directory = "~/.cache/space-recorder/fal-videos"
# Maximum cache size in MB (default: 2048)
max_size_mb = 2048
```

## Specification Documents

### v1 - Core Functionality

| Document | Layer | Description |
|----------|-------|-------------|
| [00-core-pipeline.md](./v1/00-core-pipeline.md) | 0 | FFmpeg process management, video formats, output targets |
| [01-capture.md](./v1/01-capture.md) | 1 | Screen capture, webcam, window detection |
| [02-audio.md](./v1/02-audio.md) | 2 | Microphone, system audio, mixing |
| [03-compositor.md](./v1/03-compositor.md) | 3 | Layout engine, ghost overlay, positioning |
| [04-effects.md](./v1/04-effects.md) | 4 | Color grading, text overlays, filters |
| [05-config.md](./v1/05-config.md) | 5 | TOML schema, profiles, presets |
| [06-cli.md](./v1/06-cli.md) | 6 | Commands, arguments, interactive mode |

### v2 - Future Enhancements

| Document | Description |
|----------|-------------|
| [00-fal-video-overlay.md](./v2/00-fal-video-overlay.md) | AI-generated video overlay via fal.ai text-to-video |

## Key FFmpeg Filter for Ghost Overlay

```bash
# Ghost overlay using alpha channel manipulation
[0:v]scale=1280:720[terminal];
[1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
[terminal][ghost]overlay=0:0:format=auto[out]
```

The `colorchannelmixer=aa=0.3` sets webcam alpha to 30% for the ghost effect.
