//! Rendering functions for terminal UI components.
//!
//! This module contains pure rendering logic separated from terminal
//! lifecycle management. All functions operate on ratatui Frame objects
//! without managing terminal state.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::{CameraModal, PtyBuffer, StatusBar};

/// Render a camera modal to a ratatui frame at the given area.
///
/// This renders the modal with:
/// - A cleared background for overlay effect
/// - Optional border (controlled by modal.border)
/// - ASCII frame content if available
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `modal` - The camera modal state to render
/// * `area` - The available area for positioning the modal
pub fn render_modal(frame: &mut ratatui::Frame, modal: &CameraModal, area: Rect) {
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
}

/// Render PTY output to a ratatui frame.
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `pty_buffer` - The PTY output buffer to render
/// * `area` - The area to render the PTY content in
pub fn render_pty_output(frame: &mut ratatui::Frame, pty_buffer: &PtyBuffer, area: Rect) {
    let pty_content = pty_buffer.content();
    let pty_paragraph = Paragraph::new(pty_content);
    frame.render_widget(pty_paragraph, area);
}

/// Render a status bar to a ratatui frame.
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `status_bar` - The status bar to render
/// * `modal` - The camera modal (used for status information)
/// * `area` - The full terminal area (status bar will be at bottom)
pub fn render_status_bar(
    frame: &mut ratatui::Frame,
    status_bar: &StatusBar,
    modal: &CameraModal,
    area: Rect,
) {
    let status_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let status_text = status_bar.format(modal);
    let status_paragraph =
        Paragraph::new(status_text).style(Style::default().fg(Color::Black).bg(Color::White));
    frame.render_widget(status_paragraph, status_area);
}

/// Render a complete frame with all layers.
///
/// This renders:
/// 1. PTY output (background layer)
/// 2. Camera modal (overlay, if visible)
/// 3. Status bar (bottom, if visible)
///
/// # Arguments
/// * `frame` - The ratatui frame to render to
/// * `pty_buffer` - The PTY output buffer
/// * `modal` - The camera modal state
/// * `status_bar` - Optional status bar
/// * `area` - The full terminal area
pub fn render_full_frame(
    frame: &mut ratatui::Frame,
    pty_buffer: &PtyBuffer,
    modal: &CameraModal,
    status_bar: Option<&StatusBar>,
    area: Rect,
) {
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
    render_pty_output(frame, pty_buffer, main_area);

    // Layer 2: Camera modal (floating overlay within main area)
    if modal.visible {
        render_modal(frame, modal, main_area);
    }

    // Layer 3: Status bar (bottom)
    if let Some(sb) = status_bar
        && sb.visible
    {
        render_status_bar(frame, sb, modal, area);
    }
}
