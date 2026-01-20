//! space-recorder: TUI app that renders webcam as ASCII art overlay while hosting a shell

use clap::Parser;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers};
use futures::StreamExt;
use std::io::{Read, Write};
use std::time::Duration;
use tokio::sync::mpsc;

mod ascii;
mod camera;
mod cli;
mod config;
mod pty;
mod terminal;

use camera::{CameraCapture, CameraSettings, Resolution};
use cli::{Args, Command};
use pty::{PtyHost, PtySize};
use terminal::{AsciiFrame, CameraModal, ModalPosition, ModalSize, StatusBar};

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Handle subcommands
    if let Some(cmd) = args.command {
        match cmd {
            Command::ListCameras => {
                cli::list_cameras();
                return;
            }
            Command::Config { action } => {
                cli::handle_config_action(action);
                return;
            }
        }
    }

    let shell = pty::select_shell(args.shell.as_deref());

    // Get terminal size
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    // Spawn PTY with the shell
    let pty = match PtyHost::spawn(&shell, size) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to spawn shell: {}", e);
            std::process::exit(1);
        }
    };

    // Split the PTY into reader (for background thread) and writer (for main thread)
    let (reader, pty_split) = pty.split();

    // Create tokio channel for PTY output (bounded for backpressure)
    let (tx, rx) = mpsc::channel::<Vec<u8>>(64);

    // Spawn background thread to read from PTY (blocking reads need their own thread)
    let reader_handle = std::thread::spawn(move || {
        pty_reader_thread(reader, tx);
    });

    // Enter raw mode with automatic cleanup on exit/panic
    let _raw_guard = terminal::RawModeGuard::enter().expect("Failed to enter raw mode");

    // Initialize camera modal state with CLI args
    let mut camera_modal = CameraModal::new();
    camera_modal.position = args.position.into();
    camera_modal.size = args.size.into();
    camera_modal.charset = args.charset.into();
    camera_modal.visible = !args.no_camera;

    // Initialize status bar (visible unless --no-status flag is set)
    let status_bar = StatusBar::with_visibility(!args.no_status);

    // Initialize camera capture if camera is enabled
    let mut camera_capture: Option<CameraCapture> = if !args.no_camera {
        let settings = CameraSettings {
            device_index: args.camera,
            resolution: Resolution::MEDIUM, // 640x480 - good balance of speed and quality
            fps: 15, // Lower FPS for ASCII rendering is fine
            mirror: args.mirror,
        };
        match CameraCapture::open(settings) {
            Ok(mut cam) => {
                if let Err(e) = cam.start() {
                    eprintln!("Warning: Failed to start camera: {}", e);
                    None
                } else {
                    Some(cam)
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to open camera: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Run the async I/O loop
    let result = run_async_loop(
        pty_split,
        rx,
        &mut camera_modal,
        &status_bar,
        camera_capture.as_mut(),
        args.invert,
    )
    .await;

    // Wait for reader thread to finish (it will exit when PTY closes)
    let _ = reader_handle.join();

    // Handle any errors from the I/O loop
    if let Err(e) = result {
        // Restore terminal before printing error
        drop(_raw_guard);
        eprintln!("\nError: {}", e);
        std::process::exit(1);
    }
}

/// Background thread that reads from PTY and sends data through channel.
/// This runs in a separate thread because PTY reads are blocking.
fn pty_reader_thread(mut reader: Box<dyn Read + Send>, tx: mpsc::Sender<Vec<u8>>) {
    let mut buf = [0u8; 4096];

    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                // EOF - shell closed
                break;
            }
            Ok(n) => {
                // Send the data to the main thread using blocking_send for sync context
                // If the receiver is dropped, this will fail and we'll exit
                if tx.blocking_send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
            Err(_) => {
                // I/O error - exit the thread
                break;
            }
        }
    }
}

/// Result of handling a key event.
enum KeyAction {
    /// Key was handled as a hotkey (don't forward to PTY)
    Handled,
    /// Key should be forwarded to PTY
    Forward(Vec<u8>),
    /// No action needed
    None,
}

/// Handle a key event, checking for hotkeys first.
///
/// Hotkeys intercepted (not forwarded to PTY):
/// - Alt+C: Toggle camera visibility
/// - Alt+P: Cycle position
/// - Alt+S: Cycle size
/// - Alt+A: Cycle charset
fn handle_key_event(event: KeyEvent, modal: &mut CameraModal) -> KeyAction {
    let KeyEvent {
        code, modifiers, ..
    } = event;

    // Check for Alt+key hotkeys first
    if modifiers.contains(KeyModifiers::ALT) {
        match code {
            KeyCode::Char('c') | KeyCode::Char('C') => {
                modal.toggle();
                return KeyAction::Handled;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                modal.cycle_position();
                return KeyAction::Handled;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                modal.cycle_size();
                return KeyAction::Handled;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                modal.cycle_charset();
                return KeyAction::Handled;
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                modal.cycle_transparency();
                return KeyAction::Handled;
            }
            _ => {
                // Other Alt+key combinations - forward to PTY
            }
        }
    }

    // Convert to bytes for PTY
    match key_event_to_bytes(event) {
        Some(bytes) => KeyAction::Forward(bytes),
        None => KeyAction::None,
    }
}

/// Async main event loop using tokio::select! for concurrent handling.
///
/// This loop handles three concurrent concerns:
/// 1. Terminal events (keyboard input, resize) via crossterm EventStream
/// 2. PTY output via tokio channel from the reader thread
/// 3. Camera frame capture and ASCII rendering (~15 FPS)
///
/// The loop exits when the shell closes (PTY channel disconnects) or on error.
async fn run_async_loop(
    mut pty: pty::PtyHostSplit,
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
                if camera_modal.visible {
                    if let Some(ref cam) = camera {
                        if let Some(frame) = cam.get_frame() {
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
                            let terminal_colors: Vec<terminal::CellColor> = color_buffer
                                .iter()
                                .map(|c| terminal::CellColor { r: c.r, g: c.g, b: c.b })
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
        }
    }

    Ok(())
}

/// Clear a modal area by filling it with spaces.
///
/// Used when modal size/position changes to erase the old rendering.
fn clear_modal_area(
    stdout: &mut std::io::Stdout,
    size: ModalSize,
    position: ModalPosition,
    term_cols: u16,
    term_rows: u16,
) -> std::io::Result<()> {
    use ratatui::layout::Rect;

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
    output.push_str("\x1b[s");  // Save cursor
    output.push_str("\x1b[?25l");  // Hide cursor

    // Fill entire modal area with spaces
    for row in 0..modal_rect.height {
        let y = modal_rect.y + row + 1;  // 1-based
        let x = modal_rect.x + 1;  // 1-based
        output.push_str(&format!("\x1b[{};{}H", y, x));
        for _ in 0..modal_rect.width {
            output.push(' ');
        }
    }

    output.push_str("\x1b[?25h");  // Show cursor
    output.push_str("\x1b[u");  // Restore cursor

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
fn render_camera_overlay(
    stdout: &mut std::io::Stdout,
    modal: &CameraModal,
    term_cols: u16,
    term_rows: u16,
) -> std::io::Result<()> {
    use ratatui::layout::Rect;

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

    // Save cursor position
    output.push_str("\x1b[s");

    // Hide cursor during rendering to reduce flicker
    output.push_str("\x1b[?25l");

    // Calculate inner area (accounting for border if present)
    let inner_x = if modal.border { modal_rect.x + 1 } else { modal_rect.x };
    let inner_y = if modal.border { modal_rect.y + 1 } else { modal_rect.y };
    let inner_width = if modal.border { modal_rect.width.saturating_sub(2) } else { modal_rect.width };
    let inner_height = if modal.border { modal_rect.height.saturating_sub(2) } else { modal_rect.height };

    // Draw border if enabled
    if modal.border {
        // Top border
        output.push_str(&format!("\x1b[{};{}H", modal_rect.y + 1, modal_rect.x + 1));
        output.push('┌');
        for _ in 0..inner_width {
            output.push('─');
        }
        output.push('┐');

        // Bottom border
        output.push_str(&format!("\x1b[{};{}H", modal_rect.y + modal_rect.height, modal_rect.x + 1));
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
            output.push_str(&format!("\x1b[{};{}H│", y, modal_rect.x + modal_rect.width));
        }
    }

    // Draw ASCII frame content line by line with colors
    // Skip bright/white pixels to let terminal content show through
    let lines: Vec<&[char]> = frame.chars.chunks(frame.width as usize).collect();
    let has_colors = frame.colors.is_some();
    let colors = frame.colors.as_ref();

    // Calculate brightness threshold from transparency setting
    // Higher transparency = lower threshold = more pixels skipped
    // transparency=0 -> threshold=765 (nothing transparent, draw everything)
    // transparency=80 -> threshold=153 (only draw very dark pixels)
    // transparency=100 -> threshold=0 (everything transparent)
    let max_brightness: u16 = 765; // 255 * 3
    let brightness_threshold = (max_brightness as u32 * (100 - modal.transparency as u32) / 100) as u16;

    for (row, line) in lines.iter().enumerate().take(inner_height as usize) {
        let y = inner_y + row as u16 + 1;  // +1 for 1-based ANSI coordinates
        let base_x = inner_x + 1;  // +1 for 1-based ANSI coordinates

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
                                output.push_str(&format!("\x1b[{};{}H", y, base_x + col as u16));
                                need_reposition = false;
                            }
                            // ANSI true color (24-bit): ESC[38;2;R;G;Bm for foreground
                            output.push_str(&format!("\x1b[38;2;{};{};{}m", color.r, color.g, color.b));
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

    // Reset colors and show cursor
    output.push_str("\x1b[0m");  // Reset all attributes
    output.push_str("\x1b[?25h");

    // Restore cursor position
    output.push_str("\x1b[u");

    // Write all at once for efficiency
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;

    Ok(())
}

/// Convert a crossterm KeyEvent to bytes that can be sent to the PTY
fn key_event_to_bytes(event: KeyEvent) -> Option<Vec<u8>> {
    let KeyEvent {
        code, modifiers, ..
    } = event;

    // Handle Ctrl+key combinations
    if modifiers.contains(KeyModifiers::CONTROL) {
        return match code {
            // Ctrl+A through Ctrl+Z map to 0x01-0x1A
            KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                let ctrl_char = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                Some(vec![ctrl_char])
            }
            // Ctrl+[ is ESC (0x1B)
            KeyCode::Char('[') => Some(vec![0x1B]),
            // Ctrl+\ is 0x1C
            KeyCode::Char('\\') => Some(vec![0x1C]),
            // Ctrl+] is 0x1D
            KeyCode::Char(']') => Some(vec![0x1D]),
            // Ctrl+^ is 0x1E
            KeyCode::Char('^') => Some(vec![0x1E]),
            // Ctrl+_ is 0x1F
            KeyCode::Char('_') => Some(vec![0x1F]),
            // Ctrl+Space is NUL (0x00)
            KeyCode::Char(' ') => Some(vec![0x00]),
            _ => None,
        };
    }

    // Handle Alt+key combinations (send ESC prefix)
    if modifiers.contains(KeyModifiers::ALT) {
        return match code {
            KeyCode::Char(c) => Some(vec![0x1B, c as u8]),
            _ => None,
        };
    }

    // Handle regular keys and special keys
    match code {
        KeyCode::Char(c) => Some(c.to_string().into_bytes()),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7F]), // DEL character
        KeyCode::Esc => Some(vec![0x1B]),
        // Arrow keys - ANSI escape sequences
        KeyCode::Up => Some(b"\x1B[A".to_vec()),
        KeyCode::Down => Some(b"\x1B[B".to_vec()),
        KeyCode::Right => Some(b"\x1B[C".to_vec()),
        KeyCode::Left => Some(b"\x1B[D".to_vec()),
        // Home/End
        KeyCode::Home => Some(b"\x1B[H".to_vec()),
        KeyCode::End => Some(b"\x1B[F".to_vec()),
        // Page Up/Down
        KeyCode::PageUp => Some(b"\x1B[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1B[6~".to_vec()),
        // Insert/Delete
        KeyCode::Insert => Some(b"\x1B[2~".to_vec()),
        KeyCode::Delete => Some(b"\x1B[3~".to_vec()),
        // Function keys F1-F12
        KeyCode::F(1) => Some(b"\x1BOP".to_vec()),
        KeyCode::F(2) => Some(b"\x1BOQ".to_vec()),
        KeyCode::F(3) => Some(b"\x1BOR".to_vec()),
        KeyCode::F(4) => Some(b"\x1BOS".to_vec()),
        KeyCode::F(5) => Some(b"\x1B[15~".to_vec()),
        KeyCode::F(6) => Some(b"\x1B[17~".to_vec()),
        KeyCode::F(7) => Some(b"\x1B[18~".to_vec()),
        KeyCode::F(8) => Some(b"\x1B[19~".to_vec()),
        KeyCode::F(9) => Some(b"\x1B[20~".to_vec()),
        KeyCode::F(10) => Some(b"\x1B[21~".to_vec()),
        KeyCode::F(11) => Some(b"\x1B[23~".to_vec()),
        KeyCode::F(12) => Some(b"\x1B[24~".to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_to_bytes_regular_char() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![b'a']));
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_c() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x03])); // ETX
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_d() {
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x04])); // EOT
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_z() {
        let event = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x1A])); // SUB
    }

    #[test]
    fn test_key_event_to_bytes_enter() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![b'\r']));
    }

    #[test]
    fn test_key_event_to_bytes_backspace() {
        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x7F]));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_up() {
        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[A".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_down() {
        let event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[B".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_left() {
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[D".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_right() {
        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[C".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_escape() {
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x1B]));
    }

    #[test]
    fn test_key_event_to_bytes_tab() {
        let event = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![b'\t']));
    }

    #[test]
    fn test_key_event_to_bytes_alt_c() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x1B, b'c']));
    }

    // ==================== Hotkey Handling Tests ====================

    #[test]
    fn test_handle_key_event_alt_c_toggles_visibility() {
        let mut modal = CameraModal::new();
        assert!(!modal.visible);

        // Alt+C should toggle visibility
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert!(modal.visible);

        // Alt+C again should toggle back
        let action2 = handle_key_event(event, &mut modal);
        assert!(matches!(action2, KeyAction::Handled));
        assert!(!modal.visible);
    }

    #[test]
    fn test_handle_key_event_alt_c_uppercase() {
        let mut modal = CameraModal::new();
        assert!(!modal.visible);

        // Alt+C (uppercase) should also work
        let event = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert!(modal.visible);
    }

    #[test]
    fn test_handle_key_event_alt_p_cycles_position() {
        let mut modal = CameraModal::new();
        assert_eq!(modal.position, terminal::ModalPosition::BottomRight);

        // Alt+P should cycle position
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert_eq!(modal.position, terminal::ModalPosition::BottomLeft);
    }

    #[test]
    fn test_handle_key_event_alt_s_cycles_size() {
        let mut modal = CameraModal::new();
        assert_eq!(modal.size, terminal::ModalSize::Small);

        // Alt+S should cycle size
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert_eq!(modal.size, terminal::ModalSize::Medium);
    }

    #[test]
    fn test_handle_key_event_alt_a_cycles_charset() {
        let mut modal = CameraModal::new();
        assert_eq!(modal.charset, ascii::CharSet::Standard);

        // Alt+A should cycle charset
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert_eq!(modal.charset, ascii::CharSet::Blocks);
    }

    #[test]
    fn test_handle_key_event_other_alt_keys_forwarded() {
        let mut modal = CameraModal::new();

        // Alt+X (not a hotkey) should be forwarded to PTY
        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        match action {
            KeyAction::Forward(bytes) => {
                assert_eq!(bytes, vec![0x1B, b'x']); // ESC + x
            }
            _ => panic!("Expected Forward action for Alt+X"),
        }
    }

    #[test]
    fn test_handle_key_event_regular_keys_forwarded() {
        let mut modal = CameraModal::new();

        // Regular 'a' (no modifier) should be forwarded
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_key_event(event, &mut modal);
        match action {
            KeyAction::Forward(bytes) => {
                assert_eq!(bytes, vec![b'a']);
            }
            _ => panic!("Expected Forward action for regular 'a'"),
        }
    }

    #[test]
    fn test_handle_key_event_ctrl_keys_forwarded() {
        let mut modal = CameraModal::new();

        // Ctrl+C should be forwarded (not our hotkey)
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = handle_key_event(event, &mut modal);
        match action {
            KeyAction::Forward(bytes) => {
                assert_eq!(bytes, vec![0x03]); // ETX (Ctrl+C)
            }
            _ => panic!("Expected Forward action for Ctrl+C"),
        }
    }
}
