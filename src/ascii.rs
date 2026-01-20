//! ASCII renderer module for converting camera frames to ASCII art.

use crate::camera::Frame;

/// Standard ASCII density ramp (10 levels).
/// Characters ordered from darkest (space) to brightest (@).
/// Works well on dark terminals.
pub const STANDARD_CHARSET: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// Block character set (5 levels).
/// Uses Unicode block characters for higher perceived resolution.
/// Characters ordered from darkest (space) to brightest (full block).
pub const BLOCKS_CHARSET: &[char] = &[' ', '░', '▒', '▓', '█'];

/// Minimal character set (4 levels).
/// Clean, less noisy look.
pub const MINIMAL_CHARSET: &[char] = &[' ', '.', ':', '#'];

/// Character set type for ASCII rendering.
///
/// Allows cycling through different character sets with hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharSet {
    /// Standard ASCII density ramp (10 levels)
    #[default]
    Standard,
    /// Block character set (5 levels) using Unicode blocks
    Blocks,
    /// Minimal character set (4 levels) for a clean look
    Minimal,
    /// Braille character set for highest resolution
    Braille,
}

impl CharSet {
    /// Get the character slice for this charset.
    ///
    /// Note: For Braille, this returns an empty slice since braille
    /// rendering uses a different algorithm (render_braille).
    pub fn chars(&self) -> &'static [char] {
        match self {
            CharSet::Standard => STANDARD_CHARSET,
            CharSet::Blocks => BLOCKS_CHARSET,
            CharSet::Minimal => MINIMAL_CHARSET,
            CharSet::Braille => &[], // Braille uses different rendering
        }
    }

    /// Cycle to the next character set.
    ///
    /// Order: Standard -> Blocks -> Minimal -> Braille -> Standard
    pub fn next(&self) -> Self {
        match self {
            CharSet::Standard => CharSet::Blocks,
            CharSet::Blocks => CharSet::Minimal,
            CharSet::Minimal => CharSet::Braille,
            CharSet::Braille => CharSet::Standard,
        }
    }

    /// Get a human-readable name for the charset.
    pub fn name(&self) -> &'static str {
        match self {
            CharSet::Standard => "standard",
            CharSet::Blocks => "blocks",
            CharSet::Minimal => "minimal",
            CharSet::Braille => "braille",
        }
    }

    /// Check if this charset uses braille rendering.
    pub fn is_braille(&self) -> bool {
        matches!(self, CharSet::Braille)
    }
}

/// Default terminal character aspect ratio.
/// Terminal characters are typically ~2x taller than wide.
/// This is used to correct the aspect ratio when rendering.
pub const DEFAULT_CHAR_ASPECT_RATIO: f32 = 2.0;

/// Calculate output dimensions that preserve aspect ratio for terminal display.
///
/// Terminal characters are typically ~2x taller than wide, so a naive
/// mapping of pixels to characters will result in a vertically stretched
/// image. This function compensates by adjusting the output dimensions.
///
/// The function calculates dimensions that:
/// 1. Preserve the original image aspect ratio when displayed in terminal
/// 2. Fit within the specified maximum character dimensions
/// 3. Account for the terminal character aspect ratio (~2:1 height:width)
///
/// # Arguments
/// * `img_width` - Width of the source image in pixels
/// * `img_height` - Height of the source image in pixels
/// * `max_char_width` - Maximum output width in characters
/// * `max_char_height` - Maximum output height in characters
///
/// # Returns
/// A tuple of (char_width, char_height) that will display with correct aspect ratio.
///
/// # Example
/// ```ignore
/// // A 640x480 (4:3) image should display as ~4:3 in the terminal
/// let (w, h) = calculate_dimensions(640, 480, 80, 24);
/// // Result might be (48, 18) which, when rendered with ~2:1 char aspect,
/// // displays as approximately 4:3
/// ```
pub fn calculate_dimensions(
    img_width: u32,
    img_height: u32,
    max_char_width: u16,
    max_char_height: u16,
) -> (u16, u16) {
    calculate_dimensions_with_aspect(
        img_width,
        img_height,
        max_char_width,
        max_char_height,
        DEFAULT_CHAR_ASPECT_RATIO,
    )
}

/// Calculate output dimensions with a custom character aspect ratio.
///
/// This is the configurable version of `calculate_dimensions` that allows
/// specifying a custom terminal character aspect ratio for non-standard
/// terminal fonts.
///
/// # Arguments
/// * `img_width` - Width of the source image in pixels
/// * `img_height` - Height of the source image in pixels
/// * `max_char_width` - Maximum output width in characters
/// * `max_char_height` - Maximum output height in characters
/// * `char_aspect` - Terminal character aspect ratio (height/width, typically ~2.0)
///
/// # Returns
/// A tuple of (char_width, char_height) that will display with correct aspect ratio.
pub fn calculate_dimensions_with_aspect(
    img_width: u32,
    img_height: u32,
    max_char_width: u16,
    max_char_height: u16,
    char_aspect: f32,
) -> (u16, u16) {
    // Handle edge cases
    if img_width == 0 || img_height == 0 || max_char_width == 0 || max_char_height == 0 {
        return (0, 0);
    }

    // Calculate the image aspect ratio (width / height)
    let img_aspect = img_width as f32 / img_height as f32;

    // Compensate for terminal character aspect ratio.
    // Characters are char_aspect times taller than wide, so to display
    // an image with the correct aspect ratio, we need char_aspect times
    // fewer rows than columns for a square image.
    //
    // For a 1:1 image: we want char_width / char_height * char_aspect = 1
    // So: char_height = char_width * char_aspect
    //
    // For an arbitrary image: target_char_aspect = img_aspect * char_aspect
    let target_char_aspect = img_aspect * char_aspect;

    // Try fitting to max width first
    let char_width = max_char_width;
    let char_height = (char_width as f32 / target_char_aspect).round() as u16;

    if char_height <= max_char_height && char_height > 0 {
        (char_width, char_height)
    } else {
        // Width-constrained doesn't fit, use height-constrained
        let char_height = max_char_height;
        let char_width = (char_height as f32 * target_char_aspect).round() as u16;
        // Clamp to max width in case of rounding
        let char_width = char_width.min(max_char_width);
        // Ensure we return at least 1x1 if possible
        (char_width.max(1), char_height.max(1))
    }
}

/// Braille base character (U+2800, empty braille pattern).
pub const BRAILLE_BASE: char = '\u{2800}';

/// Convert a 2x4 boolean grid to a braille character.
///
/// Each braille character represents a 2x4 dot matrix where each dot can be on or off.
/// The bit positions are:
/// ```text
/// [0,0]=1   [1,0]=8
/// [0,1]=2   [1,1]=16
/// [0,2]=4   [1,2]=32
/// [0,3]=64  [1,3]=128
/// ```
///
/// # Arguments
/// * `grid` - A 2x4 boolean array where grid[x][y] indicates if dot at (x,y) is on
///
/// # Returns
/// The corresponding braille character (U+2800 to U+28FF)
pub fn grid_to_braille(grid: [[bool; 4]; 2]) -> char {
    let mut code = 0u8;
    if grid[0][0] { code |= 0x01; }
    if grid[0][1] { code |= 0x02; }
    if grid[0][2] { code |= 0x04; }
    if grid[0][3] { code |= 0x40; }
    if grid[1][0] { code |= 0x08; }
    if grid[1][1] { code |= 0x10; }
    if grid[1][2] { code |= 0x20; }
    if grid[1][3] { code |= 0x80; }
    char::from_u32(BRAILLE_BASE as u32 + code as u32).unwrap_or(BRAILLE_BASE)
}

/// Render grayscale data as braille characters.
///
/// Each braille character represents a 2x4 pixel area. Pixels above the threshold
/// are shown as dots, pixels below are empty. This provides 2x4 subpixel resolution
/// per character cell, giving the highest detail mode.
///
/// # Arguments
/// * `gray` - Grayscale pixel data (0-255 per pixel)
/// * `img_width` - Width of the source image in pixels
/// * `img_height` - Height of the source image in pixels
/// * `char_width` - Desired output width in characters
/// * `char_height` - Desired output height in characters
/// * `threshold` - Brightness threshold (0-255) for dot activation
/// * `invert` - If true, invert brightness before thresholding
///
/// # Returns
/// A vector of braille characters representing the image
pub fn render_braille(
    gray: &[u8],
    img_width: u32,
    img_height: u32,
    char_width: u16,
    char_height: u16,
    threshold: u8,
    invert: bool,
) -> Vec<char> {
    if char_width == 0 || char_height == 0 || img_width == 0 || img_height == 0 || gray.is_empty() {
        return Vec::new();
    }

    // Each braille character represents 2x4 pixels
    // Calculate pixel dimensions for the braille grid
    let braille_pixel_width = char_width as u32 * 2;
    let braille_pixel_height = char_height as u32 * 4;

    // Scale factors from source image to braille pixel grid
    let scale_x = img_width as f32 / braille_pixel_width as f32;
    let scale_y = img_height as f32 / braille_pixel_height as f32;

    let mut result = Vec::with_capacity((char_width as usize) * (char_height as usize));

    for cy in 0..char_height {
        for cx in 0..char_width {
            let mut grid = [[false; 4]; 2];

            // Sample 2x4 pixels for this braille character
            for dy in 0..4 {
                for dx in 0..2 {
                    // Map braille subpixel to source image pixel
                    let bx = cx as u32 * 2 + dx;
                    let by = cy as u32 * 4 + dy;
                    let src_x = (bx as f32 * scale_x) as u32;
                    let src_y = (by as f32 * scale_y) as u32;

                    // Bounds check
                    if src_x < img_width && src_y < img_height {
                        let idx = (src_y * img_width + src_x) as usize;
                        if idx < gray.len() {
                            let brightness = if invert { 255 - gray[idx] } else { gray[idx] };
                            grid[dx as usize][dy as usize] = brightness >= threshold;
                        }
                    }
                }
            }

            result.push(grid_to_braille(grid));
        }
    }

    result
}

/// Render grayscale data as braille characters into an existing buffer.
///
/// This is the allocation-free version for use in hot paths.
#[allow(clippy::too_many_arguments)]
pub fn render_braille_into(
    gray: &[u8],
    img_width: u32,
    img_height: u32,
    char_width: u16,
    char_height: u16,
    threshold: u8,
    invert: bool,
    buffer: &mut Vec<char>,
) -> usize {
    buffer.clear();

    if char_width == 0 || char_height == 0 || img_width == 0 || img_height == 0 || gray.is_empty() {
        return 0;
    }

    let output_size = (char_width as usize) * (char_height as usize);
    buffer.reserve(output_size);

    let braille_pixel_width = char_width as u32 * 2;
    let braille_pixel_height = char_height as u32 * 4;
    let scale_x = img_width as f32 / braille_pixel_width as f32;
    let scale_y = img_height as f32 / braille_pixel_height as f32;

    for cy in 0..char_height {
        for cx in 0..char_width {
            let mut grid = [[false; 4]; 2];

            for dy in 0..4 {
                for dx in 0..2 {
                    let bx = cx as u32 * 2 + dx;
                    let by = cy as u32 * 4 + dy;
                    let src_x = (bx as f32 * scale_x) as u32;
                    let src_y = (by as f32 * scale_y) as u32;

                    if src_x < img_width && src_y < img_height {
                        let idx = (src_y * img_width + src_x) as usize;
                        if idx < gray.len() {
                            let brightness = if invert { 255 - gray[idx] } else { gray[idx] };
                            grid[dx as usize][dy as usize] = brightness >= threshold;
                        }
                    }
                }
            }

            buffer.push(grid_to_braille(grid));
        }
    }

    output_size
}

/// Convert an RGB frame to grayscale using ITU-R BT.601 luminance formula.
///
/// The luminance formula is: Y = 0.299*R + 0.587*G + 0.114*B
///
/// This function uses integer math for efficiency, avoiding floating-point
/// operations in the hot path. The coefficients are scaled by 1000:
/// - R: 299/1000
/// - G: 587/1000
/// - B: 114/1000
///
/// # Arguments
/// * `frame` - An RGB frame from the camera
///
/// # Returns
/// A vector of grayscale values (0-255), one per pixel
pub fn to_grayscale(frame: &Frame) -> Vec<u8> {
    // Pre-allocate with exact capacity
    let pixel_count = (frame.width * frame.height) as usize;
    let mut gray = Vec::with_capacity(pixel_count);

    // Process RGB triplets using integer math for speed
    // Coefficients scaled by 1000: 299 + 587 + 114 = 1000
    for rgb in frame.data.chunks_exact(3) {
        let r = rgb[0] as u32;
        let g = rgb[1] as u32;
        let b = rgb[2] as u32;
        // ITU-R BT.601 luminance formula with integer math
        let luminance = (299 * r + 587 * g + 114 * b) / 1000;
        gray.push(luminance as u8);
    }

    gray
}

/// Convert an RGB frame to grayscale in-place, reusing an existing buffer.
///
/// This avoids allocation when called repeatedly (e.g., each frame).
///
/// # Arguments
/// * `frame` - An RGB frame from the camera
/// * `buffer` - A mutable buffer to store grayscale values
///
/// # Returns
/// The number of pixels written to the buffer
pub fn to_grayscale_into(frame: &Frame, buffer: &mut Vec<u8>) -> usize {
    let pixel_count = (frame.width * frame.height) as usize;
    buffer.clear();
    buffer.reserve(pixel_count);

    for rgb in frame.data.chunks_exact(3) {
        let r = rgb[0] as u32;
        let g = rgb[1] as u32;
        let b = rgb[2] as u32;
        let luminance = (299 * r + 587 * g + 114 * b) / 1000;
        buffer.push(luminance as u8);
    }

    pixel_count
}

/// Downsample a grayscale image to a character grid.
///
/// Maps image pixels to character grid cells by averaging the brightness
/// of all pixels within each cell. This reduces the resolution from the
/// camera's pixel dimensions to the desired character dimensions.
///
/// # Arguments
/// * `gray` - Grayscale pixel data (one byte per pixel, row-major order)
/// * `img_width` - Width of the source image in pixels
/// * `img_height` - Height of the source image in pixels
/// * `char_width` - Desired output width in characters
/// * `char_height` - Desired output height in characters
///
/// # Returns
/// A vector of brightness values (0-255), one per character cell, in row-major order.
/// The length is `char_width * char_height`.
///
/// # Example
/// ```ignore
/// // Downsample a 640x480 image to a 40x20 character grid
/// let brightness = downsample(&grayscale, 640, 480, 40, 20);
/// assert_eq!(brightness.len(), 40 * 20);
/// ```
pub fn downsample(
    gray: &[u8],
    img_width: u32,
    img_height: u32,
    char_width: u16,
    char_height: u16,
) -> Vec<u8> {
    // Handle edge cases
    if char_width == 0 || char_height == 0 || img_width == 0 || img_height == 0 || gray.is_empty() {
        return Vec::new();
    }

    // Calculate the size of each cell in pixels (as floats for accurate mapping)
    let cell_w = img_width as f32 / char_width as f32;
    let cell_h = img_height as f32 / char_height as f32;

    let mut result = Vec::with_capacity((char_width as usize) * (char_height as usize));

    for cy in 0..char_height {
        for cx in 0..char_width {
            // Calculate pixel bounds for this cell
            let start_x = (cx as f32 * cell_w) as u32;
            let end_x = ((cx + 1) as f32 * cell_w) as u32;
            let start_y = (cy as f32 * cell_h) as u32;
            let end_y = ((cy + 1) as f32 * cell_h) as u32;

            // Average brightness of all pixels in this cell
            let mut sum = 0u32;
            let mut count = 0u32;

            for py in start_y..end_y {
                for px in start_x..end_x {
                    let idx = (py * img_width + px) as usize;
                    if idx < gray.len() {
                        sum += gray[idx] as u32;
                        count += 1;
                    }
                }
            }

            // Store average brightness (or 0 if no pixels in cell)
            result.push(if count > 0 { (sum / count) as u8 } else { 0 });
        }
    }

    result
}

/// Downsample a grayscale image into an existing buffer to avoid allocation.
///
/// This is the allocation-free version of `downsample` for use in hot paths.
///
/// # Arguments
/// * `gray` - Grayscale pixel data (one byte per pixel, row-major order)
/// * `img_width` - Width of the source image in pixels
/// * `img_height` - Height of the source image in pixels
/// * `char_width` - Desired output width in characters
/// * `char_height` - Desired output height in characters
/// * `buffer` - A mutable buffer to store the result
///
/// # Returns
/// The number of brightness values written to the buffer.
pub fn downsample_into(
    gray: &[u8],
    img_width: u32,
    img_height: u32,
    char_width: u16,
    char_height: u16,
    buffer: &mut Vec<u8>,
) -> usize {
    buffer.clear();

    // Handle edge cases
    if char_width == 0 || char_height == 0 || img_width == 0 || img_height == 0 || gray.is_empty() {
        return 0;
    }

    let output_size = (char_width as usize) * (char_height as usize);
    buffer.reserve(output_size);

    let cell_w = img_width as f32 / char_width as f32;
    let cell_h = img_height as f32 / char_height as f32;

    for cy in 0..char_height {
        for cx in 0..char_width {
            let start_x = (cx as f32 * cell_w) as u32;
            let end_x = ((cx + 1) as f32 * cell_w) as u32;
            let start_y = (cy as f32 * cell_h) as u32;
            let end_y = ((cy + 1) as f32 * cell_h) as u32;

            let mut sum = 0u32;
            let mut count = 0u32;

            for py in start_y..end_y {
                for px in start_x..end_x {
                    let idx = (py * img_width + px) as usize;
                    if idx < gray.len() {
                        sum += gray[idx] as u32;
                        count += 1;
                    }
                }
            }

            buffer.push(if count > 0 { (sum / count) as u8 } else { 0 });
        }
    }

    output_size
}

/// RGB color for downsampled cells.
#[derive(Debug, Clone, Copy, Default)]
pub struct CellColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Downsample an RGB frame to get average colors per character cell.
///
/// Each cell's color is the average of all pixels in that cell area.
///
/// # Arguments
/// * `frame` - RGB frame from the camera (3 bytes per pixel: R, G, B)
/// * `char_width` - Desired output width in characters
/// * `char_height` - Desired output height in characters
/// * `buffer` - A mutable buffer to store the result
///
/// # Returns
/// The number of color values written to the buffer.
pub fn downsample_colors_into(
    frame: &crate::camera::Frame,
    char_width: u16,
    char_height: u16,
    buffer: &mut Vec<CellColor>,
) -> usize {
    buffer.clear();

    let img_width = frame.width;
    let img_height = frame.height;

    if char_width == 0 || char_height == 0 || img_width == 0 || img_height == 0 || frame.data.is_empty() {
        return 0;
    }

    let output_size = (char_width as usize) * (char_height as usize);
    buffer.reserve(output_size);

    let cell_w = img_width as f32 / char_width as f32;
    let cell_h = img_height as f32 / char_height as f32;

    for cy in 0..char_height {
        for cx in 0..char_width {
            let start_x = (cx as f32 * cell_w) as u32;
            let end_x = ((cx + 1) as f32 * cell_w) as u32;
            let start_y = (cy as f32 * cell_h) as u32;
            let end_y = ((cy + 1) as f32 * cell_h) as u32;

            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            let mut count = 0u32;

            for py in start_y..end_y {
                for px in start_x..end_x {
                    let idx = ((py * img_width + px) * 3) as usize;
                    if idx + 2 < frame.data.len() {
                        sum_r += frame.data[idx] as u32;
                        sum_g += frame.data[idx + 1] as u32;
                        sum_b += frame.data[idx + 2] as u32;
                        count += 1;
                    }
                }
            }

            buffer.push(if count > 0 {
                CellColor {
                    r: (sum_r / count) as u8,
                    g: (sum_g / count) as u8,
                    b: (sum_b / count) as u8,
                }
            } else {
                CellColor::default()
            });
        }
    }

    output_size
}

/// Map brightness values to ASCII characters.
///
/// Converts a grid of brightness values (0-255) to characters from the
/// provided charset. Lower brightness maps to earlier characters (typically
/// darker/less dense), higher brightness to later characters (brighter/denser).
///
/// # Arguments
/// * `brightness` - Brightness values (0-255), one per character cell
/// * `charset` - Character set to use, ordered from darkest to brightest
/// * `invert` - If true, invert brightness before mapping (for light terminals)
///
/// # Returns
/// A vector of characters, one per input brightness value.
///
/// # Example
/// ```ignore
/// let brightness = vec![0, 127, 255];
/// let chars = map_to_chars(&brightness, STANDARD_CHARSET, false);
/// // chars[0] = ' ' (darkest)
/// // chars[1] = '+' (mid)
/// // chars[2] = '@' (brightest)
/// ```
pub fn map_to_chars(brightness: &[u8], charset: &[char], invert: bool) -> Vec<char> {
    if charset.is_empty() {
        return vec![' '; brightness.len()];
    }

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

/// Map brightness values to ASCII characters in-place, reusing an existing buffer.
///
/// This is the allocation-free version of `map_to_chars` for use in hot paths.
///
/// # Arguments
/// * `brightness` - Brightness values (0-255), one per character cell
/// * `charset` - Character set to use, ordered from darkest to brightest
/// * `invert` - If true, invert brightness before mapping (for light terminals)
/// * `buffer` - A mutable buffer to store the result
///
/// # Returns
/// The number of characters written to the buffer.
pub fn map_to_chars_into(
    brightness: &[u8],
    charset: &[char],
    invert: bool,
    buffer: &mut Vec<char>,
) -> usize {
    buffer.clear();

    if charset.is_empty() {
        buffer.resize(brightness.len(), ' ');
        return brightness.len();
    }

    buffer.reserve(brightness.len());
    let levels = charset.len();

    for &b in brightness {
        let b = if invert { 255 - b } else { b };
        let idx = (b as usize * (levels - 1)) / 255;
        buffer.push(charset[idx]);
    }

    brightness.len()
}

/// Apply Sobel edge detection to a grayscale image.
///
/// The Sobel operator calculates the gradient magnitude at each pixel,
/// which highlights edges and transitions. This is useful for rendering
/// sharper facial features in ASCII art.
///
/// The Sobel kernels used are:
/// ```text
/// Gx:          Gy:
/// [-1  0  1]   [-1 -2 -1]
/// [-2  0  2]   [ 0  0  0]
/// [-1  0  1]   [ 1  2  1]
/// ```
///
/// # Arguments
/// * `gray` - Grayscale pixel data (one byte per pixel, row-major order)
/// * `width` - Width of the image in pixels
/// * `height` - Height of the image in pixels
///
/// # Returns
/// A vector of edge magnitudes (0-255), same dimensions as input.
/// Edge pixels (1-pixel border) are set to 0 since the kernel can't be applied there.
pub fn apply_edge_detection(gray: &[u8], width: u32, height: u32) -> Vec<u8> {
    // Handle edge cases
    if width < 3 || height < 3 || gray.len() < (width * height) as usize {
        return gray.to_vec();
    }

    let mut edges = vec![0u8; gray.len()];

    // Sobel kernels
    // Gx detects vertical edges (horizontal gradient)
    // Gy detects horizontal edges (vertical gradient)
    let sobel_x: [[i32; 3]; 3] = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]];
    let sobel_y: [[i32; 3]; 3] = [[-1, -2, -1], [0, 0, 0], [1, 2, 1]];

    // Process interior pixels (skip 1-pixel border)
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let mut gx: i32 = 0;
            let mut gy: i32 = 0;

            // Apply 3x3 kernel
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

            // Calculate gradient magnitude
            // Using integer approximation: |gx| + |gy| is faster than sqrt(gx² + gy²)
            // and produces similar visual results for edge detection
            let magnitude = (gx.abs() + gy.abs()).min(255) as u8;
            edges[(y * width + x) as usize] = magnitude;
        }
    }

    edges
}

