//! ASCII frame types for camera modal display.

/// RGB color for a character cell.
#[derive(Debug, Clone, Copy, Default)]
pub struct CellColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// ASCII-rendered frame for display in the camera modal.
///
/// This struct holds the character grid produced by the ASCII renderer.
/// Each character represents a "cell" of the source image mapped to
/// a brightness level. Optionally includes color data for true-color rendering.
#[derive(Debug, Clone)]
pub struct AsciiFrame {
    /// Character data for the frame (row-major order)
    pub chars: Vec<char>,
    /// Optional color data for each character (same length as chars)
    pub colors: Option<Vec<CellColor>>,
    /// Width in characters
    pub width: u16,
    /// Height in characters
    pub height: u16,
}

impl Default for AsciiFrame {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl AsciiFrame {
    /// Create a new ASCII frame with the given dimensions.
    pub fn new(width: u16, height: u16) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            chars: vec![' '; size],
            colors: None,
            width,
            height,
        }
    }

    /// Create a frame from a character vector.
    pub fn from_chars(chars: Vec<char>, width: u16, height: u16) -> Self {
        Self {
            chars,
            colors: None,
            width,
            height,
        }
    }

    /// Create a frame with characters and colors.
    pub fn from_chars_colored(chars: Vec<char>, colors: Vec<CellColor>, width: u16, height: u16) -> Self {
        Self {
            chars,
            colors: Some(colors),
            width,
            height,
        }
    }

    /// Convert the frame to a string (for rendering).
    ///
    /// Each row is joined by newlines.
    pub fn to_string_display(&self) -> String {
        if self.width == 0 || self.height == 0 {
            return String::new();
        }

        self.chars
            .chunks(self.width as usize)
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_frame_new() {
        let frame = AsciiFrame::new(20, 10);
        assert_eq!(frame.width, 20);
        assert_eq!(frame.height, 10);
        assert_eq!(frame.chars.len(), 200);
        // All chars should be spaces
        assert!(frame.chars.iter().all(|&c| c == ' '));
    }

    #[test]
    fn test_ascii_frame_default() {
        let frame = AsciiFrame::default();
        assert_eq!(frame.width, 0);
        assert_eq!(frame.height, 0);
        assert!(frame.chars.is_empty());
    }

    #[test]
    fn test_ascii_frame_from_chars() {
        let chars = vec!['#', '.', ':', '#', '.', ':'];
        let frame = AsciiFrame::from_chars(chars.clone(), 3, 2);
        assert_eq!(frame.width, 3);
        assert_eq!(frame.height, 2);
        assert_eq!(frame.chars, chars);
    }

    #[test]
    fn test_ascii_frame_to_string_display() {
        let chars = vec!['#', '.', ':', '@', '*', '+'];
        let frame = AsciiFrame::from_chars(chars, 3, 2);
        let s = frame.to_string_display();
        assert_eq!(s, "#.:\n@*+");
    }

    #[test]
    fn test_ascii_frame_to_string_display_empty() {
        let frame = AsciiFrame::new(0, 0);
        assert_eq!(frame.to_string_display(), "");
    }

    #[test]
    fn test_ascii_frame_to_string_display_single_row() {
        let chars = vec!['A', 'B', 'C'];
        let frame = AsciiFrame::from_chars(chars, 3, 1);
        assert_eq!(frame.to_string_display(), "ABC");
    }

    #[test]
    fn test_cell_color_default() {
        let color = CellColor::default();
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn test_ascii_frame_from_chars_colored() {
        let chars = vec!['#', '.'];
        let colors = vec![
            CellColor { r: 255, g: 0, b: 0 },
            CellColor { r: 0, g: 255, b: 0 },
        ];
        let frame = AsciiFrame::from_chars_colored(chars.clone(), colors.clone(), 2, 1);
        assert_eq!(frame.chars, chars);
        assert!(frame.colors.is_some());
        assert_eq!(frame.colors.as_ref().unwrap().len(), 2);
    }
}
