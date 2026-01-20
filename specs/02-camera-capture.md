# 02 - Camera Capture

Captures frames from the webcam for ASCII rendering. Runs in a background thread to avoid blocking the main I/O loop.

## Overview

```
┌─────────────┐      ┌─────────────┐      ┌─────────────┐
│   Webcam    │ ───▶ │   Capture   │ ───▶ │   Frame     │
│   Hardware  │      │   Thread    │      │   Buffer    │
└─────────────┘      └─────────────┘      └─────────────┘
                                                │
                                                ▼
                                         ┌─────────────┐
                                         │   ASCII     │
                                         │   Renderer  │
                                         └─────────────┘
```

## Crate Dependencies

```toml
[dependencies]
nokhwa = { version = "0.10", features = ["input-avfoundation"] }  # macOS
# or for cross-platform:
# nokhwa = { version = "0.10", features = ["input-native"] }
```

Alternative crates:
- `eye` - simpler API, less features
- `v4l` - Linux only, lower level
- Raw AVFoundation via `objc` - maximum control, more code

## Data Structures

```rust
pub struct CameraCapture {
    /// Camera handle
    camera: Camera,
    /// Latest captured frame
    frame_buffer: Arc<Mutex<Option<Frame>>>,
    /// Capture thread handle
    capture_thread: Option<JoinHandle<()>>,
    /// Signal to stop capture
    stop_signal: Arc<AtomicBool>,
    /// Current settings
    settings: CameraSettings,
}

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

pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

pub struct Frame {
    /// Raw pixel data (RGB or RGBA)
    pub data: Vec<u8>,
    /// Frame dimensions
    pub width: u32,
    pub height: u32,
    /// Pixel format
    pub format: FrameFormat,
    /// Timestamp
    pub timestamp: Instant,
}

pub enum FrameFormat {
    Rgb,
    Rgba,
    Yuyv,
    Mjpeg,
}
```

## API

```rust
impl CameraCapture {
    /// List available cameras
    pub fn list_devices() -> Result<Vec<CameraInfo>>;

    /// Open camera with settings
    pub fn open(settings: CameraSettings) -> Result<Self>;

    /// Start capturing frames in background
    pub fn start(&mut self) -> Result<()>;

    /// Stop capturing
    pub fn stop(&mut self);

    /// Get the latest frame (non-blocking)
    pub fn get_frame(&self) -> Option<Frame>;

    /// Check if camera is running
    pub fn is_running(&self) -> bool;
}
```

## Device Enumeration

```rust
pub fn list_devices() -> Result<Vec<CameraInfo>> {
    let devices = nokhwa::query(ApiBackend::Auto)?;
    Ok(devices.into_iter().map(|d| CameraInfo {
        index: d.index().as_index().unwrap_or(0),
        name: d.human_name(),
        description: d.description().to_string(),
    }).collect())
}
```

## Camera Initialization

```rust
pub fn open(settings: CameraSettings) -> Result<Self> {
    let index = CameraIndex::Index(settings.device_index);
    let requested = RequestedFormat::new::<RgbFormat>(
        RequestedFormatType::Closest(CameraFormat::new(
            nokhwa::utils::Resolution::new(
                settings.resolution.width,
                settings.resolution.height,
            ),
            FrameFormat::MJPEG,
            settings.fps,
        ))
    );

    let camera = Camera::new(index, requested)?;

    Ok(Self {
        camera,
        frame_buffer: Arc::new(Mutex::new(None)),
        capture_thread: None,
        stop_signal: Arc::new(AtomicBool::new(false)),
        settings,
    })
}
```

## Capture Thread

Runs in background, continuously captures frames:

```rust
pub fn start(&mut self) -> Result<()> {
    self.camera.open_stream()?;

    let camera = self.camera.clone(); // if Camera is Clone, else restructure
    let buffer = Arc::clone(&self.frame_buffer);
    let stop = Arc::clone(&self.stop_signal);
    let mirror = self.settings.mirror;

    self.capture_thread = Some(std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if let Ok(frame) = camera.frame() {
                let mut frame = convert_to_rgb(frame);
                if mirror {
                    mirror_horizontal(&mut frame);
                }
                *buffer.lock().unwrap() = Some(frame);
            }
            // Small sleep to not spin too fast
            std::thread::sleep(Duration::from_millis(16)); // ~60fps max
        }
    }));

    Ok(())
}
```

## Frame Conversion

Convert from camera format to RGB:

```rust
fn convert_to_rgb(frame: nokhwa::Buffer) -> Frame {
    let decoded = frame.decode_image::<RgbFormat>().unwrap();
    Frame {
        data: decoded.into_raw(),
        width: frame.resolution().width(),
        height: frame.resolution().height(),
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    }
}
```

## Mirroring

Flip horizontally for natural selfie view:

```rust
fn mirror_horizontal(frame: &mut Frame) {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let bpp = 3; // RGB = 3 bytes per pixel

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
```

## Resolution Selection

Recommended capture resolutions (lower = faster ASCII rendering):

| Name | Resolution | Use Case |
|------|------------|----------|
| Low | 320x240 | Fast, small ASCII modal |
| Medium | 640x480 | Balanced (recommended) |
| High | 1280x720 | Large ASCII modal |

The camera will give us the closest supported resolution. Don't capture at 1080p if we're rendering to 40x20 ASCII characters.

## Frame Rate Considerations

- Target: 15 FPS for ASCII rendering (more is wasteful)
- Don't render faster than terminal refresh (~60hz max)
- Use frame timestamp to skip stale frames

```rust
fn should_render(frame: &Frame, last_render: Instant) -> bool {
    frame.timestamp > last_render &&
    frame.timestamp.elapsed() < Duration::from_millis(100)
}
```

## macOS Permissions

Camera access requires permission. Check with:

```rust
#[cfg(target_os = "macos")]
fn check_camera_permission() -> bool {
    // nokhwa handles this internally, but we might want to pre-check
    // AVCaptureDevice.authorizationStatus(for: .video)
    true
}
```

The app should prompt for permission on first run. If denied, show clear error message.

## Error Handling

```rust
pub enum CameraError {
    /// No cameras found
    NoDevices,
    /// Failed to open camera
    OpenFailed(String),
    /// Camera permission denied
    PermissionDenied,
    /// Failed to start stream
    StreamFailed(String),
    /// Frame capture failed
    CaptureFailed(String),
}
```

## Graceful Degradation

If camera fails, the app should continue without it:

```rust
match CameraCapture::open(settings) {
    Ok(camera) => Some(camera),
    Err(e) => {
        eprintln!("Camera unavailable: {e}. Continuing without camera.");
        None
    }
}
```

## Testing

### Unit Tests

```rust
#[test]
fn test_list_devices() {
    // May return empty list if no camera, but shouldn't error
    let devices = CameraCapture::list_devices();
    assert!(devices.is_ok());
}

#[test]
fn test_mirror() {
    let mut frame = Frame {
        data: vec![1,2,3, 4,5,6], // 2 pixels
        width: 2,
        height: 1,
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    };
    mirror_horizontal(&mut frame);
    assert_eq!(frame.data, vec![4,5,6, 1,2,3]);
}
```

### Manual Testing

```bash
# List cameras
space-recorder --list-cameras

# Test camera capture (show in ASCII immediately)
space-recorder --camera-test
```

## Implementation Checklist

- [ ] Device enumeration with nokhwa
- [ ] Camera opening with resolution selection
- [ ] Background capture thread
- [ ] Frame buffer with mutex
- [ ] RGB conversion
- [ ] Horizontal mirroring
- [ ] Graceful error handling
- [ ] Permission handling on macOS
- [ ] Clean shutdown
