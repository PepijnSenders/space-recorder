//! Downsampling algorithms for converting pixel data to character grids.

use crate::camera::Frame;

/// RGB color for downsampled cells.
#[derive(Debug, Clone, Copy, Default)]
pub struct CellColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
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
    frame: &Frame,
    char_width: u16,
    char_height: u16,
    buffer: &mut Vec<CellColor>,
) -> usize {
    buffer.clear();

    let img_width = frame.width;
    let img_height = frame.height;

    if char_width == 0
        || char_height == 0
        || img_width == 0
        || img_height == 0
        || frame.data.is_empty()
    {
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
