//! Async event loop for concurrent handling of terminal, PTY, and camera.
//!
//! This module separates the main event loop logic from initialization,
//! making the code more testable and maintainable.

use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use std::io::Write;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ascii;
use crate::camera::CameraCapture;
use crate::input::{handle_key_event, KeyAction};
use crate::pty::{PtyHostSplit, PtySize};
use crate::renderer::{clear_modal_area, render_camera_overlay};
use crate::terminal::{AsciiFrame, CameraModal, CellColor, StatusBar};

/// Async main event loop using tokio::select! for concurrent handling.
///
/// This loop handles three concurrent concerns:
/// 1. Terminal events (keyboard input, resize) via crossterm EventStream
/// 2. PTY output via tokio channel from the reader thread
/// 3. Camera frame capture and ASCII rendering (~15 FPS)
///
/// The loop exits when the shell closes (PTY channel disconnects) or on error.
pub async fn run(
    mut pty: PtyHostSplit,
    mut pty_rx: mpsc::Receiver<Vec<u8>>,
    camera_modal: &mut CameraModal,
    _status_bar: &StatusBar,
    camera: Option<&mut CameraCapture>,
    invert: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut stdout = std::io::stdout();
    let mut event_stream = EventStream::new();

    // Camera frame interval (~15 FPS for ASCII rendering)
    let mut camera_interval = tokio::time::interval(Duration::from_millis(67));
    camera_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Reusable buffers for ASCII conversion (avoid allocations in hot path)
    let mut gray_buffer: Vec<u8> = Vec::new();
    let mut brightness_buffer: Vec<u8> = Vec::new();
    let mut char_buffer: Vec<char> = Vec::new();
    let mut color_buffer: Vec<ascii::CellColor> = Vec::new();

    // Track terminal size for modal positioning
    let (mut term_cols, mut term_rows) = crossterm::terminal::size().unwrap_or((80, 24));

    // Track previous modal state to clear old area when size/position/visibility changes
    let mut prev_modal_size = camera_modal.size;
    let mut prev_modal_position = camera_modal.position;
    let mut prev_modal_visible = camera_modal.visible;

    loop {
        // Check if shell has exited (non-blocking)
        if let Some(_status) = pty.try_wait()? {
            break;
        }

        tokio::select! {
            // Handle terminal events (keyboard input, resize)
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        match event {
                            Event::Key(key_event) => {
                                // Handle hotkeys first, then forward other keys to PTY
                                match handle_key_event(key_event, camera_modal) {
                                    KeyAction::Handled => {
                                        // Check if camera was toggled off - need to clear the area
                                        if prev_modal_visible && !camera_modal.visible {
                                            clear_modal_area(
                                                &mut stdout,
                                                prev_modal_size,
                                                prev_modal_position,
                                                term_cols,
                                                term_rows,
                                            )?;
                                        }
                                        prev_modal_visible = camera_modal.visible;
                                    }
                                    KeyAction::Forward(bytes) => {
                                        pty.write(&bytes)?;
                                    }
                                    KeyAction::None => {
                                        // Key not recognized, ignore
                                    }
                                }
                            }
                            Event::Resize(cols, rows) => {
                                // Terminal was resized (SIGWINCH) - resize the PTY to match
                                term_cols = cols;
                                term_rows = rows;
                                let new_size = PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                };
                                pty.resize(new_size)?;
                            }
                            _ => {
                                // Ignore other events (mouse, focus, etc.)
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Err(Box::new(e));
                    }
                    None => {
                        // Event stream ended - shouldn't happen normally
                        break;
                    }
                }
            }

            // Handle PTY output from the reader thread
            maybe_data = pty_rx.recv() => {
                match maybe_data {
                    Some(data) => {
                        // Write PTY output to stdout - colors and escape sequences pass through
                        stdout.write_all(&data)?;
                        stdout.flush()?;
                    }
                    None => {
                        // Channel closed - reader thread exited (shell closed)
                        break;
                    }
                }
            }

            // Camera frame capture and rendering
            _ = camera_interval.tick() => {
                if camera_modal.visible
                    && let Some(ref cam) = camera
                    && let Some(frame) = cam.get_frame()
                {
                    // Get modal dimensions
                    let (modal_width, modal_height) = camera_modal.size.inner_dimensions();

                    // Downsample colors for the frame
                    ascii::downsample_colors_into(
                        &frame,
                        modal_width,
                        modal_height,
                        &mut color_buffer,
                    );

                    // Convert colors to terminal CellColor format
                    let terminal_colors: Vec<CellColor> = color_buffer
                        .iter()
                        .map(|c| CellColor { r: c.r, g: c.g, b: c.b })
                        .collect();

                    // Convert frame to ASCII
                    let ascii_frame = if camera_modal.charset.is_braille() {
                        // Braille rendering (2x4 subpixel resolution)
                        ascii::to_grayscale_into(&frame, &mut gray_buffer);
                        let chars = ascii::render_braille(
                            &gray_buffer,
                            frame.width,
                            frame.height,
                            modal_width,
                            modal_height,
                            128, // threshold
                            invert,
                        );
                        AsciiFrame::from_chars_colored(chars, terminal_colors, modal_width, modal_height)
                    } else {
                        // Standard/blocks/minimal charset rendering
                        ascii::to_grayscale_into(&frame, &mut gray_buffer);
                        ascii::downsample_into(
                            &gray_buffer,
                            frame.width,
                            frame.height,
                            modal_width,
                            modal_height,
                            &mut brightness_buffer,
                        );
                        ascii::map_to_chars_into(
                            &brightness_buffer,
                            camera_modal.charset.chars(),
                            invert,
                            &mut char_buffer,
                        );
                        AsciiFrame::from_chars_colored(char_buffer.clone(), terminal_colors, modal_width, modal_height)
                    };

                    camera_modal.set_frame(ascii_frame);

                    // Check if modal size/position changed - need to clear old area
                    let size_changed = prev_modal_size != camera_modal.size;
                    let position_changed = prev_modal_position != camera_modal.position;

                    if size_changed || position_changed {
                        // Clear the old modal area
                        clear_modal_area(
                            &mut stdout,
                            prev_modal_size,
                            prev_modal_position,
                            term_cols,
                            term_rows,
                        )?;
                        prev_modal_size = camera_modal.size;
                        prev_modal_position = camera_modal.position;
                    }

                    // Render the overlay
                    render_camera_overlay(
                        &mut stdout,
                        camera_modal,
                        term_cols,
                        term_rows,
                    )?;
                }
            }
        }
    }

    Ok(())
}
