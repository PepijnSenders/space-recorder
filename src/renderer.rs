//! Terminal overlay rendering for camera modal.
//!
//! This module handles rendering the ASCII camera overlay on top of
//! the terminal without disturbing the underlying PTY output.

use crate::ascii;
use crate::terminal::{CameraModal, ModalPosition, ModalSize};
use ratatui::layout::Rect;
use std::io::Write;

/// Clear a modal area by filling it with spaces.
///
/// Used when modal size/position changes to erase the old rendering.
pub fn clear_modal_area(
    stdout: &mut std::io::Stdout,
    size: ModalSize,
    position: ModalPosition,
    term_cols: u16,
    term_rows: u16,
) -> std::io::Result<()> {
    let container = Rect {
        x: 0,
        y: 0,
        width: term_cols,
        height: term_rows,
    };

    // Create a temporary modal to calculate the rect
    let temp_modal = CameraModal {
        visible: true,
        position,
        size,
        charset: ascii::CharSet::Standard,
        border: false,
        frame: None,
        transparency: 80,
    };

    let modal_rect = temp_modal.calculate_rect(container);

    let mut output = String::new();
    output.push_str("\x1b7"); // Save cursor (DEC)
    output.push_str("\x1b[?25l"); // Hide cursor

    // Fill entire modal area with spaces
    for row in 0..modal_rect.height {
        let y = modal_rect.y + row + 1; // 1-based
        let x = modal_rect.x + 1; // 1-based
        output.push_str(&format!("\x1b[{};{}H", y, x));
        for _ in 0..modal_rect.width {
            output.push(' ');
        }
    }

    output.push_str("\x1b[?25h"); // Show cursor
    output.push_str("\x1b8"); // Restore cursor (DEC)

    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;

    Ok(())
}

/// Render the camera modal overlay on top of the terminal.
///
/// Uses ANSI escape codes to position and draw the overlay without
/// disturbing the underlying PTY output. This approach:
/// 1. Saves cursor position
/// 2. Moves to modal location
/// 3. Draws each line of the ASCII frame
/// 4. Restores cursor position
pub fn render_camera_overlay(
    stdout: &mut std::io::Stdout,
    modal: &CameraModal,
    term_cols: u16,
    term_rows: u16,
) -> std::io::Result<()> {
    let Some(ref frame) = modal.frame else {
        return Ok(());
    };

    // Calculate modal position
    let container = Rect {
        x: 0,
        y: 0,
        width: term_cols,
        height: term_rows,
    };
    let modal_rect = modal.calculate_rect(container);

    // Build the output string with ANSI escape codes
    let mut output = String::new();

    // Save cursor position (using DEC sequence - different slot than SCO \x1b[s)
    output.push_str("\x1b7");

    // Hide cursor during rendering to reduce flicker
    output.push_str("\x1b[?25l");

    // Calculate inner area (accounting for border if present)
    let inner_x = if modal.border {
        modal_rect.x + 1
    } else {
        modal_rect.x
    };
    let inner_y = if modal.border {
        modal_rect.y + 1
    } else {
        modal_rect.y
    };
    let inner_width = if modal.border {
        modal_rect.width.saturating_sub(2)
    } else {
        modal_rect.width
    };
    let inner_height = if modal.border {
        modal_rect.height.saturating_sub(2)
    } else {
        modal_rect.height
    };

    // Draw border if enabled
    if modal.border {
        render_border(&mut output, &modal_rect, inner_width, inner_height, inner_y);
    }

    // Draw ASCII frame content line by line with colors
    render_frame_content(
        &mut output,
        frame,
        modal.transparency,
        inner_x,
        inner_y,
        inner_width,
        inner_height,
    );

    // Reset colors and show cursor
    output.push_str("\x1b[0m"); // Reset all attributes
    output.push_str("\x1b[?25h");

    // Restore cursor position (using DEC sequence to match save)
    output.push_str("\x1b8");

    // Write all at once for efficiency
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;

    Ok(())
}

/// Render the modal border using box-drawing characters.
fn render_border(
    output: &mut String,
    modal_rect: &Rect,
    inner_width: u16,
    inner_height: u16,
    inner_y: u16,
) {
    // Top border
    output.push_str(&format!(
        "\x1b[{};{}H",
        modal_rect.y + 1,
        modal_rect.x + 1
    ));
    output.push('┌');
    for _ in 0..inner_width {
        output.push('─');
    }
    output.push('┐');

    // Bottom border
    output.push_str(&format!(
        "\x1b[{};{}H",
        modal_rect.y + modal_rect.height,
        modal_rect.x + 1
    ));
    output.push('└');
    for _ in 0..inner_width {
        output.push('─');
    }
    output.push('┘');

    // Side borders
    for row in 0..inner_height {
        let y = inner_y + row + 1;
        // Left border
        output.push_str(&format!("\x1b[{};{}H│", y, modal_rect.x + 1));
        // Right border
        output.push_str(&format!(
            "\x1b[{};{}H│",
            y,
            modal_rect.x + modal_rect.width
        ));
    }
}

/// Render the ASCII frame content with transparency support.
///
/// Skips pixels below the brightness threshold to let terminal content show through.
fn render_frame_content(
    output: &mut String,
    frame: &crate::terminal::AsciiFrame,
    transparency: u8,
    inner_x: u16,
    inner_y: u16,
    inner_width: u16,
    inner_height: u16,
) {
    let lines: Vec<&[char]> = frame.chars.chunks(frame.width as usize).collect();
    let has_colors = frame.colors.is_some();
    let colors = frame.colors.as_ref();

    // Calculate brightness threshold from transparency setting
    // Higher transparency = lower threshold = more pixels skipped
    // transparency=0 -> threshold=765 (nothing transparent, draw everything)
    // transparency=80 -> threshold=153 (only draw very dark pixels)
    // transparency=100 -> threshold=0 (everything transparent)
    let max_brightness: u16 = 765; // 255 * 3
    let brightness_threshold =
        (max_brightness as u32 * (100 - transparency as u32) / 100) as u16;

    for (row, line) in lines.iter().enumerate().take(inner_height as usize) {
        let y = inner_y + row as u16 + 1; // +1 for 1-based ANSI coordinates
        let base_x = inner_x + 1; // +1 for 1-based ANSI coordinates

        let chars_to_write = line.len().min(inner_width as usize);
        let row_start = row * frame.width as usize;

        // Track if we need to reposition cursor (after skipping transparent pixels)
        let mut need_reposition = true;

        for (col, &c) in line[..chars_to_write].iter().enumerate() {
            let mut is_transparent = false;

            if has_colors {
                if let Some(colors) = colors {
                    let idx = row_start + col;
                    if idx < colors.len() {
                        let color = &colors[idx];
                        let brightness = color.r as u16 + color.g as u16 + color.b as u16;

                        if brightness < brightness_threshold {
                            // Skip this pixel - it's too dark, let background show
                            is_transparent = true;
                            need_reposition = true;
                        } else {
                            // Position cursor if needed
                            if need_reposition {
                                output.push_str(&format!(
                                    "\x1b[{};{}H",
                                    y,
                                    base_x + col as u16
                                ));
                                need_reposition = false;
                            }
                            // ANSI true color (24-bit): ESC[38;2;R;G;Bm for foreground
                            output.push_str(&format!(
                                "\x1b[38;2;{};{};{}m",
                                color.r, color.g, color.b
                            ));
                        }
                    }
                }
            } else {
                // No colors - position if needed
                if need_reposition {
                    output.push_str(&format!("\x1b[{};{}H", y, base_x + col as u16));
                    need_reposition = false;
                }
            }

            if !is_transparent {
                output.push(c);
            }
        }
    }
}
