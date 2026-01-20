//! Camera capture handle and public API.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::capture_loop::{run_capture_loop, CaptureCommand};
use super::device::list_devices;
use super::types::{CameraError, CameraSettings, Frame, Resolution};

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
        let handle = std::thread::spawn(move || {
            run_capture_loop(settings, buffer, stop, rx, info_tx);
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
                Err(CameraError::StreamFailed(
                    "Capture thread terminated unexpectedly".to_string(),
                ))
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
        self.capture_thread
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }
}

impl Drop for CameraCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
