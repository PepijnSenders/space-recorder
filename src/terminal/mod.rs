//! Terminal management module - raw mode, TUI wrapper, modal types, and cleanup.

mod frame;
mod modal;
mod pty_buffer;
mod raw_mode;
mod rendering;
mod status_bar;
mod tui;

// Re-export public types from submodules
pub use frame::{AsciiFrame, CellColor};
pub use modal::{CameraModal, ModalPosition, ModalSize};
pub use pty_buffer::PtyBuffer;
pub use raw_mode::RawModeGuard;
pub use status_bar::StatusBar;
pub use tui::Tui;
