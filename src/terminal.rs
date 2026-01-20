//! Terminal management module - raw mode, TUI wrapper, and cleanup

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::ascii::CharSet;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};

/// Static flag to track if raw mode is active (for panic handler)
static RAW_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Guard that ensures terminal is restored to normal mode on drop.
/// This handles both normal exits and panics.
pub struct RawModeGuard {
    /// Whether this guard is responsible for cleanup
    active: bool,
}

impl RawModeGuard {
    /// Enter raw mode and return a guard that will restore it on drop.
    ///
    /// # Returns
    /// A guard that will disable raw mode when dropped
    ///
    /// # Errors
    /// Returns an error if enabling raw mode fails
    pub fn enter() -> std::io::Result<Self> {
        // Install panic hook before entering raw mode
        install_panic_hook();

        enable_raw_mode()?;
        RAW_MODE_ACTIVE.store(true, Ordering::SeqCst);

        Ok(Self { active: true })
    }

    /// Manually exit raw mode without dropping the guard.
    /// After calling this, the guard's drop will be a no-op.
    pub fn exit(&mut self) -> std::io::Result<()> {
        if self.active {
            self.active = false;
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);
            disable_raw_mode()?;
        }
        Ok(())
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);
            // Best-effort cleanup - ignore errors during drop
            let _ = disable_raw_mode();
        }
    }
}

/// Install a panic hook that restores terminal state before panicking.
/// This ensures the terminal is usable even if the app panics.
fn install_panic_hook() {
    // Only install once - check if we've already installed
    static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

    if HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return; // Already installed
    }

    let original_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal before showing panic message
        if RAW_MODE_ACTIVE.load(Ordering::SeqCst) {
            // Leave alternate screen first
            let _ = crossterm::execute!(
                io::stdout(),
                crossterm::terminal::LeaveAlternateScreen,
            );
            let _ = disable_raw_mode();
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);
        }

        // Call the original panic hook to print the panic message
        original_hook(panic_info);
    }));
}

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
                let paragraph = Paragraph::new(text)
                    .style(Style::default().fg(Color::White));
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
    pub fn render_frame(
        &mut self,
        pty_buffer: &PtyBuffer,
        modal: &CameraModal,
    ) -> io::Result<()> {
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
                    let paragraph = Paragraph::new(text)
                        .style(Style::default().fg(Color::White));
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

/// Buffer for storing PTY output.
///
/// This buffer accumulates raw PTY output and provides a view for rendering.
/// For MVP, this stores the raw output string that gets rendered as a Paragraph.
/// Future versions may implement VT100 parsing for proper terminal emulation.
#[derive(Debug)]
pub struct PtyBuffer {
    /// Raw output content (accumulated from PTY)
    content: String,
    /// Scroll offset (lines from the end)
    scroll: u16,
    /// Maximum number of lines to keep (prevents unbounded growth)
    max_lines: usize,
}

impl Default for PtyBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyBuffer {
    /// Create a new empty PTY buffer.
    pub fn new() -> Self {
        Self {
            content: String::new(),
            scroll: 0,
            max_lines: 10_000, // Keep last 10k lines by default
        }
    }

    /// Create a new buffer with a custom max lines limit.
    pub fn with_max_lines(max_lines: usize) -> Self {
        Self {
            content: String::new(),
            scroll: 0,
            max_lines,
        }
    }

    /// Append raw bytes from PTY output.
    ///
    /// Converts bytes to string (lossy for non-UTF8) and appends to buffer.
    /// Trims buffer to max_lines if exceeded.
    pub fn append(&mut self, data: &[u8]) {
        // Convert bytes to string, replacing invalid UTF-8 sequences
        let text = String::from_utf8_lossy(data);
        self.content.push_str(&text);

        // Trim to max_lines if exceeded
        self.trim_to_max_lines();
    }

    /// Append a string directly.
    pub fn append_str(&mut self, text: &str) {
        self.content.push_str(text);
        self.trim_to_max_lines();
    }

    /// Clear the buffer contents.
    pub fn clear(&mut self) {
        self.content.clear();
        self.scroll = 0;
    }

    /// Get the raw content as a string slice.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the current scroll offset.
    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Set the scroll offset.
    pub fn set_scroll(&mut self, scroll: u16) {
        self.scroll = scroll;
    }

    /// Scroll up by the given number of lines.
    pub fn scroll_up(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_add(lines);
    }

    /// Scroll down by the given number of lines.
    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_sub(lines);
    }

    /// Get the number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Trim the buffer to keep only the last max_lines lines.
    fn trim_to_max_lines(&mut self) {
        let line_count = self.content.lines().count();
        if line_count > self.max_lines {
            // Find the byte index where we should start keeping content
            let lines_to_remove = line_count - self.max_lines;
            let mut lines_seen = 0;
            let mut byte_index = 0;

            for (i, c) in self.content.char_indices() {
                if c == '\n' {
                    lines_seen += 1;
                    if lines_seen >= lines_to_remove {
                        byte_index = i + 1; // Start after the newline
                        break;
                    }
                }
            }

            if byte_index > 0 && byte_index < self.content.len() {
                self.content = self.content[byte_index..].to_string();
            }
        }
    }

    /// Get visible content for rendering (accounting for scroll offset).
    ///
    /// Returns lines from the end of the buffer, offset by scroll position.
    /// This is suitable for rendering in a fixed-height viewport.
    pub fn visible_content(&self, viewport_height: usize) -> String {
        if self.content.is_empty() || viewport_height == 0 {
            return String::new();
        }

        let lines: Vec<&str> = self.content.lines().collect();
        let total_lines = lines.len();

        // Calculate the range of lines to show
        let scroll = self.scroll as usize;
        let end = total_lines.saturating_sub(scroll);
        let start = end.saturating_sub(viewport_height);

        lines[start..end].join("\n")
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

        Rect { x, y, width, height }
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
            ModalSize::Small => (22, 12),   // 20x10 + 2 for border
            ModalSize::Medium => (42, 22),  // 40x20 + 2 for border
            ModalSize::Large => (62, 32),   // 60x30 + 2 for border
            ModalSize::XLarge => (82, 42),  // 80x40 + 2 for border
            ModalSize::Huge => (122, 62),   // 120x60 + 2 for border
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

    #[test]
    fn test_raw_mode_guard_enter_and_drop() {
        // Skip test if not running in a terminal (e.g., CI environment)
        // Raw mode requires a real TTY
        match RawModeGuard::enter() {
            Ok(guard) => {
                assert!(RAW_MODE_ACTIVE.load(Ordering::SeqCst));
                drop(guard);
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_raw_mode_guard_manual_exit() {
        // Skip test if not running in a terminal
        match RawModeGuard::enter() {
            Ok(mut guard) => {
                assert!(RAW_MODE_ACTIVE.load(Ordering::SeqCst));

                // Manual exit
                guard.exit().expect("Should exit raw mode");
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));

                // Drop should be a no-op now
                drop(guard);
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_panic_hook_installation() {
        // Just verify the hook can be installed without crashing
        install_panic_hook();
        install_panic_hook(); // Second call should be no-op
    }

    #[test]
    fn test_raw_mode_active_flag_initial_state() {
        // The flag should be false initially (or after previous tests cleanup)
        // Note: This test may be affected by other tests running in parallel
        // but the atomic flag should still be valid
        let _ = RAW_MODE_ACTIVE.load(Ordering::SeqCst);
        // Just verify we can read the flag without panicking
    }

    // ==================== Tui Tests ====================

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

    // ==================== PtyBuffer Tests ====================

    #[test]
    fn test_pty_buffer_new() {
        let buf = PtyBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.content(), "");
        assert_eq!(buf.scroll(), 0);
        assert_eq!(buf.line_count(), 0);
    }

    #[test]
    fn test_pty_buffer_append_bytes() {
        let mut buf = PtyBuffer::new();
        buf.append(b"Hello, world!\n");
        assert_eq!(buf.content(), "Hello, world!\n");
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_pty_buffer_append_str() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Line 1\nLine 2\n");
        assert_eq!(buf.content(), "Line 1\nLine 2\n");
        assert_eq!(buf.line_count(), 2);
    }

    #[test]
    fn test_pty_buffer_multiple_appends() {
        let mut buf = PtyBuffer::new();
        buf.append(b"First ");
        buf.append(b"Second ");
        buf.append_str("Third");
        assert_eq!(buf.content(), "First Second Third");
    }

    #[test]
    fn test_pty_buffer_clear() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Some content\n");
        buf.set_scroll(5);
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.scroll(), 0);
    }

    #[test]
    fn test_pty_buffer_scroll() {
        let mut buf = PtyBuffer::new();
        assert_eq!(buf.scroll(), 0);

        buf.set_scroll(10);
        assert_eq!(buf.scroll(), 10);

        buf.scroll_up(5);
        assert_eq!(buf.scroll(), 15);

        buf.scroll_down(3);
        assert_eq!(buf.scroll(), 12);

        // Scroll down shouldn't go below 0
        buf.scroll_down(100);
        assert_eq!(buf.scroll(), 0);
    }

    #[test]
    fn test_pty_buffer_max_lines() {
        let mut buf = PtyBuffer::with_max_lines(3);

        // Add 5 lines
        buf.append_str("Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n");

        // Should only keep the last 3 lines
        assert_eq!(buf.line_count(), 3);
        assert!(buf.content().contains("Line 3"));
        assert!(buf.content().contains("Line 4"));
        assert!(buf.content().contains("Line 5"));
        assert!(!buf.content().contains("Line 1"));
        assert!(!buf.content().contains("Line 2"));
    }

    #[test]
    fn test_pty_buffer_visible_content() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Line 1\nLine 2\nLine 3\nLine 4\nLine 5");

        // View last 3 lines (no scroll)
        let visible = buf.visible_content(3);
        assert!(visible.contains("Line 3"));
        assert!(visible.contains("Line 4"));
        assert!(visible.contains("Line 5"));
        assert!(!visible.contains("Line 1"));

        // Scroll up by 1 line
        buf.set_scroll(1);
        let visible = buf.visible_content(3);
        assert!(visible.contains("Line 2"));
        assert!(visible.contains("Line 3"));
        assert!(visible.contains("Line 4"));
        assert!(!visible.contains("Line 5"));
    }

    #[test]
    fn test_pty_buffer_visible_content_empty() {
        let buf = PtyBuffer::new();
        assert_eq!(buf.visible_content(10), "");
    }

    #[test]
    fn test_pty_buffer_visible_content_zero_height() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Some content\n");
        assert_eq!(buf.visible_content(0), "");
    }

    #[test]
    fn test_pty_buffer_lossy_utf8() {
        let mut buf = PtyBuffer::new();
        // Invalid UTF-8 sequence
        buf.append(&[0xff, 0xfe, b'H', b'i']);
        // Should contain the valid part plus replacement characters
        assert!(buf.content().contains("Hi"));
    }

    #[test]
    fn test_pty_buffer_default() {
        let buf = PtyBuffer::default();
        assert!(buf.is_empty());
        assert_eq!(buf.max_lines, 10_000);
    }

    // ==================== ModalPosition Tests ====================

    #[test]
    fn test_modal_position_default() {
        let pos = ModalPosition::default();
        assert_eq!(pos, ModalPosition::BottomRight);
    }

    #[test]
    fn test_modal_position_top_left() {
        let container = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = ModalPosition::TopLeft.calculate_rect(container, 20, 10);
        // 1-char margin from top-left
        assert_eq!(rect.x, 1);
        assert_eq!(rect.y, 1);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_modal_position_top_right() {
        let container = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = ModalPosition::TopRight.calculate_rect(container, 20, 10);
        // 80 - 20 - 1 (margin) = 59
        assert_eq!(rect.x, 59);
        assert_eq!(rect.y, 1);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_modal_position_bottom_left() {
        let container = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = ModalPosition::BottomLeft.calculate_rect(container, 20, 10);
        // 1-char margin from left, 24 - 10 - 1 = 13 from top
        assert_eq!(rect.x, 1);
        assert_eq!(rect.y, 13);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_modal_position_bottom_right() {
        let container = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = ModalPosition::BottomRight.calculate_rect(container, 20, 10);
        // 80 - 20 - 1 = 59, 24 - 10 - 1 = 13
        assert_eq!(rect.x, 59);
        assert_eq!(rect.y, 13);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_modal_position_center() {
        let container = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = ModalPosition::Center.calculate_rect(container, 20, 10);
        // Centered: (80 - 20) / 2 = 30, (24 - 10) / 2 = 7
        assert_eq!(rect.x, 30);
        assert_eq!(rect.y, 7);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
    }

    #[test]
    fn test_modal_position_with_offset_container() {
        // Container not at origin
        let container = Rect {
            x: 10,
            y: 5,
            width: 80,
            height: 24,
        };
        let rect = ModalPosition::TopLeft.calculate_rect(container, 20, 10);
        // Should be relative to container origin
        assert_eq!(rect.x, 11); // 10 + 1 margin
        assert_eq!(rect.y, 6); // 5 + 1 margin
    }

    #[test]
    fn test_modal_position_clamps_to_container() {
        // Small container that can't fit the modal
        let container = Rect {
            x: 0,
            y: 0,
            width: 10,
            height: 5,
        };
        let rect = ModalPosition::BottomRight.calculate_rect(container, 20, 10);
        // Should clamp to available space (minus margins)
        assert!(rect.width <= container.width);
        assert!(rect.height <= container.height);
        // With 1-char margin on each side, max is 8x3
        assert_eq!(rect.width, 8);
        assert_eq!(rect.height, 3);
    }

    #[test]
    fn test_modal_position_next_cycle() {
        assert_eq!(ModalPosition::TopLeft.next(), ModalPosition::TopRight);
        assert_eq!(ModalPosition::TopRight.next(), ModalPosition::BottomRight);
        assert_eq!(ModalPosition::BottomRight.next(), ModalPosition::BottomLeft);
        assert_eq!(ModalPosition::BottomLeft.next(), ModalPosition::Center);
        assert_eq!(ModalPosition::Center.next(), ModalPosition::TopLeft);
    }

    #[test]
    fn test_modal_position_names() {
        assert_eq!(ModalPosition::TopLeft.name(), "top-left");
        assert_eq!(ModalPosition::TopRight.name(), "top-right");
        assert_eq!(ModalPosition::BottomLeft.name(), "bottom-left");
        assert_eq!(ModalPosition::BottomRight.name(), "bottom-right");
        assert_eq!(ModalPosition::Center.name(), "center");
    }

    // ==================== ModalSize Tests ====================

    #[test]
    fn test_modal_size_default() {
        let size = ModalSize::default();
        assert_eq!(size, ModalSize::Small);
    }

    #[test]
    fn test_modal_size_dimensions() {
        assert_eq!(ModalSize::Small.dimensions(), (22, 12));
        assert_eq!(ModalSize::Medium.dimensions(), (42, 22));
        assert_eq!(ModalSize::Large.dimensions(), (62, 32));
    }

    #[test]
    fn test_modal_size_inner_dimensions() {
        assert_eq!(ModalSize::Small.inner_dimensions(), (20, 10));
        assert_eq!(ModalSize::Medium.inner_dimensions(), (40, 20));
        assert_eq!(ModalSize::Large.inner_dimensions(), (60, 30));
    }

    #[test]
    fn test_modal_size_next_cycle() {
        assert_eq!(ModalSize::Small.next(), ModalSize::Medium);
        assert_eq!(ModalSize::Medium.next(), ModalSize::Large);
        assert_eq!(ModalSize::Large.next(), ModalSize::Small);
    }

    #[test]
    fn test_modal_size_names() {
        assert_eq!(ModalSize::Small.name(), "small");
        assert_eq!(ModalSize::Medium.name(), "medium");
        assert_eq!(ModalSize::Large.name(), "large");
    }

    // ==================== CameraModal Tests ====================

    #[test]
    fn test_camera_modal_new() {
        let modal = CameraModal::new();
        assert!(!modal.visible);
        assert_eq!(modal.position, ModalPosition::BottomRight);
        assert_eq!(modal.size, ModalSize::Small);
        assert!(modal.frame.is_none());
        assert!(modal.border);
        assert_eq!(modal.charset, CharSet::Standard);
    }

    #[test]
    fn test_camera_modal_default() {
        let modal = CameraModal::default();
        assert!(!modal.visible);
        assert_eq!(modal.position, ModalPosition::BottomRight);
        assert_eq!(modal.size, ModalSize::Small);
        assert_eq!(modal.charset, CharSet::Standard);
    }

    #[test]
    fn test_camera_modal_toggle() {
        let mut modal = CameraModal::new();
        assert!(!modal.visible);
        modal.toggle();
        assert!(modal.visible);
        modal.toggle();
        assert!(!modal.visible);
    }

    #[test]
    fn test_camera_modal_cycle_position() {
        let mut modal = CameraModal::new();
        assert_eq!(modal.position, ModalPosition::BottomRight);
        modal.cycle_position();
        assert_eq!(modal.position, ModalPosition::BottomLeft);
        modal.cycle_position();
        assert_eq!(modal.position, ModalPosition::Center);
    }

    #[test]
    fn test_camera_modal_cycle_size() {
        let mut modal = CameraModal::new();
        assert_eq!(modal.size, ModalSize::Small);
        modal.cycle_size();
        assert_eq!(modal.size, ModalSize::Medium);
        modal.cycle_size();
        assert_eq!(modal.size, ModalSize::Large);
        modal.cycle_size();
        assert_eq!(modal.size, ModalSize::Small);
    }

    #[test]
    fn test_camera_modal_cycle_charset() {
        let mut modal = CameraModal::new();
        assert_eq!(modal.charset, CharSet::Standard);
        modal.cycle_charset();
        assert_eq!(modal.charset, CharSet::Blocks);
        modal.cycle_charset();
        assert_eq!(modal.charset, CharSet::Minimal);
        modal.cycle_charset();
        assert_eq!(modal.charset, CharSet::Braille);
        modal.cycle_charset();
        assert_eq!(modal.charset, CharSet::Standard);
    }

    #[test]
    fn test_camera_modal_calculate_rect() {
        let modal = CameraModal::new();
        let container = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };
        let rect = modal.calculate_rect(container);
        // BottomRight with Small (22x12)
        // x = 80 - 22 - 1 = 57, y = 24 - 12 - 1 = 11
        assert_eq!(rect.x, 57);
        assert_eq!(rect.y, 11);
        assert_eq!(rect.width, 22);
        assert_eq!(rect.height, 12);
    }

    // ==================== AsciiFrame Tests ====================

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
    fn test_camera_modal_set_frame() {
        let mut modal = CameraModal::new();
        assert!(modal.frame.is_none());

        let frame = AsciiFrame::from_chars(vec!['#'; 6], 3, 2);
        modal.set_frame(frame);

        assert!(modal.frame.is_some());
        let f = modal.frame.as_ref().unwrap();
        assert_eq!(f.width, 3);
        assert_eq!(f.height, 2);
    }

    #[test]
    fn test_camera_modal_clear_frame() {
        let mut modal = CameraModal::new();
        modal.set_frame(AsciiFrame::from_chars(vec!['#'; 6], 3, 2));
        assert!(modal.frame.is_some());

        modal.clear_frame();
        assert!(modal.frame.is_none());
    }

    #[test]
    fn test_camera_modal_with_frame_visible() {
        let mut modal = CameraModal::new();
        modal.visible = true;
        modal.set_frame(AsciiFrame::from_chars(vec!['@'; 200], 20, 10));

        // Verify frame content
        let f = modal.frame.as_ref().unwrap();
        assert_eq!(f.chars.len(), 200);
        assert!(f.chars.iter().all(|&c| c == '@'));
    }

    // ==================== StatusBar Tests ====================

    #[test]
    fn test_status_bar_new() {
        let sb = StatusBar::new();
        assert!(sb.visible);
    }

    #[test]
    fn test_status_bar_default() {
        let sb = StatusBar::default();
        assert!(sb.visible);
    }

    #[test]
    fn test_status_bar_with_visibility_true() {
        let sb = StatusBar::with_visibility(true);
        assert!(sb.visible);
    }

    #[test]
    fn test_status_bar_with_visibility_false() {
        let sb = StatusBar::with_visibility(false);
        assert!(!sb.visible);
    }

    #[test]
    fn test_status_bar_toggle() {
        let mut sb = StatusBar::new();
        assert!(sb.visible);
        sb.toggle();
        assert!(!sb.visible);
        sb.toggle();
        assert!(sb.visible);
    }

    #[test]
    fn test_status_bar_format_camera_on() {
        let sb = StatusBar::new();
        let mut modal = CameraModal::new();
        modal.visible = true;

        let text = sb.format(&modal);
        assert!(text.contains("cam:on"));
        assert!(text.contains("bottom-right")); // default position
        assert!(text.contains("small")); // default size
        assert!(text.contains("standard")); // default charset
    }

    #[test]
    fn test_status_bar_format_camera_off() {
        let sb = StatusBar::new();
        let modal = CameraModal::new(); // visible=false by default

        let text = sb.format(&modal);
        assert!(text.contains("cam:off"));
    }

    #[test]
    fn test_status_bar_format_reflects_position() {
        let sb = StatusBar::new();
        let mut modal = CameraModal::new();

        modal.position = ModalPosition::TopLeft;
        assert!(sb.format(&modal).contains("top-left"));

        modal.position = ModalPosition::TopRight;
        assert!(sb.format(&modal).contains("top-right"));

        modal.position = ModalPosition::BottomLeft;
        assert!(sb.format(&modal).contains("bottom-left"));

        modal.position = ModalPosition::Center;
        assert!(sb.format(&modal).contains("center"));
    }

    #[test]
    fn test_status_bar_format_reflects_size() {
        let sb = StatusBar::new();
        let mut modal = CameraModal::new();

        modal.size = ModalSize::Small;
        assert!(sb.format(&modal).contains("small"));

        modal.size = ModalSize::Medium;
        assert!(sb.format(&modal).contains("medium"));

        modal.size = ModalSize::Large;
        assert!(sb.format(&modal).contains("large"));
    }

    #[test]
    fn test_status_bar_format_reflects_charset() {
        let sb = StatusBar::new();
        let mut modal = CameraModal::new();

        modal.charset = CharSet::Standard;
        assert!(sb.format(&modal).contains("standard"));

        modal.charset = CharSet::Blocks;
        assert!(sb.format(&modal).contains("blocks"));

        modal.charset = CharSet::Minimal;
        assert!(sb.format(&modal).contains("minimal"));

        modal.charset = CharSet::Braille;
        assert!(sb.format(&modal).contains("braille"));
    }

    #[test]
    fn test_status_bar_format_has_separators() {
        let sb = StatusBar::new();
        let modal = CameraModal::new();

        let text = sb.format(&modal);
        // Format is: " cam:off | bottom-right | small | standard "
        assert_eq!(text.matches('|').count(), 3);
    }

    #[test]
    fn test_status_bar_format_has_padding() {
        let sb = StatusBar::new();
        let modal = CameraModal::new();

        let text = sb.format(&modal);
        // Should have leading and trailing space for visual padding
        assert!(text.starts_with(' '));
        assert!(text.ends_with(' '));
    }
}
