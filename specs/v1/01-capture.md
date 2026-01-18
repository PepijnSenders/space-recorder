# 01: Capture

**Layer 1** - Video capture from screens, windows, and webcams.

## Overview

The capture layer handles:
- Screen capture (full screen or region)
- Window-specific capture via bounds detection
- Webcam capture
- Device enumeration and selection
- Multi-monitor support

## Dependencies

- **Layer 0**: Core Pipeline (for FFmpeg command generation)

## macOS Capture Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Capture Sources                           │
├──────────────────────┬──────────────────────┬───────────────────┤
│    Screen Capture    │   Window Capture     │  Webcam Capture   │
│  (AVFoundation)      │  (Screen + Crop)     │  (AVFoundation)   │
├──────────────────────┼──────────────────────┼───────────────────┤
│  -f avfoundation     │  AppleScript for     │  -f avfoundation  │
│  -i "screen_index"   │  window bounds       │  -i "camera_idx"  │
│                      │  + crop filter       │                   │
└──────────────────────┴──────────────────────┴───────────────────┘
```

## Device Enumeration

### FFmpeg Device List

```bash
ffmpeg -f avfoundation -list_devices true -i ""
```

Example output:
```
[AVFoundation indev @ 0x...] AVFoundation video devices:
[AVFoundation indev @ 0x...] [0] FaceTime HD Camera
[AVFoundation indev @ 0x...] [1] Capture screen 0
[AVFoundation indev @ 0x...] [2] Capture screen 1
[AVFoundation indev @ 0x...] AVFoundation audio devices:
[AVFoundation indev @ 0x...] [0] MacBook Pro Microphone
[AVFoundation indev @ 0x...] [1] BlackHole 2ch
```

### Device Selection

| Device Type | Index Pattern | Example |
|-------------|---------------|---------|
| Webcam | 0, or by name | `"FaceTime HD Camera"` |
| Screen | 1+ (after cameras) | `"Capture screen 0"` |
| Audio | Separate index | `"0"` for mic |

### Combined Video + Audio Input

```bash
# Video device : Audio device
-i "1:0"  # Screen 0 with Microphone
-i "0"    # Webcam only (no audio)
```

## Screen Capture

### Full Screen Capture

```bash
ffmpeg -f avfoundation \
  -framerate 30 \
  -capture_cursor 1 \
  -i "Capture screen 0" \
  ...
```

### Options

| Option | Value | Description |
|--------|-------|-------------|
| `-framerate` | 30 | Capture frame rate |
| `-capture_cursor` | 1 | Include mouse cursor |
| `-video_size` | 1920x1080 | Force resolution (optional) |

### Multi-Monitor

Screens are indexed as separate devices:
- `Capture screen 0` - Primary display
- `Capture screen 1` - Secondary display

## Window Capture

macOS doesn't have native window capture in FFmpeg. We use:
1. Full screen capture
2. AppleScript to get window bounds
3. FFmpeg crop filter

### AppleScript for Window Bounds

```applescript
tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set appName to name of frontApp
end tell

tell application appName
    set winBounds to bounds of front window
    -- Returns: {x, y, x2, y2}
end tell
```

### Window Bounds to Crop Filter

```bash
# Given bounds (100, 50, 900, 650) -> width=800, height=600, x=100, y=50
-vf "crop=800:600:100:50"
```

### Handling Retina Displays

Retina displays report logical pixels, but capture is in physical pixels.
Scale factor is typically 2x on Retina displays.

Get scale factor:
```bash
system_profiler SPDisplaysDataType | grep "Resolution"
```

## Webcam Capture

### Basic Webcam Input

```bash
ffmpeg -f avfoundation \
  -framerate 30 \
  -video_size 1280x720 \
  -i "FaceTime HD Camera" \
  ...
```

### Webcam Options

| Option | Recommended | Notes |
|--------|-------------|-------|
| `-framerate` | 30 | Match output framerate |
| `-video_size` | 1280x720 | Request specific resolution |
| `-pixel_format` | uyvy422 | Usually auto-detected |

### Mirror/Flip Webcam

```bash
# Horizontal flip (mirror)
-vf "hflip"
```

## Window Detection Strategies

### Strategy 1: By Application Name (MVP)

User specifies app name, we find its front window:

```bash
space-recorder start --window "Terminal"
```

AppleScript finds Terminal.app's front window bounds.

### Strategy 2: Interactive Selection (Post-MVP)

Show clickable overlay to select window.

### Strategy 3: By Window Title

```applescript
tell application "System Events"
    tell process "Terminal"
        set w to first window whose name contains "bash"
        set b to position of w & size of w
    end tell
end tell
```

## Permissions

### Required macOS Permissions

1. **Screen Recording**: System Preferences > Privacy > Screen Recording
2. **Camera**: System Preferences > Privacy > Camera
3. **Microphone**: System Preferences > Privacy > Microphone
4. **Accessibility**: For AppleScript window detection

## Acceptance Criteria

### MVP Requirements

1. [ ] List available video devices (screens + webcams)
2. [ ] Capture full screen at 30fps
3. [ ] Capture webcam at 720p 30fps
4. [ ] Get window bounds for named application via AppleScript
5. [ ] Generate crop filter for window capture

### Post-MVP Requirements

1. [ ] Interactive window selection
2. [ ] Track window movement and update crop
3. [ ] Handle Retina display scaling
4. [ ] Multi-window support

## Example Commands

### List Devices

```bash
ffmpeg -f avfoundation -list_devices true -i ""
```

### Capture Screen + Webcam

```bash
ffmpeg \
  -f avfoundation -framerate 30 -capture_cursor 1 -i "1:0" \
  -f avfoundation -framerate 30 -video_size 1280x720 -i "0" \
  ...
```

### Capture Specific Window

```bash
# First, get bounds via AppleScript:
osascript -e 'tell app "Terminal" to get bounds of front window'
# Returns: 100, 50, 900, 650

# Then capture with crop:
ffmpeg \
  -f avfoundation -framerate 30 -i "1:0" \
  -vf "crop=800:600:100:50" \
  ...
```
