//! Sobel edge detection for sharper ASCII rendering.

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
