//! PTY host module - spawns user's shell and relays I/O
//!
//! This module provides abstractions for creating and managing pseudo-terminal
//! (PTY) sessions with shell processes.
//!
//! # Structure
//!
//! - [`error`] - Error types for PTY operations
//! - [`size`] - Terminal size configuration
//! - [`host`] - PTY host implementation
//! - [`shell`] - Shell selection utilities

mod error;
mod host;
mod shell;
mod size;

pub use error::PtyError;
pub use host::{PtyHost, PtyHostSplit};
pub use shell::{default_shell, select_shell};
pub use size::PtySize;
