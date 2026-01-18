# 00: Core Pipeline

**Layer 0** - Foundation layer for FFmpeg process management, video formats, and output handling.

## Overview

The core pipeline manages FFmpeg as a subprocess, handling:
- Process spawning and lifecycle
- Input/output pipe management
- Video format conversion
- Frame timing and synchronization
- Output routing (preview window, file, virtual camera)

## Dependencies

None - this is the foundation layer.

## FFmpeg Process Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     space-recorder process                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌──────────┐      ┌──────────┐      ┌──────────┐             │
│   │ Capture  │─────▶│ FFmpeg   │─────▶│  Output  │             │
│   │  Layer   │ pipe │ Process  │ pipe │  Target  │             │
│   └──────────┘      └──────────┘      └──────────┘             │
│                           │                                      │
│                           │ stderr                               │
│                           ▼                                      │
│                     ┌──────────┐                                │
│                     │  Logger  │                                │
│                     └──────────┘                                │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## FFmpeg Command Structure

### Basic Pipeline Command

```bash
ffmpeg \
  -f avfoundation -framerate 30 -i "1:0" \        # Screen capture + mic
  -f avfoundation -framerate 30 -i "0" \          # Webcam
  -filter_complex "[filter_graph]" \               # Compositing
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -c:a aac -b:a 128k \
  -f nut pipe:1                                    # Output to pipe
```

### Output to mpv Preview

```bash
ffmpeg [inputs] [filters] -f nut pipe:1 | mpv --no-cache --untimed -
```

### Output to File

```bash
ffmpeg [inputs] [filters] -c:v libx264 -crf 23 output.mp4
```

## Video Format Specifications

### Internal Processing Format

| Property | Value | Rationale |
|----------|-------|-----------|
| Resolution | 1280x720 (720p) | Good balance of quality and performance |
| Frame Rate | 30 fps | Standard for screen sharing |
| Pixel Format | yuv420p | Universal compatibility |
| Color Space | bt709 | Standard HD color space |

### Supported Input Formats

- **Screen capture**: uyvy422 (AVFoundation native)
- **Webcam**: yuyv422 or mjpeg (device dependent)
- **Files**: Any FFmpeg-supported format

### Output Formats

| Target | Container | Video Codec | Audio Codec |
|--------|-----------|-------------|-------------|
| Preview (mpv) | NUT | rawvideo | pcm_s16le |
| Recording | MP4 | H.264 | AAC |
| Stream | FLV | H.264 | AAC |

## Process Management

### Lifecycle States

```
┌─────────┐     start()     ┌─────────┐
│  Idle   │────────────────▶│ Running │
└─────────┘                 └────┬────┘
     ▲                           │
     │         stop()            │
     │◀──────────────────────────┤
     │                           │
     │         error             │
     │◀──────────────────────────┘
     │                      ┌─────────┐
     └──────────────────────│  Error  │
              restart()     └─────────┘
```

### Error Handling

1. **FFmpeg crash**: Capture exit code, log stderr, notify user
2. **Pipe broken**: Detect SIGPIPE, attempt restart
3. **Invalid input**: Validate before spawning, fail fast

### Graceful Shutdown

1. Send SIGINT to FFmpeg process
2. Wait up to 2 seconds for clean exit
3. Send SIGKILL if still running
4. Close all pipes
5. Clean up temp files

## Frame Timing

### Synchronization Strategy

- Use `-vsync cfr` for constant frame rate output
- Set explicit `-r 30` on both input and output
- Use `-async 1` for audio sync

### Latency Targets

| Metric | Target | Maximum |
|--------|--------|---------|
| Capture to preview | < 100ms | 200ms |
| End-to-end | < 150ms | 300ms |

### Reducing Latency

```bash
# Low-latency encoding settings
-preset ultrafast \
-tune zerolatency \
-g 30 \                    # Keyframe every 1 second
-bf 0 \                    # No B-frames
-flags +low_delay \
-fflags +nobuffer
```

## Pipe Management

### Input Pipes

For advanced use cases (not MVP), raw frames can be piped in:

```bash
ffmpeg -f rawvideo -pix_fmt rgb24 -s 1280x720 -r 30 -i pipe:0 ...
```

### Output Pipes

Standard output pipe to mpv:

```bash
ffmpeg ... -f nut pipe:1 | mpv --no-cache --untimed --no-terminal -
```

### Buffer Sizes

- Input buffer: 4 frames (133ms at 30fps)
- Output buffer: 2 frames (66ms at 30fps)
- Keep buffers small for low latency

## FFmpeg Filter Graph Basics

### Filter Graph Syntax

```
[input_label]filter=param=value[output_label];
[label1][label2]filter[output]
```

### Example: Simple Composite

```
[0:v]scale=1280:720[main];
[1:v]scale=320:180[pip];
[main][pip]overlay=W-w-10:H-h-10[out]
```

### Filter Graph for Ghost Overlay

```
# Terminal as base, webcam as ghost
[0:v]scale=1280:720[terminal];
[1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
[terminal][ghost]overlay=0:0:format=auto[out]
```

## Acceptance Criteria

### MVP Requirements

1. [ ] FFmpeg process spawns successfully with valid command
2. [ ] Output pipes to mpv and displays video
3. [ ] Process terminates cleanly on SIGINT
4. [ ] Errors are logged with meaningful messages
5. [ ] Latency under 200ms capture-to-preview

### Post-MVP Requirements

1. [ ] Hot-reload filter graph without restart
2. [ ] Simultaneous preview + recording
3. [ ] Virtual camera output
4. [ ] Process restart on crash

## Example FFmpeg Commands

### Full MVP Command

```bash
ffmpeg -hide_banner \
  -f avfoundation -framerate 30 -capture_cursor 1 -i "1:0" \
  -f avfoundation -framerate 30 -video_size 1280x720 -i "0" \
  -filter_complex "
    [0:v]scale=1280:720[terminal];
    [1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
    [terminal][ghost]overlay=0:0:format=auto[out]
  " \
  -map "[out]" -map 0:a \
  -c:v libx264 -preset ultrafast -tune zerolatency \
  -c:a aac -b:a 128k \
  -f nut pipe:1 \
| mpv --no-cache --untimed --no-terminal -
```

### List Available Devices

```bash
ffmpeg -f avfoundation -list_devices true -i ""
```
