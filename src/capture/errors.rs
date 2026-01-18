//! Error types for capture operations.
//!
//! This module contains all error types related to device capture,
//! including screen, webcam, audio, and window capture errors.

/// Errors that can occur during capture operations
#[derive(Debug)]
pub enum CaptureError {
    /// FFmpeg not found
    FfmpegNotFound,
    /// Failed to run FFmpeg
    FfmpegFailed(String),
    /// No screen capture devices found
    NoScreenDevices,
    /// Requested screen index not found
    ScreenNotFound { requested: usize, available: usize },
    /// No webcam devices found
    NoWebcamDevices,
    /// Requested webcam not found
    WebcamNotFound { requested: String, available: Vec<String> },
    /// No audio devices found
    NoAudioDevices,
    /// Requested audio device not found
    AudioDeviceNotFound { requested: String, available: Vec<String> },
    /// Application not running (for window capture)
    AppNotRunning { app_name: String, available_apps: Vec<String> },
    /// Application has no windows (for window capture)
    AppNoWindows { app_name: String },
    /// Failed to get window bounds
    WindowBoundsFailed { app_name: String, message: String },
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::FfmpegNotFound => {
                write!(
                    f,
                    "FFmpeg not found. Please install it with:\n\n    brew install ffmpeg\n"
                )
            }
            CaptureError::FfmpegFailed(msg) => write!(f, "FFmpeg failed: {}", msg),
            CaptureError::NoScreenDevices => {
                write!(
                    f,
                    "No screen capture devices found.\n\nMake sure screen recording permission is granted:\n  System Preferences > Privacy & Security > Screen Recording"
                )
            }
            CaptureError::ScreenNotFound {
                requested,
                available,
            } => {
                write!(
                    f,
                    "Screen {} not found. {} screen(s) available (indices 0-{}).\n\nUse `space-recorder list-devices` to see available screens.",
                    requested,
                    available,
                    available.saturating_sub(1)
                )
            }
            CaptureError::NoWebcamDevices => {
                write!(
                    f,
                    "No webcam devices found.\n\nMake sure a camera is connected and camera permission is granted:\n  System Preferences > Privacy & Security > Camera"
                )
            }
            CaptureError::WebcamNotFound {
                requested,
                available,
            } => {
                write!(
                    f,
                    "Webcam '{}' not found.\n\nAvailable webcams:\n",
                    requested
                )?;
                for (i, name) in available.iter().enumerate() {
                    writeln!(f, "  [{}] {}", i, name)?;
                }
                write!(f, "\nUse `space-recorder list-devices --video` to see all video devices.")
            }
            CaptureError::NoAudioDevices => {
                write!(
                    f,
                    "No audio devices found.\n\nMake sure microphone permission is granted:\n  System Preferences > Privacy & Security > Microphone"
                )
            }
            CaptureError::AudioDeviceNotFound {
                requested,
                available,
            } => {
                write!(
                    f,
                    "Audio device '{}' not found.\n\nAvailable audio devices:\n",
                    requested
                )?;
                for (i, name) in available.iter().enumerate() {
                    writeln!(f, "  [{}] {}", i, name)?;
                }
                write!(f, "\nUse `space-recorder list-devices --audio` to see all audio devices.")
            }
            CaptureError::AppNotRunning { app_name, available_apps } => {
                write!(
                    f,
                    "Application '{}' is not running.",
                    app_name
                )?;
                if !available_apps.is_empty() {
                    write!(f, "\n\nRunning applications with windows:\n")?;
                    for app in available_apps.iter().take(10) {
                        writeln!(f, "  - {}", app)?;
                    }
                    if available_apps.len() > 10 {
                        writeln!(f, "  ... and {} more", available_apps.len() - 10)?;
                    }
                }
                write!(f, "\nPlease start the application first, or try one of the listed apps.")
            }
            CaptureError::AppNoWindows { app_name } => {
                write!(
                    f,
                    "Application '{}' has no visible windows.\n\nMake sure the application has at least one window open.",
                    app_name
                )
            }
            CaptureError::WindowBoundsFailed { app_name, message } => {
                write!(
                    f,
                    "Failed to get window bounds for '{}'.\n\nDetails: {}\n\nMake sure Accessibility permission is granted:\n  System Preferences > Privacy & Security > Accessibility",
                    app_name, message
                )
            }
        }
    }
}

impl std::error::Error for CaptureError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_error_display() {
        let err = CaptureError::NoScreenDevices;
        let msg = format!("{}", err);
        assert!(msg.contains("No screen capture devices"));
        assert!(msg.contains("Screen Recording"));
    }

    #[test]
    fn test_screen_not_found_error() {
        let err = CaptureError::ScreenNotFound {
            requested: 3,
            available: 2,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Screen 3 not found"));
        assert!(msg.contains("2 screen(s) available"));
    }

    #[test]
    fn test_no_webcam_devices_error() {
        let err = CaptureError::NoWebcamDevices;
        let msg = format!("{}", err);
        assert!(msg.contains("No webcam devices"));
        assert!(msg.contains("Camera"));
    }

    #[test]
    fn test_webcam_not_found_error() {
        let err = CaptureError::WebcamNotFound {
            requested: "USB Camera".to_string(),
            available: vec!["FaceTime HD Camera".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Webcam 'USB Camera' not found"));
        assert!(msg.contains("FaceTime HD Camera"));
    }

    #[test]
    fn test_no_audio_devices_error() {
        let err = CaptureError::NoAudioDevices;
        let msg = format!("{}", err);
        assert!(msg.contains("No audio devices"));
        assert!(msg.contains("Microphone"));
    }

    #[test]
    fn test_audio_device_not_found_error() {
        let err = CaptureError::AudioDeviceNotFound {
            requested: "External USB Mic".to_string(),
            available: vec!["MacBook Pro Microphone".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Audio device 'External USB Mic' not found"));
        assert!(msg.contains("MacBook Pro Microphone"));
    }

    #[test]
    fn test_app_not_running_error_display() {
        let err = CaptureError::AppNotRunning {
            app_name: "MyApp".to_string(),
            available_apps: vec!["Terminal".to_string(), "Safari".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("MyApp"));
        assert!(msg.contains("not running"));
        assert!(msg.contains("Terminal"));
        assert!(msg.contains("Safari"));
        assert!(msg.contains("Running applications with windows"));
    }

    #[test]
    fn test_app_not_running_error_display_empty_list() {
        let err = CaptureError::AppNotRunning {
            app_name: "MyApp".to_string(),
            available_apps: vec![],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("MyApp"));
        assert!(msg.contains("not running"));
        assert!(!msg.contains("Running applications with windows"));
    }

    #[test]
    fn test_app_no_windows_error_display() {
        let err = CaptureError::AppNoWindows {
            app_name: "MyApp".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("MyApp"));
        assert!(msg.contains("no visible windows"));
        assert!(msg.contains("at least one window"));
    }

    #[test]
    fn test_window_bounds_failed_error_display() {
        let err = CaptureError::WindowBoundsFailed {
            app_name: "MyApp".to_string(),
            message: "Some error".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("MyApp"));
        assert!(msg.contains("Some error"));
        assert!(msg.contains("Accessibility"));
    }
}
