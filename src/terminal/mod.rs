//! Terminal management module - raw mode, TUI wrapper, modal types, and cleanup.

mod frame;
mod pty_buffer;
mod raw_mode;

// Re-export public types from submodules
pub use frame::{AsciiFrame, CellColor};
pub use pty_buffer::PtyBuffer;
pub use raw_mode::RawModeGuard;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::io::{self, Stdout};

use crate::ascii::CharSet;
use raw_mode::{RAW_MODE_ACTIVE, install_panic_hook};
use std::sync::atomic::Ordering;

// ==================== Tui ====================

/// TUI wrapper that manages the ratatui terminal with crossterm backend.
///
/// This struct handles:
/// - Entering raw mode and alternate screen on creation
/// - Restoring terminal state on drop (or explicit restore)
/// - Panic recovery (terminal is restored even if the app panics)
///
/// # Example
///
/// ```ignore
/// let mut tui = Tui::new()?;
///
/// // Use tui.terminal() to draw with ratatui
/// tui.terminal().draw(|frame| {
///     // render widgets
/// })?;
///
/// // Terminal is restored automatically on drop, or explicitly:
/// tui.restore()?;
/// ```
pub struct Tui {
    /// The ratatui terminal handle
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Whether this TUI is responsible for cleanup
    active: bool,
}

impl Tui {
    /// Create a new TUI, entering raw mode and alternate screen.
    ///
    /// This will:
    /// 1. Install a panic hook (if not already installed)
    /// 2. Enable raw mode
    /// 3. Enter alternate screen (preserves original terminal content)
    /// 4. Create the ratatui terminal with crossterm backend
    ///
    /// # Returns
    /// A new Tui instance that will restore terminal state on drop.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Enabling raw mode fails
    /// - Entering alternate screen fails
    /// - Creating the terminal fails
    pub fn new() -> io::Result<Self> {
        // Install panic hook before entering raw mode
        install_panic_hook();

        // Enter raw mode
        enable_raw_mode()?;
        RAW_MODE_ACTIVE.store(true, Ordering::SeqCst);

        // Enter alternate screen
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

        // Create ratatui terminal with crossterm backend
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            active: true,
        })
    }

    /// Get a mutable reference to the underlying ratatui terminal.
    ///
    /// Use this to draw frames with ratatui's `terminal.draw()` method.
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Restore the terminal to its original state.
    ///
    /// This will:
    /// 1. Leave alternate screen
    /// 2. Disable raw mode
    /// 3. Show the cursor
    ///
    /// After calling this, the Tui's drop will be a no-op.
    ///
    /// # Errors
    /// Returns an error if any cleanup step fails.
    pub fn restore(&mut self) -> io::Result<()> {
        if self.active {
            self.active = false;
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);

            // Leave alternate screen
            crossterm::execute!(
                self.terminal.backend_mut(),
                crossterm::terminal::LeaveAlternateScreen,
            )?;

            // Disable raw mode
            disable_raw_mode()?;

            // Show cursor (might be hidden during TUI operation)
            self.terminal.show_cursor()?;
        }
        Ok(())
    }

    /// Check if the TUI is still active (not yet restored).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Render the camera modal overlay on the terminal.
    ///
    /// This method renders the camera modal at its configured position and size.
    /// It clears the modal area first to create an overlay effect, then renders
    /// the ASCII frame content with an optional border.
    ///
    /// # Arguments
    /// * `modal` - The camera modal state to render
    ///
    /// # Returns
    /// Returns an error if terminal drawing fails.
    pub fn render_camera_modal(&mut self, modal: &CameraModal) -> io::Result<()> {
        if !modal.visible {
            return Ok(());
        }

        self.terminal.draw(|frame| {
            let area = frame.area();
            let modal_rect = modal.calculate_rect(area);

            // Clear the modal area (important for overlay effect)
            frame.render_widget(Clear, modal_rect);

            // Build the block (with or without border)
            let block = if modal.border {
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
            } else {
                Block::default()
            };

            // Calculate inner area for content
            let inner = block.inner(modal_rect);

            // Render the block first
            frame.render_widget(block, modal_rect);

            // Render ASCII frame content
            if let Some(ref ascii_frame) = modal.frame {
                let text = ascii_frame.to_string_display();
                let paragraph = Paragraph::new(text).style(Style::default().fg(Color::White));
                frame.render_widget(paragraph, inner);
            }
        })?;

        Ok(())
    }

    /// Render a full frame with PTY output and camera modal overlay.
    ///
    /// This method renders both layers:
    /// 1. PTY output (background)
    /// 2. Camera modal (floating overlay)
    ///
    /// # Arguments
    /// * `pty_buffer` - The PTY output buffer to render
    /// * `modal` - The camera modal state to render
    ///
    /// # Returns
    /// Returns an error if terminal drawing fails.
    pub fn render_frame(&mut self, pty_buffer: &PtyBuffer, modal: &CameraModal) -> io::Result<()> {
        self.render_frame_with_status(pty_buffer, modal, None)
    }

    /// Render a full frame with PTY output, camera modal overlay, and optional status bar.
    ///
    /// This method renders all layers:
    /// 1. PTY output (background)
    /// 2. Camera modal (floating overlay)
    /// 3. Status bar (bottom, if visible)
    ///
    /// # Arguments
    /// * `pty_buffer` - The PTY output buffer to render
    /// * `modal` - The camera modal state to render
    /// * `status_bar` - Optional status bar to render at the bottom
    ///
    /// # Returns
    /// Returns an error if terminal drawing fails.
    pub fn render_frame_with_status(
        &mut self,
        pty_buffer: &PtyBuffer,
        modal: &CameraModal,
        status_bar: Option<&StatusBar>,
    ) -> io::Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();

            // Calculate main area (excluding status bar if visible)
            let show_status = status_bar.is_some_and(|sb| sb.visible);
            let main_area = if show_status {
                Rect {
                    height: area.height.saturating_sub(1),
                    ..area
                }
            } else {
                area
            };

            // Layer 1: PTY output (full screen, minus status bar)
            // For now, render raw PTY content (pass-through mode)
            let pty_content = pty_buffer.content();
            let pty_paragraph = Paragraph::new(pty_content);
            frame.render_widget(pty_paragraph, main_area);

            // Layer 2: Camera modal (floating overlay within main area)
            if modal.visible {
                let modal_rect = modal.calculate_rect(main_area);

                // Clear the modal area
                frame.render_widget(Clear, modal_rect);

                // Build the block (with or without border)
                let block = if modal.border {
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray))
                } else {
                    Block::default()
                };

                // Calculate inner area for content
                let inner = block.inner(modal_rect);

                // Render the block first
                frame.render_widget(block, modal_rect);

                // Render ASCII frame content
                if let Some(ref ascii_frame) = modal.frame {
                    let text = ascii_frame.to_string_display();
                    let paragraph = Paragraph::new(text).style(Style::default().fg(Color::White));
                    frame.render_widget(paragraph, inner);
                }
            }

            // Layer 3: Status bar (bottom)
            if let Some(sb) = status_bar
                && sb.visible
            {
                let status_area = Rect {
                    x: area.x,
                    y: area.y + area.height.saturating_sub(1),
                    width: area.width,
                    height: 1,
                };
                let status_text = sb.format(modal);
                let status_paragraph = Paragraph::new(status_text)
                    .style(Style::default().fg(Color::Black).bg(Color::White));
                frame.render_widget(status_paragraph, status_area);
            }
        })?;

        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);

            // Best-effort cleanup - ignore errors during drop
            let _ = crossterm::execute!(
                self.terminal.backend_mut(),
                crossterm::terminal::LeaveAlternateScreen,
            );
            let _ = disable_raw_mode();
            let _ = self.terminal.show_cursor();
        }
    }
}

// ==================== Modal Types ====================

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

// ==================== Status Bar ====================

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

#[cfg(test)]
mod tests {
    use super::*;

    // Note: TTY-dependent tests for Tui (new, restore, drop) are kept here
    // as they require internal access. Pure logic tests for modal types
    // have been moved to tests/terminal_unit.rs.

    #[test]
    fn test_tui_new_and_drop() {
        // Skip test if not running in a terminal (e.g., CI environment)
        // TUI requires a real TTY
        match Tui::new() {
            Ok(tui) => {
                assert!(tui.is_active());
                assert!(RAW_MODE_ACTIVE.load(Ordering::SeqCst));
                drop(tui);
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_tui_manual_restore() {
        // Skip test if not running in a terminal
        match Tui::new() {
            Ok(mut tui) => {
                assert!(tui.is_active());
                assert!(RAW_MODE_ACTIVE.load(Ordering::SeqCst));

                // Manual restore
                tui.restore().expect("Should restore terminal");
                assert!(!tui.is_active());
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));

                // Drop should be a no-op now
                drop(tui);
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_tui_double_restore() {
        // Skip test if not running in a terminal
        match Tui::new() {
            Ok(mut tui) => {
                // First restore
                tui.restore().expect("Should restore terminal");
                assert!(!tui.is_active());

                // Second restore should be a no-op (not an error)
                tui.restore().expect("Second restore should not fail");
                assert!(!tui.is_active());
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_tui_terminal_access() {
        // Skip test if not running in a terminal
        match Tui::new() {
            Ok(mut tui) => {
                // Should be able to get terminal reference
                let _terminal = tui.terminal();

                // Cleanup
                tui.restore().expect("Should restore terminal");
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }
}
