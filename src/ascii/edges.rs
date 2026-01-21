//! Sobel edge detection for sharper ASCII rendering.

use super::mapping::gamma_correct;

/// Edge direction detected by gradient analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDirection {
    /// No significant edge (smooth area)
    None,
    /// Horizontal edge (use `-`, `_`, `=`)
    Horizontal,
    /// Vertical edge (use `|`, `!`, `:`)
    Vertical,
    /// Diagonal from top-left to bottom-right (use `\`)
    DiagonalDown,
    /// Diagonal from bottom-left to top-right (use `/`)
    DiagonalUp,
}

/// Structure-aware character set for edge-sensitive rendering.
/// Characters are grouped by brightness level (0-4) and edge direction.
pub struct StructureCharset {
    /// Characters for smooth areas (no edge), by brightness level
    pub smooth: [char; 5],
    /// Characters for horizontal edges, by brightness level
    pub horizontal: [char; 5],
    /// Characters for vertical edges, by brightness level
    pub vertical: [char; 5],
    /// Characters for diagonal-down edges, by brightness level
    pub diagonal_down: [char; 5],
    /// Characters for diagonal-up edges, by brightness level
    pub diagonal_up: [char; 5],
}

/// Default structure-aware charset optimized for ASCII art.
pub const STRUCTURE_CHARSET: StructureCharset = StructureCharset {
    smooth: [' ', '.', '+', '#', '@'],
    horizontal: [' ', '-', '=', '≡', '▬'],
    vertical: [' ', ':', '|', '║', '█'],
    diagonal_down: [' ', '.', '\\', '╲', '▓'],
    diagonal_up: [' ', '.', '/', '╱', '▓'],
};

/// ASCII-only structure-aware charset (no Unicode).
pub const STRUCTURE_CHARSET_ASCII: StructureCharset = StructureCharset {
    smooth: [' ', '.', '+', '#', '@'],
    horizontal: [' ', '-', '=', '=', '#'],
    vertical: [' ', ':', '|', '|', '#'],
    diagonal_down: [' ', '.', '\\', '\\', '#'],
    diagonal_up: [' ', '.', '/', '/', '#'],
};

/// Analyze gradient direction and magnitude for a cell.
///
/// Returns (magnitude, direction) where magnitude is 0-255 and direction
/// indicates the dominant edge orientation.
fn analyze_gradient(gx: i32, gy: i32) -> (u8, EdgeDirection) {
    let magnitude = ((gx.abs() + gy.abs()) / 2).min(255) as u8;

    // Threshold for considering this an edge
    const EDGE_THRESHOLD: i32 = 30;

    if gx.abs() < EDGE_THRESHOLD && gy.abs() < EDGE_THRESHOLD {
        return (magnitude, EdgeDirection::None);
    }

    // Determine direction based on gradient angle
    // atan2(gy, gx) gives angle, but we can use ratio comparison instead
    let abs_gx = gx.abs();
    let abs_gy = gy.abs();

    if abs_gx > abs_gy * 2 {
        // Mostly horizontal gradient = vertical edge
        EdgeDirection::Vertical
    } else if abs_gy > abs_gx * 2 {
        // Mostly vertical gradient = horizontal edge
        EdgeDirection::Horizontal
    } else if (gx > 0) == (gy > 0) {
        // Same sign = diagonal down
        EdgeDirection::DiagonalDown
    } else {
        // Different signs = diagonal up
        EdgeDirection::DiagonalUp
    };

    (magnitude, if abs_gx > abs_gy * 2 {
        EdgeDirection::Vertical
    } else if abs_gy > abs_gx * 2 {
        EdgeDirection::Horizontal
    } else if (gx > 0) == (gy > 0) {
        EdgeDirection::DiagonalDown
    } else {
        EdgeDirection::DiagonalUp
    })
}

/// Map brightness and edge info to a structure-aware character.
fn get_structure_char(brightness: u8, direction: EdgeDirection, edge_strength: u8, charset: &StructureCharset) -> char {
    // Map brightness to 5 levels (0-4)
    let level = (brightness as usize * 4) / 255;
    let level = level.min(4);

    // Blend between smooth and edge character based on edge strength
    // If edge is strong, use directional char; if weak, use smooth char
    const EDGE_BLEND_THRESHOLD: u8 = 50;

    if edge_strength < EDGE_BLEND_THRESHOLD {
        charset.smooth[level]
    } else {
        match direction {
            EdgeDirection::None => charset.smooth[level],
            EdgeDirection::Horizontal => charset.horizontal[level],
            EdgeDirection::Vertical => charset.vertical[level],
            EdgeDirection::DiagonalDown => charset.diagonal_down[level],
            EdgeDirection::DiagonalUp => charset.diagonal_up[level],
        }
    }
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

/// Map a downsampled grayscale image to characters using structure-aware selection.
///
/// This function analyzes local gradients within each character cell and picks
/// characters that match both the brightness AND the edge direction. This produces
/// sharper edges and more recognizable features compared to brightness-only mapping.
///
/// # Arguments
/// * `gray` - Original grayscale image (full resolution)
/// * `img_width` - Width of the source image in pixels
/// * `img_height` - Height of the source image in pixels
/// * `char_width` - Desired output width in characters
/// * `char_height` - Desired output height in characters
/// * `charset` - Structure-aware character set to use
/// * `use_gamma` - Whether to apply gamma correction
///
/// # Returns
/// A vector of characters, one per cell, in row-major order.
pub fn map_structure_aware(
    gray: &[u8],
    img_width: u32,
    img_height: u32,
    char_width: u16,
    char_height: u16,
    charset: &StructureCharset,
    use_gamma: bool,
) -> Vec<char> {
    if char_width == 0 || char_height == 0 || img_width < 3 || img_height < 3 || gray.is_empty() {
        return vec![' '; (char_width as usize) * (char_height as usize)];
    }

    let cell_w = img_width as f32 / char_width as f32;
    let cell_h = img_height as f32 / char_height as f32;

    // Sobel kernels
    let sobel_x: [[i32; 3]; 3] = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]];
    let sobel_y: [[i32; 3]; 3] = [[-1, -2, -1], [0, 0, 0], [1, 2, 1]];

    let mut result = Vec::with_capacity((char_width as usize) * (char_height as usize));

    for cy in 0..char_height {
        for cx in 0..char_width {
            // Calculate pixel bounds for this cell
            let start_x = (cx as f32 * cell_w) as u32;
            let end_x = ((cx + 1) as f32 * cell_w) as u32;
            let start_y = (cy as f32 * cell_h) as u32;
            let end_y = ((cy + 1) as f32 * cell_h) as u32;

            // Calculate average brightness and gradient for the cell
            let mut brightness_sum = 0u32;
            let mut gx_sum: i32 = 0;
            let mut gy_sum: i32 = 0;
            let mut count = 0u32;
            let mut gradient_count = 0u32;

            for py in start_y..end_y {
                for px in start_x..end_x {
                    let idx = (py * img_width + px) as usize;
                    if idx < gray.len() {
                        brightness_sum += gray[idx] as u32;
                        count += 1;

                        // Compute gradient if not on border
                        if px > 0 && px < img_width - 1 && py > 0 && py < img_height - 1 {
                            let mut gx: i32 = 0;
                            let mut gy: i32 = 0;

                            for ky in 0..3 {
                                for kx in 0..3 {
                                    let npx = (px as i32 + kx as i32 - 1) as usize;
                                    let npy = (py as i32 + ky as i32 - 1) as usize;
                                    let nidx = npy * img_width as usize + npx;
                                    if nidx < gray.len() {
                                        let val = gray[nidx] as i32;
                                        gx += val * sobel_x[ky][kx];
                                        gy += val * sobel_y[ky][kx];
                                    }
                                }
                            }
                            gx_sum += gx;
                            gy_sum += gy;
                            gradient_count += 1;
                        }
                    }
                }
            }

            let brightness = if count > 0 {
                (brightness_sum / count) as u8
            } else {
                0
            };
            let brightness = if use_gamma { gamma_correct(brightness) } else { brightness };

            // Average gradient for the cell
            let avg_gx = if gradient_count > 0 { gx_sum / gradient_count as i32 } else { 0 };
            let avg_gy = if gradient_count > 0 { gy_sum / gradient_count as i32 } else { 0 };

            let (edge_strength, direction) = analyze_gradient(avg_gx, avg_gy);
            let ch = get_structure_char(brightness, direction, edge_strength, charset);
            result.push(ch);
        }
    }

    result
}
