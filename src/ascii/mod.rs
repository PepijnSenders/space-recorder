//! ASCII renderer module for converting camera frames to ASCII art.
//!
//! This module provides a complete pipeline for converting camera frames
//! to ASCII art suitable for terminal display:
//!
//! 1. **Grayscale conversion** - RGB to luminance using BT.601
//! 2. **Downsampling** - Reduce resolution to character grid
//! 3. **Character mapping** - Map brightness to ASCII characters
//! 4. **Edge detection** - Optional Sobel filter for sharper output
//!
//! # Character Sets
//!
//! Multiple character sets are available via [`CharSet`]:
//! - `Standard` - 10-level ASCII density ramp
//! - `Blocks` - Unicode block characters
//! - `Minimal` - 4-level clean look
//! - `Braille` - Highest resolution using braille patterns

pub mod braille;
mod charset;
mod dimensions;
mod downsample;
mod edges;
mod grayscale;
mod mapping;

// Re-export all public items for backwards compatibility
pub use charset::{CharSet, BLOCKS_CHARSET, MINIMAL_CHARSET, STANDARD_CHARSET};
pub use dimensions::{calculate_dimensions, calculate_dimensions_with_aspect, DEFAULT_CHAR_ASPECT_RATIO};
pub use downsample::{downsample, downsample_colors_into, downsample_into, CellColor};
pub use edges::apply_edge_detection;
pub use grayscale::{to_grayscale, to_grayscale_into};
pub use mapping::{map_to_chars, map_to_chars_into};

// Re-export braille functions at the module level for convenience
pub use braille::render as render_braille;
#[allow(unused_imports)]
pub use braille::render_into as render_braille_into;
#[allow(unused_imports)]
pub use braille::grid_to_braille;
#[allow(unused_imports)]
pub use braille::BRAILLE_BASE;
