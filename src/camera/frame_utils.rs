//! Frame conversion and transformation utilities.

use nokhwa::pixel_format::RgbFormat;
use std::time::Instant;

use super::types::{Frame, FrameFormat};

/// Convert a nokhwa buffer to our RGB Frame format.
///
/// Handles various camera formats (MJPEG, YUYV, NV12, etc.) by using
/// nokhwa's built-in decode_image which automatically converts from
/// the camera's native format to RGB.
///
/// Returns `None` if the conversion fails (unsupported format or corrupt data).
pub fn convert_to_rgb(buffer: &nokhwa::Buffer) -> Option<Frame> {
    let decoded = buffer.decode_image::<RgbFormat>().ok()?;
    let resolution = buffer.resolution();

    Some(Frame {
        data: decoded.into_raw(),
        width: resolution.width(),
        height: resolution.height(),
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    })
}

/// Mirror a frame horizontally (flip left-right) for selfie mode.
pub fn mirror_horizontal(frame: &mut Frame) {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let bpp = frame.bytes_per_pixel();

    for y in 0..height {
        let row_start = y * width * bpp;
        let row = &mut frame.data[row_start..row_start + width * bpp];

        // Swap pixels from left and right
        for x in 0..width / 2 {
            let left = x * bpp;
            let right = (width - 1 - x) * bpp;
            for i in 0..bpp {
                row.swap(left + i, right + i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_horizontal_2x1() {
        // Simple 2x1 image: pixel A (R=1,G=2,B=3) and pixel B (R=4,G=5,B=6)
        let mut frame = Frame {
            data: vec![1, 2, 3, 4, 5, 6],
            width: 2,
            height: 1,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        mirror_horizontal(&mut frame);
        // After mirroring: pixel B, pixel A
        assert_eq!(frame.data, vec![4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn test_mirror_horizontal_3x2() {
        // 3x2 image:
        // Row 0: [A, B, C]
        // Row 1: [D, E, F]
        let mut frame = Frame {
            data: vec![
                1, 1, 1, 2, 2, 2, 3, 3, 3, // Row 0: A, B, C
                4, 4, 4, 5, 5, 5, 6, 6, 6, // Row 1: D, E, F
            ],
            width: 3,
            height: 2,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        mirror_horizontal(&mut frame);
        // After mirroring:
        // Row 0: [C, B, A]
        // Row 1: [F, E, D]
        assert_eq!(
            frame.data,
            vec![
                3, 3, 3, 2, 2, 2, 1, 1, 1, // Row 0: C, B, A
                6, 6, 6, 5, 5, 5, 4, 4, 4, // Row 1: F, E, D
            ]
        );
    }

    #[test]
    fn test_mirror_horizontal_single_pixel() {
        // Edge case: 1x1 image should remain unchanged
        let mut frame = Frame {
            data: vec![1, 2, 3],
            width: 1,
            height: 1,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        mirror_horizontal(&mut frame);
        assert_eq!(frame.data, vec![1, 2, 3]);
    }
}
