//! Command-line interface definitions and helpers.
//!
//! This module contains all CLI argument parsing, enums, and subcommand handlers.

mod args;
mod commands;
mod enums;

pub use args::{Args, Command, ConfigAction};
pub use commands::{handle_config_action, list_cameras};
pub use enums::{CharacterSet, Position, Size};
