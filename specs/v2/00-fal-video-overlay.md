# 00 - fal.ai Video Overlay

Generate and stream AI videos as an additional overlay layer using fal.ai's text-to-video API.

## Overview

This feature adds a new compositing layer that displays AI-generated videos from fal.ai. Videos are generated from text prompts entered during the stream, blend at configurable opacity (like the webcam ghost), and loop until replaced by a new generation.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Composited Output                         │
├─────────────────────────────────────────────────────────────┤
│  Layer 3: AI Video Overlay (fal.ai generated, looping)      │
│  Layer 2: Webcam Ghost (existing)                           │
│  Layer 1: Terminal/Screen Capture (existing)                │
└─────────────────────────────────────────────────────────────┘
```

## Components

### FalClient

Handles communication with fal.ai API.

```rust
pub struct FalClient {
    api_key: String,
    http_client: reqwest::Client,
}

impl FalClient {
    /// Generate video from text prompt
    /// Returns path to downloaded video file
    pub async fn generate_video(&self, prompt: &str) -> Result<PathBuf>;

    /// Check generation status
    pub async fn poll_status(&self, request_id: &str) -> Result<GenerationStatus>;
}
```

### VideoCache

Persistent disk cache for generated videos.

```rust
pub struct VideoCache {
    cache_dir: PathBuf,  // ~/.cache/space-recorder/fal-videos/
}

impl VideoCache {
    /// Get cached video by prompt hash, if exists
    pub fn get(&self, prompt: &str) -> Option<PathBuf>;

    /// Store video with prompt hash
    pub fn store(&self, prompt: &str, video_path: &Path) -> Result<PathBuf>;

    /// Generate deterministic hash for prompt
    fn hash_prompt(prompt: &str) -> String;
}
```

### OverlayManager

Manages the AI video overlay layer and transitions.

```rust
pub struct OverlayManager {
    current_video: Option<PathBuf>,
    pending_video: Option<PathBuf>,
    opacity: f32,
    transition_state: TransitionState,
}

pub enum TransitionState {
    Idle,
    CrossfadeIn { progress: f32, duration_ms: u32 },
}

impl OverlayManager {
    /// Queue new video, triggers crossfade from current
    pub fn queue_video(&mut self, video_path: PathBuf);

    /// Get FFmpeg filter for current overlay state
    pub fn get_ffmpeg_filter(&self) -> String;

    /// Update transition progress (called per frame)
    pub fn tick(&mut self, delta_ms: u32);
}
```

### PromptInput

CLI prompt input handler during stream.

```rust
pub struct PromptInput {
    tx: mpsc::Sender<String>,
}

impl PromptInput {
    /// Start listening for prompt input on stdin
    /// Non-blocking, sends prompts to channel
    pub fn spawn_listener() -> (Self, mpsc::Receiver<String>);
}
```

## Configuration

### Environment Variable

```bash
export FAL_API_KEY="your-api-key-here"
```

### TOML Config (extends v1 05-config)

```toml
[fal]
enabled = true
default_model = "fal-ai/fast-svd-lcm"  # or other text-to-video model

[fal.overlay]
opacity = 0.3                    # Same as webcam ghost by default
crossfade_duration_ms = 500      # Transition duration
loop = true                      # Loop video until replaced

[fal.cache]
enabled = true
directory = "~/.cache/space-recorder/fal-videos"
max_size_mb = 1000               # Auto-cleanup when exceeded
```

## FFmpeg Filter Integration

The AI video becomes an additional input to the FFmpeg filter chain:

```bash
# Three inputs: terminal, webcam, ai_video
ffmpeg \
  -i terminal_capture \
  -i webcam \
  -i ai_video.mp4 \
  -filter_complex "
    [0:v]scale=1280:720[terminal];
    [1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
    [2:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3,loop=-1:size=9999[ai];
    [terminal][ghost]overlay=0:0[base];
    [base][ai]overlay=0:0[out]
  " \
  -map "[out]" ...
```

### Crossfade Filter

During transitions, blend between old and new AI videos:

```bash
# Crossfade between two AI videos over 0.5 seconds
[old_ai][new_ai]xfade=transition=fade:duration=0.5[ai_blended]
```

## Workflow

1. User starts stream with `space-recorder start ...`
2. Stream runs with terminal + webcam ghost (v1 behavior)
3. User types prompt in terminal: `> cyberpunk rain on neon streets`
4. System checks cache for prompt hash
   - Cache hit: Load video immediately
   - Cache miss: Call fal.ai API, show "generating..." in logs
5. When video ready, crossfade from current overlay (or fade in if none)
6. Video loops at configured opacity
7. User enters new prompt, repeat from step 4

## CLI Integration

### New Commands

```bash
# Enable fal.ai overlay during stream
space-recorder start --window "Terminal" --fal

# Set AI overlay opacity independently (optional)
space-recorder start --fal --fal-opacity 0.2

# Pre-generate videos before stream
space-recorder fal-generate "cyberpunk cityscape"
space-recorder fal-generate "abstract particles flowing"

# List cached videos
space-recorder fal-cache list

# Clear cache
space-recorder fal-cache clear
```

### Runtime Prompt Input

While streaming, type prompts directly:

```
[space-recorder running...]
> neon grid horizon
Generating video... (checking cache)
Cache miss, calling fal.ai...
Video ready, crossfading in...
> abstract smoke particles
Found in cache, crossfading in...
> /clear
AI overlay cleared.
> /opacity 0.5
AI overlay opacity set to 50%
```

## Error Handling

| Error | Behavior |
|-------|----------|
| No API key | Skip fal.ai features, warn on startup |
| API rate limit | Queue prompt, retry with backoff |
| Generation timeout | Log error, keep current overlay |
| Invalid prompt | Log warning, ignore |
| Network error | Retry 3x, then skip |

## Dependencies

```toml
[dependencies]
reqwest = { version = "0.11", features = ["json"] }
sha2 = "0.10"          # For prompt hashing
tokio = { version = "1", features = ["full"] }
```

## File Structure

```
src/
├── fal/
│   ├── mod.rs
│   ├── client.rs       # FalClient - API communication
│   ├── cache.rs        # VideoCache - disk caching
│   ├── overlay.rs      # OverlayManager - compositing
│   └── prompt.rs       # PromptInput - CLI input
```

## Future Considerations (v3+)

- Image-to-video: Animate screenshots or webcam frames
- Style transfer: Real-time AI filter on webcam
- Multiple AI layers at different positions
- WebSocket for faster prompt submission
- Local model support (when available)
