# 04: Effects

**Layer 4** - Visual effects, color grading, and text overlays.

## Overview

The effects layer handles:
- Color grading presets (cyberpunk, dark mode friendly)
- Text overlays (LIVE badge, timestamp)
- Visual filters (vignette, glow, scan lines)
- Custom filter support
- Effect chaining and ordering

## Dependencies

- **Layer 0**: Core Pipeline (FFmpeg filter execution)
- **Layer 3**: Compositor (effects applied after compositing)

## Effect Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                      Effects Pipeline                            │
├─────────────────────────────────────────────────────────────────┤
│  Input (from Compositor)                                         │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────┐                                            │
│  │  Color Grading  │  curves, eq, colorbalance                  │
│  └────────┬────────┘                                            │
│           ▼                                                      │
│  ┌─────────────────┐                                            │
│  │  Visual FX      │  vignette, glow, scan lines               │
│  └────────┬────────┘                                            │
│           ▼                                                      │
│  ┌─────────────────┐                                            │
│  │  Text Overlays  │  LIVE badge, timestamp                    │
│  └────────┬────────┘                                            │
│           ▼                                                      │
│  Output (to encoder)                                             │
└─────────────────────────────────────────────────────────────────┘
```

## Color Grading Presets

### Cyberpunk (Primary)

Neon-tinted, high contrast look ideal for dark terminals.

```bash
curves=r='0/0 0.25/0.2 0.5/0.45 0.75/0.8 1/1':
      g='0/0 0.25/0.25 0.5/0.5 0.75/0.75 1/1':
      b='0/0 0.25/0.3 0.5/0.6 0.75/0.85 1/1',
eq=saturation=1.4:contrast=1.1,
colorbalance=rs=0.1:gs=-0.05:bs=0.2:rm=0.1:gm=-0.1:bm=0.15
```

### Dark Mode Friendly

Preserves readability of dark terminal text.

```bash
eq=brightness=0.05:contrast=1.05:saturation=1.1,
unsharp=5:5:0.5:5:5:0
```

### Warm Vintage

```bash
curves=r='0/0 0.5/0.55 1/1':
      g='0/0 0.5/0.5 1/0.95':
      b='0/0 0.5/0.45 1/0.9',
eq=saturation=0.9
```

### High Contrast

```bash
eq=contrast=1.3:saturation=0.8,
curves=all='0/0 0.1/0 0.9/1 1/1'
```

## Visual Effects

### Vignette

```bash
vignette=PI/4
```

### Film Grain

```bash
noise=alls=10:allf=t
```

### Chromatic Aberration (Post-MVP)

RGB channel offset for glitch effect.

## Text Overlays

### LIVE Badge

```bash
drawtext=text='LIVE':
         fontfile=/System/Library/Fonts/Helvetica.ttc:
         fontsize=24:
         fontcolor=white:
         box=1:
         boxcolor=red@0.8:
         boxborderw=8:
         x=20:
         y=20
```

### Timestamp

```bash
drawtext=text='%{localtime\:%H\\\:%M\\\:%S}':
         fontfile=/System/Library/Fonts/Helvetica.ttc:
         fontsize=18:
         fontcolor=white@0.8:
         x=w-tw-20:
         y=20
```

## Effect Presets

### Preset: Coding Stream

```bash
-vf "
  curves=r='0/0 0.5/0.45 1/1':b='0/0 0.5/0.6 1/1',
  eq=saturation=1.3:contrast=1.05,
  vignette=PI/5,
  drawtext=text='LIVE':fontsize=20:fontcolor=white:box=1:boxcolor=red@0.8:boxborderw=6:x=15:y=15,
  drawtext=text='%{localtime\:%H\\\:%M}':fontsize=14:fontcolor=white@0.7:x=w-tw-15:y=15
"
```

### Preset: Clean Professional

```bash
-vf "
  eq=contrast=1.05:saturation=1.05,
  drawtext=text='%{localtime\:%H\\\:%M}':fontsize=12:fontcolor=white@0.5:x=w-tw-10:y=h-th-10
"
```

## Font Handling

### macOS System Fonts

| Font | Path |
|------|------|
| Helvetica | `/System/Library/Fonts/Helvetica.ttc` |
| SF Pro | `/System/Library/Fonts/SFNS.ttf` |
| Menlo | `/System/Library/Fonts/Menlo.ttc` |
| Monaco | `/System/Library/Fonts/Monaco.ttf` |

## Acceptance Criteria

### MVP Requirements

1. [ ] Cyberpunk color grading preset
2. [ ] LIVE badge overlay
3. [ ] Timestamp overlay
4. [ ] At least one visual effect (vignette)
5. [ ] Effects chain combines correctly with compositor output

### Post-MVP Requirements

1. [ ] 5+ color grade presets
2. [ ] Glow/bloom effect
3. [ ] Scan lines effect
4. [ ] Custom filter string support
5. [ ] Effect intensity runtime adjustment
6. [ ] Animated text overlays

## Example Filter Chains

### MVP Chain

```bash
-filter_complex "
  [0:v]scale=1280:720[terminal];
  [1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=0.3[ghost];
  [terminal][ghost]overlay=0:0:format=auto,
  curves=r='0/0 0.5/0.45 1/1':b='0/0 0.5/0.6 1/1',
  eq=saturation=1.3,
  vignette=PI/5,
  drawtext=text='LIVE':fontsize=20:fontcolor=white:box=1:boxcolor=red@0.8:boxborderw=6:x=15:y=15,
  drawtext=text='%{localtime\:%H\\\:%M\\\:%S}':fontsize=14:fontcolor=white@0.7:x=w-tw-15:y=15
  [out]
" -map "[out]" -map 0:a
```
