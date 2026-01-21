//! space-recorder: TUI app that renders webcam as ASCII art overlay while hosting a shell.

use clap::Parser;
use std::io::Read;
use tokio::sync::mpsc;

use space_recorder::camera::{CameraCapture, CameraSettings, Resolution};
use space_recorder::cli::{self, Args, Command};
use space_recorder::event_loop;
use space_recorder::pty::{self, PtyHost, PtySize};
use space_recorder::terminal::{self, CameraModal, StatusBar};

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
            fps: 15,                        // Lower FPS for ASCII rendering is fine
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
    let result = event_loop::run(
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
