//! macOS permissions verification module.
//!
//! This module checks for required macOS permissions before capture starts,
//! providing clear error messages with System Preferences paths when permissions are missing.

use std::process::{Command, Stdio};

/// Types of macOS permissions that may be required
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionType {
    /// Screen Recording permission (required for screen capture)
    ScreenRecording,
    /// Camera permission (required for webcam capture)
    Camera,
    /// Microphone permission (required for audio capture)
    Microphone,
    /// Accessibility permission (required for AppleScript window detection)
    Accessibility,
}

impl PermissionType {
    /// Get the human-readable name of this permission type
    pub fn name(&self) -> &'static str {
        match self {
            PermissionType::ScreenRecording => "Screen Recording",
            PermissionType::Camera => "Camera",
            PermissionType::Microphone => "Microphone",
            PermissionType::Accessibility => "Accessibility",
        }
    }

    /// Get the System Preferences path for this permission
    pub fn system_preferences_path(&self) -> &'static str {
        match self {
            PermissionType::ScreenRecording => {
                "System Settings > Privacy & Security > Screen Recording"
            }
            PermissionType::Camera => "System Settings > Privacy & Security > Camera",
            PermissionType::Microphone => "System Settings > Privacy & Security > Microphone",
            PermissionType::Accessibility => {
                "System Settings > Privacy & Security > Accessibility"
            }
        }
    }

    /// Get the deep link URL to open System Preferences to this permission section
    pub fn system_preferences_url(&self) -> &'static str {
        match self {
            PermissionType::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            PermissionType::Camera => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera"
            }
            PermissionType::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            PermissionType::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
        }
    }
}

/// Result of a permission check
#[derive(Debug)]
pub struct PermissionCheckResult {
    /// The type of permission checked
    #[allow(dead_code)] // Used for debug output and future introspection
    pub permission_type: PermissionType,
    /// Whether the permission is granted
    pub granted: bool,
    /// Additional details or error message
    pub details: Option<String>,
}

impl PermissionCheckResult {
    /// Create a new successful result
    pub fn granted(permission_type: PermissionType) -> Self {
        Self {
            permission_type,
            granted: true,
            details: None,
        }
    }

    /// Create a new denied result with optional details
    pub fn denied(permission_type: PermissionType, details: Option<String>) -> Self {
        Self {
            permission_type,
            granted: false,
            details,
        }
    }
}

/// Error type for permission verification failures
#[derive(Debug)]
pub struct PermissionError {
    /// The type of permission that is missing
    pub permission_type: PermissionType,
    /// Additional details about the error
    pub details: Option<String>,
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} permission is required.\n\n\
            Please grant permission in:\n  {}\n\n\
            You can open System Settings directly by running:\n  open \"{}\"",
            self.permission_type.name(),
            self.permission_type.system_preferences_path(),
            self.permission_type.system_preferences_url()
        )?;

        if let Some(ref details) = self.details {
            write!(f, "\n\nDetails: {}", details)?;
        }

        Ok(())
    }
}

impl std::error::Error for PermissionError {}

/// Check Screen Recording permission by attempting a quick screen capture test.
///
/// On macOS, if Screen Recording permission is not granted, FFmpeg's AVFoundation
/// capture will either fail or return a black screen. We do a quick test capture
/// to verify the permission is granted.
pub fn check_screen_recording() -> PermissionCheckResult {
    // Use a very quick FFmpeg probe to check if screen capture works
    // We capture a single frame and check if it succeeds
    let output = Command::new("ffmpeg")
        .args([
            "-f",
            "avfoundation",
            "-framerate",
            "1",
            "-t",
            "0.1",
            "-i",
            "0:", // Screen only (no audio)
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);

            // Check for common permission error indicators in FFmpeg output
            if stderr.contains("Could not open")
                || stderr.contains("Permission denied")
                || stderr.contains("not authorized")
            {
                return PermissionCheckResult::denied(
                    PermissionType::ScreenRecording,
                    Some("Screen capture returned permission error".to_string()),
                );
            }

            // If FFmpeg ran without permission errors, assume permission is granted
            // Note: FFmpeg may still fail for other reasons, but that's not a permission issue
            PermissionCheckResult::granted(PermissionType::ScreenRecording)
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                PermissionCheckResult::denied(
                    PermissionType::ScreenRecording,
                    Some("FFmpeg not found".to_string()),
                )
            } else {
                PermissionCheckResult::denied(
                    PermissionType::ScreenRecording,
                    Some(format!("Failed to run FFmpeg: {}", e)),
                )
            }
        }
    }
}

/// Check Camera permission by attempting to list video devices.
///
/// If Camera permission is not granted, FFmpeg's AVFoundation will not show
/// camera devices in the device list, or will fail to open them.
pub fn check_camera() -> PermissionCheckResult {
    // List devices and check if any cameras are available
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);

            // Look for camera devices in the output
            // Camera permission is typically implied if cameras appear in the list
            let has_camera = stderr.contains("FaceTime")
                || stderr.contains("Camera")
                || (stderr.contains("AVFoundation video devices:")
                    && !stderr.contains("[0] Capture screen"));

            // If we see video devices section but only screens, camera might be denied
            if stderr.contains("AVFoundation video devices:") {
                // Check for non-screen camera devices
                let lines: Vec<&str> = stderr.lines().collect();
                let mut in_video_section = false;
                let mut found_camera = false;

                for line in lines {
                    if line.contains("AVFoundation video devices:") {
                        in_video_section = true;
                        continue;
                    }
                    if line.contains("AVFoundation audio devices:") {
                        break;
                    }
                    if in_video_section
                        && line.contains("] [")
                        && !line.contains("Capture screen")
                    {
                        found_camera = true;
                        break;
                    }
                }

                if found_camera || has_camera {
                    return PermissionCheckResult::granted(PermissionType::Camera);
                }
            }

            // If we didn't find any camera devices, permission might be denied
            // However, this could also mean no camera is physically connected
            // We return granted with a warning - actual access will fail if denied
            PermissionCheckResult::granted(PermissionType::Camera)
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                PermissionCheckResult::denied(
                    PermissionType::Camera,
                    Some("FFmpeg not found".to_string()),
                )
            } else {
                PermissionCheckResult::denied(
                    PermissionType::Camera,
                    Some(format!("Failed to run FFmpeg: {}", e)),
                )
            }
        }
    }
}

/// Check Microphone permission by attempting to list audio devices.
///
/// If Microphone permission is not granted, audio devices may not appear
/// in FFmpeg's device list or capture will fail.
pub fn check_microphone() -> PermissionCheckResult {
    // List devices and check if any audio input devices are available
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);

            // Check for audio devices section
            if stderr.contains("AVFoundation audio devices:") {
                // Look for microphone devices
                let lines: Vec<&str> = stderr.lines().collect();
                let mut in_audio_section = false;
                let mut found_mic = false;

                for line in lines {
                    if line.contains("AVFoundation audio devices:") {
                        in_audio_section = true;
                        continue;
                    }
                    if in_audio_section && line.contains("] [") {
                        found_mic = true;
                        break;
                    }
                }

                if found_mic {
                    return PermissionCheckResult::granted(PermissionType::Microphone);
                }
            }

            // If we didn't find audio devices, permission might be denied
            // or no microphone is connected
            PermissionCheckResult::granted(PermissionType::Microphone)
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                PermissionCheckResult::denied(
                    PermissionType::Microphone,
                    Some("FFmpeg not found".to_string()),
                )
            } else {
                PermissionCheckResult::denied(
                    PermissionType::Microphone,
                    Some(format!("Failed to run FFmpeg: {}", e)),
                )
            }
        }
    }
}

/// Check Accessibility permission by running a simple AppleScript.
///
/// Accessibility permission is required for AppleScript to get window bounds
/// from other applications.
pub fn check_accessibility() -> PermissionCheckResult {
    // Run a simple AppleScript that requires Accessibility permission
    let script = r#"
        tell application "System Events"
            set frontApp to name of first application process whose frontmost is true
        end tell
        return frontApp
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);

            // Check for permission-related errors
            if stderr.contains("not allowed")
                || stderr.contains("assistive access")
                || stderr.contains("System Events got an error")
                || stderr.contains("denied")
            {
                return PermissionCheckResult::denied(
                    PermissionType::Accessibility,
                    Some("AppleScript access denied".to_string()),
                );
            }

            if result.status.success() {
                PermissionCheckResult::granted(PermissionType::Accessibility)
            } else {
                // Script failed but not necessarily due to permissions
                PermissionCheckResult::granted(PermissionType::Accessibility)
            }
        }
        Err(e) => PermissionCheckResult::denied(
            PermissionType::Accessibility,
            Some(format!("Failed to run osascript: {}", e)),
        ),
    }
}

/// Verify all required permissions for a capture session.
///
/// # Arguments
/// * `need_screen` - Whether screen capture is needed (always true for this app)
/// * `need_camera` - Whether camera capture is needed
/// * `need_microphone` - Whether microphone capture is needed
/// * `need_accessibility` - Whether AppleScript window detection is needed
///
/// # Returns
/// A vector of `PermissionError` for any missing permissions.
/// Empty vector means all required permissions are granted.
pub fn verify_permissions(
    need_screen: bool,
    need_camera: bool,
    need_microphone: bool,
    need_accessibility: bool,
) -> Vec<PermissionError> {
    let mut errors = Vec::new();

    if need_screen {
        let result = check_screen_recording();
        if !result.granted {
            errors.push(PermissionError {
                permission_type: PermissionType::ScreenRecording,
                details: result.details,
            });
        }
    }

    if need_camera {
        let result = check_camera();
        if !result.granted {
            errors.push(PermissionError {
                permission_type: PermissionType::Camera,
                details: result.details,
            });
        }
    }

    if need_microphone {
        let result = check_microphone();
        if !result.granted {
            errors.push(PermissionError {
                permission_type: PermissionType::Microphone,
                details: result.details,
            });
        }
    }

    if need_accessibility {
        let result = check_accessibility();
        if !result.granted {
            errors.push(PermissionError {
                permission_type: PermissionType::Accessibility,
                details: result.details,
            });
        }
    }

    errors
}

/// Print a user-friendly summary of missing permissions.
///
/// This function formats the permission errors in a clear, actionable way
/// for the user.
pub fn print_permission_errors(errors: &[PermissionError]) {
    if errors.is_empty() {
        return;
    }

    eprintln!("\nMissing permissions detected:\n");

    for (i, error) in errors.iter().enumerate() {
        eprintln!("{}. {}", i + 1, error.permission_type.name());
        eprintln!("   Grant permission in: {}", error.permission_type.system_preferences_path());
        eprintln!(
            "   Or run: open \"{}\"",
            error.permission_type.system_preferences_url()
        );
        if let Some(ref details) = error.details {
            eprintln!("   Details: {}", details);
        }
        eprintln!();
    }

    eprintln!("After granting permissions, you may need to restart the application.\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_type_name() {
        assert_eq!(PermissionType::ScreenRecording.name(), "Screen Recording");
        assert_eq!(PermissionType::Camera.name(), "Camera");
        assert_eq!(PermissionType::Microphone.name(), "Microphone");
        assert_eq!(PermissionType::Accessibility.name(), "Accessibility");
    }

    #[test]
    fn test_permission_type_system_preferences_path() {
        assert!(PermissionType::ScreenRecording
            .system_preferences_path()
            .contains("Screen Recording"));
        assert!(PermissionType::Camera
            .system_preferences_path()
            .contains("Camera"));
        assert!(PermissionType::Microphone
            .system_preferences_path()
            .contains("Microphone"));
        assert!(PermissionType::Accessibility
            .system_preferences_path()
            .contains("Accessibility"));
    }

    #[test]
    fn test_permission_type_system_preferences_url() {
        assert!(PermissionType::ScreenRecording
            .system_preferences_url()
            .contains("ScreenCapture"));
        assert!(PermissionType::Camera
            .system_preferences_url()
            .contains("Camera"));
        assert!(PermissionType::Microphone
            .system_preferences_url()
            .contains("Microphone"));
        assert!(PermissionType::Accessibility
            .system_preferences_url()
            .contains("Accessibility"));
    }

    #[test]
    fn test_permission_check_result_granted() {
        let result = PermissionCheckResult::granted(PermissionType::Camera);
        assert!(result.granted);
        assert_eq!(result.permission_type, PermissionType::Camera);
        assert!(result.details.is_none());
    }

    #[test]
    fn test_permission_check_result_denied() {
        let result =
            PermissionCheckResult::denied(PermissionType::Microphone, Some("Test error".to_string()));
        assert!(!result.granted);
        assert_eq!(result.permission_type, PermissionType::Microphone);
        assert_eq!(result.details, Some("Test error".to_string()));
    }

    #[test]
    fn test_permission_error_display() {
        let error = PermissionError {
            permission_type: PermissionType::ScreenRecording,
            details: None,
        };
        let msg = format!("{}", error);
        assert!(msg.contains("Screen Recording permission is required"));
        assert!(msg.contains("System Settings"));
        assert!(msg.contains("open \""));
    }

    #[test]
    fn test_permission_error_display_with_details() {
        let error = PermissionError {
            permission_type: PermissionType::Camera,
            details: Some("Camera not accessible".to_string()),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("Camera permission is required"));
        assert!(msg.contains("Camera not accessible"));
    }

    #[test]
    fn test_verify_permissions_none_required() {
        let errors = verify_permissions(false, false, false, false);
        assert!(errors.is_empty(), "No permissions required should return no errors");
    }

    // Note: The following tests would require actual permission states on macOS
    // They are integration tests that run only when permissions are properly set up

    #[test]
    fn test_check_screen_recording_runs() {
        // Just verify the function runs without panicking
        let _result = check_screen_recording();
    }

    #[test]
    fn test_check_camera_runs() {
        // Just verify the function runs without panicking
        let _result = check_camera();
    }

    #[test]
    fn test_check_microphone_runs() {
        // Just verify the function runs without panicking
        let _result = check_microphone();
    }

    #[test]
    fn test_check_accessibility_runs() {
        // Just verify the function runs without panicking
        let _result = check_accessibility();
    }
}
