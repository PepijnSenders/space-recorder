//! Brightness to character mapping.

/// Standard display gamma (sRGB).
pub const GAMMA: f32 = 2.2;

/// Precomputed gamma correction lookup table for fast mapping.
/// Maps linear brightness [0-255] to perceptually-corrected brightness [0-255].
/// Formula: output = (input/255)^(1/2.2) * 255
/// Generated with: (0..256).map(|i| ((i as f64 / 255.0).powf(1.0/2.2) * 255.0).round() as u8)
#[rustfmt::skip]
const GAMMA_LUT: [u8; 256] = [
    0, 21, 28, 34, 39, 43, 46, 50, 53, 56, 59, 61, 64, 66, 68, 70,
    72, 74, 76, 78, 80, 82, 84, 85, 87, 89, 90, 92, 93, 95, 96, 98,
    99, 101, 102, 103, 105, 106, 107, 109, 110, 111, 112, 114, 115, 116, 117, 118,
    119, 120, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135,
    136, 137, 138, 139, 140, 141, 142, 143, 144, 144, 145, 146, 147, 148, 149, 150,
    150, 151, 152, 153, 154, 155, 155, 156, 157, 158, 159, 159, 160, 161, 162, 162,
    163, 164, 165, 165, 166, 167, 168, 168, 169, 170, 171, 171, 172, 173, 173, 174,
    175, 175, 176, 177, 177, 178, 179, 179, 180, 181, 181, 182, 183, 183, 184, 185,
    185, 186, 186, 187, 188, 188, 189, 190, 190, 191, 191, 192, 193, 193, 194, 194,
    195, 196, 196, 197, 197, 198, 199, 199, 200, 200, 201, 201, 202, 203, 203, 204,
    204, 205, 205, 206, 207, 207, 208, 208, 209, 209, 210, 210, 211, 212, 212, 213,
    213, 214, 214, 215, 215, 216, 216, 217, 217, 218, 218, 219, 220, 220, 221, 221,
    222, 222, 223, 223, 224, 224, 225, 225, 226, 226, 227, 227, 228, 228, 229, 229,
    230, 230, 231, 231, 232, 232, 233, 233, 234, 234, 234, 235, 235, 236, 236, 237,
    237, 238, 238, 239, 239, 240, 240, 241, 241, 241, 242, 242, 243, 243, 244, 244,
    245, 245, 246, 246, 246, 247, 247, 248, 248, 249, 249, 250, 250, 250, 251, 255,
];

/// Apply gamma correction to a brightness value.
/// Converts linear brightness to perceptually-correct brightness.
#[inline]
pub fn gamma_correct(linear: u8) -> u8 {
    GAMMA_LUT[linear as usize]
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

/// Map brightness values to ASCII characters with gamma correction.
///
/// Same as `map_to_chars` but applies gamma correction for perceptually
/// accurate brightness mapping. This produces better results for photographic
/// images by accounting for human visual perception.
///
/// # Arguments
/// * `brightness` - Brightness values (0-255), one per character cell
/// * `charset` - Character set to use, ordered from darkest to brightest
/// * `invert` - If true, invert brightness before mapping (for light terminals)
///
/// # Returns
/// A vector of characters, one per input brightness value.
pub fn map_to_chars_gamma(brightness: &[u8], charset: &[char], invert: bool) -> Vec<char> {
    if charset.is_empty() {
        return vec![' '; brightness.len()];
    }

    let levels = charset.len();
    brightness
        .iter()
        .map(|&b| {
            let b = if invert { 255 - b } else { b };
            let corrected = gamma_correct(b);
            let idx = (corrected as usize * (levels - 1)) / 255;
            charset[idx]
        })
        .collect()
}

/// Map brightness values to ASCII characters with gamma correction, in-place.
///
/// Same as `map_to_chars_into` but applies gamma correction for perceptually
/// accurate brightness mapping.
///
/// # Arguments
/// * `brightness` - Brightness values (0-255), one per character cell
/// * `charset` - Character set to use, ordered from darkest to brightest
/// * `invert` - If true, invert brightness before mapping (for light terminals)
/// * `buffer` - A mutable buffer to store the result
///
/// # Returns
/// The number of characters written to the buffer.
pub fn map_to_chars_gamma_into(
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
        let corrected = gamma_correct(b);
        let idx = (corrected as usize * (levels - 1)) / 255;
        buffer.push(charset[idx]);
    }

    brightness.len()
}

/// Map brightness values to ASCII characters using Floyd-Steinberg dithering.
///
/// Dithering distributes quantization error to neighboring pixels, creating
/// smoother gradients with fewer visible bands. Best for images with smooth
/// transitions like skies, shadows, or skin tones.
///
/// # Arguments
/// * `brightness` - Brightness values (0-255), one per character cell
/// * `width` - Width of the character grid
/// * `height` - Height of the character grid
/// * `charset` - Character set to use, ordered from darkest to brightest
/// * `invert` - If true, invert brightness before mapping
/// * `use_gamma` - If true, apply gamma correction before dithering
///
/// # Returns
/// A vector of characters, one per input brightness value.
pub fn map_to_chars_dithered(
    brightness: &[u8],
    width: u16,
    height: u16,
    charset: &[char],
    invert: bool,
    use_gamma: bool,
) -> Vec<char> {
    if charset.is_empty() || width == 0 || height == 0 {
        return vec![' '; brightness.len()];
    }

    let w = width as usize;
    let h = height as usize;
    let levels = charset.len();

    // Work buffer with signed values for error diffusion
    let mut buffer: Vec<i16> = brightness
        .iter()
        .map(|&b| {
            let b = if invert { 255 - b } else { b };
            let b = if use_gamma { gamma_correct(b) } else { b };
            b as i16
        })
        .collect();

    let mut result = vec![' '; w * h];

    // Floyd-Steinberg error diffusion pattern:
    //       [*] 7/16
    // 3/16 5/16 1/16
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let old_val = buffer[idx].clamp(0, 255) as u8;

            // Quantize to nearest character level
            let char_idx = (old_val as usize * (levels - 1)) / 255;
            result[idx] = charset[char_idx];

            // Calculate quantized value (what this character represents)
            let new_val = (char_idx * 255 / (levels - 1)) as i16;
            let error = buffer[idx] - new_val;

            // Distribute error to neighbors
            // Right: 7/16
            if x + 1 < w {
                buffer[idx + 1] += error * 7 / 16;
            }
            // Bottom-left: 3/16
            if y + 1 < h && x > 0 {
                buffer[idx + w - 1] += error * 3 / 16;
            }
            // Bottom: 5/16
            if y + 1 < h {
                buffer[idx + w] += error * 5 / 16;
            }
            // Bottom-right: 1/16
            if y + 1 < h && x + 1 < w {
                buffer[idx + w + 1] += error / 16;
            }
        }
    }

    result
}

/// Map brightness values to ASCII characters using ordered (Bayer) dithering.
///
/// Ordered dithering uses a fixed threshold pattern, which is faster than
/// Floyd-Steinberg and doesn't have directional artifacts. Good for
/// real-time rendering where speed matters.
///
/// # Arguments
/// * `brightness` - Brightness values (0-255), one per character cell
/// * `width` - Width of the character grid
/// * `charset` - Character set to use, ordered from darkest to brightest
/// * `invert` - If true, invert brightness before mapping
/// * `use_gamma` - If true, apply gamma correction before dithering
///
/// # Returns
/// A vector of characters, one per input brightness value.
pub fn map_to_chars_ordered_dither(
    brightness: &[u8],
    width: u16,
    charset: &[char],
    invert: bool,
    use_gamma: bool,
) -> Vec<char> {
    if charset.is_empty() || width == 0 {
        return vec![' '; brightness.len()];
    }

    // 4x4 Bayer matrix (normalized to 0-255 range)
    #[rustfmt::skip]
    const BAYER_4X4: [[i16; 4]; 4] = [
        [  0, 128,  32, 160],
        [192,  64, 224,  96],
        [ 48, 176,  16, 144],
        [240, 112, 208,  80],
    ];

    let w = width as usize;
    let levels = charset.len();
    // Threshold spread based on number of levels
    let spread = 255 / levels as i16;

    brightness
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            let b = if invert { 255 - b } else { b };
            let b = if use_gamma { gamma_correct(b) } else { b };

            let x = i % w;
            let y = i / w;
            let threshold = BAYER_4X4[y % 4][x % 4];

            // Add threshold offset (scaled by spread) to brightness
            let adjusted = (b as i16) + (threshold - 128) * spread / 256;
            let adjusted = adjusted.clamp(0, 255) as u8;

            let idx = (adjusted as usize * (levels - 1)) / 255;
            charset[idx]
        })
        .collect()
}
