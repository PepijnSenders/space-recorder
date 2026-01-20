//! Camera capture module for webcam access and frame capture.

use nokhwa::pixel_format::RgbFormat;
use nokhwa::query;
use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat as NokhwaFrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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
                write!(f, "Camera permission denied. On macOS, grant access in System Settings > Privacy & Security > Camera")
            }
            CameraError::DeviceNotFound(index) => {
                write!(f, "Camera device {} not found. Run 'list-cameras' to see available devices", index)
            }
            CameraError::StreamFailed(msg) => write!(f, "Failed to start camera stream: {}", msg),
            CameraError::AlreadyRunning => write!(f, "Capture thread is already running"),
        }
    }
}

impl std::error::Error for CameraError {}

/// List all available camera devices on the system.
///
/// Returns a vector of `CameraInfo` structs, or an error if querying fails.
/// If no cameras are found, returns an empty vector (not an error).
pub fn list_devices() -> Result<Vec<CameraInfo>, CameraError> {
    let devices = query(ApiBackend::Auto).map_err(|e| CameraError::QueryFailed(e.to_string()))?;

    Ok(devices
        .into_iter()
        .map(|d| CameraInfo {
            index: d.index().as_index().unwrap_or(0),
            name: d.human_name(),
            description: d.description().to_string(),
        })
        .collect())
}

/// Commands sent to the capture thread.
enum CaptureCommand {
    Stop,
}

/// Camera capture handle.
///
/// Wraps a nokhwa Camera and provides methods for capture operations.
/// Use `open()` to create a new instance with specified settings.
///
/// The camera runs a background thread that continuously captures frames
/// and stores the latest frame in a shared buffer. Call `start()` to begin
/// capturing and `get_frame()` to retrieve the latest frame.
pub struct CameraCapture {
    /// Latest captured frame (shared with capture thread)
    frame_buffer: Arc<Mutex<Option<Frame>>>,
    /// Capture thread handle
    capture_thread: Option<JoinHandle<()>>,
    /// Channel to send commands to capture thread
    command_tx: Option<Sender<CaptureCommand>>,
    /// Signal to stop capture thread
    stop_signal: Arc<AtomicBool>,
    /// Current settings
    settings: CameraSettings,
    /// Actual resolution (set after camera opens)
    actual_resolution: Option<Resolution>,
    /// Actual FPS (set after camera opens)
    actual_fps: Option<u32>,
}

impl std::fmt::Debug for CameraCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CameraCapture")
            .field("settings", &self.settings)
            .field("is_running", &self.is_running())
            .finish_non_exhaustive()
    }
}

impl CameraCapture {
    /// Open a camera with the specified settings.
    ///
    /// This validates that the camera exists but doesn't actually open
    /// the camera stream until `start()` is called. The camera is opened
    /// inside the background thread to avoid thread-safety issues.
    ///
    /// # Arguments
    /// * `settings` - Camera configuration including device index and resolution
    ///
    /// # Errors
    /// * `CameraError::DeviceNotFound` - If the device index doesn't exist
    pub fn open(settings: CameraSettings) -> Result<Self, CameraError> {
        // First check if the device exists
        let devices = list_devices()?;
        if !devices.iter().any(|d| d.index == settings.device_index) {
            return Err(CameraError::DeviceNotFound(settings.device_index));
        }

        Ok(Self {
            frame_buffer: Arc::new(Mutex::new(None)),
            capture_thread: None,
            command_tx: None,
            stop_signal: Arc::new(AtomicBool::new(false)),
            settings,
            actual_resolution: None,
            actual_fps: None,
        })
    }

    /// Get the current camera settings.
    pub fn settings(&self) -> &CameraSettings {
        &self.settings
    }

    /// Get the actual resolution the camera is using.
    ///
    /// Returns `None` if the camera hasn't been started yet.
    /// This may differ from the requested resolution if the camera
    /// doesn't support it exactly.
    pub fn actual_resolution(&self) -> Option<Resolution> {
        self.actual_resolution
    }

    /// Get the actual frame rate the camera is using.
    ///
    /// Returns `None` if the camera hasn't been started yet.
    pub fn actual_fps(&self) -> Option<u32> {
        self.actual_fps
    }

    /// Start capturing frames in a background thread.
    ///
    /// Frames are continuously captured and stored in a shared buffer.
    /// Use `get_frame()` to retrieve the latest frame.
    ///
    /// # Errors
    /// * `CameraError::AlreadyRunning` - If capture is already running
    /// * `CameraError::StreamFailed` - If the camera stream fails to start
    /// * `CameraError::PermissionDenied` - If camera access is denied (macOS)
    /// * `CameraError::OpenFailed` - If camera fails to open for other reasons
    pub fn start(&mut self) -> Result<(), CameraError> {
        if self.is_running() {
            return Err(CameraError::AlreadyRunning);
        }

        // Reset stop signal
        self.stop_signal.store(false, Ordering::SeqCst);

        // Create channel for commands
        let (tx, rx) = mpsc::channel();
        self.command_tx = Some(tx);

        // Clone values for the capture thread
        let buffer = Arc::clone(&self.frame_buffer);
        let stop = Arc::clone(&self.stop_signal);
        let settings = self.settings.clone();

        // Channel to receive actual resolution/fps from thread
        let (info_tx, info_rx) = mpsc::channel::<Result<(Resolution, u32), CameraError>>();

        // Spawn background capture thread
        // The camera is created inside the thread since nokhwa::Camera isn't Send
        let handle = thread::spawn(move || {
            // Open camera inside the thread
            let index = CameraIndex::Index(settings.device_index);

            // Try multiple format strategies in order of preference:
            // 1. Closest match with NV12 (common on macOS)
            // 2. Closest match with MJPEG (widely supported)
            // 3. Highest resolution available (let camera decide format)
            let format_attempts: Vec<RequestedFormat> = vec![
                // Try NV12 first (native macOS format)
                RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(CameraFormat::new(
                    nokhwa::utils::Resolution::new(settings.resolution.width, settings.resolution.height),
                    NokhwaFrameFormat::NV12,
                    settings.fps,
                ))),
                // Try MJPEG (widely supported, good compression)
                RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(CameraFormat::new(
                    nokhwa::utils::Resolution::new(settings.resolution.width, settings.resolution.height),
                    NokhwaFrameFormat::MJPEG,
                    settings.fps,
                ))),
                // Fallback: let the camera pick whatever format works best
                RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestResolution),
            ];

            let mut camera = None;
            let mut last_error = None;

            for requested in format_attempts {
                match Camera::new(index.clone(), requested) {
                    Ok(cam) => {
                        camera = Some(cam);
                        break;
                    }
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                }
            }

            let mut camera = match camera {
                Some(cam) => cam,
                None => {
                    let e = last_error.unwrap();
                    let msg = e.to_string().to_lowercase();
                    let err = if msg.contains("permission")
                        || msg.contains("denied")
                        || msg.contains("authorization")
                        || msg.contains("access")
                    {
                        CameraError::PermissionDenied
                    } else {
                        CameraError::OpenFailed(e.to_string())
                    };
                    let _ = info_tx.send(Err(err));
                    return;
                }
            };

            // Open stream
            if let Err(e) = camera.open_stream() {
                let _ = info_tx.send(Err(CameraError::StreamFailed(e.to_string())));
                return;
            }

            // Send back the actual resolution and fps
            let res = camera.resolution();
            let actual_res = Resolution {
                width: res.width(),
                height: res.height(),
            };
            let actual_fps = camera.frame_rate();
            let _ = info_tx.send(Ok((actual_res, actual_fps)));

            // Capture loop
            while !stop.load(Ordering::Relaxed) {
                // Check for commands (non-blocking)
                if let Ok(CaptureCommand::Stop) = rx.try_recv() {
                    break;
                }

                // Try to capture a frame
                if let Ok(raw_frame) = camera.frame() {
                    // Convert to RGB Frame (handles MJPEG, YUYV, and other formats)
                    if let Some(mut frame) = convert_to_rgb(&raw_frame) {
                        // Apply mirroring if enabled
                        if settings.mirror {
                            mirror_horizontal(&mut frame);
                        }

                        // Store in shared buffer
                        if let Ok(mut buf) = buffer.lock() {
                            *buf = Some(frame);
                        }
                    }
                    // If conversion fails, silently skip this frame and try the next one
                }

                // Small sleep to allow checking stop signal, but don't delay too much
                // since camera.frame() already blocks waiting for the next frame.
                thread::sleep(Duration::from_millis(1));
            }

            // Clean up
            let _ = camera.stop_stream();
        });

        self.capture_thread = Some(handle);

        // Wait for the thread to report success or failure
        match info_rx.recv() {
            Ok(Ok((res, fps))) => {
                self.actual_resolution = Some(res);
                self.actual_fps = Some(fps);
                Ok(())
            }
            Ok(Err(e)) => {
                // Thread encountered an error, clean up
                self.stop_signal.store(true, Ordering::SeqCst);
                if let Some(h) = self.capture_thread.take() {
                    let _ = h.join();
                }
                Err(e)
            }
            Err(_) => {
                // Channel closed unexpectedly
                self.stop_signal.store(true, Ordering::SeqCst);
                if let Some(h) = self.capture_thread.take() {
                    let _ = h.join();
                }
                Err(CameraError::StreamFailed("Capture thread terminated unexpectedly".to_string()))
            }
        }
    }

    /// Stop the capture thread.
    ///
    /// This will signal the background thread to stop and wait for it to finish.
    pub fn stop(&mut self) {
        // Signal the thread to stop via atomic flag
        self.stop_signal.store(true, Ordering::SeqCst);

        // Also send stop command via channel (in case thread is blocked)
        if let Some(tx) = self.command_tx.take() {
            let _ = tx.send(CaptureCommand::Stop);
        }

        // Wait for thread to finish
        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }
    }

    /// Get the latest captured frame.
    ///
    /// Returns `None` if no frame has been captured yet or if capturing
    /// is not running.
    pub fn get_frame(&self) -> Option<Frame> {
        let buffer = self.frame_buffer.lock().ok()?;
        buffer.clone()
    }

    /// Check if the capture thread is currently running.
    pub fn is_running(&self) -> bool {
        self.capture_thread.as_ref().is_some_and(|h| !h.is_finished())
    }
}

impl Drop for CameraCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Convert a nokhwa buffer to our RGB Frame format.
///
/// Handles various camera formats (MJPEG, YUYV, NV12, etc.) by using
/// nokhwa's built-in decode_image which automatically converts from
/// the camera's native format to RGB.
///
/// Returns `None` if the conversion fails (unsupported format or corrupt data).
fn convert_to_rgb(buffer: &nokhwa::Buffer) -> Option<Frame> {
    // Decode to RGB format - handles MJPEG, YUYV, NV12, and other formats
    let decoded = buffer.decode_image::<RgbFormat>().ok()?;
    let resolution = buffer.resolution();

    Some(Frame {
        data: decoded.into_raw(),
        width: resolution.width(),
        height: resolution.height(),
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    })
}

/// Mirror a frame horizontally (flip left-right) for selfie mode.
fn mirror_horizontal(frame: &mut Frame) {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let bpp = frame.bytes_per_pixel();

    for y in 0..height {
        let row_start = y * width * bpp;
        let row = &mut frame.data[row_start..row_start + width * bpp];

        // Swap pixels from left and right
        for x in 0..width / 2 {
            let left = x * bpp;
            let right = (width - 1 - x) * bpp;
            for i in 0..bpp {
                row.swap(left + i, right + i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices_does_not_error() {
        // Should not error even if no cameras are present
        // (returns empty list instead)
        let result = list_devices();
        assert!(result.is_ok());
    }

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
        assert_eq!(
            format!("{}", CameraError::NoDevices),
            "No cameras found"
        );
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
    fn test_camera_open_invalid_device() {
        // Use a device index that is very unlikely to exist
        let settings = CameraSettings {
            device_index: 999,
            resolution: Resolution::default(),
            fps: 30,
            mirror: true,
        };
        let result = CameraCapture::open(settings);
        assert!(result.is_err());
        match result.unwrap_err() {
            CameraError::DeviceNotFound(idx) => assert_eq!(idx, 999),
            other => panic!("Expected DeviceNotFound, got {:?}", other),
        }
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

    #[test]
    fn test_mirror_horizontal_2x1() {
        // Simple 2x1 image: pixel A (R=1,G=2,B=3) and pixel B (R=4,G=5,B=6)
        let mut frame = Frame {
            data: vec![1, 2, 3, 4, 5, 6],
            width: 2,
            height: 1,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        mirror_horizontal(&mut frame);
        // After mirroring: pixel B, pixel A
        assert_eq!(frame.data, vec![4, 5, 6, 1, 2, 3]);
    }

    #[test]
    fn test_mirror_horizontal_3x2() {
        // 3x2 image:
        // Row 0: [A, B, C]
        // Row 1: [D, E, F]
        let mut frame = Frame {
            data: vec![
                1, 1, 1, 2, 2, 2, 3, 3, 3, // Row 0: A, B, C
                4, 4, 4, 5, 5, 5, 6, 6, 6, // Row 1: D, E, F
            ],
            width: 3,
            height: 2,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        mirror_horizontal(&mut frame);
        // After mirroring:
        // Row 0: [C, B, A]
        // Row 1: [F, E, D]
        assert_eq!(
            frame.data,
            vec![
                3, 3, 3, 2, 2, 2, 1, 1, 1, // Row 0: C, B, A
                6, 6, 6, 5, 5, 5, 4, 4, 4, // Row 1: F, E, D
            ]
        );
    }

    #[test]
    fn test_mirror_horizontal_single_pixel() {
        // Edge case: 1x1 image should remain unchanged
        let mut frame = Frame {
            data: vec![1, 2, 3],
            width: 1,
            height: 1,
            format: FrameFormat::Rgb,
            timestamp: Instant::now(),
        };
        mirror_horizontal(&mut frame);
        assert_eq!(frame.data, vec![1, 2, 3]);
    }
}
