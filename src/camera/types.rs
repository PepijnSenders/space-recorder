//! Camera types and data structures.

use std::fmt;
use std::time::Instant;

/// Information about an available camera device.
#[derive(Debug, Clone)]
pub struct CameraInfo {
    /// Device index for selection
    pub index: u32,
    /// Human-readable device name
    pub name: String,
    /// Device description
    pub description: String,
}

impl fmt::Display for CameraInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {} ({})", self.index, self.name, self.description)
    }
}

/// Camera resolution settings.
#[derive(Debug, Clone, Copy)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    /// Low resolution (320x240) - fast, for small ASCII modal
    pub const LOW: Resolution = Resolution {
        width: 320,
        height: 240,
    };

    /// Medium resolution (640x480) - balanced, recommended
    pub const MEDIUM: Resolution = Resolution {
        width: 640,
        height: 480,
    };

    /// High resolution (1280x720) - for large ASCII modal
    pub const HIGH: Resolution = Resolution {
        width: 1280,
        height: 720,
    };
}

impl Default for Resolution {
    fn default() -> Self {
        Self::MEDIUM
    }
}

/// Pixel format of a captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    /// RGB format (3 bytes per pixel)
    Rgb,
}

/// A captured camera frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Raw pixel data in RGB format
    pub data: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Pixel format
    pub format: FrameFormat,
    /// Timestamp when frame was captured
    pub timestamp: Instant,
}

impl Frame {
    /// Get the number of bytes per pixel (3 for RGB).
    pub fn bytes_per_pixel(&self) -> usize {
        match self.format {
            FrameFormat::Rgb => 3,
        }
    }
}

/// Settings for camera capture.
#[derive(Debug, Clone)]
pub struct CameraSettings {
    /// Camera device index
    pub device_index: u32,
    /// Capture resolution
    pub resolution: Resolution,
    /// Target FPS (actual may vary)
    pub fps: u32,
    /// Mirror horizontally (selfie mode)
    pub mirror: bool,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            device_index: 0,
            resolution: Resolution::default(),
            fps: 30,
            mirror: true, // Default to selfie mode
        }
    }
}

/// Errors that can occur during camera operations.
#[derive(Debug)]
pub enum CameraError {
    /// No cameras found on the system
    NoDevices,
    /// Failed to query camera devices
    QueryFailed(String),
    /// Failed to open camera
    OpenFailed(String),
    /// Camera permission denied (macOS/iOS)
    PermissionDenied,
    /// Camera device not found at specified index
    DeviceNotFound(u32),
    /// Failed to start video stream
    StreamFailed(String),
    /// Capture thread is already running
    AlreadyRunning,
}

impl fmt::Display for CameraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CameraError::NoDevices => write!(f, "No cameras found"),
            CameraError::QueryFailed(msg) => write!(f, "Failed to query cameras: {}", msg),
            CameraError::OpenFailed(msg) => write!(f, "Failed to open camera: {}", msg),
            CameraError::PermissionDenied => {
                write!(
                    f,
                    "Camera permission denied. On macOS, grant access in System Settings > Privacy & Security > Camera"
                )
            }
            CameraError::DeviceNotFound(index) => {
                write!(
                    f,
                    "Camera device {} not found. Run 'list-cameras' to see available devices",
                    index
                )
            }
            CameraError::StreamFailed(msg) => write!(f, "Failed to start camera stream: {}", msg),
            CameraError::AlreadyRunning => write!(f, "Capture thread is already running"),
        }
    }
}

impl std::error::Error for CameraError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_info_display() {
        let info = CameraInfo {
            index: 0,
            name: "Test Camera".to_string(),
            description: "Built-in".to_string(),
        };
        assert_eq!(format!("{}", info), "[0] Test Camera (Built-in)");
    }

    #[test]
    fn test_resolution_constants() {
        assert_eq!(Resolution::LOW.width, 320);
        assert_eq!(Resolution::LOW.height, 240);
        assert_eq!(Resolution::MEDIUM.width, 640);
        assert_eq!(Resolution::MEDIUM.height, 480);
        assert_eq!(Resolution::HIGH.width, 1280);
        assert_eq!(Resolution::HIGH.height, 720);
    }

    #[test]
    fn test_resolution_default() {
        let res = Resolution::default();
        assert_eq!(res.width, Resolution::MEDIUM.width);
        assert_eq!(res.height, Resolution::MEDIUM.height);
    }

    #[test]
    fn test_camera_settings_default() {
        let settings = CameraSettings::default();
        assert_eq!(settings.device_index, 0);
        assert_eq!(settings.resolution.width, 640);
        assert_eq!(settings.resolution.height, 480);
        assert_eq!(settings.fps, 30);
        assert!(settings.mirror); // Default to selfie mode
    }

    #[test]
    fn test_camera_error_display() {
        assert_eq!(format!("{}", CameraError::NoDevices), "No cameras found");
        assert_eq!(
            format!("{}", CameraError::QueryFailed("test".to_string())),
            "Failed to query cameras: test"
        );
        assert_eq!(
            format!("{}", CameraError::OpenFailed("test".to_string())),
            "Failed to open camera: test"
        );
        assert!(format!("{}", CameraError::PermissionDenied).contains("permission denied"));
        assert!(format!("{}", CameraError::DeviceNotFound(5)).contains("5"));
    }

    #[test]
    fn test_camera_error_display_new_variants() {
        assert_eq!(
            format!("{}", CameraError::StreamFailed("test".to_string())),
            "Failed to start camera stream: test"
        );
        assert_eq!(
            format!("{}", CameraError::AlreadyRunning),
            "Capture thread is already running"
        );
    }

    #[test]
    fn test_frame_bytes_per_pixel() {
        let frame = Frame {
            data: vec![0; 6], // 2 RGB pixels
            width: 2,
            height: 1,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        assert_eq!(frame.bytes_per_pixel(), 3);
    }
}
