//! RGB to grayscale conversion using ITU-R BT.601 luminance formula.

use crate::camera::Frame;

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
