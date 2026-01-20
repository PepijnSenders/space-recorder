//! Braille character rendering for high-resolution ASCII art.
//!
//! Each braille character represents a 2x4 dot matrix, providing 8 subpixels
//! per terminal character cell. This gives the highest detail mode for
//! rendering camera frames.

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
    if grid[0][0] {
        code |= 0x01;
    }
    if grid[0][1] {
        code |= 0x02;
    }
    if grid[0][2] {
        code |= 0x04;
    }
    if grid[0][3] {
        code |= 0x40;
    }
    if grid[1][0] {
        code |= 0x08;
    }
    if grid[1][1] {
        code |= 0x10;
    }
    if grid[1][2] {
        code |= 0x20;
    }
    if grid[1][3] {
        code |= 0x80;
    }
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
pub fn render(
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
pub fn render_into(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braille_base() {
        assert_eq!(BRAILLE_BASE, '\u{2800}');
    }

    #[test]
    fn test_grid_to_braille_empty() {
        let grid = [[false; 4]; 2];
        assert_eq!(grid_to_braille(grid), BRAILLE_BASE);
    }

    #[test]
    fn test_grid_to_braille_full() {
        let grid = [[true; 4]; 2];
        assert_eq!(grid_to_braille(grid), '\u{28FF}');
    }

    #[test]
    fn test_grid_to_braille_single_dots() {
        // Top-left dot only
        let mut grid = [[false; 4]; 2];
        grid[0][0] = true;
        assert_eq!(grid_to_braille(grid), '\u{2801}');

        // Top-right dot only
        let mut grid = [[false; 4]; 2];
        grid[1][0] = true;
        assert_eq!(grid_to_braille(grid), '\u{2808}');
    }

    #[test]
    fn test_render_empty_input() {
        assert!(render(&[], 0, 0, 10, 10, 128, false).is_empty());
        assert!(render(&[128], 1, 1, 0, 0, 128, false).is_empty());
    }

    #[test]
    fn test_render_into_empty_input() {
        let mut buffer = Vec::new();
        assert_eq!(render_into(&[], 0, 0, 10, 10, 128, false, &mut buffer), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_render_basic() {
        // 2x4 white pixels should produce a full braille character
        let gray = vec![255u8; 8];
        let result = render(&gray, 2, 4, 1, 1, 128, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], '\u{28FF}');
    }

    #[test]
    fn test_render_invert() {
        // 2x4 black pixels with invert should produce a full braille character
        let gray = vec![0u8; 8];
        let result = render(&gray, 2, 4, 1, 1, 128, true);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], '\u{28FF}');
    }
}
