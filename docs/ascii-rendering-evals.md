# ASCII Rendering Evals & Improvements

## Overview

This document describes the evaluation framework and rendering improvements added to the ASCII art pipeline. The goal was to create a way to measure rendering quality and then improve face/feature recognition, edge sharpness, and gradient smoothness.

## What Was Built

### 1. Evaluation Framework

**Location:** `tests/ascii_visual.rs`

A visual snapshot testing system that:
- Loads test images and renders them through the ASCII pipeline
- Compares output against golden reference files
- Generates HTML previews with full color for visual inspection
- Provides diff output when tests fail

**Test Images:** `tests/fixtures/images/`
- `face.jpg` - Portrait photo (facial feature recognition)
- `geometry.jpg` - Architecture (edge sharpness)
- `gradient.jpg` - Sky/landscape (smooth transitions)
- `contrast.jpg` - High contrast scene (detail preservation)

**Golden References:** `tests/fixtures/ascii_golden/`
- Format: `{image}_{charset}_{width}x{height}.txt`
- These are the "ideal" outputs to compare against

### 2. Rendering Improvements

Four new rendering techniques were implemented:

| Improvement | Location | Purpose |
|-------------|----------|---------|
| Gamma Correction | `src/ascii/mapping.rs` | Perceptually accurate brightness |
| Dithering | `src/ascii/mapping.rs` | Smoother gradients |
| Structure-Aware | `src/ascii/edges.rs` | Edge-sensitive character selection |
| Improved Downsampling | `src/ascii/downsample.rs` | Better detail preservation |

## API Reference

### Gamma Correction

```rust
use space_recorder::ascii::{gamma_correct, map_to_chars_gamma, map_to_chars_gamma_into};

// Apply gamma correction to a single value
let corrected = gamma_correct(128); // Returns ~186

// Map with gamma correction
let chars = map_to_chars_gamma(&brightness, STANDARD_CHARSET, false);

// In-place version for hot paths
map_to_chars_gamma_into(&brightness, STANDARD_CHARSET, false, &mut buffer);
```

### Dithering

```rust
use space_recorder::ascii::{map_to_chars_dithered, map_to_chars_ordered_dither};

// Floyd-Steinberg dithering (best quality, slower)
let chars = map_to_chars_dithered(
    &brightness,
    width,
    height,
    STANDARD_CHARSET,
    false,      // invert
    true,       // use_gamma
);

// Ordered dithering (faster, regular pattern)
let chars = map_to_chars_ordered_dither(
    &brightness,
    width,
    STANDARD_CHARSET,
    false,      // invert
    true,       // use_gamma
);
```

### Structure-Aware Characters

```rust
use space_recorder::ascii::{
    map_structure_aware, STRUCTURE_CHARSET, STRUCTURE_CHARSET_ASCII
};

// Analyzes local gradients and picks directional characters
let chars = map_structure_aware(
    &grayscale,         // Full-resolution grayscale image
    img_width,
    img_height,
    char_width,
    char_height,
    &STRUCTURE_CHARSET_ASCII,  // or &STRUCTURE_CHARSET for Unicode
    true,               // use_gamma
);
```

**Character Sets:**
- `STRUCTURE_CHARSET` - Uses Unicode: `≡ ║ ╲ ╱ ▓`
- `STRUCTURE_CHARSET_ASCII` - ASCII only: `= | \ / #`

### Improved Downsampling

```rust
use space_recorder::ascii::{downsample_contrast, downsample_edge_preserve};

// Contrast-preserving downsample
let brightness = downsample_contrast(
    &grayscale,
    img_width,
    img_height,
    char_width,
    char_height,
    1.3,        // contrast_boost (1.0 = normal)
);

// Edge-preserving downsample
let brightness = downsample_edge_preserve(
    &grayscale,
    img_width,
    img_height,
    char_width,
    char_height,
    0.3,        // edge_bias (0.0-1.0)
);
```

## Commands

### Run Visual Tests

```bash
# Run all visual snapshot tests
cargo test --test ascii_visual

# Update golden references after making improvements
UPDATE_GOLDEN=1 cargo test --test ascii_visual
```

### Generate HTML Previews

```bash
# Full comparison with colors
GENERATE_HTML=1 cargo test --test ascii_visual generate_html_preview -- --nocapture

# Gamma comparison (linear vs corrected)
COMPARE_GAMMA=1 cargo test --test ascii_visual compare_gamma_correction -- --nocapture

# Dithering methods comparison
COMPARE_DITHER=1 cargo test --test ascii_visual compare_dithering -- --nocapture

# Structure-aware vs standard
COMPARE_STRUCTURE=1 cargo test --test ascii_visual compare_structure_aware -- --nocapture

# Downsampling methods comparison
COMPARE_DOWNSAMPLE=1 cargo test --test ascii_visual compare_downsampling -- --nocapture
```

### View Results

```bash
open tests/fixtures/html_preview/comparison.html
open tests/fixtures/html_preview/gamma_comparison.html
open tests/fixtures/html_preview/dither_comparison.html
open tests/fixtures/html_preview/structure_comparison.html
open tests/fixtures/html_preview/downsample_comparison.html
```

## File Structure

```
src/ascii/
├── mod.rs              # Module exports
├── mapping.rs          # Character mapping (+ gamma, dithering)
├── edges.rs            # Edge detection (+ structure-aware)
├── downsample.rs       # Downsampling (+ contrast, edge-preserve)
├── grayscale.rs        # RGB to grayscale
├── charset.rs          # Character set definitions
├── dimensions.rs       # Aspect ratio calculations
└── braille.rs          # Braille rendering

tests/
├── ascii_visual.rs     # Visual snapshot tests
└── fixtures/
    ├── images/         # Test input images
    ├── ascii_golden/   # Golden reference outputs
    └── html_preview/   # Generated HTML comparisons
```

## Recommended Combinations

For different use cases:

| Use Case | Downsample | Mapping | Notes |
|----------|------------|---------|-------|
| **Real-time video** | `downsample` | `map_to_chars_gamma` | Fast, good quality |
| **Smooth gradients** | `downsample` | `map_to_chars_dithered` | Best for skies, skin |
| **Sharp edges** | `downsample_edge_preserve` | `map_to_chars_gamma` | Architecture, text |
| **Maximum detail** | `downsample_contrast` | `map_structure_aware` | Portraits, complex scenes |

## Integration Example

To use gamma correction in the main event loop:

```rust
// In src/event_loop.rs, replace:
ascii::map_to_chars_into(&brightness_buffer, charset, invert, &mut char_buffer);

// With:
ascii::map_to_chars_gamma_into(&brightness_buffer, charset, invert, &mut char_buffer);
```

## Future Improvements

Potential areas for further work:

1. **Adaptive thresholding** - Adjust dithering/contrast per-region
2. **Temporal dithering** - Reduce flicker in video by varying dither pattern
3. **Custom character training** - Learn optimal char-to-brightness mapping from fonts
4. **Perceptual charset ordering** - Measure actual perceived density of characters
5. **GPU acceleration** - Move downsampling/mapping to compute shaders

## Dependencies Added

```toml
[dev-dependencies]
image = "0.25"  # For loading test images
```
