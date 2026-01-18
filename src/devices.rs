//! AVFoundation device discovery and management.
//!
//! This module provides functions for listing and parsing video/audio capture devices
//! available via macOS AVFoundation framework through FFmpeg.

use std::process::{Command, Stdio};

/// Represents a single capture device (video or audio).
#[derive(Debug, Clone)]
pub struct Device {
    pub index: usize,
    pub name: String,
}

/// Collection of available video and audio devices.
#[derive(Debug)]
pub struct DeviceList {
    pub video_devices: Vec<Device>,
    pub audio_devices: Vec<Device>,
}

/// Run ffmpeg to list available AVFoundation devices.
///
/// # Returns
/// A `DeviceList` containing all discovered video and audio devices.
///
/// # Errors
/// Returns an error if FFmpeg is not found or fails to run.
pub fn list_avfoundation_devices() -> Result<DeviceList, String> {
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Err(
                    "FFmpeg not found. Please install it with:\n\n    brew install ffmpeg\n"
                        .to_string(),
                );
            }
            return Err(format!("Failed to run ffmpeg: {}", e));
        }
    };

    // FFmpeg outputs device list to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_device_list(&stderr)
}

/// Parse FFmpeg's device list output.
///
/// # Arguments
/// * `stderr` - The stderr output from FFmpeg's device listing command.
///
/// # Returns
/// A `DeviceList` parsed from the FFmpeg output.
pub fn parse_device_list(stderr: &str) -> Result<DeviceList, String> {
    let mut video_devices = Vec::new();
    let mut audio_devices = Vec::new();
    let mut in_video_section = false;
    let mut in_audio_section = false;

    for line in stderr.lines() {
        // Detect section headers
        if line.contains("AVFoundation video devices:") {
            in_video_section = true;
            in_audio_section = false;
            continue;
        }
        if line.contains("AVFoundation audio devices:") {
            in_video_section = false;
            in_audio_section = true;
            continue;
        }

        // Parse device lines: [AVFoundation indev @ 0x...] [index] device name
        // Example: [AVFoundation indev @ 0x12345678] [0] FaceTime HD Camera
        if let Some(device) = parse_device_line(line) {
            if in_video_section {
                video_devices.push(device);
            } else if in_audio_section {
                audio_devices.push(device);
            }
        }
    }

    Ok(DeviceList {
        video_devices,
        audio_devices,
    })
}

/// Parse a single device line from FFmpeg output.
///
/// # Arguments
/// * `line` - A single line from FFmpeg's device listing output.
///
/// # Returns
/// `Some(Device)` if the line contains valid device information, `None` otherwise.
pub fn parse_device_line(line: &str) -> Option<Device> {
    // Look for lines containing device entries with format: [index] device name
    // The line format is: [AVFoundation indev @ 0x...] [index] device name

    // Find the pattern "] [" which separates the prefix from the device index
    let bracket_idx = line.find("] [")?;
    let after_bracket = &line[bracket_idx + 3..]; // Skip "] ["

    // Find the closing bracket of the index
    let close_bracket = after_bracket.find(']')?;
    let index_str = &after_bracket[..close_bracket];
    let index: usize = index_str.parse().ok()?;

    // The device name is everything after "] " following the index
    let name = after_bracket[close_bracket + 2..].trim().to_string();

    if name.is_empty() {
        return None;
    }

    Some(Device { index, name })
}

/// Print the device list to stdout.
///
/// # Arguments
/// * `devices` - The device list to print.
/// * `show_video` - If true, show only video devices.
/// * `show_audio` - If true, show only audio devices.
///
/// If both `show_video` and `show_audio` are false, all devices are shown.
pub fn print_devices(devices: &DeviceList, show_video: bool, show_audio: bool) {
    let show_both = !show_video && !show_audio;

    if show_video || show_both {
        println!("Video Devices:");
        if devices.video_devices.is_empty() {
            println!("  (none found)");
        } else {
            for device in &devices.video_devices {
                println!("  [{}] {}", device.index, device.name);
            }
        }
        if show_both {
            println!();
        }
    }

    if show_audio || show_both {
        println!("Audio Devices:");
        if devices.audio_devices.is_empty() {
            println!("  (none found)");
        } else {
            for device in &devices.audio_devices {
                println!("  [{}] {}", device.index, device.name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_line_valid() {
        let line = "[AVFoundation indev @ 0x12345678] [0] FaceTime HD Camera";
        let device = parse_device_line(line).unwrap();
        assert_eq!(device.index, 0);
        assert_eq!(device.name, "FaceTime HD Camera");
    }

    #[test]
    fn test_parse_device_line_with_special_chars() {
        let line = "[AVFoundation indev @ 0xabcdef] [2] MacBook Pro Microphone (Built-in)";
        let device = parse_device_line(line).unwrap();
        assert_eq!(device.index, 2);
        assert_eq!(device.name, "MacBook Pro Microphone (Built-in)");
    }

    #[test]
    fn test_parse_device_line_invalid() {
        let line = "Some random line without device info";
        assert!(parse_device_line(line).is_none());
    }

    #[test]
    fn test_parse_device_list() {
        let stderr = r#"
[AVFoundation indev @ 0x123] AVFoundation video devices:
[AVFoundation indev @ 0x123] [0] FaceTime HD Camera
[AVFoundation indev @ 0x123] [1] Capture screen 0
[AVFoundation indev @ 0x123] AVFoundation audio devices:
[AVFoundation indev @ 0x123] [0] MacBook Pro Microphone
[AVFoundation indev @ 0x123] [1] External USB Mic
"#;
        let devices = parse_device_list(stderr).unwrap();
        assert_eq!(devices.video_devices.len(), 2);
        assert_eq!(devices.audio_devices.len(), 2);
        assert_eq!(devices.video_devices[0].name, "FaceTime HD Camera");
        assert_eq!(devices.video_devices[1].name, "Capture screen 0");
        assert_eq!(devices.audio_devices[0].name, "MacBook Pro Microphone");
        assert_eq!(devices.audio_devices[1].name, "External USB Mic");
    }
}
