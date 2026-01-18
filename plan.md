# space-recorder Implementation Plan

## Overview

MVP implementation plan for space-recorder - a Rust CLI tool for compositing terminal + webcam with effects for coding streams.

**Goal**: Working tool in 3 days that can be used in actual Google Meet calls.

---

## Phase 1: Core Pipeline (Day 1)

### 1.1 Project Setup

- [x] **Initialize Rust project**
  - AC: `cargo new space-recorder` creates valid project
  - AC: `cargo build` succeeds with no errors
  - AC: `cargo run -- --help` shows placeholder help text

- [x] **Add core dependencies to Cargo.toml**
  - AC: `clap` added for CLI parsing
  - AC: `serde` + `toml` added for config
  - AC: `thiserror` added for error handling
  - AC: `cargo build` succeeds with all dependencies

### 1.2 Device Enumeration

- [x] **Implement `list-devices` command**
  - AC: Runs `ffmpeg -f avfoundation -list_devices true -i ""`
  - AC: Parses FFmpeg stderr output to extract device names
  - AC: Displays video devices (cameras + screens) with indices
  - AC: Displays audio devices with indices
  - AC: `--video` flag filters to show only video devices
  - AC: `--audio` flag filters to show only audio devices
  - AC: Handles "ffmpeg not found" error gracefully with install instructions

### 1.3 Basic FFmpeg Pipeline

- [x] **Implement FFmpeg process spawning**
  - AC: Can spawn FFmpeg as subprocess
  - AC: Captures stderr for logging/errors
  - AC: Handles process exit codes
  - AC: SIGINT (Ctrl+C) terminates FFmpeg cleanly

- [x] **Implement screen capture input**
  - AC: Captures from screen index (default 0)
  - AC: `--screen` flag selects which screen to capture (by index)
  - AC: Captures at 30fps
  - AC: Includes mouse cursor (`-capture_cursor 1`)

- [x] **Implement webcam capture input**
  - AC: Captures from webcam device (by name or index)
  - AC: Captures at 720p 30fps
  - AC: `--webcam` flag selects specific webcam device (by name or index, auto-detects if not specified)
  - AC: `--no-webcam` flag disables webcam input
  - AC: `--mirror` flag enables horizontal flip for webcam (hflip filter)

- [x] **Implement audio capture**
  - AC: Captures from default microphone
  - AC: Audio is muxed into output stream
  - AC: Audio syncs with video (< 100ms drift)
  - AC: Basic volume control via `--volume` flag (0.0-2.0 range, default 1.0)

### 1.4 Basic Compositing

- [x] **Implement ghost overlay filter graph**
  - AC: Terminal scaled to 1280x720
  - AC: Webcam scaled to 1280x720
  - AC: Webcam converted to RGBA with alpha channel
  - AC: Default opacity is 0.3 (30%)
  - AC: `--opacity` flag accepts 0.0-1.0 value

- [x] **Pipe output to mpv preview**
  - AC: Output encoded as NUT format to stdout
  - AC: Pipes to `mpv --no-cache --untimed --no-terminal -`
  - AC: Preview window opens and displays composited video
  - AC: Latency from capture to preview < 200ms

### 1.5 Day 1 Milestone Verification

- [x] **End-to-end test: basic streaming**
  - AC: `space-recorder start` opens preview window
  - AC: Screen capture visible as base layer
  - AC: Webcam visible as ghost overlay at 30% opacity
  - AC: Audio from mic is present in preview
  - AC: Ctrl+C stops cleanly without zombie processes

---

## Phase 2: Effects & Controls (Day 2)

### 2.1 Opacity Hotkey Control

- [x] **Implement keyboard listener**
  - AC: Global hotkey capture works (using `rdev` or similar)
  - AC: `+` or `=` increases opacity by 0.1
  - AC: `-` decreases opacity by 0.1
  - AC: Opacity clamped to 0.0-1.0 range

- [x] **Implement pipeline restart on opacity change**
  - AC: FFmpeg process killed cleanly
  - AC: New process spawned with updated opacity
  - AC: Restart completes in < 500ms
  - AC: No audio/video artifacts during restart

### 2.2 Per-Stream Video Effects

- [x] **Implement webcam-specific effects**
  - AC: Color grading applied to webcam stream only
  - AC: Terminal stream remains unmodified (readable)
  - AC: Effects applied before alpha/ghost blend

- [x] **Implement cyberpunk color preset**
  - AC: Blue/magenta color shift on webcam
  - AC: Increased saturation (1.3-1.4x)
  - AC: Slight contrast boost
  - AC: `--effect cyberpunk` enables preset

- [x] **Implement dark_mode color preset**
  - AC: Subtle brightness/contrast adjustment
  - AC: Preserves terminal readability
  - AC: `--effect dark_mode` enables preset

- [x] **Implement --no-effects flag**
  - AC: Disables all color grading
  - AC: Ghost overlay still works (just no color effects)

- [x] **Implement vignette effect**
  - AC: Subtle darkening around frame edges
  - AC: Uses FFmpeg `vignette=PI/5` filter
  - AC: Applied after color grading, before text overlays
  - AC: `--vignette` flag enables it (default: on with effects)

- [x] **Implement film grain effect**
  - AC: Adds subtle noise texture to video
  - AC: Uses FFmpeg `noise=alls=10:allf=t` filter
  - AC: `--grain` flag enables it (default: off)
  - AC: Applied after color grading, before text overlays

### 2.3 Audio Effects

- [x] **Implement noise gate filter**
  - AC: Reduces background noise when not speaking
  - AC: Threshold configurable (default 0.01)
  - AC: Attack/release times reasonable (20ms/250ms)

- [x] **Implement basic compressor**
  - AC: Evens out volume levels
  - AC: Prevents clipping on loud sounds
  - AC: Optional via config (default: off for MVP)

### 2.4 Text Overlays

- [x] **Implement LIVE badge**
  - AC: Red badge with white "LIVE" text
  - AC: Positioned top-left (20px margin)
  - AC: Semi-transparent background (0.8 alpha)
  - AC: Uses system font (Helvetica)

- [x] **Implement timestamp overlay**
  - AC: Shows current time HH:MM:SS
  - AC: Positioned top-right (20px margin)
  - AC: Updates in real-time
  - AC: Semi-transparent (0.7-0.8 alpha)

- [x] **Implement overlay toggles**
  - AC: `--no-live-badge` hides LIVE badge
  - AC: `--no-timestamp` hides timestamp

### 2.5 Day 2 Milestone Verification

- [x] **End-to-end test: effects and controls**
  - AC: Pressing +/- adjusts ghost opacity visibly
  - AC: Cyberpunk effect makes webcam look neon/cool
  - AC: Terminal text remains readable
  - AC: LIVE badge visible in top-left
  - AC: Timestamp updates every second
  - AC: Noise gate reduces keyboard/background noise

---

## Phase 3: Window Capture & Polish (Day 3)

### 3.1 Window Detection

- [x] **Implement AppleScript window bounds detection**
  - AC: Given app name, returns window bounds (x, y, width, height)
  - AC: Handles app not running (error message)
  - AC: Handles app with no windows (error message)
  - AC: Works with Terminal.app
  - AC: Works with common apps (VSCode, iTerm2, Safari)

- [x] **Implement crop filter for window capture**
  - AC: Crop filter generated from window bounds
  - AC: Handles Retina scaling (2x multiplier)
  - AC: Window content captured correctly (not offset)

- [x] **Implement `--window` flag**
  - AC: `--window Terminal` captures Terminal.app window
  - AC: Overrides full-screen capture
  - AC: Error message if window not found

### 3.2 Permissions Check

- [x] **Implement macOS permissions verification**
  - AC: Check Screen Recording permission before capture starts
  - AC: Check Camera permission if webcam enabled
  - AC: Check Microphone permission if audio enabled
  - AC: Check Accessibility permission for AppleScript window detection
  - AC: Display clear error with System Preferences path if permission missing

### 3.3 CLI Polish

- [x] **Implement proper help text**
  - AC: `--help` shows all commands and options
  - AC: Each option has description
  - AC: Examples section included
  - AC: Version shown with `--version`

- [x] **Implement status display**
  - AC: Shows current settings on start (window, opacity, effect)
  - AC: Shows hotkey hints
  - AC: Shows "Streaming..." status

- [x] **Implement error messages with suggestions**
  - AC: "FFmpeg not found" suggests `brew install ffmpeg`
  - AC: "Permission denied" suggests System Preferences path
  - AC: "Window not found" lists available windows
  - AC: "Device not found" suggests `list-devices` command

### 3.4 Recording Output

- [x] **Implement `--output` flag for recording**
  - AC: `--output recording.mp4` saves to file
  - AC: Uses H.264 codec with reasonable quality (CRF 23)
  - AC: AAC audio at 128kbps
  - AC: Can record while also showing preview (both outputs)

- [x] **Implement `--resolution` and `--framerate` flags**
  - AC: `--resolution 1920x1080` or `-r 1920x1080` sets output resolution
  - AC: `--framerate 60` or `-f 60` sets output framerate
  - AC: Defaults to 1280x720 @ 30fps if not specified
  - AC: Validates resolution format (WIDTHxHEIGHT)
  - AC: Validates framerate is reasonable (1-120 fps)

### 3.5 Basic Config File

- [x] **Implement config file loading**
  - AC: Reads from `~/.config/space-recorder/config.toml`
  - AC: Falls back to defaults if file missing
  - AC: CLI args override config file values

- [x] **Implement `--config` flag for custom config path**
  - AC: `--config ~/myconfig.toml` or `-c ~/myconfig.toml` loads from specified path
  - AC: Error message if specified config file doesn't exist
  - AC: Works in combination with other CLI flags (CLI still overrides config)

- [x] **Implement minimal config schema**
  - AC: `opacity` setting works
  - AC: `effect` setting works
  - AC: `window` setting works
  - AC: Invalid config shows helpful error

### 3.6 Day 3 Milestone Verification

- [x] **End-to-end test: complete MVP**
  - AC: `space-recorder start --window Terminal` captures just Terminal
  - AC: Window bounds detected correctly via AppleScript
  - AC: All effects working (cyberpunk, overlays)
  - AC: Opacity hotkeys working
  - AC: Can record to file with `--output`
  - AC: Preview window shareable in Google Meet
  - AC: Audio quality acceptable for calls

---

## v2: fal.ai Video Overlay

> **Spec**: [specs/v2/00-fal-video-overlay.md](./specs/v2/00-fal-video-overlay.md)

AI-generated video overlay via fal.ai text-to-video API. Type prompts during stream, videos crossfade in as full-screen ghost layers.

### v2.1 Project Setup

- [x] **Add v2 dependencies to Cargo.toml**
  - AC: `reqwest` added with `json` feature for HTTP/API calls
  - AC: `sha2` added for prompt hashing
  - AC: `tokio` updated with `full` features for async runtime
  - AC: `dotenv` added for .env file loading
  - AC: `cargo build` succeeds with all dependencies

- [x] **Create fal module structure**
  - AC: `src/fal/mod.rs` created with module exports
  - AC: `src/fal/client.rs` created (empty struct)
  - AC: `src/fal/cache.rs` created (empty struct)
  - AC: `src/fal/overlay.rs` created (empty struct)
  - AC: `src/fal/prompt.rs` created (empty struct)
  - AC: Module compiles without errors

- [x] **Implement .env file loading**
  - AC: Loads `.env` file from project root on startup
  - AC: `FAL_API_KEY` environment variable accessible
  - AC: Warning logged if FAL_API_KEY not set (but app continues)
  - AC: Works with existing env vars (doesn't override)

### v2.2 FalClient - API Communication

- [x] **Implement FalClient struct and constructor**
  - AC: `FalClient::new()` reads API key from environment
  - AC: Returns error if API key missing and fal features requested
  - AC: Creates reqwest HTTP client with reasonable timeouts
  - AC: Stores base URL for fal.ai API

- [x] **Implement video generation request**
  - AC: `generate_video(prompt: &str)` sends POST to fal.ai
  - AC: Uses correct model endpoint (e.g., `fal-ai/fast-svd-lcm`)
  - AC: Request includes prompt and video parameters
  - AC: Returns request ID for polling
  - AC: Handles API authentication via header

- [x] **Implement generation status polling**
  - AC: `poll_status(request_id: &str)` checks generation status
  - AC: Returns `GenerationStatus` enum (Pending, Processing, Completed, Failed)
  - AC: Includes video URL when completed
  - AC: Includes error message when failed
  - AC: Implements exponential backoff between polls

- [x] **Implement video download**
  - AC: `download_video(url: &str, dest: &Path)` downloads video file
  - AC: Streams download to disk (doesn't load full video into memory)
  - AC: Returns path to downloaded file
  - AC: Handles download errors gracefully

- [x] **Implement end-to-end generate flow**
  - AC: `FalClient::generate_and_download(prompt)` combines all steps
  - AC: Submits request, polls until complete, downloads video
  - AC: Returns `Result<PathBuf>` with local video path
  - AC: Timeout after configurable duration (default 120s)
  - AC: Logs progress (generating, downloading, complete)

### v2.3 VideoCache - Persistent Disk Cache

- [x] **Implement VideoCache struct and initialization**
  - AC: Default cache dir is `~/.cache/space-recorder/fal-videos/`
  - AC: Creates cache directory if doesn't exist
  - AC: Configurable cache directory via config

- [x] **Implement prompt hashing**
  - AC: `hash_prompt(prompt: &str)` returns deterministic SHA256 hash
  - AC: Same prompt always produces same hash
  - AC: Hash is filesystem-safe (hex string)

- [x] **Implement cache lookup**
  - AC: `get(prompt: &str)` returns `Option<PathBuf>`
  - AC: Returns `Some(path)` if video exists for prompt hash
  - AC: Returns `None` if not cached
  - AC: Verifies file exists before returning (handles deleted files)

- [x] **Implement cache storage**
  - AC: `store(prompt: &str, video_path: &Path)` copies video to cache
  - AC: Uses prompt hash as filename
  - AC: Returns path to cached file
  - AC: Overwrites existing file if same prompt

- [x] **Implement cache size management**
  - AC: `cleanup_if_needed(max_size_mb: u64)` removes old files
  - AC: Deletes oldest files first (by modification time)
  - AC: Runs automatically when storing new video
  - AC: Configurable max size (default 1GB)

- [x] **Implement cache CLI commands**
  - AC: `fal-cache list` shows cached videos with prompts and sizes
  - AC: `fal-cache clear` removes all cached videos
  - AC: `fal-cache clear <hash>` removes specific cached video
  - AC: Shows total cache size in list output

### v2.4 OverlayManager - Video Compositing

- [x] **Implement OverlayManager struct**
  - AC: Tracks `current_video: Option<PathBuf>`
  - AC: Tracks `pending_video: Option<PathBuf>`
  - AC: Tracks `opacity: f32` (0.0-1.0)
  - AC: Tracks `transition_state: TransitionState`

- [x] **Implement TransitionState enum**
  - AC: `Idle` - no transition in progress
  - AC: `CrossfadeIn { progress: f32, duration_ms: u32 }` - fading in new video
  - AC: Progress ranges from 0.0 to 1.0

- [x] **Implement video queueing**
  - AC: `queue_video(path: PathBuf)` sets pending video
  - AC: Starts crossfade transition if current video exists
  - AC: Directly sets current if no existing video
  - AC: Logs transition start

- [x] **Implement transition tick**
  - AC: `tick(delta_ms: u32)` updates transition progress
  - AC: Increments progress based on elapsed time and duration
  - AC: Sets state to Idle when progress reaches 1.0
  - AC: Swaps pending to current when transition completes

- [x] **Implement FFmpeg filter generation**
  - AC: `get_ffmpeg_filter()` returns filter string for AI overlay
  - AC: Handles no video (returns empty filter)
  - AC: Handles single video (scale + alpha + overlay)
  - AC: Handles crossfade (xfade between two videos)
  - AC: Applies configured opacity via colorchannelmixer

- [x] **Implement clear overlay**
  - AC: `clear()` removes current and pending videos
  - AC: Triggers fade-out transition (opacity to 0)
  - AC: Resets state to Idle after fade

### v2.5 PromptInput - CLI Input Handler

- [x] **Implement PromptInput struct**
  - AC: Contains `mpsc::Sender<PromptCommand>` for communication
  - AC: `PromptCommand` enum: `Generate(String)`, `Clear`, `SetOpacity(f32)`

- [x] **Implement stdin listener**
  - AC: `spawn_listener()` returns `(PromptInput, Receiver<PromptCommand>)`
  - AC: Spawns background thread reading stdin
  - AC: Non-blocking, doesn't interfere with main loop
  - AC: Sends commands through channel

- [x] **Implement prompt parsing**
  - AC: Regular text treated as `Generate(text)` command
  - AC: `/clear` parsed as `Clear` command
  - AC: `/opacity 0.5` parsed as `SetOpacity(0.5)` command
  - AC: Empty input ignored
  - AC: Trims whitespace from prompts

- [x] **Implement input prompt display**
  - AC: Shows `> ` prompt when waiting for input
  - AC: Displays status messages (generating, cached, ready)
  - AC: Doesn't interfere with FFmpeg stderr logging

### v2.6 FFmpeg Pipeline Integration

- [x] **Update filter graph for three inputs**
  - AC: Pipeline accepts terminal, webcam, AND ai_video inputs
  - AC: AI video input is optional (pipeline works without it)
  - AC: Layer order: terminal → webcam ghost → AI overlay
  - AC: All layers scaled to output resolution

- [x] **Implement video looping filter**
  - AC: AI video loops indefinitely using `loop=-1:size=9999`
  - AC: Seamless loop (no visible jump)
  - AC: Loop resets when new video queued

- [x] **Implement dynamic video replacement**
  - AC: Can swap AI video input without full pipeline restart
  - AC: OR: Fast pipeline restart with new video (<500ms)
  - AC: No audio interruption during swap
  - AC: Handles video format differences (resolution, codec)

- [x] **Implement crossfade in FFmpeg**
  - AC: `xfade=transition=fade:duration=0.5` between videos
  - AC: Smooth transition over configured duration
  - AC: Falls back to cut if xfade not possible

### v2.7 CLI Integration

- [x] **Implement `--fal` flag**
  - AC: `space-recorder start --fal` enables fal.ai overlay mode
  - AC: Starts prompt input listener
  - AC: Logs instructions for entering prompts
  - AC: Errors if FAL_API_KEY not set

- [x] **Implement `--fal-opacity` flag**
  - AC: `--fal-opacity 0.2` sets AI overlay opacity
  - AC: Defaults to same as webcam opacity if not specified
  - AC: Validates range 0.0-1.0
  - AC: Can be changed during stream via `/opacity` command

- [x] **Implement `fal-generate` command**
  - AC: `space-recorder fal-generate "prompt"` pre-generates video
  - AC: Downloads and caches video
  - AC: Shows progress (generating, downloading)
  - AC: Useful for pre-warming cache before stream

- [x] **Implement `fal-generate --batch` flag**
  - AC: `space-recorder fal-generate --batch prompts.txt` generates multiple
  - AC: Reads prompts from file (one per line)
  - AC: Generates sequentially, caches all
  - AC: Shows progress for each prompt

### v2.8 Configuration

- [x] **Add fal section to TOML config schema**
  - AC: `[fal]` section recognized in config
  - AC: `enabled = true/false` controls feature
  - AC: `default_model` sets fal.ai model to use
  - AC: Invalid config shows helpful error

- [x] **Add fal.overlay config options**
  - AC: `[fal.overlay]` section for overlay settings
  - AC: `opacity` setting (0.0-1.0)
  - AC: `crossfade_duration_ms` setting (default 500)
  - AC: `loop` setting (default true)

- [x] **Add fal.cache config options**
  - AC: `[fal.cache]` section for cache settings
  - AC: `enabled` setting (default true)
  - AC: `directory` setting (default ~/.cache/space-recorder/fal-videos)
  - AC: `max_size_mb` setting (default 1000)

### v2.9 Error Handling

- [x] **Implement API key missing error**
  - AC: Clear error message when FAL_API_KEY not set
  - AC: Suggests adding to .env file
  - AC: fal features disabled but app continues

- [x] **Implement rate limit handling**
  - AC: Detects 429 rate limit response
  - AC: Queues prompt for retry
  - AC: Implements exponential backoff
  - AC: Logs rate limit status to user

- [x] **Implement generation timeout handling**
  - AC: Timeout after configurable duration (default 120s)
  - AC: Logs timeout error
  - AC: Keeps current overlay unchanged
  - AC: Allows retry with same prompt

- [x] **Implement network error handling**
  - AC: Retries on transient network errors (3x)
  - AC: Logs retry attempts
  - AC: Final failure keeps current overlay
  - AC: Clear error message to user

- [x] **Implement invalid prompt handling**
  - AC: Detects empty prompts (ignores)
  - AC: Handles API rejection (content policy, etc.)
  - AC: Logs warning with reason
  - AC: Continues accepting new prompts

### v2.10 Testing & Verification

- [x] **Unit tests for FalClient**
  - AC: Test API request formatting
  - AC: Test status parsing
  - AC: Test error handling
  - AC: Mock HTTP responses

- [x] **Unit tests for VideoCache**
  - AC: Test hash generation
  - AC: Test cache hit/miss
  - AC: Test cleanup logic
  - AC: Use temp directory

- [x] **Unit tests for OverlayManager**
  - AC: Test transition state machine
  - AC: Test filter generation
  - AC: Test video queueing

- [x] **Integration test: cache flow**
  - AC: Generate video, verify cached
  - AC: Request same prompt, verify cache hit
  - AC: Clear cache, verify cache miss

- [x] **End-to-end test: fal overlay streaming**
  - AC: Start stream with `--fal` flag
  - AC: Enter prompt, video generates and appears
  - AC: Enter new prompt, crossfade occurs
  - AC: `/clear` removes overlay
  - AC: `/opacity 0.5` changes opacity
  - AC: Cache persists across restarts

### v2.11 Documentation

- [x] **Update README with fal.ai section**
  - AC: Document FAL_API_KEY setup
  - AC: Document `--fal` usage
  - AC: Document prompt commands
  - AC: Document cache management

- [x] **Update --help with fal options**
  - AC: `--fal` documented in help
  - AC: `--fal-opacity` documented
  - AC: `fal-generate` command documented
  - AC: `fal-cache` command documented

---

## Final Verification

- [ ] **Real-world test: Google Meet call (v1 + v2)**
  - AC: Share mpv preview window in Google Meet
  - AC: Other participants see composited stream
  - AC: Audio from mic comes through clearly
  - AC: Ghost overlay visible but not distracting
  - AC: Effects enhance visual without hurting readability
  - AC: No significant lag or sync issues
  - AC: Can adjust opacity during call
  - AC: fal.ai overlay works when enabled
  - AC: Prompt input generates and displays AI video
  - AC: Crossfade transitions work smoothly

---

## Backlog (Future)

These items are deferred until after v2 is complete:

### v1 Enhancements
- [ ] `init` command to generate default config file
- [ ] Hot-reload filter graph without pipeline restart
- [ ] Profile system (`--profile coding`)
- [ ] Interactive TUI mode for positioning
- [ ] System audio capture (BlackHole integration)
- [ ] PIP layout mode (corner webcam)
- [ ] Side-by-side layout mode
- [ ] Glow/bloom visual effect
- [ ] Scan lines CRT effect
- [ ] Chromatic aberration effect
- [ ] Animated text overlays
- [ ] Virtual camera output
- [ ] Config hot-reload on file change
- [ ] Shell completion scripts
- [ ] Window movement tracking (auto-update crop)
- [ ] `--json` output flag for `list-devices` command

### v2 Enhancements
- [ ] Image-to-video: Animate screenshots or webcam frames
- [ ] Style transfer: Real-time AI filter on webcam
- [ ] Multiple AI layers at different positions
- [ ] WebSocket for faster prompt submission
- [ ] Local model support (when available)

---

## Quick Reference: Key Commands

```bash
# Development
cargo build
cargo run -- --help
cargo run -- list-devices
cargo run -- start --window Terminal --opacity 0.3 --effect cyberpunk

# Testing FFmpeg directly
ffmpeg -f avfoundation -list_devices true -i ""
ffmpeg -f avfoundation -framerate 30 -i "1:0" -f avfoundation -i "0" \
  -filter_complex "[0:v]scale=1280:720[t];[1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[g];[t][g]overlay[out]" \
  -map "[out]" -map 0:a -f nut pipe:1 | mpv --no-cache --untimed -
```

## Notes

- Each checkbox represents a discrete, testable piece of work
- ACs should be verifiable by running the tool or inspecting output
- Check off items as completed during implementation
- If an AC fails, fix before moving to next task
