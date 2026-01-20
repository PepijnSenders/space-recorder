//! Camera modal types for the TUI overlay.
//!
//! Contains position, size, and state types for the floating camera preview.

use ratatui::layout::Rect;

use super::frame::AsciiFrame;
use crate::ascii::CharSet;

/// Position of the camera modal on screen.
///
/// The modal can be positioned in any of the four corners or centered.
/// Each position maintains a 1-character margin from the container edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalPosition {
    /// Top-left corner with 1-char margin
    TopLeft,
    /// Top-right corner with 1-char margin
    TopRight,
    /// Bottom-left corner with 1-char margin
    BottomLeft,
    /// Bottom-right corner with 1-char margin (default for selfie view)
    #[default]
    BottomRight,
    /// Centered in the container
    Center,
}

impl ModalPosition {
    /// Calculate the rectangle for the modal within a container.
    ///
    /// Returns a `Rect` positioned according to this variant, with:
    /// - 1-character margin from edges (except Center which has no margin)
    /// - Clamped to container boundaries
    ///
    /// # Arguments
    /// * `container` - The available area to position within
    /// * `width` - Desired width of the modal
    /// * `height` - Desired height of the modal
    pub fn calculate_rect(&self, container: Rect, width: u16, height: u16) -> Rect {
        const MARGIN: u16 = 1;

        // Clamp dimensions to container size (with margin space)
        let max_width = container.width.saturating_sub(MARGIN * 2);
        let max_height = container.height.saturating_sub(MARGIN * 2);
        let width = width.min(max_width);
        let height = height.min(max_height);

        let (x, y) = match self {
            ModalPosition::TopLeft => (container.x + MARGIN, container.y + MARGIN),
            ModalPosition::TopRight => (
                container.x + container.width.saturating_sub(width + MARGIN),
                container.y + MARGIN,
            ),
            ModalPosition::BottomLeft => (
                container.x + MARGIN,
                container.y + container.height.saturating_sub(height + MARGIN),
            ),
            ModalPosition::BottomRight => (
                container.x + container.width.saturating_sub(width + MARGIN),
                container.y + container.height.saturating_sub(height + MARGIN),
            ),
            ModalPosition::Center => (
                container.x + (container.width.saturating_sub(width)) / 2,
                container.y + (container.height.saturating_sub(height)) / 2,
            ),
        };

        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Cycle to the next position.
    ///
    /// Order: TopLeft -> TopRight -> BottomRight -> BottomLeft -> Center -> TopLeft
    pub fn next(&self) -> Self {
        match self {
            ModalPosition::TopLeft => ModalPosition::TopRight,
            ModalPosition::TopRight => ModalPosition::BottomRight,
            ModalPosition::BottomRight => ModalPosition::BottomLeft,
            ModalPosition::BottomLeft => ModalPosition::Center,
            ModalPosition::Center => ModalPosition::TopLeft,
        }
    }

    /// Get a human-readable name for the position.
    pub fn name(&self) -> &'static str {
        match self {
            ModalPosition::TopLeft => "top-left",
            ModalPosition::TopRight => "top-right",
            ModalPosition::BottomLeft => "bottom-left",
            ModalPosition::BottomRight => "bottom-right",
            ModalPosition::Center => "center",
        }
    }
}

/// Size preset for the camera modal.
///
/// Each size includes space for a border (2 chars total for width/height).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalSize {
    /// Small: 20x10 inner (22x12 with border)
    #[default]
    Small,
    /// Medium: 40x20 inner (42x22 with border)
    Medium,
    /// Large: 60x30 inner (62x32 with border)
    Large,
    /// XLarge: 80x40 inner (82x42 with border)
    XLarge,
    /// Huge: 120x60 inner (122x62 with border)
    Huge,
}

impl ModalSize {
    /// Get the outer dimensions (including border).
    ///
    /// Returns (width, height) in characters.
    pub fn dimensions(&self) -> (u16, u16) {
        match self {
            ModalSize::Small => (22, 12),
            ModalSize::Medium => (42, 22),
            ModalSize::Large => (62, 32),
            ModalSize::XLarge => (82, 42),
            ModalSize::Huge => (122, 62),
        }
    }

    /// Get the inner dimensions (content area without border).
    ///
    /// Returns (width, height) in characters.
    pub fn inner_dimensions(&self) -> (u16, u16) {
        match self {
            ModalSize::Small => (20, 10),
            ModalSize::Medium => (40, 20),
            ModalSize::Large => (60, 30),
            ModalSize::XLarge => (80, 40),
            ModalSize::Huge => (120, 60),
        }
    }

    /// Cycle to the next size.
    ///
    /// Order: Small -> Medium -> Large -> XLarge -> Huge -> Small
    pub fn next(&self) -> Self {
        match self {
            ModalSize::Small => ModalSize::Medium,
            ModalSize::Medium => ModalSize::Large,
            ModalSize::Large => ModalSize::XLarge,
            ModalSize::XLarge => ModalSize::Huge,
            ModalSize::Huge => ModalSize::Small,
        }
    }

    /// Get a human-readable name for the size.
    pub fn name(&self) -> &'static str {
        match self {
            ModalSize::Small => "small",
            ModalSize::Medium => "medium",
            ModalSize::Large => "large",
            ModalSize::XLarge => "xlarge",
            ModalSize::Huge => "huge",
        }
    }
}

/// Camera modal state for the TUI overlay.
///
/// Controls the floating camera preview window that displays
/// ASCII-rendered webcam output over the terminal.
#[derive(Debug)]
pub struct CameraModal {
    /// Whether the camera modal is visible
    pub visible: bool,
    /// Position on screen
    pub position: ModalPosition,
    /// Size preset
    pub size: ModalSize,
    /// Current ASCII frame to display
    pub frame: Option<AsciiFrame>,
    /// Whether to show a border around the modal
    pub border: bool,
    /// Character set for ASCII rendering
    pub charset: CharSet,
    /// Transparency level (0-100, higher = more transparent)
    /// Dark pixels below this threshold are skipped
    pub transparency: u8,
}

impl Default for CameraModal {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraModal {
    /// Create a new camera modal with default settings.
    ///
    /// Defaults: visible=false, position=BottomRight, size=Small, border=false,
    /// charset=Standard, transparency=80
    pub fn new() -> Self {
        Self {
            visible: false,
            position: ModalPosition::BottomRight,
            size: ModalSize::Small,
            frame: None,
            border: false,
            charset: CharSet::default(),
            transparency: 80,
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Cycle to the next position.
    pub fn cycle_position(&mut self) {
        self.position = self.position.next();
    }

    /// Cycle to the next size.
    pub fn cycle_size(&mut self) {
        self.size = self.size.next();
    }

    /// Cycle to the next character set.
    pub fn cycle_charset(&mut self) {
        self.charset = self.charset.next();
    }

    /// Cycle transparency in steps of 10 (0 -> 10 -> 20 -> ... -> 100 -> 0).
    pub fn cycle_transparency(&mut self) {
        self.transparency = if self.transparency >= 100 {
            0
        } else {
            self.transparency + 10
        };
    }

    /// Calculate the rectangle for this modal in the given container.
    pub fn calculate_rect(&self, container: Rect) -> Rect {
        let (width, height) = self.size.dimensions();
        self.position.calculate_rect(container, width, height)
    }

    /// Update the ASCII frame.
    pub fn set_frame(&mut self, frame: AsciiFrame) {
        self.frame = Some(frame);
    }

    /// Clear the ASCII frame.
    pub fn clear_frame(&mut self) {
        self.frame = None;
    }
}
