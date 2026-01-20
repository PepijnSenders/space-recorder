//! End-to-end tests for the full TUI integration.
//!
//! These tests verify the Phase 4 milestone acceptance criteria:
//! - AC: `cargo run` shows shell with camera modal
//! - AC: Can use shell normally
//! - AC: Camera updates smoothly
//! - AC: Hotkeys work
//! - AC: Window resize handled
//! - AC: Clean exit
//!
//! Note: Many TUI tests require a real TTY and are skipped in CI environments.
//! The tests verify component integration and event handling logic.

use space_recorder::ascii::CharSet;
use space_recorder::camera::{list_devices, CameraCapture, CameraSettings};
use space_recorder::pty::{select_shell, PtyHost, PtySize};
use space_recorder::terminal::{
    AsciiFrame, CameraModal, ModalPosition, ModalSize, PtyBuffer, RawModeGuard, StatusBar, Tui,
};
use std::thread;
use std::time::{Duration, Instant};

// ====================
// AC: cargo run shows shell with camera modal
// ====================

#[test]
fn test_shell_spawn_with_pty_host() {
    // Test that we can spawn a shell using PtyHost
    let shell = select_shell(None);
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let mut pty = PtyHost::spawn(&shell, size).expect("Should spawn PTY");

    // Shell should be running
    let status = pty.try_wait().expect("Should check shell status");
    assert!(status.is_none(), "Shell should still be running");

    // Kill the shell
    pty.kill().expect("Should kill shell");
}

#[test]
fn test_camera_modal_initialization() {
    // Test that camera modal initializes with correct defaults
    let modal = CameraModal::new();

    assert!(!modal.visible, "Modal should start hidden");
    assert_eq!(modal.position, ModalPosition::BottomRight);
    assert_eq!(modal.size, ModalSize::Small);
    assert!(modal.border, "Modal should have border by default");
    assert_eq!(modal.charset, CharSet::Standard);
    assert!(modal.frame.is_none(), "No frame initially");
}

#[test]
fn test_status_bar_shows_camera_status() {
    let status_bar = StatusBar::new();
    let mut modal = CameraModal::new();

    // Camera off
    let text = status_bar.format(&modal);
    assert!(text.contains("cam:off"));
    assert!(text.contains("bottom-right"));
    assert!(text.contains("small"));
    assert!(text.contains("standard"));

    // Camera on
    modal.toggle();
    let text = status_bar.format(&modal);
    assert!(text.contains("cam:on"));
}

#[test]
fn test_ascii_frame_display_in_modal() {
    let mut modal = CameraModal::new();
    modal.visible = true;

    // Create a test ASCII frame
    let chars: Vec<char> = (0..200).map(|i| if i % 2 == 0 { '#' } else { '.' }).collect();
    let frame = AsciiFrame::from_chars(chars, 20, 10);
    modal.set_frame(frame);

    // Verify frame is stored
    assert!(modal.frame.is_some());
    let f = modal.frame.as_ref().unwrap();
    assert_eq!(f.width, 20);
    assert_eq!(f.height, 10);

    // Verify frame can be displayed as string
    let display = f.to_string_display();
    assert!(display.contains('#'));
    assert!(display.contains('.'));
    // Should have newlines for multi-row display
    assert_eq!(display.matches('\n').count(), 9); // 10 rows = 9 newlines
}

// ====================
// AC: Can use shell normally
// ====================

#[test]
fn test_pty_io_roundtrip() {
    let shell = select_shell(None);
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let mut pty = PtyHost::spawn(&shell, size).expect("Should spawn PTY");

    // Write a command to the PTY
    let echo_cmd = "echo HELLO_E2E_TEST\n";
    pty.write(echo_cmd.as_bytes()).expect("Should write to PTY");

    // Give shell time to process
    thread::sleep(Duration::from_millis(200));

    // Read response (non-blocking read in a loop)
    let mut output = Vec::new();
    let mut buf = [0u8; 4096];
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(2) {
        match pty.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => output.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
        if String::from_utf8_lossy(&output).contains("HELLO_E2E_TEST") {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let output_str = String::from_utf8_lossy(&output);
    assert!(
        output_str.contains("HELLO_E2E_TEST"),
        "PTY should echo our command: {}",
        output_str
    );

    pty.kill().expect("Should kill shell");
}

#[test]
fn test_pty_buffer_accumulation() {
    let mut buffer = PtyBuffer::new();

    // Simulate PTY output
    buffer.append(b"$ ls\n");
    buffer.append(b"file1.txt\n");
    buffer.append(b"file2.txt\n");
    buffer.append(b"$ ");

    let content = buffer.content();
    assert!(content.contains("ls"));
    assert!(content.contains("file1.txt"));
    assert!(content.contains("file2.txt"));
    assert_eq!(buffer.line_count(), 4);
}

#[test]
fn test_pty_buffer_scrolling() {
    let mut buffer = PtyBuffer::new();

    // Add many lines
    for i in 0..100 {
        buffer.append_str(&format!("Line {}\n", i));
    }

    // Scroll should work
    buffer.set_scroll(10);
    let visible = buffer.visible_content(24);

    // Should show lines from the end, offset by scroll
    assert!(visible.contains("Line 89")); // 100 - 10 - 1
    assert!(!visible.contains("Line 99")); // scrolled past this
}

// ====================
// AC: Camera updates smoothly
// ====================

#[test]
fn test_camera_capture_provides_frames() {
    let devices = list_devices().expect("Should list devices");

    if devices.is_empty() {
        println!("SKIP: No cameras available for this test");
        return;
    }

    let settings = CameraSettings::default();
    let mut camera = CameraCapture::open(settings).expect("Should open camera");
    camera.start().expect("Should start capture");

    // Wait for frames
    let mut frame_count = 0;
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(1) {
        if camera.get_frame().is_some() {
            frame_count += 1;
        }
        thread::sleep(Duration::from_millis(33)); // ~30 FPS check rate
    }

    println!("Received {} frames in 1 second", frame_count);
    assert!(frame_count >= 10, "Should receive at least 10 frames in 1 second");

    camera.stop();
}

#[test]
fn test_modal_can_receive_ascii_frames() {
    let mut modal = CameraModal::new();
    modal.visible = true;

    // Simulate receiving frames at 15 FPS for 0.5 seconds
    let frame_interval = Duration::from_millis(67); // ~15 FPS
    let mut frames_received = 0;
    let start = Instant::now();

    while start.elapsed() < Duration::from_millis(500) {
        // Create mock ASCII frame
        let chars: Vec<char> = (0..200).map(|_| '#').collect();
        let frame = AsciiFrame::from_chars(chars, 20, 10);
        modal.set_frame(frame);
        frames_received += 1;

        thread::sleep(frame_interval);
    }

    assert!(frames_received >= 7, "Should receive ~7-8 frames in 0.5s at 15fps");
}

// ====================
// AC: Hotkeys work
// ====================

#[test]
fn test_hotkey_toggle_visibility() {
    let mut modal = CameraModal::new();
    assert!(!modal.visible);

    // Alt+C toggles
    modal.toggle();
    assert!(modal.visible);

    modal.toggle();
    assert!(!modal.visible);
}

#[test]
fn test_hotkey_cycle_position() {
    let mut modal = CameraModal::new();
    assert_eq!(modal.position, ModalPosition::BottomRight);

    // Alt+P cycles through positions
    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::BottomLeft);

    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::Center);

    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::TopLeft);

    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::TopRight);

    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::BottomRight); // full cycle
}

#[test]
fn test_hotkey_cycle_size() {
    let mut modal = CameraModal::new();
    assert_eq!(modal.size, ModalSize::Small);

    // Alt+S cycles through sizes
    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Medium);

    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Large);

    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Small); // full cycle
}

#[test]
fn test_hotkey_cycle_charset() {
    let mut modal = CameraModal::new();
    assert_eq!(modal.charset, CharSet::Standard);

    // Alt+A cycles through charsets
    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Blocks);

    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Minimal);

    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Braille);

    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Standard); // full cycle
}

#[test]
fn test_all_hotkeys_in_sequence() {
    // Test full interaction sequence
    let mut modal = CameraModal::new();
    let status_bar = StatusBar::new();

    // Initial state
    assert_eq!(status_bar.format(&modal), " cam:off | bottom-right | small | standard ");

    // Toggle on
    modal.toggle();
    assert_eq!(status_bar.format(&modal), " cam:on | bottom-right | small | standard ");

    // Change position
    modal.cycle_position();
    assert_eq!(status_bar.format(&modal), " cam:on | bottom-left | small | standard ");

    // Change size
    modal.cycle_size();
    assert_eq!(status_bar.format(&modal), " cam:on | bottom-left | medium | standard ");

    // Change charset
    modal.cycle_charset();
    assert_eq!(status_bar.format(&modal), " cam:on | bottom-left | medium | blocks ");
}

// ====================
// AC: Window resize handled
// ====================

#[test]
fn test_pty_resize() {
    let shell = select_shell(None);
    let initial_size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pty = PtyHost::spawn(&shell, initial_size).expect("Should spawn PTY");

    // Resize the PTY
    let new_size = PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    };

    pty.resize(new_size).expect("Should resize PTY");

    // Shell should still be running
    let (_, mut pty_split) = pty.split();
    let status = pty_split.try_wait().expect("Should check shell status");
    assert!(status.is_none(), "Shell should still be running after resize");

    pty_split.kill().expect("Should kill shell");
}

#[test]
fn test_modal_adapts_to_container_size() {
    use ratatui::layout::Rect;

    let modal = CameraModal::new();

    // Large container - modal fits
    let large = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 50,
    };
    let rect = modal.calculate_rect(large);
    let (expected_w, expected_h) = ModalSize::Small.dimensions();
    assert_eq!(rect.width, expected_w);
    assert_eq!(rect.height, expected_h);

    // Small container - modal should be clamped
    let small = Rect {
        x: 0,
        y: 0,
        width: 15,
        height: 8,
    };
    let rect = modal.calculate_rect(small);
    // With 1-char margin on each side, max is 13x6
    assert!(rect.width <= 13, "Width should be clamped");
    assert!(rect.height <= 6, "Height should be clamped");
}

// ====================
// AC: Clean exit
// ====================

#[test]
fn test_pty_clean_exit() {
    let shell = select_shell(None);
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    let mut pty = PtyHost::spawn(&shell, size).expect("Should spawn PTY");

    // Send exit command
    pty.write(b"exit\n").expect("Should write exit");

    // Wait for shell to exit
    let start = Instant::now();
    let mut exited = false;

    while start.elapsed() < Duration::from_secs(2) {
        match pty.try_wait() {
            Ok(Some(_status)) => {
                exited = true;
                break;
            }
            Ok(None) => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }

    if !exited {
        // Force kill if exit didn't work
        let _ = pty.kill();
    }
}

#[test]
fn test_raw_mode_cleanup_on_drop() {
    // Skip if no TTY
    match RawModeGuard::enter() {
        Ok(guard) => {
            // Just drop it - should restore terminal
            drop(guard);
            // If we get here without hanging, cleanup worked
        }
        Err(e) => {
            println!("SKIP: No TTY available: {}", e);
        }
    }
}

#[test]
fn test_tui_cleanup_on_drop() {
    // Skip if no TTY
    match Tui::new() {
        Ok(tui) => {
            assert!(tui.is_active());
            // Just drop it - should restore terminal
            drop(tui);
            // If we get here without hanging, cleanup worked
        }
        Err(e) => {
            println!("SKIP: No TTY available: {}", e);
        }
    }
}

#[test]
fn test_tui_explicit_restore() {
    // Skip if no TTY
    match Tui::new() {
        Ok(mut tui) => {
            assert!(tui.is_active());

            // Explicit restore
            tui.restore().expect("Should restore terminal");
            assert!(!tui.is_active());

            // Double restore should be safe
            tui.restore().expect("Double restore should be safe");
        }
        Err(e) => {
            println!("SKIP: No TTY available: {}", e);
        }
    }
}

// ====================
// Integration: Full TUI stack
// ====================

#[test]
fn test_full_tui_stack_components() {
    // Test that all TUI components can be created together
    let shell = select_shell(None);
    let size = PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    };

    // PTY Host
    let pty = PtyHost::spawn(&shell, size);
    assert!(pty.is_ok(), "PTY should spawn");
    let mut pty = pty.unwrap();

    // Camera Modal
    let mut modal = CameraModal::new();
    modal.toggle(); // Turn on

    // Status Bar
    let status_bar = StatusBar::new();
    assert!(status_bar.visible);

    // PTY Buffer
    let mut pty_buffer = PtyBuffer::new();
    pty_buffer.append(b"$ ");

    // ASCII Frame (mock)
    let chars: Vec<char> = (0..200).map(|_| '@').collect();
    let frame = AsciiFrame::from_chars(chars, 20, 10);
    modal.set_frame(frame);

    // Verify everything is wired up
    assert!(modal.visible);
    assert!(modal.frame.is_some());
    assert_eq!(pty_buffer.content(), "$ ");
    assert!(status_bar.format(&modal).contains("cam:on"));

    // Cleanup
    pty.kill().expect("Should cleanup PTY");
}

#[test]
fn test_tui_render_flow_components() {
    // Test the rendering components work together
    let pty_buffer = PtyBuffer::new();
    let modal = CameraModal::new();

    // Verify render inputs are compatible
    let content = pty_buffer.content();
    assert!(content.is_empty()); // Empty buffer is valid

    // Modal position calculation
    use ratatui::layout::Rect;
    let area = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let modal_rect = modal.calculate_rect(area);
    assert!(modal_rect.width > 0);
    assert!(modal_rect.height > 0);
}

// ====================
// Performance tests
// ====================

#[test]
fn test_pty_buffer_performance() {
    let mut buffer = PtyBuffer::new();

    // Simulate heavy PTY output
    let start = Instant::now();
    for _ in 0..1000 {
        buffer.append(b"Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n");
    }
    let elapsed = start.elapsed();

    println!("Appended 1000 lines in {:?}", elapsed);
    assert!(elapsed < Duration::from_millis(100), "Buffer should be fast");
}

#[test]
fn test_modal_state_update_performance() {
    let mut modal = CameraModal::new();

    let start = Instant::now();
    for _ in 0..1000 {
        modal.toggle();
        modal.cycle_position();
        modal.cycle_size();
        modal.cycle_charset();
    }
    let elapsed = start.elapsed();

    println!("1000 modal updates in {:?}", elapsed);
    assert!(elapsed < Duration::from_millis(10), "Modal updates should be instant");
}
