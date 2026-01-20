//! Background capture thread implementation.

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{
    CameraFormat, CameraIndex, FrameFormat as NokhwaFrameFormat, RequestedFormat,
    RequestedFormatType,
};
use nokhwa::Camera;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::frame_utils::{convert_to_rgb, mirror_horizontal};
use super::types::{CameraError, CameraSettings, Frame, Resolution};

/// Commands sent to the capture thread.
pub enum CaptureCommand {
    Stop,
}

/// Run the capture loop in a background thread.
pub fn run_capture_loop(
    settings: CameraSettings,
    buffer: Arc<Mutex<Option<Frame>>>,
    stop: Arc<AtomicBool>,
    rx: Receiver<CaptureCommand>,
    info_tx: Sender<Result<(Resolution, u32), CameraError>>,
) {
    let index = CameraIndex::Index(settings.device_index);

    // Try multiple format strategies in order of preference
    let camera = match open_camera_with_fallback(&index, &settings) {
        Ok(cam) => cam,
        Err(e) => {
            let _ = info_tx.send(Err(e));
            return;
        }
    };

    let mut camera = camera;

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

        // Small sleep to allow checking stop signal
        thread::sleep(Duration::from_millis(1));
    }

    // Clean up
    let _ = camera.stop_stream();
}

/// Try to open a camera with multiple format fallback strategies.
fn open_camera_with_fallback(
    index: &CameraIndex,
    settings: &CameraSettings,
) -> Result<Camera, CameraError> {
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

    let mut last_error = None;

    for requested in format_attempts {
        match Camera::new(index.clone(), requested) {
            Ok(cam) => return Ok(cam),
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }

    let e = last_error.unwrap();
    let msg = e.to_string().to_lowercase();
    if msg.contains("permission")
        || msg.contains("denied")
        || msg.contains("authorization")
        || msg.contains("access")
    {
        Err(CameraError::PermissionDenied)
    } else {
        Err(CameraError::OpenFailed(e.to_string()))
    }
}
