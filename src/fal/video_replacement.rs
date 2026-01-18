//! Video replacement functionality for dynamic AI video swapping.
//!
//! This module handles the mechanics of replacing AI videos during streaming
//! with minimal pipeline disruption.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Error types for video replacement operations.
#[derive(Debug)]
pub enum VideoReplacementError {
    /// The video file does not exist.
    FileNotFound(PathBuf),
    /// Failed to probe video format.
    ProbeError(String),
    /// Video format is not supported.
    UnsupportedFormat(String),
}

impl std::fmt::Display for VideoReplacementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoReplacementError::FileNotFound(path) => {
                write!(f, "Video file not found: {}", path.display())
            }
            VideoReplacementError::ProbeError(msg) => {
                write!(f, "Failed to probe video format: {}", msg)
            }
            VideoReplacementError::UnsupportedFormat(msg) => {
                write!(f, "Unsupported video format: {}", msg)
            }
        }
    }
}

impl std::error::Error for VideoReplacementError {}

/// Information about a video file's format.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoFormat {
    /// Video width in pixels.
    pub width: u32,
    /// Video height in pixels.
    pub height: u32,
    /// Video codec name (e.g., "h264", "vp9").
    pub codec: String,
    /// Frame rate as floating point.
    pub fps: f64,
    /// Duration in seconds (if known).
    pub duration_secs: Option<f64>,
}

impl Default for VideoFormat {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            codec: "h264".to_string(),
            fps: 30.0,
            duration_secs: None,
        }
    }
}

/// Tracks video replacement operations and timing.
#[derive(Debug)]
pub struct VideoReplacement {
    /// The new video path to use.
    new_video_path: PathBuf,
    /// Format of the new video (if probed).
    video_format: Option<VideoFormat>,
    /// Timestamp when replacement was requested.
    requested_at: Instant,
    /// Target output resolution for normalization.
    target_resolution: (u32, u32),
}

impl VideoReplacement {
    /// Create a new video replacement request.
    ///
    /// # Arguments
    /// * `video_path` - Path to the new video file
    /// * `target_resolution` - Output resolution to scale the video to
    pub fn new(video_path: PathBuf, target_resolution: (u32, u32)) -> Self {
        Self {
            new_video_path: video_path,
            video_format: None,
            requested_at: Instant::now(),
            target_resolution,
        }
    }

    /// Validate that the video file exists and is readable.
    pub fn validate(&self) -> Result<(), VideoReplacementError> {
        if !self.new_video_path.exists() {
            return Err(VideoReplacementError::FileNotFound(self.new_video_path.clone()));
        }
        Ok(())
    }

    /// Get the path to the new video.
    pub fn path(&self) -> &Path {
        &self.new_video_path
    }

    /// Get the elapsed time since the replacement was requested.
    pub fn elapsed(&self) -> Duration {
        self.requested_at.elapsed()
    }

    /// Get the target resolution for the video.
    pub fn target_resolution(&self) -> (u32, u32) {
        self.target_resolution
    }

    /// Get the video format if it has been probed.
    pub fn format(&self) -> Option<&VideoFormat> {
        self.video_format.as_ref()
    }

    /// Set the video format after probing.
    pub fn set_format(&mut self, format: VideoFormat) {
        self.video_format = Some(format);
    }

    /// Build an FFmpeg filter to normalize this video to the target resolution.
    ///
    /// This handles format differences by:
    /// - Scaling to target resolution (handles different source sizes)
    /// - Converting to rgba format (handles codec differences)
    /// - Applying the specified opacity
    ///
    /// # Arguments
    /// * `opacity` - Alpha value for the video overlay (0.0-1.0)
    /// * `input_label` - The input stream label (e.g., "[2:v]")
    /// * `output_label` - The output stream label (e.g., "[ai]")
    pub fn build_normalization_filter(
        &self,
        opacity: f32,
        input_label: &str,
        output_label: &str,
    ) -> String {
        let (width, height) = self.target_resolution;

        // The filter chain handles video format differences:
        // 1. scale - handles resolution differences, force to target size
        // 2. format=rgba - handles codec differences by converting to common format
        // 3. colorchannelmixer=aa - applies opacity via alpha channel
        format!(
            "{}scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,format=rgba,colorchannelmixer=aa={:.2}{}",
            input_label,
            width,
            height,
            width,
            height,
            opacity,
            output_label
        )
    }

    /// Check if the video requires format conversion.
    ///
    /// Returns true if the video's native format differs significantly from
    /// the target resolution or uses an uncommon codec.
    pub fn needs_conversion(&self) -> bool {
        match &self.video_format {
            Some(format) => {
                let (target_w, target_h) = self.target_resolution;
                // Check if resolution differs
                let resolution_differs = format.width != target_w || format.height != target_h;
                // Check for non-standard codecs
                let codec_differs = !["h264", "hevc", "vp9", "av1"]
                    .contains(&format.codec.to_lowercase().as_str());
                resolution_differs || codec_differs
            }
            None => true, // Assume conversion needed if format unknown
        }
    }
}

/// Manager for tracking and executing video replacement operations.
#[derive(Debug, Default)]
pub struct VideoReplacementManager {
    /// Pending video replacement (if any).
    pending: Option<VideoReplacement>,
    /// Last replacement timing for metrics.
    last_replacement_duration: Option<Duration>,
    /// Total number of replacements performed.
    replacement_count: u32,
}

impl VideoReplacementManager {
    /// Create a new video replacement manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a new video for replacement.
    ///
    /// # Arguments
    /// * `video_path` - Path to the new video file
    /// * `target_resolution` - Output resolution to scale to
    ///
    /// # Returns
    /// Result indicating if the video is valid and queued
    pub fn queue_replacement(
        &mut self,
        video_path: PathBuf,
        target_resolution: (u32, u32),
    ) -> Result<(), VideoReplacementError> {
        let replacement = VideoReplacement::new(video_path, target_resolution);
        replacement.validate()?;
        self.pending = Some(replacement);
        log::info!("Queued video replacement: {:?}", self.pending.as_ref().unwrap().path());
        Ok(())
    }

    /// Check if there is a pending video replacement.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Take the pending video replacement (clears the pending state).
    pub fn take_pending(&mut self) -> Option<VideoReplacement> {
        self.pending.take()
    }

    /// Record that a replacement was completed.
    ///
    /// # Arguments
    /// * `duration` - How long the replacement took
    pub fn record_replacement(&mut self, duration: Duration) {
        self.last_replacement_duration = Some(duration);
        self.replacement_count += 1;
        log::info!(
            "Video replacement #{} completed in {:?}",
            self.replacement_count,
            duration
        );
    }

    /// Get the duration of the last replacement.
    pub fn last_replacement_duration(&self) -> Option<Duration> {
        self.last_replacement_duration
    }

    /// Get the total number of replacements performed.
    pub fn replacement_count(&self) -> u32 {
        self.replacement_count
    }

    /// Check if the last replacement was fast enough (<500ms).
    pub fn last_replacement_was_fast(&self) -> bool {
        self.last_replacement_duration
            .map(|d| d < Duration::from_millis(500))
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_video_replacement_new() {
        let path = PathBuf::from("/tmp/test_video.mp4");
        let replacement = VideoReplacement::new(path.clone(), (1920, 1080));

        assert_eq!(replacement.path(), Path::new("/tmp/test_video.mp4"));
        assert_eq!(replacement.target_resolution(), (1920, 1080));
        assert!(replacement.format().is_none());
    }

    #[test]
    fn test_video_replacement_validate_file_not_found() {
        let path = PathBuf::from("/nonexistent/video.mp4");
        let replacement = VideoReplacement::new(path.clone(), (1280, 720));

        let result = replacement.validate();
        assert!(result.is_err());

        if let Err(VideoReplacementError::FileNotFound(p)) = result {
            assert_eq!(p, path);
        } else {
            panic!("Expected FileNotFound error");
        }
    }

    #[test]
    fn test_video_replacement_validate_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_video.mp4");
        File::create(&file_path).unwrap();

        let replacement = VideoReplacement::new(file_path, (1280, 720));
        assert!(replacement.validate().is_ok());
    }

    #[test]
    fn test_video_replacement_elapsed_time() {
        let path = PathBuf::from("/tmp/test.mp4");
        let replacement = VideoReplacement::new(path, (1280, 720));

        std::thread::sleep(Duration::from_millis(10));

        assert!(replacement.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn test_video_replacement_build_normalization_filter() {
        let path = PathBuf::from("/tmp/test.mp4");
        let replacement = VideoReplacement::new(path, (1280, 720));

        let filter = replacement.build_normalization_filter(0.5, "[2:v]", "[ai]");

        // Verify filter contains expected components
        assert!(filter.contains("[2:v]"), "Should have input label");
        assert!(filter.contains("scale=1280:720"), "Should scale to target resolution");
        assert!(filter.contains("format=rgba"), "Should convert to rgba");
        assert!(filter.contains("colorchannelmixer=aa=0.50"), "Should apply opacity");
        assert!(filter.contains("[ai]"), "Should have output label");
    }

    #[test]
    fn test_video_replacement_build_normalization_filter_different_opacity() {
        let path = PathBuf::from("/tmp/test.mp4");
        let replacement = VideoReplacement::new(path, (1920, 1080));

        let filter = replacement.build_normalization_filter(0.25, "[1:v]", "[out]");

        assert!(filter.contains("scale=1920:1080"));
        assert!(filter.contains("colorchannelmixer=aa=0.25"));
    }

    #[test]
    fn test_video_replacement_needs_conversion_unknown_format() {
        let path = PathBuf::from("/tmp/test.mp4");
        let replacement = VideoReplacement::new(path, (1280, 720));

        // Without format info, should assume conversion needed
        assert!(replacement.needs_conversion());
    }

    #[test]
    fn test_video_replacement_needs_conversion_matching_format() {
        let path = PathBuf::from("/tmp/test.mp4");
        let mut replacement = VideoReplacement::new(path, (1280, 720));

        replacement.set_format(VideoFormat {
            width: 1280,
            height: 720,
            codec: "h264".to_string(),
            fps: 30.0,
            duration_secs: Some(5.0),
        });

        // Matching format should not need conversion
        assert!(!replacement.needs_conversion());
    }

    #[test]
    fn test_video_replacement_needs_conversion_different_resolution() {
        let path = PathBuf::from("/tmp/test.mp4");
        let mut replacement = VideoReplacement::new(path, (1280, 720));

        replacement.set_format(VideoFormat {
            width: 1920,
            height: 1080,
            codec: "h264".to_string(),
            fps: 30.0,
            duration_secs: Some(5.0),
        });

        // Different resolution needs conversion
        assert!(replacement.needs_conversion());
    }

    #[test]
    fn test_video_replacement_needs_conversion_unusual_codec() {
        let path = PathBuf::from("/tmp/test.mp4");
        let mut replacement = VideoReplacement::new(path, (1280, 720));

        replacement.set_format(VideoFormat {
            width: 1280,
            height: 720,
            codec: "prores".to_string(), // Unusual codec
            fps: 30.0,
            duration_secs: Some(5.0),
        });

        // Unusual codec needs conversion
        assert!(replacement.needs_conversion());
    }

    #[test]
    fn test_video_format_default() {
        let format = VideoFormat::default();

        assert_eq!(format.width, 1280);
        assert_eq!(format.height, 720);
        assert_eq!(format.codec, "h264");
        assert!((format.fps - 30.0).abs() < f64::EPSILON);
        assert!(format.duration_secs.is_none());
    }

    #[test]
    fn test_video_replacement_manager_new() {
        let manager = VideoReplacementManager::new();

        assert!(!manager.has_pending());
        assert!(manager.last_replacement_duration().is_none());
        assert_eq!(manager.replacement_count(), 0);
    }

    #[test]
    fn test_video_replacement_manager_queue_replacement_file_not_found() {
        let mut manager = VideoReplacementManager::new();

        let result = manager.queue_replacement(
            PathBuf::from("/nonexistent/video.mp4"),
            (1280, 720),
        );

        assert!(result.is_err());
        assert!(!manager.has_pending());
    }

    #[test]
    fn test_video_replacement_manager_queue_replacement_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_video.mp4");
        File::create(&file_path).unwrap();

        let mut manager = VideoReplacementManager::new();

        let result = manager.queue_replacement(file_path.clone(), (1280, 720));

        assert!(result.is_ok());
        assert!(manager.has_pending());
    }

    #[test]
    fn test_video_replacement_manager_take_pending() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_video.mp4");
        File::create(&file_path).unwrap();

        let mut manager = VideoReplacementManager::new();
        manager.queue_replacement(file_path.clone(), (1280, 720)).unwrap();

        assert!(manager.has_pending());

        let replacement = manager.take_pending();
        assert!(replacement.is_some());
        assert_eq!(replacement.unwrap().path(), file_path.as_path());

        // After taking, should no longer be pending
        assert!(!manager.has_pending());
    }

    #[test]
    fn test_video_replacement_manager_record_replacement() {
        let mut manager = VideoReplacementManager::new();

        manager.record_replacement(Duration::from_millis(250));

        assert_eq!(manager.replacement_count(), 1);
        assert_eq!(manager.last_replacement_duration(), Some(Duration::from_millis(250)));
        assert!(manager.last_replacement_was_fast());
    }

    #[test]
    fn test_video_replacement_manager_record_multiple_replacements() {
        let mut manager = VideoReplacementManager::new();

        manager.record_replacement(Duration::from_millis(100));
        manager.record_replacement(Duration::from_millis(200));
        manager.record_replacement(Duration::from_millis(150));

        assert_eq!(manager.replacement_count(), 3);
        // Last duration should be the most recent
        assert_eq!(manager.last_replacement_duration(), Some(Duration::from_millis(150)));
    }

    #[test]
    fn test_video_replacement_manager_last_replacement_was_fast_slow() {
        let mut manager = VideoReplacementManager::new();

        // Record a slow replacement (>500ms)
        manager.record_replacement(Duration::from_millis(600));

        assert!(!manager.last_replacement_was_fast());
    }

    #[test]
    fn test_video_replacement_manager_last_replacement_was_fast_boundary() {
        let mut manager = VideoReplacementManager::new();

        // Record exactly at the boundary
        manager.record_replacement(Duration::from_millis(500));

        // 500ms is NOT less than 500ms, so should be false
        assert!(!manager.last_replacement_was_fast());

        // 499ms should be fast
        manager.record_replacement(Duration::from_millis(499));
        assert!(manager.last_replacement_was_fast());
    }

    #[test]
    fn test_video_replacement_manager_last_replacement_was_fast_no_replacements() {
        let manager = VideoReplacementManager::new();

        // With no replacements, should return true (optimistic default)
        assert!(manager.last_replacement_was_fast());
    }

    #[test]
    fn test_video_replacement_error_display() {
        let err1 = VideoReplacementError::FileNotFound(PathBuf::from("/test/video.mp4"));
        assert!(err1.to_string().contains("/test/video.mp4"));

        let err2 = VideoReplacementError::ProbeError("ffprobe failed".to_string());
        assert!(err2.to_string().contains("ffprobe failed"));

        let err3 = VideoReplacementError::UnsupportedFormat("mkv".to_string());
        assert!(err3.to_string().contains("mkv"));
    }
}
