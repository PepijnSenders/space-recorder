//! space-recorder: TUI app that renders webcam as ASCII art overlay while hosting a shell
//!
//! This library provides the core components for:
//! - PTY hosting (spawn shell, I/O relay)
//! - Camera capture (nokhwa, frame buffer)
//! - ASCII rendering (grayscale, downsample, charset)
//! - TUI (ratatui, modal, hotkeys) [future]

pub mod ascii;
pub mod camera;
pub mod pty;
pub mod terminal;
