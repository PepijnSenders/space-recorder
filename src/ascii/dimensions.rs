//! Dimension calculation for aspect-ratio-correct ASCII rendering.

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
