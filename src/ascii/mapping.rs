//! Brightness to character mapping.

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
