//! Capture module for screen, webcam, window, and audio sources.
//!
//! This module handles device detection and FFmpeg argument generation
//! for video and audio capture on macOS using AVFoundation.

use std::process::{Command, Stdio};

/// Represents a webcam capture configuration
#[derive(Debug, Clone)]
pub struct WebcamCapture {
    /// Device name or index (None = auto-detect first camera)
    pub device: Option<String>,
    /// Capture framerate
    pub framerate: u32,
    /// Capture width
    pub width: u32,
    /// Capture height
    pub height: u32,
    /// Mirror (horizontal flip) the webcam
    pub mirror: bool,
    /// Whether webcam capture is enabled
    pub enabled: bool,
}

impl Default for WebcamCapture {
    fn default() -> Self {
        Self {
            device: None,
            framerate: 30,
            width: 1280,
            height: 720,
            mirror: false,
            enabled: true,
        }
    }
}

impl WebcamCapture {
    /// Create a new webcam capture configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the device by name or index
    pub fn with_device(mut self, device: impl Into<String>) -> Self {
        self.device = Some(device.into());
        self
    }

    /// Set mirroring (horizontal flip)
    pub fn with_mirror(mut self, mirror: bool) -> Self {
        self.mirror = mirror;
        self
    }

    /// Set the capture framerate
    pub fn with_framerate(mut self, framerate: u32) -> Self {
        self.framerate = framerate;
        self
    }

    /// Disable webcam capture
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Find the webcam device to use
    ///
    /// If a device name/index is specified, validates it exists.
    /// Otherwise, auto-detects the first camera device.
    pub fn find_webcam_device(&self) -> Result<String, CaptureError> {
        let devices = list_video_devices()?;

        // Filter to only camera devices (not screens)
        let cameras: Vec<_> = devices
            .iter()
            .filter(|d| !d.name.starts_with("Capture screen"))
            .collect();

        if cameras.is_empty() {
            return Err(CaptureError::NoWebcamDevices);
        }

        match &self.device {
            Some(device_spec) => {
                // Try to parse as index first
                if let Ok(index) = device_spec.parse::<usize>() {
                    // Find camera at this index
                    if index >= cameras.len() {
                        return Err(CaptureError::WebcamNotFound {
                            requested: device_spec.clone(),
                            available: cameras.iter().map(|d| d.name.clone()).collect(),
                        });
                    }
                    return Ok(cameras[index].name.clone());
                }

                // Try to match by name (case-insensitive contains)
                for camera in &cameras {
                    if camera.name.to_lowercase().contains(&device_spec.to_lowercase()) {
                        return Ok(camera.name.clone());
                    }
                }

                // Exact match attempt
                for camera in &cameras {
                    if camera.name == *device_spec {
                        return Ok(camera.name.clone());
                    }
                }

                Err(CaptureError::WebcamNotFound {
                    requested: device_spec.clone(),
                    available: cameras.iter().map(|d| d.name.clone()).collect(),
                })
            }
            None => {
                // Auto-detect first camera
                Ok(cameras[0].name.clone())
            }
        }
    }

    /// Generate FFmpeg input arguments for this webcam capture
    pub fn to_ffmpeg_args(&self, device_name: &str) -> Vec<String> {
        vec![
            "-f".to_string(),
            "avfoundation".to_string(),
            "-framerate".to_string(),
            self.framerate.to_string(),
            "-video_size".to_string(),
            format!("{}x{}", self.width, self.height),
            "-i".to_string(),
            device_name.to_string(),
        ]
    }

    /// Generate FFmpeg filter for webcam (includes mirror if enabled)
    #[allow(dead_code)] // Used in tests and may be useful for future modular filter construction
    pub fn to_filter(&self) -> Option<String> {
        if self.mirror {
            Some("hflip".to_string())
        } else {
            None
        }
    }
}

/// Noise gate configuration for reducing background noise
#[derive(Debug, Clone, Copy)]
pub struct NoiseGateConfig {
    /// Whether noise gate is enabled
    pub enabled: bool,
    /// Threshold level (0.0-1.0) - signals below this are gated (default: 0.01)
    pub threshold: f32,
    /// Ratio for how much to reduce signal (default: 2.0)
    pub ratio: f32,
    /// Attack time in milliseconds (default: 20ms)
    pub attack_ms: u32,
    /// Release time in milliseconds (default: 250ms)
    pub release_ms: u32,
}

/// Compressor configuration for evening out volume levels and preventing clipping
#[derive(Debug, Clone, Copy)]
pub struct CompressorConfig {
    /// Whether compressor is enabled
    pub enabled: bool,
    /// Threshold in dB (signals above this are compressed, default: -20dB)
    pub threshold_db: f32,
    /// Compression ratio (e.g., 4 means 4:1 compression, default: 4.0)
    pub ratio: f32,
    /// Attack time in milliseconds (default: 5ms)
    pub attack_ms: u32,
    /// Release time in milliseconds (default: 50ms)
    pub release_ms: u32,
}

impl Default for NoiseGateConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 0.01,
            ratio: 2.0,
            attack_ms: 20,
            release_ms: 250,
        }
    }
}

impl NoiseGateConfig {
    /// Create a new noise gate config with default settings, enabled
    pub fn new() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Set the threshold (0.0-1.0)
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Generate FFmpeg filter string for noise gate
    pub fn to_filter(self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        Some(format!(
            "agate=threshold={}:ratio={}:attack={}:release={}",
            self.threshold, self.ratio, self.attack_ms, self.release_ms
        ))
    }
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 5,
            release_ms: 50,
        }
    }
}

impl CompressorConfig {
    /// Create a new compressor config with default settings, enabled
    pub fn new() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Generate FFmpeg filter string for compressor
    ///
    /// Uses acompressor filter: threshold in dB, ratio as N:1
    /// Example output: acompressor=threshold=-20dB:ratio=4:attack=5:release=50
    pub fn to_filter(self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        Some(format!(
            "acompressor=threshold={}dB:ratio={}:attack={}:release={}",
            self.threshold_db as i32, self.ratio, self.attack_ms, self.release_ms
        ))
    }
}

/// Represents an audio capture configuration
#[derive(Debug, Clone)]
pub struct AudioCapture {
    /// Audio device name or index (None = auto-detect default microphone)
    pub device: Option<String>,
    /// Audio sample rate
    #[allow(dead_code)] // Used for future audio format configuration
    pub sample_rate: u32,
    /// Number of audio channels
    #[allow(dead_code)] // Used for future audio format configuration
    pub channels: u8,
    /// Volume level (0.0 = mute, 1.0 = normal, 2.0 = double)
    pub volume: f32,
    /// Whether audio capture is enabled
    pub enabled: bool,
    /// Noise gate configuration
    pub noise_gate: NoiseGateConfig,
    /// Compressor configuration
    pub compressor: CompressorConfig,
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self {
            device: None,
            sample_rate: 48000,
            channels: 2,
            volume: 1.0,
            enabled: true,
            noise_gate: NoiseGateConfig::default(),
            compressor: CompressorConfig::default(),
        }
    }
}

impl AudioCapture {
    /// Create a new audio capture configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the audio device by name or index
    #[allow(dead_code)] // Used for future --audio-device CLI flag
    pub fn with_device(mut self, device: impl Into<String>) -> Self {
        self.device = Some(device.into());
        self
    }

    /// Set the volume level (0.0-2.0)
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 2.0);
        self
    }

    /// Enable noise gate with default settings
    #[allow(dead_code)] // Used in tests
    pub fn with_noise_gate(mut self) -> Self {
        self.noise_gate = NoiseGateConfig::new();
        self
    }

    /// Enable noise gate with custom threshold
    pub fn with_noise_gate_threshold(mut self, threshold: f32) -> Self {
        self.noise_gate = NoiseGateConfig::new().with_threshold(threshold);
        self
    }

    /// Enable compressor with default settings
    pub fn with_compressor(mut self) -> Self {
        self.compressor = CompressorConfig::new();
        self
    }

    /// Disable audio capture
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Find the audio device to use
    ///
    /// If a device name/index is specified, validates it exists.
    /// Otherwise, auto-detects the first microphone device.
    pub fn find_audio_device(&self) -> Result<String, CaptureError> {
        let devices = list_audio_devices()?;

        if devices.is_empty() {
            return Err(CaptureError::NoAudioDevices);
        }

        match &self.device {
            Some(device_spec) => {
                // Try to parse as index first
                if let Ok(index) = device_spec.parse::<usize>() {
                    if index >= devices.len() {
                        return Err(CaptureError::AudioDeviceNotFound {
                            requested: device_spec.clone(),
                            available: devices.iter().map(|d| d.name.clone()).collect(),
                        });
                    }
                    return Ok(devices[index].name.clone());
                }

                // Try to match by name (case-insensitive contains)
                for device in &devices {
                    if device.name.to_lowercase().contains(&device_spec.to_lowercase()) {
                        return Ok(device.name.clone());
                    }
                }

                // Exact match attempt
                for device in &devices {
                    if device.name == *device_spec {
                        return Ok(device.name.clone());
                    }
                }

                Err(CaptureError::AudioDeviceNotFound {
                    requested: device_spec.clone(),
                    available: devices.iter().map(|d| d.name.clone()).collect(),
                })
            }
            None => {
                // Auto-detect first audio device (typically the default microphone)
                Ok(devices[0].name.clone())
            }
        }
    }

    /// Generate FFmpeg input arguments for this audio capture
    ///
    /// Uses the format ":device_name" for audio-only AVFoundation input
    pub fn to_ffmpeg_args(&self, device_name: &str) -> Vec<String> {
        vec![
            "-f".to_string(),
            "avfoundation".to_string(),
            "-i".to_string(),
            format!(":{}", device_name), // ":" prefix = audio-only
        ]
    }

    /// Generate FFmpeg audio filter chain
    ///
    /// Combines noise gate, compressor, and volume control filters as needed.
    /// Returns None if no audio processing is required.
    ///
    /// Filter order: noise_gate -> compressor -> volume
    /// - Noise gate first: reduces background noise before any amplification
    /// - Compressor second: evens out volume levels and prevents clipping
    /// - Volume last: final level adjustment after dynamic processing
    pub fn to_filter(&self) -> Option<String> {
        let mut filters = Vec::new();

        // Add noise gate first (reduces noise before any volume adjustment)
        if let Some(gate_filter) = self.noise_gate.to_filter() {
            filters.push(gate_filter);
        }

        // Add compressor (evens out volume levels, prevents clipping)
        if let Some(comp_filter) = self.compressor.to_filter() {
            filters.push(comp_filter);
        }

        // Add volume adjustment if not 1.0
        if (self.volume - 1.0).abs() >= f32::EPSILON {
            filters.push(format!("volume={:.2}", self.volume));
        }

        if filters.is_empty() {
            None
        } else {
            Some(filters.join(","))
        }
    }
}

/// Represents a screen capture configuration
#[derive(Debug, Clone)]
pub struct ScreenCapture {
    /// Screen device index (relative to available screens, not absolute device index)
    pub screen_index: usize,
    /// Capture framerate
    pub framerate: u32,
    /// Include mouse cursor in capture
    pub capture_cursor: bool,
}

impl Default for ScreenCapture {
    fn default() -> Self {
        Self {
            screen_index: 0,
            framerate: 30,
            capture_cursor: true,
        }
    }
}

impl ScreenCapture {
    /// Create a new screen capture configuration
    pub fn new(screen_index: usize) -> Self {
        Self {
            screen_index,
            ..Default::default()
        }
    }

    /// Set the capture framerate
    pub fn with_framerate(mut self, framerate: u32) -> Self {
        self.framerate = framerate;
        self
    }

    /// Find the actual device name for a screen index by querying FFmpeg
    pub fn find_screen_device(&self) -> Result<String, CaptureError> {
        let devices = list_video_devices()?;

        // Filter to only screen devices (those starting with "Capture screen")
        let screens: Vec<_> = devices
            .iter()
            .filter(|d| d.name.starts_with("Capture screen"))
            .collect();

        if screens.is_empty() {
            return Err(CaptureError::NoScreenDevices);
        }

        if self.screen_index >= screens.len() {
            return Err(CaptureError::ScreenNotFound {
                requested: self.screen_index,
                available: screens.len(),
            });
        }

        Ok(screens[self.screen_index].name.clone())
    }

    /// Generate FFmpeg input arguments for this screen capture
    pub fn to_ffmpeg_args(&self, device_name: &str) -> Vec<String> {
        vec![
            "-f".to_string(),
            "avfoundation".to_string(),
            "-framerate".to_string(),
            self.framerate.to_string(),
            "-capture_cursor".to_string(),
            if self.capture_cursor { "1" } else { "0" }.to_string(),
            "-i".to_string(),
            device_name.to_string(),
        ]
    }
}

/// Device information parsed from FFmpeg
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    #[allow(dead_code)] // Used in tests and for future device selection
    pub index: usize,
    pub name: String,
}

/// Window bounds information returned from AppleScript detection
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowBounds {
    /// X position of the window (left edge)
    pub x: i32,
    /// Y position of the window (top edge)
    pub y: i32,
    /// Width of the window
    pub width: u32,
    /// Height of the window
    pub height: u32,
}

impl WindowBounds {
    /// Generate an FFmpeg crop filter string for these window bounds
    ///
    /// Format: crop=width:height:x:y
    pub fn crop_filter(&self) -> String {
        format!("crop={}:{}:{}:{}", self.width, self.height, self.x, self.y)
    }

    /// Generate an FFmpeg crop filter with Retina scaling (2x)
    ///
    /// On Retina displays, AppleScript returns logical pixels but FFmpeg
    /// captures in physical pixels, so we need to multiply by 2.
    pub fn crop_filter_retina(&self) -> String {
        format!(
            "crop={}:{}:{}:{}",
            self.width * 2,
            self.height * 2,
            self.x * 2,
            self.y * 2
        )
    }

    /// Generate an FFmpeg crop filter with custom scale factor
    ///
    /// Useful for displays with non-standard scaling (e.g., 1.5x, 3x)
    #[allow(dead_code)] // Used in tests and for future high-DPI display support
    pub fn crop_filter_scaled(&self, scale: u32) -> String {
        format!(
            "crop={}:{}:{}:{}",
            self.width * scale,
            self.height * scale,
            self.x as i64 * scale as i64,
            self.y as i64 * scale as i64
        )
    }

    /// Generate the appropriate crop filter based on whether display is Retina
    ///
    /// Automatically detects Retina display and applies 2x scaling if needed.
    pub fn to_crop_filter(self) -> String {
        if is_retina_display() {
            self.crop_filter_retina()
        } else {
            self.crop_filter()
        }
    }
}

/// Check if the primary display is a Retina display (2x scaling)
///
/// On macOS, Retina displays report 2x backing scale factor.
/// This function queries the system to detect Retina mode.
pub fn is_retina_display() -> bool {
    // Use AppleScript to check the backing scale factor
    // This is more reliable than parsing system_profiler output
    let output = Command::new("osascript")
        .args([
            "-e",
            r#"tell application "Finder" to get bounds of window of desktop"#,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    // If we can't determine, assume Retina (safer for modern Macs)
    // Most Macs sold since 2012 have Retina displays
    if output.is_err() {
        return true;
    }

    // Alternative: Check NSScreen backing scale factor via a small script
    // For simplicity, we'll check if the screen resolution is typically Retina
    // by using system_profiler
    let profiler_output = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match profiler_output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Look for "Retina" in the output, or check if resolution > physical
            // Most modern Macs have Retina - if the output mentions "Retina" we know for sure
            stdout.contains("Retina") || stdout.contains("retina")
        }
        _ => true, // Default to Retina for modern Macs
    }
}

/// Window capture configuration that combines screen capture with window bounds
#[derive(Debug, Clone)]
pub struct WindowCapture {
    /// Application name to capture
    pub app_name: String,
    /// Window bounds (populated after detection)
    pub bounds: Option<WindowBounds>,
    /// Base screen capture settings
    #[allow(dead_code)] // Reserved for future use with per-window framerate settings
    pub screen_capture: ScreenCapture,
}

impl WindowCapture {
    /// Create a new window capture configuration for an application
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            bounds: None,
            screen_capture: ScreenCapture::default(),
        }
    }

    /// Detect window bounds for the configured application
    pub fn detect_bounds(&mut self) -> Result<WindowBounds, CaptureError> {
        let bounds = get_window_bounds(&self.app_name)?;
        self.bounds = Some(bounds);
        Ok(bounds)
    }

    /// Generate the crop filter for the detected window
    ///
    /// Returns None if bounds haven't been detected yet.
    pub fn crop_filter(&self) -> Option<String> {
        self.bounds.map(|b| b.to_crop_filter())
    }

    /// Generate FFmpeg input arguments for screen capture
    #[allow(dead_code)] // Used in tests and for future standalone window capture mode
    pub fn to_ffmpeg_input_args(&self, screen_device: &str) -> Vec<String> {
        self.screen_capture.to_ffmpeg_args(screen_device)
    }
}

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

/// List running applications that have visible windows.
///
/// Returns a sorted list of application names that currently have windows open.
/// This is useful for showing users which applications they can capture.
pub fn list_running_apps_with_windows() -> Vec<String> {
    let script = r#"
        tell application "System Events"
            set appList to {}
            repeat with proc in (every process whose visible is true)
                try
                    set procName to name of proc
                    set winCount to count of windows of proc
                    if winCount > 0 then
                        set end of appList to procName
                    end if
                end try
            end repeat
            return appList
        end tell
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
            // AppleScript returns comma-separated list
            let mut apps: Vec<String> = stdout
                .split(", ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            apps.sort();
            apps.dedup();
            apps
        }
        _ => Vec::new(), // Return empty on error - don't fail window detection
    }
}

/// Get window bounds for a named application using AppleScript
///
/// This function runs an AppleScript that:
/// 1. Checks if the application is running
/// 2. Gets the bounds of the front window
/// 3. Returns the bounds as (x, y, width, height)
///
/// # Arguments
/// * `app_name` - The name of the application (e.g., "Terminal", "Safari", "Code")
///
/// # Returns
/// * `Ok(WindowBounds)` - The window bounds on success
/// * `Err(CaptureError)` - An error if the app is not running, has no windows, or bounds detection fails
///
/// # Example
/// ```ignore
/// let bounds = get_window_bounds("Terminal")?;
/// println!("Window at ({}, {}), size {}x{}", bounds.x, bounds.y, bounds.width, bounds.height);
/// ```
pub fn get_window_bounds(app_name: &str) -> Result<WindowBounds, CaptureError> {
    // AppleScript that checks if app is running, then gets window bounds
    // Returns comma-separated: x,y,width,height
    // Outputs special error codes for different failure modes
    let script = format!(
        r#"
        -- Check if application is running
        tell application "System Events"
            set appRunning to (name of processes) contains "{app_name}"
        end tell

        if not appRunning then
            return "ERROR:NOT_RUNNING"
        end if

        -- Get window bounds
        tell application "{app_name}"
            try
                set windowCount to count of windows
                if windowCount is 0 then
                    return "ERROR:NO_WINDOWS"
                end if

                set b to bounds of front window
                -- bounds returns {{x1, y1, x2, y2}}
                set x1 to item 1 of b
                set y1 to item 2 of b
                set x2 to item 3 of b
                set y2 to item 4 of b
                set w to x2 - x1
                set h to y2 - y1
                return (x1 as string) & "," & (y1 as string) & "," & (w as string) & "," & (h as string)
            on error errMsg
                return "ERROR:BOUNDS_FAILED:" & errMsg
            end try
        end tell
        "#,
        app_name = app_name
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| CaptureError::WindowBoundsFailed {
            app_name: app_name.to_string(),
            message: format!("Failed to run osascript: {}", e),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    // Check for osascript execution errors first
    if !output.status.success() {
        return Err(CaptureError::WindowBoundsFailed {
            app_name: app_name.to_string(),
            message: if stderr.is_empty() { stdout } else { stderr },
        });
    }

    // Parse the result
    if let Some(error_part) = stdout.strip_prefix("ERROR:") {
        if error_part == "NOT_RUNNING" {
            return Err(CaptureError::AppNotRunning {
                app_name: app_name.to_string(),
                available_apps: list_running_apps_with_windows(),
            });
        } else if error_part == "NO_WINDOWS" {
            return Err(CaptureError::AppNoWindows {
                app_name: app_name.to_string(),
            });
        } else if let Some(bounds_msg) = error_part.strip_prefix("BOUNDS_FAILED:") {
            return Err(CaptureError::WindowBoundsFailed {
                app_name: app_name.to_string(),
                message: bounds_msg.to_string(),
            });
        } else {
            return Err(CaptureError::WindowBoundsFailed {
                app_name: app_name.to_string(),
                message: error_part.to_string(),
            });
        }
    }

    // Parse the comma-separated bounds: x,y,width,height
    parse_window_bounds(&stdout, app_name)
}

/// Parse window bounds from AppleScript output
///
/// Expected format: "x,y,width,height" (e.g., "100,50,800,600")
fn parse_window_bounds(output: &str, app_name: &str) -> Result<WindowBounds, CaptureError> {
    let parts: Vec<&str> = output.split(',').collect();
    if parts.len() != 4 {
        return Err(CaptureError::WindowBoundsFailed {
            app_name: app_name.to_string(),
            message: format!("Unexpected bounds format: '{}'. Expected 'x,y,width,height'", output),
        });
    }

    let x: i32 = parts[0].trim().parse().map_err(|_| CaptureError::WindowBoundsFailed {
        app_name: app_name.to_string(),
        message: format!("Invalid x coordinate: '{}'", parts[0]),
    })?;

    let y: i32 = parts[1].trim().parse().map_err(|_| CaptureError::WindowBoundsFailed {
        app_name: app_name.to_string(),
        message: format!("Invalid y coordinate: '{}'", parts[1]),
    })?;

    let width: i32 = parts[2].trim().parse().map_err(|_| CaptureError::WindowBoundsFailed {
        app_name: app_name.to_string(),
        message: format!("Invalid width: '{}'", parts[2]),
    })?;

    let height: i32 = parts[3].trim().parse().map_err(|_| CaptureError::WindowBoundsFailed {
        app_name: app_name.to_string(),
        message: format!("Invalid height: '{}'", parts[3]),
    })?;

    // Validate values are non-negative
    if width < 0 || height < 0 {
        return Err(CaptureError::WindowBoundsFailed {
            app_name: app_name.to_string(),
            message: format!("Invalid window dimensions: {}x{}", width, height),
        });
    }

    Ok(WindowBounds {
        x,
        y,
        width: width as u32,
        height: height as u32,
    })
}

/// List available video devices from FFmpeg
fn list_video_devices() -> Result<Vec<DeviceInfo>, CaptureError> {
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CaptureError::FfmpegNotFound
            } else {
                CaptureError::FfmpegFailed(e.to_string())
            }
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_video_devices(&stderr)
}

/// Parse video devices from FFmpeg stderr output
fn parse_video_devices(stderr: &str) -> Result<Vec<DeviceInfo>, CaptureError> {
    let mut devices = Vec::new();
    let mut in_video_section = false;

    for line in stderr.lines() {
        if line.contains("AVFoundation video devices:") {
            in_video_section = true;
            continue;
        }
        if line.contains("AVFoundation audio devices:") {
            break; // Done with video section
        }

        if in_video_section {
            if let Some(device) = parse_device_line(line) {
                devices.push(device);
            }
        }
    }

    Ok(devices)
}

/// Parse a single device line from FFmpeg output
fn parse_device_line(line: &str) -> Option<DeviceInfo> {
    // Format: [AVFoundation indev @ 0x...] [index] device name
    let bracket_idx = line.find("] [")?;
    let after_bracket = &line[bracket_idx + 3..];
    let close_bracket = after_bracket.find(']')?;
    let index_str = &after_bracket[..close_bracket];
    let index: usize = index_str.parse().ok()?;
    let name = after_bracket[close_bracket + 2..].trim().to_string();

    if name.is_empty() {
        return None;
    }

    Some(DeviceInfo { index, name })
}

/// List available audio devices from FFmpeg
fn list_audio_devices() -> Result<Vec<DeviceInfo>, CaptureError> {
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CaptureError::FfmpegNotFound
            } else {
                CaptureError::FfmpegFailed(e.to_string())
            }
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_audio_devices(&stderr)
}

/// Parse audio devices from FFmpeg stderr output
fn parse_audio_devices(stderr: &str) -> Result<Vec<DeviceInfo>, CaptureError> {
    let mut devices = Vec::new();
    let mut in_audio_section = false;

    for line in stderr.lines() {
        if line.contains("AVFoundation audio devices:") {
            in_audio_section = true;
            continue;
        }

        if in_audio_section {
            if let Some(device) = parse_device_line(line) {
                devices.push(device);
            }
        }
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capture_default() {
        let capture = ScreenCapture::default();
        assert_eq!(capture.screen_index, 0);
        assert_eq!(capture.framerate, 30);
        assert!(capture.capture_cursor);
    }

    #[test]
    fn test_screen_capture_new() {
        let capture = ScreenCapture::new(1);
        assert_eq!(capture.screen_index, 1);
        assert_eq!(capture.framerate, 30);
        assert!(capture.capture_cursor);
    }

    #[test]
    fn test_screen_capture_with_framerate() {
        let capture = ScreenCapture::new(0).with_framerate(60);
        assert_eq!(capture.framerate, 60);
        let args = capture.to_ffmpeg_args("Capture screen 0");
        let framerate_idx = args.iter().position(|a| a == "-framerate").unwrap();
        assert_eq!(args[framerate_idx + 1], "60");
    }

    #[test]
    fn test_to_ffmpeg_args() {
        let capture = ScreenCapture::default();
        let args = capture.to_ffmpeg_args("Capture screen 0");

        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"avfoundation".to_string()));
        assert!(args.contains(&"-framerate".to_string()));
        assert!(args.contains(&"30".to_string()));
        assert!(args.contains(&"-capture_cursor".to_string()));
        assert!(args.contains(&"1".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"Capture screen 0".to_string()));
    }

    #[test]
    fn test_to_ffmpeg_args_no_cursor() {
        let capture = ScreenCapture {
            screen_index: 0,
            framerate: 30,
            capture_cursor: false,
        };
        let args = capture.to_ffmpeg_args("Capture screen 0");

        // Check cursor flag is 0
        let cursor_idx = args.iter().position(|a| a == "-capture_cursor").unwrap();
        assert_eq!(args[cursor_idx + 1], "0");
    }

    #[test]
    fn test_parse_device_line_screen() {
        let line = "[AVFoundation indev @ 0x12345678] [1] Capture screen 0";
        let device = parse_device_line(line).unwrap();
        assert_eq!(device.index, 1);
        assert_eq!(device.name, "Capture screen 0");
    }

    #[test]
    fn test_parse_video_devices() {
        let stderr = r#"
[AVFoundation indev @ 0x123] AVFoundation video devices:
[AVFoundation indev @ 0x123] [0] FaceTime HD Camera
[AVFoundation indev @ 0x123] [1] Capture screen 0
[AVFoundation indev @ 0x123] [2] Capture screen 1
[AVFoundation indev @ 0x123] AVFoundation audio devices:
[AVFoundation indev @ 0x123] [0] MacBook Pro Microphone
"#;
        let devices = parse_video_devices(stderr).unwrap();
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].name, "FaceTime HD Camera");
        assert_eq!(devices[1].name, "Capture screen 0");
        assert_eq!(devices[2].name, "Capture screen 1");
    }

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

    // Webcam capture tests

    #[test]
    fn test_webcam_capture_default() {
        let capture = WebcamCapture::default();
        assert!(capture.device.is_none());
        assert_eq!(capture.framerate, 30);
        assert_eq!(capture.width, 1280);
        assert_eq!(capture.height, 720);
        assert!(!capture.mirror);
        assert!(capture.enabled);
    }

    #[test]
    fn test_webcam_capture_new() {
        let capture = WebcamCapture::new();
        assert!(capture.enabled);
        assert!(!capture.mirror);
        assert!(capture.device.is_none());
    }

    #[test]
    fn test_webcam_capture_with_device() {
        let capture = WebcamCapture::new().with_device("FaceTime HD Camera");
        assert_eq!(capture.device, Some("FaceTime HD Camera".to_string()));
    }

    #[test]
    fn test_webcam_capture_with_mirror() {
        let capture = WebcamCapture::new().with_mirror(true);
        assert!(capture.mirror);
    }

    #[test]
    fn test_webcam_capture_with_framerate() {
        let capture = WebcamCapture::new().with_framerate(60);
        assert_eq!(capture.framerate, 60);
        let args = capture.to_ffmpeg_args("FaceTime HD Camera");
        let framerate_idx = args.iter().position(|a| a == "-framerate").unwrap();
        assert_eq!(args[framerate_idx + 1], "60");
    }

    #[test]
    fn test_webcam_capture_disabled() {
        let capture = WebcamCapture::disabled();
        assert!(!capture.enabled);
    }

    #[test]
    fn test_webcam_to_ffmpeg_args() {
        let capture = WebcamCapture::default();
        let args = capture.to_ffmpeg_args("FaceTime HD Camera");

        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"avfoundation".to_string()));
        assert!(args.contains(&"-framerate".to_string()));
        assert!(args.contains(&"30".to_string()));
        assert!(args.contains(&"-video_size".to_string()));
        assert!(args.contains(&"1280x720".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"FaceTime HD Camera".to_string()));
    }

    #[test]
    fn test_webcam_to_filter_no_mirror() {
        let capture = WebcamCapture::new();
        assert!(capture.to_filter().is_none());
    }

    #[test]
    fn test_webcam_to_filter_with_mirror() {
        let capture = WebcamCapture::new().with_mirror(true);
        let filter = capture.to_filter().unwrap();
        assert_eq!(filter, "hflip");
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

    // Audio capture tests

    #[test]
    fn test_audio_capture_default() {
        let capture = AudioCapture::default();
        assert!(capture.device.is_none());
        assert_eq!(capture.sample_rate, 48000);
        assert_eq!(capture.channels, 2);
        assert_eq!(capture.volume, 1.0);
        assert!(capture.enabled);
    }

    #[test]
    fn test_audio_capture_new() {
        let capture = AudioCapture::new();
        assert!(capture.enabled);
        assert!(capture.device.is_none());
        assert_eq!(capture.volume, 1.0);
    }

    #[test]
    fn test_audio_capture_with_device() {
        let capture = AudioCapture::new().with_device("MacBook Pro Microphone");
        assert_eq!(capture.device, Some("MacBook Pro Microphone".to_string()));
    }

    #[test]
    fn test_audio_capture_with_volume() {
        let capture = AudioCapture::new().with_volume(0.5);
        assert_eq!(capture.volume, 0.5);
    }

    #[test]
    fn test_audio_capture_volume_clamped() {
        let capture_high = AudioCapture::new().with_volume(3.0);
        assert_eq!(capture_high.volume, 2.0); // Clamped to max

        let capture_low = AudioCapture::new().with_volume(-1.0);
        assert_eq!(capture_low.volume, 0.0); // Clamped to min
    }

    #[test]
    fn test_audio_capture_disabled() {
        let capture = AudioCapture::disabled();
        assert!(!capture.enabled);
    }

    #[test]
    fn test_audio_to_ffmpeg_args() {
        let capture = AudioCapture::default();
        let args = capture.to_ffmpeg_args("MacBook Pro Microphone");

        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"avfoundation".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&":MacBook Pro Microphone".to_string()));
    }

    #[test]
    fn test_audio_to_filter_no_adjustment() {
        let capture = AudioCapture::new();
        assert!(capture.to_filter().is_none()); // Volume 1.0 = no filter needed
    }

    #[test]
    fn test_audio_to_filter_with_volume() {
        let capture = AudioCapture::new().with_volume(0.5);
        let filter = capture.to_filter().unwrap();
        assert_eq!(filter, "volume=0.50");
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
    fn test_parse_audio_devices() {
        let stderr = r#"
[AVFoundation indev @ 0x123] AVFoundation video devices:
[AVFoundation indev @ 0x123] [0] FaceTime HD Camera
[AVFoundation indev @ 0x123] [1] Capture screen 0
[AVFoundation indev @ 0x123] AVFoundation audio devices:
[AVFoundation indev @ 0x123] [0] MacBook Pro Microphone
[AVFoundation indev @ 0x123] [1] External USB Mic
"#;
        let devices = parse_audio_devices(stderr).unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "MacBook Pro Microphone");
        assert_eq!(devices[1].name, "External USB Mic");
    }

    // Noise gate tests

    #[test]
    fn test_noise_gate_config_default() {
        let config = NoiseGateConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.threshold, 0.01);
        assert_eq!(config.ratio, 2.0);
        assert_eq!(config.attack_ms, 20);
        assert_eq!(config.release_ms, 250);
    }

    #[test]
    fn test_noise_gate_config_new() {
        let config = NoiseGateConfig::new();
        assert!(config.enabled);
        assert_eq!(config.threshold, 0.01);
        assert_eq!(config.ratio, 2.0);
        assert_eq!(config.attack_ms, 20);
        assert_eq!(config.release_ms, 250);
    }

    #[test]
    fn test_noise_gate_config_with_threshold() {
        let config = NoiseGateConfig::new().with_threshold(0.05);
        assert!(config.enabled);
        assert_eq!(config.threshold, 0.05);
    }

    #[test]
    fn test_noise_gate_config_threshold_clamped() {
        let config_high = NoiseGateConfig::new().with_threshold(1.5);
        assert_eq!(config_high.threshold, 1.0); // Clamped to max

        let config_low = NoiseGateConfig::new().with_threshold(-0.5);
        assert_eq!(config_low.threshold, 0.0); // Clamped to min
    }

    #[test]
    fn test_noise_gate_to_filter_enabled() {
        let config = NoiseGateConfig::new();
        let filter = config.to_filter().unwrap();
        assert_eq!(filter, "agate=threshold=0.01:ratio=2:attack=20:release=250");
    }

    #[test]
    fn test_noise_gate_to_filter_custom_threshold() {
        let config = NoiseGateConfig::new().with_threshold(0.05);
        let filter = config.to_filter().unwrap();
        assert_eq!(filter, "agate=threshold=0.05:ratio=2:attack=20:release=250");
    }

    #[test]
    fn test_noise_gate_to_filter_disabled() {
        let config = NoiseGateConfig::default(); // disabled by default
        assert!(config.to_filter().is_none());
    }

    #[test]
    fn test_audio_capture_with_noise_gate() {
        let capture = AudioCapture::new().with_noise_gate();
        assert!(capture.noise_gate.enabled);
        assert_eq!(capture.noise_gate.threshold, 0.01);
    }

    #[test]
    fn test_audio_capture_with_noise_gate_threshold() {
        let capture = AudioCapture::new().with_noise_gate_threshold(0.02);
        assert!(capture.noise_gate.enabled);
        assert_eq!(capture.noise_gate.threshold, 0.02);
    }

    #[test]
    fn test_audio_to_filter_with_noise_gate_only() {
        let capture = AudioCapture::new().with_noise_gate();
        let filter = capture.to_filter().unwrap();
        // Should have noise gate but no volume adjustment (volume is 1.0)
        assert_eq!(filter, "agate=threshold=0.01:ratio=2:attack=20:release=250");
    }

    #[test]
    fn test_audio_to_filter_with_volume_and_noise_gate() {
        let capture = AudioCapture::new().with_volume(0.5).with_noise_gate();
        let filter = capture.to_filter().unwrap();
        // Noise gate should come first, then volume
        assert_eq!(filter, "agate=threshold=0.01:ratio=2:attack=20:release=250,volume=0.50");
    }

    #[test]
    fn test_audio_to_filter_noise_gate_before_volume() {
        let capture = AudioCapture::new()
            .with_noise_gate_threshold(0.03)
            .with_volume(1.5);
        let filter = capture.to_filter().unwrap();
        // Verify noise gate comes before volume in the filter chain
        let gate_pos = filter.find("agate=").unwrap();
        let volume_pos = filter.find("volume=").unwrap();
        assert!(gate_pos < volume_pos, "Noise gate should be applied before volume");
    }

    #[test]
    fn test_audio_capture_default_has_noise_gate_disabled() {
        let capture = AudioCapture::default();
        assert!(!capture.noise_gate.enabled);
        // to_filter should return None when noise gate is disabled and volume is 1.0
        assert!(capture.to_filter().is_none());
    }

    // Compressor tests

    #[test]
    fn test_compressor_config_default() {
        let config = CompressorConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.threshold_db, -20.0);
        assert_eq!(config.ratio, 4.0);
        assert_eq!(config.attack_ms, 5);
        assert_eq!(config.release_ms, 50);
    }

    #[test]
    fn test_compressor_config_new() {
        let config = CompressorConfig::new();
        assert!(config.enabled);
        assert_eq!(config.threshold_db, -20.0);
        assert_eq!(config.ratio, 4.0);
        assert_eq!(config.attack_ms, 5);
        assert_eq!(config.release_ms, 50);
    }

    #[test]
    fn test_compressor_to_filter_enabled() {
        let config = CompressorConfig::new();
        let filter = config.to_filter().unwrap();
        assert_eq!(filter, "acompressor=threshold=-20dB:ratio=4:attack=5:release=50");
    }

    #[test]
    fn test_compressor_to_filter_disabled() {
        let config = CompressorConfig::default(); // disabled by default
        assert!(config.to_filter().is_none());
    }

    #[test]
    fn test_audio_capture_with_compressor() {
        let capture = AudioCapture::new().with_compressor();
        assert!(capture.compressor.enabled);
        assert_eq!(capture.compressor.threshold_db, -20.0);
    }

    #[test]
    fn test_audio_capture_default_has_compressor_disabled() {
        let capture = AudioCapture::default();
        assert!(!capture.compressor.enabled);
    }

    #[test]
    fn test_audio_to_filter_with_compressor_only() {
        let capture = AudioCapture::new().with_compressor();
        let filter = capture.to_filter().unwrap();
        // Should have compressor but no volume adjustment (volume is 1.0) or noise gate
        assert_eq!(filter, "acompressor=threshold=-20dB:ratio=4:attack=5:release=50");
    }

    #[test]
    fn test_audio_to_filter_with_volume_and_compressor() {
        let capture = AudioCapture::new().with_volume(0.5).with_compressor();
        let filter = capture.to_filter().unwrap();
        // Compressor should come first, then volume
        assert_eq!(filter, "acompressor=threshold=-20dB:ratio=4:attack=5:release=50,volume=0.50");
    }

    #[test]
    fn test_audio_to_filter_full_chain() {
        // Test with noise gate + compressor + volume
        let capture = AudioCapture::new()
            .with_noise_gate()
            .with_compressor()
            .with_volume(0.8);
        let filter = capture.to_filter().unwrap();

        // Order should be: noise_gate -> compressor -> volume
        let gate_pos = filter.find("agate=").unwrap();
        let comp_pos = filter.find("acompressor=").unwrap();
        let volume_pos = filter.find("volume=").unwrap();

        assert!(gate_pos < comp_pos, "Noise gate should come before compressor");
        assert!(comp_pos < volume_pos, "Compressor should come before volume");
    }

    #[test]
    fn test_audio_to_filter_noise_gate_and_compressor() {
        let capture = AudioCapture::new()
            .with_noise_gate()
            .with_compressor();
        let filter = capture.to_filter().unwrap();

        // Should have both filters in correct order
        assert!(filter.contains("agate="));
        assert!(filter.contains("acompressor="));

        let gate_pos = filter.find("agate=").unwrap();
        let comp_pos = filter.find("acompressor=").unwrap();
        assert!(gate_pos < comp_pos, "Noise gate should come before compressor");
    }

    // Window bounds tests

    #[test]
    fn test_window_bounds_struct() {
        let bounds = WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        };
        assert_eq!(bounds.x, 100);
        assert_eq!(bounds.y, 50);
        assert_eq!(bounds.width, 800);
        assert_eq!(bounds.height, 600);
    }

    #[test]
    fn test_window_bounds_crop_filter() {
        let bounds = WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        };
        let filter = bounds.crop_filter();
        assert_eq!(filter, "crop=800:600:100:50");
    }

    #[test]
    fn test_window_bounds_crop_filter_retina() {
        let bounds = WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        };
        let filter = bounds.crop_filter_retina();
        // Should be 2x multiplied for Retina
        assert_eq!(filter, "crop=1600:1200:200:100");
    }

    #[test]
    fn test_window_bounds_crop_filter_zero_offset() {
        let bounds = WindowBounds {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let filter = bounds.crop_filter();
        assert_eq!(filter, "crop=1920:1080:0:0");
    }

    #[test]
    fn test_window_bounds_negative_position() {
        // Windows can have negative positions on multi-monitor setups
        let bounds = WindowBounds {
            x: -100,
            y: 50,
            width: 800,
            height: 600,
        };
        let filter = bounds.crop_filter();
        assert_eq!(filter, "crop=800:600:-100:50");
    }

    #[test]
    fn test_parse_window_bounds_valid() {
        let result = parse_window_bounds("100,50,800,600", "TestApp");
        assert!(result.is_ok());
        let bounds = result.unwrap();
        assert_eq!(bounds.x, 100);
        assert_eq!(bounds.y, 50);
        assert_eq!(bounds.width, 800);
        assert_eq!(bounds.height, 600);
    }

    #[test]
    fn test_parse_window_bounds_with_spaces() {
        let result = parse_window_bounds(" 100 , 50 , 800 , 600 ", "TestApp");
        assert!(result.is_ok());
        let bounds = result.unwrap();
        assert_eq!(bounds.x, 100);
        assert_eq!(bounds.y, 50);
        assert_eq!(bounds.width, 800);
        assert_eq!(bounds.height, 600);
    }

    #[test]
    fn test_parse_window_bounds_negative_x() {
        let result = parse_window_bounds("-100,50,800,600", "TestApp");
        assert!(result.is_ok());
        let bounds = result.unwrap();
        assert_eq!(bounds.x, -100);
    }

    #[test]
    fn test_parse_window_bounds_invalid_format_too_few() {
        let result = parse_window_bounds("100,50,800", "TestApp");
        assert!(result.is_err());
        if let Err(CaptureError::WindowBoundsFailed { app_name, message }) = result {
            assert_eq!(app_name, "TestApp");
            assert!(message.contains("Unexpected bounds format"));
        } else {
            panic!("Expected WindowBoundsFailed error");
        }
    }

    #[test]
    fn test_parse_window_bounds_invalid_format_too_many() {
        let result = parse_window_bounds("100,50,800,600,extra", "TestApp");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_window_bounds_invalid_number() {
        let result = parse_window_bounds("abc,50,800,600", "TestApp");
        assert!(result.is_err());
        if let Err(CaptureError::WindowBoundsFailed { app_name, message }) = result {
            assert_eq!(app_name, "TestApp");
            assert!(message.contains("Invalid x coordinate"));
        } else {
            panic!("Expected WindowBoundsFailed error");
        }
    }

    #[test]
    fn test_parse_window_bounds_negative_width() {
        let result = parse_window_bounds("100,50,-800,600", "TestApp");
        assert!(result.is_err());
        if let Err(CaptureError::WindowBoundsFailed { app_name, message }) = result {
            assert_eq!(app_name, "TestApp");
            assert!(message.contains("Invalid window dimensions"));
        } else {
            panic!("Expected WindowBoundsFailed error");
        }
    }

    #[test]
    fn test_parse_window_bounds_negative_height() {
        let result = parse_window_bounds("100,50,800,-600", "TestApp");
        assert!(result.is_err());
        if let Err(CaptureError::WindowBoundsFailed { app_name, message }) = result {
            assert_eq!(app_name, "TestApp");
            assert!(message.contains("Invalid window dimensions"));
        } else {
            panic!("Expected WindowBoundsFailed error");
        }
    }

    // Error display tests for window errors

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

    // New crop filter tests for window capture integration

    #[test]
    fn test_window_bounds_crop_filter_scaled() {
        let bounds = WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        };
        // Test 1x scale (same as crop_filter)
        let filter_1x = bounds.crop_filter_scaled(1);
        assert_eq!(filter_1x, "crop=800:600:100:50");

        // Test 2x scale (same as crop_filter_retina)
        let filter_2x = bounds.crop_filter_scaled(2);
        assert_eq!(filter_2x, "crop=1600:1200:200:100");

        // Test 3x scale (for future high-DPI displays)
        let filter_3x = bounds.crop_filter_scaled(3);
        assert_eq!(filter_3x, "crop=2400:1800:300:150");
    }

    #[test]
    fn test_window_bounds_crop_filter_scaled_negative_position() {
        // Multi-monitor setups can have negative positions
        let bounds = WindowBounds {
            x: -200,
            y: 100,
            width: 1024,
            height: 768,
        };
        let filter = bounds.crop_filter_scaled(2);
        assert_eq!(filter, "crop=2048:1536:-400:200");
    }

    #[test]
    fn test_window_capture_new() {
        let capture = WindowCapture::new("Terminal");
        assert_eq!(capture.app_name, "Terminal");
        assert!(capture.bounds.is_none());
        assert_eq!(capture.screen_capture.screen_index, 0);
    }

    #[test]
    fn test_window_capture_crop_filter_none_without_bounds() {
        let capture = WindowCapture::new("Terminal");
        assert!(capture.crop_filter().is_none());
    }

    #[test]
    fn test_window_capture_to_ffmpeg_input_args() {
        let capture = WindowCapture::new("Terminal");
        let args = capture.to_ffmpeg_input_args("Capture screen 0");

        // Should have the standard screen capture args
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"avfoundation".to_string()));
        assert!(args.contains(&"-framerate".to_string()));
        assert!(args.contains(&"30".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"Capture screen 0".to_string()));
    }

    #[test]
    fn test_window_bounds_to_crop_filter() {
        // This test verifies the auto-detection method exists
        // The actual retina detection will vary by system
        let bounds = WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        };
        let filter = bounds.to_crop_filter();

        // Should produce a valid crop filter (either scaled or not)
        assert!(filter.starts_with("crop="));
        // Width should be 800 or 1600 (depending on Retina)
        assert!(filter.contains("800:600:100:50") || filter.contains("1600:1200:200:100"));
    }

    #[test]
    fn test_is_retina_display() {
        // Just verify the function runs without panicking
        // Result will vary based on actual hardware
        let _is_retina = is_retina_display();
        // No assertion on the value - it depends on the actual display
    }

    #[test]
    fn test_list_running_apps_with_windows() {
        // Just verify the function runs without panicking
        // Result will vary based on what apps are running
        let apps = list_running_apps_with_windows();
        // The list should be sorted and deduplicated (no assertion on contents)
        let mut sorted_apps = apps.clone();
        sorted_apps.sort();
        sorted_apps.dedup();
        assert_eq!(apps, sorted_apps, "List should be sorted and deduplicated");
    }
}
