//! Character set definitions for ASCII rendering.

/// Standard ASCII density ramp (10 levels).
/// Characters ordered from darkest (space) to brightest (@).
/// Works well on dark terminals.
pub const STANDARD_CHARSET: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// Block character set (5 levels).
/// Uses Unicode block characters for higher perceived resolution.
/// Characters ordered from darkest (space) to brightest (full block).
pub const BLOCKS_CHARSET: &[char] = &[' ', '░', '▒', '▓', '█'];

/// Minimal character set (4 levels).
/// Clean, less noisy look.
pub const MINIMAL_CHARSET: &[char] = &[' ', '.', ':', '#'];

/// Character set type for ASCII rendering.
///
/// Allows cycling through different character sets with hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharSet {
    /// Standard ASCII density ramp (10 levels)
    #[default]
    Standard,
    /// Block character set (5 levels) using Unicode blocks
    Blocks,
    /// Minimal character set (4 levels) for a clean look
    Minimal,
    /// Braille character set for highest resolution
    Braille,
}

impl CharSet {
    /// Get the character slice for this charset.
    ///
    /// Note: For Braille, this returns an empty slice since braille
    /// rendering uses a different algorithm (braille::render).
    pub fn chars(&self) -> &'static [char] {
        match self {
            CharSet::Standard => STANDARD_CHARSET,
            CharSet::Blocks => BLOCKS_CHARSET,
            CharSet::Minimal => MINIMAL_CHARSET,
            CharSet::Braille => &[], // Braille uses different rendering
        }
    }

    /// Cycle to the next character set.
    ///
    /// Order: Standard -> Blocks -> Minimal -> Braille -> Standard
    pub fn next(&self) -> Self {
        match self {
            CharSet::Standard => CharSet::Blocks,
            CharSet::Blocks => CharSet::Minimal,
            CharSet::Minimal => CharSet::Braille,
            CharSet::Braille => CharSet::Standard,
        }
    }

    /// Get a human-readable name for the charset.
    pub fn name(&self) -> &'static str {
        match self {
            CharSet::Standard => "standard",
            CharSet::Blocks => "blocks",
            CharSet::Minimal => "minimal",
            CharSet::Braille => "braille",
        }
    }

    /// Check if this charset uses braille rendering.
    pub fn is_braille(&self) -> bool {
        matches!(self, CharSet::Braille)
    }
}
