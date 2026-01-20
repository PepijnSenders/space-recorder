//! Status bar for displaying camera state at the bottom of the screen.

use super::modal::CameraModal;

/// Status bar for displaying camera state at the bottom of the screen.
///
/// Shows: camera on/off | position | size | charset
#[derive(Debug, Clone)]
pub struct StatusBar {
    /// Whether the status bar is visible
    pub visible: bool,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    /// Create a new status bar with default settings (visible).
    pub fn new() -> Self {
        Self { visible: true }
    }

    /// Create a status bar with the specified visibility.
    pub fn with_visibility(visible: bool) -> Self {
        Self { visible }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Format the status bar text based on the camera modal state.
    ///
    /// Format: " cam:on/off | position | size | charset "
    pub fn format(&self, modal: &CameraModal) -> String {
        format!(
            " {} | {} | {} | {} ",
            if modal.visible { "cam:on" } else { "cam:off" },
            modal.position.name(),
            modal.size.name(),
            modal.charset.name(),
        )
    }
}
