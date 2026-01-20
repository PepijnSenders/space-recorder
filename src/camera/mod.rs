//! Camera capture module for webcam access and frame capture.
//!
//! This module provides a high-level API for camera capture operations:
//! - Device enumeration via [`list_devices`]
//! - Camera capture via [`CameraCapture`]
//! - Configuration via [`CameraSettings`] and [`Resolution`]

mod capture;
mod capture_loop;
mod device;
mod frame_utils;
mod types;

pub use capture::CameraCapture;
pub use device::list_devices;
pub use types::{CameraError, CameraInfo, CameraSettings, Frame, FrameFormat, Resolution};
