# 03 - ASCII Renderer

Converts camera frames to ASCII art for display in the terminal. The core visual transformation of the application.

## Overview

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   Frame     │ ───▶ │  Grayscale  │ ───▶ │  Downsample │
│   (RGB)     │      │  Convert    │      │  to Grid    │
└─────────────┘      └─────────────┘      └─────────────┘
                                                │
                                                ▼
                                         ┌─────────────┐
                                         │  Map to     │
                                         │  Characters │
                                         └─────────────┘
                                                │
                                                ▼
                                         ┌─────────────┐
                                         │  ASCII      │
                                         │  String     │
                                         └─────────────┘
```

## Data Structures

```rust
pub struct AsciiRenderer {
    /// Character set to use
    pub charset: CharacterSet,
    /// Output dimensions in characters
    pub width: u16,
    pub height: u16,
    /// Enable edge detection
    pub edge_detection: bool,
    /// Invert brightness (for dark terminals)
    pub invert: bool,
}

pub enum CharacterSet {
    /// Standard ASCII density ramp
    Standard,
    /// Block characters for higher resolution
    Blocks,
    /// Minimal - just a few chars
    Minimal,
    /// Braille patterns (highest resolution)
    Braille,
    /// Custom character set
    Custom(Vec<char>),
}

pub struct AsciiFrame {
    /// Characters in row-major order
    pub chars: Vec<char>,
    /// Width in characters
    pub width: u16,
    /// Height in characters
    pub height: u16,
}
```

## Character Sets

### Standard (10 levels)

```rust
const STANDARD: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
```

Ordered from darkest (space) to brightest (@). Works in any terminal.

### Blocks (4 levels)

```rust
const BLOCKS: &[char] = &[' ', '░', '▒', '▓', '█'];
```

Higher perceived resolution due to partial fill characters.

### Minimal (4 levels)

```rust
const MINIMAL: &[char] = &[' ', '.', ':', '#'];
```

Clean, less noisy look.

### Braille (256 levels)

Uses Unicode Braille patterns (U+2800 to U+28FF) where each character is a 2x4 dot matrix:

```rust
const BRAILLE_BASE: char = '\u{2800}';

fn brightness_to_braille(grid: [[bool; 4]; 2]) -> char {
    // Each dot position has a bit value:
    // [0,0]=1  [1,0]=8
    // [0,1]=2  [1,1]=16
    // [0,2]=4  [1,2]=32
    // [0,3]=64 [1,3]=128
    let mut code = 0u8;
    if grid[0][0] { code |= 0x01; }
    if grid[0][1] { code |= 0x02; }
    if grid[0][2] { code |= 0x04; }
    if grid[1][0] { code |= 0x08; }
    if grid[1][1] { code |= 0x10; }
    if grid[1][2] { code |= 0x20; }
    if grid[0][3] { code |= 0x40; }
    if grid[1][3] { code |= 0x80; }
    char::from_u32(BRAILLE_BASE as u32 + code as u32).unwrap()
}
```

Braille effectively gives 2x4 resolution per character cell.

## Rendering Pipeline

### Step 1: Grayscale Conversion

```rust
fn to_grayscale(frame: &Frame) -> Vec<u8> {
    frame.data
        .chunks(3)
        .map(|rgb| {
            // Luminance formula (ITU-R BT.601)
            let r = rgb[0] as f32;
            let g = rgb[1] as f32;
            let b = rgb[2] as f32;
            (0.299 * r + 0.587 * g + 0.114 * b) as u8
        })
        .collect()
}
```

### Step 2: Downsampling

Map image pixels to character grid cells:

```rust
fn downsample(
    gray: &[u8],
    img_width: u32,
    img_height: u32,
    char_width: u16,
    char_height: u16,
) -> Vec<u8> {
    let cell_w = img_width as f32 / char_width as f32;
    let cell_h = img_height as f32 / char_height as f32;

    let mut result = Vec::with_capacity((char_width * char_height) as usize);

    for cy in 0..char_height {
        for cx in 0..char_width {
            // Average brightness in this cell
            let mut sum = 0u32;
            let mut count = 0u32;

            let start_x = (cx as f32 * cell_w) as u32;
            let end_x = ((cx + 1) as f32 * cell_w) as u32;
            let start_y = (cy as f32 * cell_h) as u32;
            let end_y = ((cy + 1) as f32 * cell_h) as u32;

            for py in start_y..end_y {
                for px in start_x..end_x {
                    let idx = (py * img_width + px) as usize;
                    if idx < gray.len() {
                        sum += gray[idx] as u32;
                        count += 1;
                    }
                }
            }

            result.push(if count > 0 { (sum / count) as u8 } else { 0 });
        }
    }

    result
}
```

### Step 3: Character Mapping

```rust
fn map_to_chars(brightness: &[u8], charset: &[char], invert: bool) -> Vec<char> {
    let levels = charset.len();
    brightness
        .iter()
        .map(|&b| {
            let b = if invert { 255 - b } else { b };
            let idx = (b as usize * (levels - 1)) / 255;
            charset[idx]
        })
        .collect()
}
```

## Main Render Function

```rust
impl AsciiRenderer {
    pub fn render(&self, frame: &Frame) -> AsciiFrame {
        // 1. Convert to grayscale
        let gray = to_grayscale(frame);

        // 2. Optional edge detection
        let gray = if self.edge_detection {
            apply_edge_detection(&gray, frame.width, frame.height)
        } else {
            gray
        };

        // 3. Downsample to character grid
        let brightness = downsample(
            &gray,
            frame.width,
            frame.height,
            self.width,
            self.height,
        );

        // 4. Map to characters
        let charset = self.charset.chars();
        let chars = map_to_chars(&brightness, charset, self.invert);

        AsciiFrame {
            chars,
            width: self.width,
            height: self.height,
        }
    }
}
```

## Edge Detection (Optional)

Sobel edge detection for sharper facial features:

```rust
fn apply_edge_detection(gray: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut edges = vec![0u8; gray.len()];

    let sobel_x: [[i32; 3]; 3] = [
        [-1, 0, 1],
        [-2, 0, 2],
        [-1, 0, 1],
    ];
    let sobel_y: [[i32; 3]; 3] = [
        [-1, -2, -1],
        [ 0,  0,  0],
        [ 1,  2,  1],
    ];

    for y in 1..height-1 {
        for x in 1..width-1 {
            let mut gx = 0i32;
            let mut gy = 0i32;

            for ky in 0..3 {
                for kx in 0..3 {
                    let px = (x as i32 + kx as i32 - 1) as u32;
                    let py = (y as i32 + ky as i32 - 1) as u32;
                    let idx = (py * width + px) as usize;
                    let val = gray[idx] as i32;
                    gx += val * sobel_x[ky][kx];
                    gy += val * sobel_y[ky][kx];
                }
            }

            let magnitude = ((gx * gx + gy * gy) as f32).sqrt() as u8;
            edges[(y * width + x) as usize] = magnitude;
        }
    }

    edges
}
```

## Aspect Ratio Correction

Terminal characters are taller than wide (~2:1). Compensate:

```rust
fn calculate_dimensions(
    img_width: u32,
    img_height: u32,
    max_char_width: u16,
    max_char_height: u16,
) -> (u16, u16) {
    let aspect = img_width as f32 / img_height as f32;
    // Characters are ~2x tall as wide
    let char_aspect = aspect * 2.0;

    let char_width = max_char_width;
    let char_height = (char_width as f32 / char_aspect) as u16;

    if char_height <= max_char_height {
        (char_width, char_height)
    } else {
        let char_height = max_char_height;
        let char_width = (char_height as f32 * char_aspect) as u16;
        (char_width, char_height)
    }
}
```

## Performance Optimization

### Frame Skipping

Don't render every frame:

```rust
const TARGET_FPS: u32 = 15;
const FRAME_DURATION: Duration = Duration::from_millis(1000 / TARGET_FPS);

fn should_render(last_render: Instant) -> bool {
    last_render.elapsed() >= FRAME_DURATION
}
```

### Pre-allocated Buffers

Reuse buffers instead of allocating each frame:

```rust
struct RenderBuffers {
    grayscale: Vec<u8>,
    brightness: Vec<u8>,
    chars: Vec<char>,
}
```

### SIMD (Future)

Could use SIMD for grayscale conversion and downsampling, but probably overkill for this use case.

## Colored ASCII (Future Enhancement)

Map pixel color to ANSI colors:

```rust
fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    // 6x6x6 color cube starts at 16
    let r = (r as u16 * 5 / 255) as u8;
    let g = (g as u16 * 5 / 255) as u8;
    let b = (b as u16 * 5 / 255) as u8;
    16 + 36 * r + 6 * g + b
}
```

Then render with:
```rust
format!("\x1b[38;5;{}m{}\x1b[0m", color, character)
```

## Testing

```rust
#[test]
fn test_grayscale() {
    let frame = Frame {
        data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255], // R, G, B pixels
        width: 3,
        height: 1,
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    };
    let gray = to_grayscale(&frame);
    // Red should be dimmer than green in luminance
    assert!(gray[1] > gray[0]); // green > red
}

#[test]
fn test_character_mapping() {
    let charset = &[' ', '.', '#'];
    let brightness = vec![0, 127, 255];
    let chars = map_to_chars(&brightness, charset, false);
    assert_eq!(chars, vec![' ', '.', '#']);
}
```

## Implementation Checklist

- [ ] Grayscale conversion
- [ ] Downsampling with cell averaging
- [ ] Standard character set mapping
- [ ] Block character set
- [ ] Braille character set
- [ ] Aspect ratio correction
- [ ] Brightness inversion option
- [ ] Edge detection (optional)
- [ ] Frame rate limiting
- [ ] Performance optimization
- [ ] Colored ASCII (future)
