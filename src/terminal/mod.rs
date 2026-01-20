//! Terminal management module - raw mode, TUI wrapper, modal types, and cleanup.

mod frame;
mod modal;
mod pty_buffer;
mod raw_mode;
mod status_bar;

// Re-export public types from submodules
pub use frame::{AsciiFrame, CellColor};
pub use modal::{CameraModal, ModalPosition, ModalSize};
pub use pty_buffer::PtyBuffer;
pub use raw_mode::RawModeGuard;
pub use status_bar::StatusBar;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::io::{self, Stdout};

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
