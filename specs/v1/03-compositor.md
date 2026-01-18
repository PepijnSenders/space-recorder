# 03: Compositor

**Layer 3** - Layout engine for combining video streams with ghost overlay and positioning.

## Overview

The compositor handles:
- Ghost overlay blending (webcam at low opacity over terminal)
- Layout definitions (ghost, PIP, side-by-side)
- Scaling and aspect ratio management
- Dynamic opacity adjustment via hotkeys
- Z-ordering of visual layers

## Dependencies

- **Layer 0**: Core Pipeline (FFmpeg filter graph execution)
- **Layer 1**: Capture (video stream inputs)

## Ghost Overlay Concept

The primary compositing mode is **ghost overlay** - webcam at low opacity covering the entire frame, with the terminal visible through.

```
┌─────────────────────────────────────────┐
│                                         │
│   Terminal.app (100% opacity)           │
│   ┌─────────────────────────────────┐   │
│   │ $ cargo build                   │   │
│   │ Compiling space-recorder v0.1   │   │
│   │ █                               │   │
│   └─────────────────────────────────┘   │
│                                         │
│   + Webcam Ghost (30% opacity)          │
│   Your face subtly visible              │
│                                         │
└─────────────────────────────────────────┘
```

## FFmpeg Filter Graph for Ghost Overlay

### Basic Ghost Blend

```bash
[0:v]scale=1280:720[terminal];
[1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
[terminal][ghost]overlay=0:0:format=auto[out]
```

### Breakdown

| Step | Filter | Purpose |
|------|--------|---------|
| 1 | `scale=1280:720` | Normalize terminal to output resolution |
| 2 | `scale=1280:720` | Scale webcam to same size |
| 3 | `format=rgba` | Convert to RGBA for alpha manipulation |
| 4 | `colorchannelmixer=aa=0.3` | Set alpha to 30% |
| 5 | `overlay=0:0` | Place ghost over terminal |

### Alternative: Blend Filter

For different blending modes:

```bash
[0:v]scale=1280:720[terminal];
[1:v]scale=1280:720[webcam];
[terminal][webcam]blend=all_mode=overlay:all_opacity=0.3[out]
```

Blend modes available: `normal`, `overlay`, `screen`, `multiply`, `softlight`, `hardlight`

## Layout Modes

### Mode 1: Ghost (Default)

Full-screen webcam at low opacity over terminal.

```
Terminal: 100% size, 100% opacity (base layer)
Webcam:   100% size, 20-50% opacity (overlay)
```

### Mode 2: Corner PIP (Alternative)

Traditional picture-in-picture, webcam in corner.

```bash
[0:v]scale=1280:720[terminal];
[1:v]scale=320:180[pip];
[terminal][pip]overlay=W-w-10:H-h-10[out]
```

### Mode 3: Side-by-Side (Alternative)

```bash
[0:v]scale=640:720[terminal];
[1:v]scale=640:720[webcam];
[terminal][webcam]hstack[out]
```

## Opacity Control

### The Key Feature

User adjusts ghost opacity during stream using hotkeys:
- `+` or `=`: Increase opacity
- `-` or `_`: Decrease opacity

### Opacity Range

| Level | Alpha Value | Use Case |
|-------|-------------|----------|
| Invisible | 0.0 | Terminal only |
| Subtle | 0.15-0.25 | Default coding mode |
| Moderate | 0.30-0.40 | Speaking to camera |
| Strong | 0.50-0.70 | Emphasis on presenter |
| Full | 1.0 | Webcam only |

### Dynamic Opacity Update

MVP: Restart pipeline with new opacity value.
Post-MVP: Hot-reload filter graph without restart.

## Scaling and Aspect Ratio

### Resolution Normalization

All inputs scaled to output resolution before compositing:

```bash
[0:v]scale=1280:720:force_original_aspect_ratio=decrease,
     pad=1280:720:(ow-iw)/2:(oh-ih)/2[terminal];
```

### Crop vs Pad

- **Crop**: Cut edges to fit
- **Pad**: Add black bars

For ghost overlay, cropping webcam is often preferred.

## Z-Ordering

Layers composited bottom to top:

```
Layer 0: Background (black or terminal)
Layer 1: Terminal window
Layer 2: Webcam ghost
Layer 3: Effects (color grading)
Layer 4: Overlays (LIVE badge, timestamp)
```

## Acceptance Criteria

### MVP Requirements

1. [ ] Ghost overlay compositing at configurable opacity
2. [ ] Scale inputs to match output resolution
3. [ ] Opacity adjustable via hotkey (restart pipeline OK)
4. [ ] Output 720p at 30fps

### Post-MVP Requirements

1. [ ] Hot-swap filter graph without restart
2. [ ] PIP layout mode
3. [ ] Side-by-side layout mode
4. [ ] Custom blend modes
5. [ ] Animated transitions between layouts

## Example Filter Graphs

### MVP Ghost Overlay

```bash
-filter_complex "
  [0:v]scale=1280:720[terminal];
  [1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
  [terminal][ghost]overlay=0:0:format=auto[out]
"
```

### Ghost with Cyberpunk Color

```bash
-filter_complex "
  [0:v]scale=1280:720[terminal];
  [1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
  [terminal][ghost]overlay=0:0:format=auto,
  curves=r='0/0 0.5/0.4 1/1':g='0/0 0.5/0.6 1/1':b='0/0 0.5/0.7 1/1',
  eq=saturation=1.3[out]
"
```

### Corner PIP

```bash
-filter_complex "
  [0:v]scale=1280:720[terminal];
  [1:v]scale=320:180[pip];
  [terminal][pip]overlay=W-w-10:H-h-10[out]
"
```
