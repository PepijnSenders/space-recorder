//! space-recorder: TUI app that renders webcam as ASCII art overlay while hosting a shell.
//!
//! This library provides the core components for:
//! - PTY hosting (spawn shell, I/O relay)
//! - Camera capture (nokhwa, frame buffer)
//! - ASCII rendering (grayscale, downsample, charset)
//! - Terminal UI (ratatui, modal, raw mode)
//! - Input handling (keyboard, hotkeys)
//! - Overlay rendering (ANSI escape codes)
//!
//! # Modules
//!
//! - [`ascii`] - Image to ASCII art conversion pipeline
//! - [`camera`] - Webcam capture and frame handling
//! - [`cli`] - Command-line argument parsing
//! - [`config`] - Configuration file utilities
//! - [`event_loop`] - Main async event loop
//! - [`input`] - Keyboard input and hotkey handling
//! - [`pty`] - Pseudo-terminal spawning and I/O
//! - [`renderer`] - ANSI overlay rendering
//! - [`terminal`] - Terminal UI and raw mode management

pub mod ascii;
pub mod camera;
pub mod cli;
pub mod config;
pub mod event_loop;
pub mod input;
pub mod pty;
pub mod renderer;
pub mod terminal;
