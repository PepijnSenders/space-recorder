//! CLI enum types for position, size, and character set options.

use clap::ValueEnum;

use crate::ascii;
use crate::terminal::{ModalPosition, ModalSize};

/// Camera modal position on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    Center,
}

impl From<Position> for ModalPosition {
    fn from(p: Position) -> Self {
        match p {
            Position::TopLeft => ModalPosition::TopLeft,
            Position::TopRight => ModalPosition::TopRight,
            Position::BottomLeft => ModalPosition::BottomLeft,
            Position::BottomRight => ModalPosition::BottomRight,
            Position::Center => ModalPosition::Center,
        }
    }
}

/// Camera modal size preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Size {
    #[default]
    Small,
    Medium,
    Large,
    Xlarge,
    Huge,
}

impl From<Size> for ModalSize {
    fn from(s: Size) -> Self {
        match s {
            Size::Small => ModalSize::Small,
            Size::Medium => ModalSize::Medium,
            Size::Large => ModalSize::Large,
            Size::Xlarge => ModalSize::XLarge,
            Size::Huge => ModalSize::Huge,
        }
    }
}

/// ASCII character set for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum CharacterSet {
    #[default]
    Standard,
    Blocks,
    Minimal,
    Braille,
}

impl From<CharacterSet> for ascii::CharSet {
    fn from(c: CharacterSet) -> Self {
        match c {
            CharacterSet::Standard => ascii::CharSet::Standard,
            CharacterSet::Blocks => ascii::CharSet::Blocks,
            CharacterSet::Minimal => ascii::CharSet::Minimal,
            CharacterSet::Braille => ascii::CharSet::Braille,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_modal_position() {
        assert_eq!(
            ModalPosition::from(Position::TopLeft),
            ModalPosition::TopLeft
        );
        assert_eq!(
            ModalPosition::from(Position::TopRight),
            ModalPosition::TopRight
        );
        assert_eq!(
            ModalPosition::from(Position::BottomLeft),
            ModalPosition::BottomLeft
        );
        assert_eq!(
            ModalPosition::from(Position::BottomRight),
            ModalPosition::BottomRight
        );
        assert_eq!(ModalPosition::from(Position::Center), ModalPosition::Center);
    }

    #[test]
    fn test_size_to_modal_size() {
        assert_eq!(ModalSize::from(Size::Small), ModalSize::Small);
        assert_eq!(ModalSize::from(Size::Medium), ModalSize::Medium);
        assert_eq!(ModalSize::from(Size::Large), ModalSize::Large);
    }

    #[test]
    fn test_charset_to_ascii_charset() {
        assert_eq!(
            ascii::CharSet::from(CharacterSet::Standard),
            ascii::CharSet::Standard
        );
        assert_eq!(
            ascii::CharSet::from(CharacterSet::Blocks),
            ascii::CharSet::Blocks
        );
        assert_eq!(
            ascii::CharSet::from(CharacterSet::Minimal),
            ascii::CharSet::Minimal
        );
        assert_eq!(
            ascii::CharSet::from(CharacterSet::Braille),
            ascii::CharSet::Braille
        );
    }
}
