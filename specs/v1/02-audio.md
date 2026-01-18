# 02: Audio

**Layer 2** - Audio capture, mixing, and processing.

## Overview

The audio layer handles:
- Microphone capture
- System audio capture (via loopback device)
- Audio mixing (mic + system)
- Basic audio processing (levels, noise gate)
- Audio synchronization with video

## Dependencies

- **Layer 0**: Core Pipeline (for FFmpeg audio encoding)

## Audio Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Audio Pipeline                            │
├───────────────────────┬─────────────────────────────────────────┤
│   Microphone Input    │     System Audio Input                  │
│   (AVFoundation)      │     (BlackHole/Loopback)               │
├───────────────────────┴─────────────────────────────────────────┤
│                      Audio Mixer                                 │
│              (FFmpeg amix filter)                               │
├─────────────────────────────────────────────────────────────────┤
│                    Audio Processing                              │
│        (noise gate, compressor, normalization)                  │
├─────────────────────────────────────────────────────────────────┤
│                     Audio Encoder                                │
│                  (AAC 128kbps)                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Microphone Capture

### AVFoundation Audio Input

```bash
ffmpeg -f avfoundation -i ":0" ...
# The ":" prefix indicates audio-only input
# Index 0 is typically the built-in microphone
```

### Combined with Video

```bash
# Video:Audio format
-i "1:0"  # Screen index 1, Audio index 0
```

### Audio Device Enumeration

```bash
ffmpeg -f avfoundation -list_devices true -i ""
```

## System Audio Capture

### The Problem

macOS doesn't allow direct system audio capture. Solutions:

1. **BlackHole** (recommended) - Virtual audio device
2. **Soundflower** - Older alternative
3. **Loopback** by Rogue Amoeba - Commercial solution

### BlackHole Setup

1. Install BlackHole: `brew install blackhole-2ch`
2. Create Multi-Output Device in Audio MIDI Setup
3. Capture from BlackHole in FFmpeg

### Capturing System Audio

```bash
ffmpeg -f avfoundation -i ":BlackHole 2ch" ...
```

## Audio Mixing

### Mixing Mic + System Audio

```bash
ffmpeg \
  -f avfoundation -i ":0" \           # Mic
  -f avfoundation -i ":BlackHole 2ch" \ # System
  -filter_complex "[0:a][1:a]amix=inputs=2:duration=first[aout]" \
  -map "[aout]" \
  ...
```

### Mix with Volume Control

```bash
-filter_complex "
  [0:a]volume=1.0[mic];
  [1:a]volume=0.5[sys];
  [mic][sys]amix=inputs=2:duration=first:dropout_transition=2[aout]
"
```

## Audio Processing

### Noise Gate

```bash
-af "agate=threshold=0.01:ratio=2:attack=20:release=250"
```

### Compressor

```bash
-af "acompressor=threshold=-20dB:ratio=4:attack=5:release=50"
```

### Normalization

```bash
-af "loudnorm=I=-16:TP=-1.5:LRA=11"
```

### Combined Processing Chain

```bash
-af "agate=threshold=0.01:ratio=2:attack=20:release=250,
     acompressor=threshold=-20dB:ratio=4:attack=5:release=50,
     loudnorm=I=-16:TP=-1.5:LRA=11"
```

## Audio Output

### AAC Encoding (for MP4/streaming)

```bash
-c:a aac -b:a 128k
```

### PCM for Preview (low latency)

```bash
-c:a pcm_s16le
```

## Acceptance Criteria

### MVP Requirements

1. [ ] Capture microphone audio
2. [ ] Mix audio into video output
3. [ ] Basic volume control
4. [ ] Audio syncs with video (within 50ms)

### Post-MVP Requirements

1. [ ] System audio capture via BlackHole
2. [ ] Noise gate filter
3. [ ] Compressor and normalization
4. [ ] Per-source volume control via hotkeys
5. [ ] Audio level meters in TUI

## Example Commands

### Mic Only

```bash
ffmpeg \
  -f avfoundation -framerate 30 -i "1:0" \
  -c:a aac -b:a 128k \
  ...
```

### Mic + System Audio

```bash
ffmpeg \
  -f avfoundation -framerate 30 -i "1:0" \
  -f avfoundation -i ":BlackHole 2ch" \
  -filter_complex "
    [0:a]volume=1.0[mic];
    [1:a]volume=0.5[sys];
    [mic][sys]amix=inputs=2:duration=first[aout]
  " \
  -map 0:v -map "[aout]" \
  ...
```
