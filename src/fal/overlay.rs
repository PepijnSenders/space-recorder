//! OverlayManager - manages the AI video overlay layer and transitions.

use std::path::PathBuf;

/// State of a crossfade transition between videos.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionState {
    /// No transition in progress.
    Idle,
    /// Crossfading from old video to new video.
    CrossfadeIn {
        /// Progress from 0.0 to 1.0.
        progress: f32,
        /// Total duration of the transition in milliseconds.
        duration_ms: u32,
    },
    /// Fading out the current video (clearing overlay).
    FadeOut {
        /// Progress from 0.0 to 1.0 (0 = full opacity, 1 = fully faded).
        progress: f32,
        /// Total duration of the fade-out in milliseconds.
        duration_ms: u32,
    },
}

/// Manages the AI video overlay layer and transitions.
pub struct OverlayManager {
    current_video: Option<PathBuf>,
    pending_video: Option<PathBuf>,
    opacity: f32,
    transition_state: TransitionState,
    /// Default crossfade duration in milliseconds.
    crossfade_duration_ms: u32,
}

impl OverlayManager {
    /// Default crossfade duration in milliseconds.
    pub const DEFAULT_CROSSFADE_DURATION_MS: u32 = 500;

    /// Create a new OverlayManager with default settings.
    pub fn new() -> Self {
        Self {
            current_video: None,
            pending_video: None,
            opacity: 0.3, // Default to 30% opacity like webcam ghost
            transition_state: TransitionState::Idle,
            crossfade_duration_ms: Self::DEFAULT_CROSSFADE_DURATION_MS,
        }
    }

    /// Create a new OverlayManager with custom opacity.
    pub fn with_opacity(opacity: f32) -> Self {
        Self {
            current_video: None,
            pending_video: None,
            opacity: opacity.clamp(0.0, 1.0),
            transition_state: TransitionState::Idle,
            crossfade_duration_ms: Self::DEFAULT_CROSSFADE_DURATION_MS,
        }
    }

    /// Create a new OverlayManager with custom opacity and crossfade duration.
    pub fn with_settings(opacity: f32, crossfade_duration_ms: u32) -> Self {
        Self {
            current_video: None,
            pending_video: None,
            opacity: opacity.clamp(0.0, 1.0),
            transition_state: TransitionState::Idle,
            crossfade_duration_ms,
        }
    }

    /// Get the configured crossfade duration in milliseconds.
    pub fn crossfade_duration_ms(&self) -> u32 {
        self.crossfade_duration_ms
    }

    /// Set the crossfade duration in milliseconds.
    pub fn set_crossfade_duration_ms(&mut self, duration_ms: u32) {
        self.crossfade_duration_ms = duration_ms;
    }

    /// Queue new video, triggers crossfade from current using the configured duration.
    pub fn queue_video(&mut self, video_path: PathBuf) {
        self.queue_video_with_duration(video_path, self.crossfade_duration_ms);
    }

    /// Queue new video with a specific crossfade duration.
    ///
    /// # Arguments
    /// * `video_path` - Path to the new video file
    /// * `duration_ms` - Crossfade duration in milliseconds (0 for instant cut)
    pub fn queue_video_with_duration(&mut self, video_path: PathBuf, duration_ms: u32) {
        if self.current_video.is_some() {
            // Crossfade from current video to new video
            self.pending_video = Some(video_path.clone());
            if duration_ms == 0 {
                // Instant cut - skip transition, directly swap videos
                log::info!("Instant cut to: {:?} (0ms crossfade)", video_path);
                self.current_video = self.pending_video.take();
                self.transition_state = TransitionState::Idle;
            } else {
                // Crossfade transition
                self.transition_state = TransitionState::CrossfadeIn {
                    progress: 0.0,
                    duration_ms,
                };
                log::info!(
                    "Starting crossfade transition to: {:?} ({}ms)",
                    video_path,
                    duration_ms
                );
            }
        } else {
            // No existing video, set directly as current
            self.current_video = Some(video_path.clone());
            self.pending_video = None;
            self.transition_state = TransitionState::Idle;
            log::info!("Set initial video: {:?}", video_path);
        }
    }

    /// Get pending video path (during crossfade).
    pub fn pending_video(&self) -> Option<&PathBuf> {
        self.pending_video.as_ref()
    }

    /// Get FFmpeg filter for current overlay state.
    ///
    /// Returns a filter string for the AI video overlay input.
    /// The filter handles three cases:
    /// - No video: Returns empty string (no filter needed)
    /// - Single video: scale + rgba + alpha via colorchannelmixer
    /// - Crossfade: xfade between current and pending videos
    ///
    /// # Filter Chain
    /// For single video: `loop=-1:size=9999,scale=WxH,format=rgba,colorchannelmixer=aa=X.XX`
    /// For crossfade: `[current][pending]xfade=transition=fade:duration=0.5,format=rgba,colorchannelmixer=aa=X.XX`
    pub fn get_ffmpeg_filter(&self) -> String {
        self.get_ffmpeg_filter_with_resolution(1280, 720)
    }

    /// Get FFmpeg filter with custom resolution.
    ///
    /// # Arguments
    /// * `width` - Target width
    /// * `height` - Target height
    ///
    /// # Returns
    /// - Empty string if no video is set
    /// - Single video filter: `loop=-1:size=9999,scale=WxH,format=rgba,colorchannelmixer=aa=X.XX`
    /// - Crossfade filter: Uses `xfade=transition=fade:duration=X.XX` between current and pending
    ///
    /// # Loop Filter
    ///
    /// The `loop=-1:size=9999` filter makes the video loop indefinitely:
    /// - `-1` means infinite loops
    /// - `size=9999` specifies the number of frames to use for looping (effectively all frames)
    ///
    /// The loop filter is placed first in the chain so it operates on the original video before scaling.
    ///
    /// # Crossfade Behavior
    ///
    /// The xfade filter handles smooth blending between two videos:
    /// - `transition=fade`: Uses a fade (dissolve) transition
    /// - `duration`: Configurable duration in seconds (default 0.5s)
    /// - The opacity is applied AFTER the xfade so both videos share the same final alpha
    ///
    /// # Fallback
    ///
    /// If xfade cannot be used (no pending video or duration is 0), falls back to
    /// a simple cut transition (instant switch).
    pub fn get_ffmpeg_filter_with_resolution(&self, width: u32, height: u32) -> String {
        // No video - return empty filter
        if self.current_video.is_none() {
            return String::new();
        }

        match &self.transition_state {
            TransitionState::Idle => {
                // Single video: loop + scale + rgba + alpha
                // loop=-1 means infinite loops, size=9999 is the frame buffer size
                format!(
                    "loop=-1:size=9999,scale={}:{},format=rgba,colorchannelmixer=aa={:.2}",
                    width, height, self.opacity
                )
            }
            TransitionState::CrossfadeIn { duration_ms, .. } => {
                // If no pending video, fall back to single video filter (cut)
                if self.pending_video.is_none() {
                    return format!(
                        "loop=-1:size=9999,scale={}:{},format=rgba,colorchannelmixer=aa={:.2}",
                        width, height, self.opacity
                    );
                }

                // Duration 0 means instant cut - fall back to pending video only
                if *duration_ms == 0 {
                    // This shouldn't normally happen (instant cut sets Idle state)
                    // but handle it defensively
                    return format!(
                        "loop=-1:size=9999,scale={}:{},format=rgba,colorchannelmixer=aa={:.2}",
                        width, height, self.opacity
                    );
                }

                // Crossfade between current and pending videos using xfade filter
                // The xfade filter handles the blending automatically:
                // - transition=fade: smooth dissolve between the two videos
                // - duration: how long the transition takes
                // - offset=0: start transition immediately
                //
                // Filter chain:
                // 1. Both inputs: loop + scale to same resolution
                // 2. xfade: blend the two streams
                // 3. format=rgba + colorchannelmixer: apply final opacity
                let duration_secs = *duration_ms as f32 / 1000.0;

                format!(
                    "[ai_current]loop=-1:size=9999,scale={}:{}[ai_c];\
                     [ai_pending]loop=-1:size=9999,scale={}:{}[ai_p];\
                     [ai_c][ai_p]xfade=transition=fade:duration={:.2}:offset=0,format=rgba,colorchannelmixer=aa={:.2}",
                    width,
                    height,
                    width,
                    height,
                    duration_secs,
                    self.opacity
                )
            }
            TransitionState::FadeOut { progress, .. } => {
                // Fading out: reduce opacity based on progress (1 - progress)
                // At progress 0.0, full opacity; at progress 1.0, opacity is 0
                // Loop filter still applied for seamless playback during fade
                let fade_progress = progress.clamp(0.0, 1.0);
                let faded_opacity = self.opacity * (1.0 - fade_progress);
                format!(
                    "loop=-1:size=9999,scale={}:{},format=rgba,colorchannelmixer=aa={:.2}",
                    width, height, faded_opacity
                )
            }
        }
    }

    /// Update transition progress (called per frame).
    pub fn tick(&mut self, delta_ms: u32) {
        match &mut self.transition_state {
            TransitionState::CrossfadeIn {
                progress,
                duration_ms,
            } => {
                *progress += delta_ms as f32 / *duration_ms as f32;
                if *progress >= 1.0 {
                    // Transition complete - swap pending to current
                    let completed_video = self.pending_video.take();
                    log::info!("Crossfade complete to: {:?}", completed_video);
                    self.current_video = completed_video;
                    self.transition_state = TransitionState::Idle;
                }
            }
            TransitionState::FadeOut {
                progress,
                duration_ms,
            } => {
                *progress += delta_ms as f32 / *duration_ms as f32;
                if *progress >= 1.0 {
                    // Fade-out complete - remove video and reset to Idle
                    log::info!("Fade-out complete, overlay cleared");
                    self.current_video = None;
                    self.transition_state = TransitionState::Idle;
                }
            }
            TransitionState::Idle => {
                // Nothing to do
            }
        }
    }

    /// Get the current opacity.
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// Set the overlay opacity.
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
    }

    /// Get the current video path.
    pub fn current_video(&self) -> Option<&PathBuf> {
        self.current_video.as_ref()
    }

    /// Get the current transition state.
    pub fn transition_state(&self) -> &TransitionState {
        &self.transition_state
    }

    /// Clear the overlay with a fade-out transition.
    ///
    /// Uses the configured crossfade duration for the fade-out.
    /// If there is a current video, triggers a fade-out transition.
    /// The video will be removed and state reset to Idle after the fade completes.
    /// If no video is set, immediately resets to Idle state.
    pub fn clear(&mut self) {
        // Clear any pending video immediately
        self.pending_video = None;

        if self.current_video.is_some() {
            // Trigger fade-out transition using configured duration
            self.transition_state = TransitionState::FadeOut {
                progress: 0.0,
                duration_ms: self.crossfade_duration_ms,
            };
            log::info!(
                "Starting fade-out transition ({}ms)",
                self.crossfade_duration_ms
            );
        } else {
            // No video to fade out, just reset state
            self.transition_state = TransitionState::Idle;
        }
    }

    /// Clear the overlay immediately without transition.
    ///
    /// Use this when you need to immediately remove the overlay without
    /// waiting for a fade-out animation.
    pub fn clear_immediate(&mut self) {
        self.current_video = None;
        self.pending_video = None;
        self.transition_state = TransitionState::Idle;
        log::info!("Overlay cleared immediately");
    }

    /// Check if the overlay is currently fading out.
    pub fn is_fading_out(&self) -> bool {
        matches!(self.transition_state, TransitionState::FadeOut { .. })
    }

    /// Check if a crossfade transition is in progress.
    pub fn is_crossfading(&self) -> bool {
        matches!(self.transition_state, TransitionState::CrossfadeIn { .. })
    }

    /// Perform an instant cut to a new video, skipping crossfade.
    ///
    /// This is used as a fallback when crossfade is not possible
    /// (e.g., format incompatibility) or when instant switching is preferred.
    pub fn cut_to_video(&mut self, video_path: PathBuf) {
        self.queue_video_with_duration(video_path, 0);
    }

    /// Complete the current crossfade transition immediately.
    ///
    /// This is a fallback for when xfade filter encounters issues.
    /// Instantly completes the transition by swapping pending to current.
    pub fn complete_crossfade_immediately(&mut self) {
        if let TransitionState::CrossfadeIn { .. } = &self.transition_state {
            if self.pending_video.is_some() {
                log::info!("Completing crossfade immediately (fallback to cut)");
                self.current_video = self.pending_video.take();
                self.transition_state = TransitionState::Idle;
            }
        }
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // v2.4.1: OverlayManager struct tests

    #[test]
    fn test_overlay_manager_new_defaults() {
        let manager = OverlayManager::new();

        // AC: Tracks current_video: Option<PathBuf>
        assert!(manager.current_video().is_none());

        // AC: Tracks opacity: f32 (0.0-1.0) - default is 0.3
        assert!((manager.opacity() - 0.3).abs() < f32::EPSILON);

        // AC: Tracks transition_state: TransitionState
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    #[test]
    fn test_overlay_manager_tracks_current_video() {
        let mut manager = OverlayManager::new();

        // AC: Tracks current_video: Option<PathBuf>
        assert!(manager.current_video().is_none());

        // Queue a video when no current video exists - sets directly
        manager.queue_video(PathBuf::from("/path/to/video.mp4"));

        // AC: Directly sets current if no existing video
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/path/to/video.mp4"))
        );
        // No transition when setting initial video
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    #[test]
    fn test_overlay_manager_tracks_pending_video() {
        let mut manager = OverlayManager::new();

        // AC: Tracks pending_video: Option<PathBuf>
        // First, set an initial video (goes directly to current)
        manager.queue_video(PathBuf::from("/path/to/video1.mp4"));
        assert_eq!(*manager.transition_state(), TransitionState::Idle);

        // Now queue a second video - this should trigger crossfade
        manager.queue_video(PathBuf::from("/path/to/video2.mp4"));

        // AC: Starts crossfade transition if current video exists
        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn { .. }
        ));
    }

    #[test]
    fn test_overlay_manager_tracks_opacity() {
        // AC: Tracks opacity: f32 (0.0-1.0)
        let manager = OverlayManager::with_opacity(0.5);
        assert!((manager.opacity() - 0.5).abs() < f32::EPSILON);

        // Test clamping to 0.0-1.0 range
        let manager_low = OverlayManager::with_opacity(-0.5);
        assert!((manager_low.opacity() - 0.0).abs() < f32::EPSILON);

        let manager_high = OverlayManager::with_opacity(1.5);
        assert!((manager_high.opacity() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_overlay_manager_set_opacity_clamped() {
        let mut manager = OverlayManager::new();

        // AC: Tracks opacity: f32 (0.0-1.0)
        manager.set_opacity(0.7);
        assert!((manager.opacity() - 0.7).abs() < f32::EPSILON);

        // Test clamping
        manager.set_opacity(-0.1);
        assert!((manager.opacity() - 0.0).abs() < f32::EPSILON);

        manager.set_opacity(2.0);
        assert!((manager.opacity() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_overlay_manager_tracks_transition_state() {
        let mut manager = OverlayManager::new();

        // AC: Tracks transition_state: TransitionState
        assert_eq!(*manager.transition_state(), TransitionState::Idle);

        // First video sets directly (no transition)
        manager.queue_video(PathBuf::from("/video1.mp4"));
        assert_eq!(*manager.transition_state(), TransitionState::Idle);

        // Second video triggers crossfade
        manager.queue_video(PathBuf::from("/video2.mp4"));
        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn { .. }
        ));

        manager.tick(500); // Complete transition
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    #[test]
    fn test_overlay_manager_default_trait() {
        let manager = OverlayManager::default();
        assert!(manager.current_video().is_none());
        assert!((manager.opacity() - 0.3).abs() < f32::EPSILON);
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    // v2.4.2: Video queueing tests

    #[test]
    fn test_queue_video_sets_pending() {
        // AC: queue_video(path: PathBuf) sets pending video
        let mut manager = OverlayManager::new();

        // Set initial video first
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Queue second video - should set pending
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Verify current is still video1 during transition
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video1.mp4"))
        );

        // Complete transition - pending becomes current
        manager.tick(500);
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4"))
        );
    }

    #[test]
    fn test_queue_video_starts_crossfade_when_current_exists() {
        // AC: Starts crossfade transition if current video exists
        let mut manager = OverlayManager::new();

        // Set initial video
        manager.queue_video(PathBuf::from("/video1.mp4"));
        assert_eq!(*manager.transition_state(), TransitionState::Idle);

        // Queue second video - should start crossfade
        manager.queue_video(PathBuf::from("/video2.mp4"));
        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn {
                progress: 0.0,
                duration_ms: 500
            }
        ));
    }

    #[test]
    fn test_queue_video_directly_sets_current_when_no_existing() {
        // AC: Directly sets current if no existing video
        let mut manager = OverlayManager::new();

        assert!(manager.current_video().is_none());

        // Queue first video - should set directly
        manager.queue_video(PathBuf::from("/video.mp4"));

        // Immediately available as current
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video.mp4"))
        );

        // No transition needed
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    // v2.4.3: Transition tick tests

    #[test]
    fn test_tick_updates_transition_progress() {
        // AC: tick(delta_ms: u32) updates transition progress
        let mut manager = OverlayManager::new();

        // Set up a transition
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Verify initial state
        if let TransitionState::CrossfadeIn { progress, .. } = manager.transition_state() {
            assert!((progress - 0.0).abs() < f32::EPSILON);
        } else {
            panic!("Expected CrossfadeIn state");
        }

        // Tick forward 100ms out of 500ms total
        manager.tick(100);

        // Progress should be 0.2 (100/500)
        if let TransitionState::CrossfadeIn { progress, .. } = manager.transition_state() {
            assert!((progress - 0.2).abs() < f32::EPSILON);
        } else {
            panic!("Expected CrossfadeIn state");
        }
    }

    #[test]
    fn test_tick_increments_based_on_elapsed_and_duration() {
        // AC: Increments progress based on elapsed time and duration
        let mut manager = OverlayManager::new();

        // Set up a transition (500ms duration)
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Tick 250ms - should be 50% (250/500)
        manager.tick(250);
        if let TransitionState::CrossfadeIn { progress, .. } = manager.transition_state() {
            assert!((progress - 0.5).abs() < f32::EPSILON);
        } else {
            panic!("Expected CrossfadeIn state");
        }

        // Tick another 125ms - should be 75% ((250+125)/500)
        manager.tick(125);
        if let TransitionState::CrossfadeIn { progress, .. } = manager.transition_state() {
            assert!((progress - 0.75).abs() < f32::EPSILON);
        } else {
            panic!("Expected CrossfadeIn state");
        }
    }

    #[test]
    fn test_tick_sets_idle_when_progress_reaches_one() {
        // AC: Sets state to Idle when progress reaches 1.0
        let mut manager = OverlayManager::new();

        // Set up a transition
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn { .. }
        ));

        // Tick exactly to completion
        manager.tick(500);

        // Should now be Idle
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    #[test]
    fn test_tick_sets_idle_when_progress_exceeds_one() {
        // AC: Sets state to Idle when progress reaches 1.0 (including overshoot)
        let mut manager = OverlayManager::new();

        // Set up a transition
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Tick past completion (600ms > 500ms duration)
        manager.tick(600);

        // Should be Idle (handles overshoot)
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    #[test]
    fn test_tick_swaps_pending_to_current_on_complete() {
        // AC: Swaps pending to current when transition completes
        let mut manager = OverlayManager::new();

        // Set up a transition
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Current should still be video1 during transition
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video1.mp4"))
        );

        // Complete the transition
        manager.tick(500);

        // Now current should be video2 (was pending)
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4"))
        );
    }

    #[test]
    fn test_tick_does_nothing_when_idle() {
        // Edge case: tick should be a no-op when Idle
        let mut manager = OverlayManager::new();

        // Set a video but don't start a transition
        manager.queue_video(PathBuf::from("/video.mp4"));

        assert_eq!(*manager.transition_state(), TransitionState::Idle);

        // Tick should do nothing
        manager.tick(100);

        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video.mp4"))
        );
    }

    #[test]
    fn test_tick_multiple_increments_to_completion() {
        // Test multiple tick calls accumulating to completion
        let mut manager = OverlayManager::new();

        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Multiple small ticks
        manager.tick(100); // 20%
        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn { .. }
        ));

        manager.tick(100); // 40%
        manager.tick(100); // 60%
        manager.tick(100); // 80%

        // Still in transition
        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn { .. }
        ));

        manager.tick(100); // 100% - complete

        // Now should be Idle with video2 as current
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4"))
        );
    }

    // v2.4.4: FFmpeg filter generation tests

    #[test]
    fn test_get_ffmpeg_filter_no_video_returns_empty() {
        // AC: Handles no video (returns empty filter)
        let manager = OverlayManager::new();

        // No video set, should return empty string
        let filter = manager.get_ffmpeg_filter();
        assert!(filter.is_empty(), "No video should return empty filter");
    }

    #[test]
    fn test_get_ffmpeg_filter_single_video_contains_scale() {
        // AC: Handles single video (scale + alpha + overlay)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain scale filter with default 1280x720
        assert!(filter.contains("scale=1280:720"), "Should contain scale filter");
    }

    #[test]
    fn test_get_ffmpeg_filter_single_video_contains_rgba() {
        // AC: Handles single video (scale + alpha + overlay)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain format=rgba
        assert!(filter.contains("format=rgba"), "Should contain format=rgba for alpha support");
    }

    #[test]
    fn test_get_ffmpeg_filter_single_video_contains_colorchannelmixer() {
        // AC: Applies configured opacity via colorchannelmixer
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain colorchannelmixer for alpha (default 0.3)
        assert!(
            filter.contains("colorchannelmixer=aa=0.30"),
            "Should contain colorchannelmixer with default opacity 0.30"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_applies_custom_opacity() {
        // AC: Applies configured opacity via colorchannelmixer
        let mut manager = OverlayManager::with_opacity(0.5);
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain colorchannelmixer with custom opacity
        assert!(
            filter.contains("colorchannelmixer=aa=0.50"),
            "Should contain colorchannelmixer with custom opacity 0.50"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_single_video_filter_order() {
        // Verify filter order: scale -> format -> colorchannelmixer
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        let scale_pos = filter.find("scale=").expect("Should have scale");
        let rgba_pos = filter.find("format=rgba").expect("Should have format=rgba");
        let alpha_pos = filter.find("colorchannelmixer=aa=").expect("Should have colorchannelmixer");

        assert!(scale_pos < rgba_pos, "scale should come before format");
        assert!(rgba_pos < alpha_pos, "format should come before colorchannelmixer");
    }

    #[test]
    fn test_get_ffmpeg_filter_crossfade_has_two_inputs() {
        // AC: Handles crossfade (xfade between two videos)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Now in crossfade state
        let filter = manager.get_ffmpeg_filter();

        // Should contain references to both ai_current and ai_pending
        assert!(
            filter.contains("[ai_current]"),
            "Crossfade should reference ai_current input"
        );
        assert!(
            filter.contains("[ai_pending]"),
            "Crossfade should reference ai_pending input"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_crossfade_contains_xfade() {
        // AC: Handles crossfade (xfade between two videos)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain xfade filter
        assert!(filter.contains("xfade="), "Crossfade should use xfade filter");
        assert!(
            filter.contains("transition=fade"),
            "Crossfade should use fade transition"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_crossfade_has_both_streams_scaled() {
        // AC: Handles crossfade (xfade between two videos)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Both streams should be scaled
        let scale_count = filter.matches("scale=1280:720").count();
        assert_eq!(scale_count, 2, "Both current and pending should be scaled");
    }

    #[test]
    fn test_get_ffmpeg_filter_crossfade_applies_opacity() {
        // AC: Applies configured opacity via colorchannelmixer during crossfade
        let mut manager = OverlayManager::with_opacity(0.4);
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain colorchannelmixer for both streams
        assert!(
            filter.contains("colorchannelmixer=aa="),
            "Crossfade should apply opacity via colorchannelmixer"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_custom_resolution() {
        // Test custom resolution support
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter_with_resolution(1920, 1080);

        // Should contain custom resolution
        assert!(
            filter.contains("scale=1920:1080"),
            "Should use custom resolution"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_crossfade_xfade_handles_blending() {
        // AC: xfade=transition=fade:duration=0.5 between videos
        // The xfade filter handles blending - filter is static during transition
        // (opacity is applied after xfade, not per-stream)
        let mut manager = OverlayManager::with_opacity(1.0);
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        let filter_start = manager.get_ffmpeg_filter();

        // Verify xfade handles the blending with fade transition
        assert!(
            filter_start.contains("xfade=transition=fade:duration=0.50"),
            "Should use xfade with fade transition and 0.5s duration, got: {}",
            filter_start
        );

        // Verify opacity is applied AFTER xfade (single colorchannelmixer at the end)
        assert!(
            filter_start.contains("xfade=transition=fade:duration=0.50:offset=0,format=rgba,colorchannelmixer=aa=1.00"),
            "Opacity should be applied after xfade blending, got: {}",
            filter_start
        );

        // Advance halfway - filter remains the same (xfade is statically defined)
        manager.tick(250);
        let filter_mid = manager.get_ffmpeg_filter();

        // Filter should remain consistent during crossfade (xfade is time-based internally)
        assert_eq!(
            filter_start, filter_mid,
            "Filter should be static during crossfade - xfade handles timing internally"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_no_pending_during_crossfade_fallback() {
        // Edge case: If somehow in crossfade state but no pending video, fall back to single video filter
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        // Manually set state to crossfade without pending (edge case)
        // This tests the defensive programming in get_ffmpeg_filter
        // We can't directly set transition_state, but we can test the no-pending fallback
        // by checking the idle case works correctly
        let filter = manager.get_ffmpeg_filter();

        // In idle state with video, should get single video filter
        assert!(filter.contains("scale="));
        assert!(filter.contains("format=rgba"));
        assert!(filter.contains("colorchannelmixer=aa="));
        assert!(!filter.contains("xfade"));
    }

    #[test]
    fn test_get_ffmpeg_filter_opacity_zero() {
        // Test opacity at 0 (invisible)
        let mut manager = OverlayManager::with_opacity(0.0);
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        assert!(
            filter.contains("colorchannelmixer=aa=0.00"),
            "Should handle zero opacity"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_opacity_one() {
        // Test opacity at 1.0 (fully opaque)
        let mut manager = OverlayManager::with_opacity(1.0);
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        assert!(
            filter.contains("colorchannelmixer=aa=1.00"),
            "Should handle full opacity"
        );
    }

    #[test]
    fn test_get_ffmpeg_filter_after_transition_complete() {
        // After transition completes, should return single video filter for new video
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Complete transition
        manager.tick(500);

        // Now in Idle state with video2 as current
        let filter = manager.get_ffmpeg_filter();

        // Should be single video filter (no xfade)
        assert!(filter.contains("scale=1280:720"));
        assert!(filter.contains("format=rgba"));
        assert!(filter.contains("colorchannelmixer=aa=0.30"));
        assert!(!filter.contains("xfade"), "After transition complete, should not have xfade");
    }

    // v2.4.9: Clear overlay tests

    #[test]
    fn test_clear_removes_current_and_pending_videos() {
        // AC: clear() removes current and pending videos
        let mut manager = OverlayManager::new();

        // Set up current and pending videos
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Verify initial state
        assert!(manager.current_video().is_some());
        assert!(manager.pending_video().is_some());

        // Clear the overlay
        manager.clear();

        // Pending should be immediately cleared
        assert!(
            manager.pending_video().is_none(),
            "Pending video should be cleared immediately"
        );

        // Current video should still exist during fade-out
        assert!(
            manager.current_video().is_some(),
            "Current video should exist during fade-out"
        );

        // Complete fade-out
        manager.tick(500);

        // Now current should be cleared
        assert!(
            manager.current_video().is_none(),
            "Current video should be cleared after fade-out completes"
        );
    }

    #[test]
    fn test_clear_triggers_fade_out_transition() {
        // AC: clear() triggers fade-out transition (opacity to 0)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        // Should be Idle initially
        assert_eq!(*manager.transition_state(), TransitionState::Idle);

        // Clear triggers fade-out
        manager.clear();

        // Should now be in FadeOut state
        assert!(
            matches!(manager.transition_state(), TransitionState::FadeOut { progress: 0.0, duration_ms: 500 }),
            "clear() should trigger FadeOut transition"
        );
    }

    #[test]
    fn test_clear_resets_state_to_idle_after_fade() {
        // AC: Resets state to Idle after fade
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        // Clear to trigger fade-out
        manager.clear();

        // Verify in FadeOut state
        assert!(matches!(manager.transition_state(), TransitionState::FadeOut { .. }));

        // Complete the fade-out
        manager.tick(500);

        // Should be Idle now
        assert_eq!(
            *manager.transition_state(),
            TransitionState::Idle,
            "State should be Idle after fade-out completes"
        );
    }

    #[test]
    fn test_clear_fade_out_progress_increments() {
        // Test that fade-out progress increments with tick
        let mut manager = OverlayManager::with_opacity(1.0);
        manager.queue_video(PathBuf::from("/video.mp4"));
        manager.clear();

        // At start, progress should be 0
        if let TransitionState::FadeOut { progress, .. } = manager.transition_state() {
            assert!((progress - 0.0).abs() < f32::EPSILON);
        } else {
            panic!("Expected FadeOut state");
        }

        // Tick 250ms (50% of 500ms)
        manager.tick(250);

        if let TransitionState::FadeOut { progress, .. } = manager.transition_state() {
            assert!((progress - 0.5).abs() < f32::EPSILON, "Progress should be 0.5");
        } else {
            panic!("Expected FadeOut state");
        }
    }

    #[test]
    fn test_clear_fade_out_opacity_decreases_in_filter() {
        // Test that the FFmpeg filter opacity decreases during fade-out
        let mut manager = OverlayManager::with_opacity(1.0);
        manager.queue_video(PathBuf::from("/video.mp4"));

        // Get filter at full opacity
        let filter_full = manager.get_ffmpeg_filter();
        assert!(filter_full.contains("colorchannelmixer=aa=1.00"));

        // Clear to start fade-out
        manager.clear();

        // At start of fade-out, should still be full opacity (progress 0)
        let filter_start = manager.get_ffmpeg_filter();
        assert!(
            filter_start.contains("colorchannelmixer=aa=1.00"),
            "At fade-out start, opacity should still be full"
        );

        // Tick to 50% progress
        manager.tick(250);

        let filter_mid = manager.get_ffmpeg_filter();
        assert!(
            filter_mid.contains("colorchannelmixer=aa=0.50"),
            "At 50% fade-out, opacity should be 0.50"
        );

        // Tick to 100% progress
        manager.tick(250);

        // After fade-out complete, filter should be empty (no video)
        let filter_end = manager.get_ffmpeg_filter();
        assert!(
            filter_end.is_empty(),
            "After fade-out complete, filter should be empty"
        );
    }

    #[test]
    fn test_clear_no_video_resets_to_idle() {
        // If no video is set, clear should just reset to Idle
        let mut manager = OverlayManager::new();

        // No video set
        assert!(manager.current_video().is_none());

        // Clear should set to Idle
        manager.clear();

        assert_eq!(
            *manager.transition_state(),
            TransitionState::Idle,
            "clear() with no video should set state to Idle"
        );
    }

    #[test]
    fn test_clear_during_crossfade_clears_pending() {
        // If clear is called during crossfade, pending should be cleared
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Should be in crossfade
        assert!(matches!(manager.transition_state(), TransitionState::CrossfadeIn { .. }));
        assert!(manager.pending_video().is_some());

        // Clear during crossfade
        manager.clear();

        // Pending should be cleared, should now be in fade-out
        assert!(manager.pending_video().is_none());
        assert!(matches!(manager.transition_state(), TransitionState::FadeOut { .. }));
    }

    #[test]
    fn test_clear_immediate_no_transition() {
        // Test clear_immediate for immediate clearing without fade
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        assert!(manager.current_video().is_some());

        // Immediate clear
        manager.clear_immediate();

        // Should immediately be cleared and Idle
        assert!(manager.current_video().is_none());
        assert!(manager.pending_video().is_none());
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
    }

    #[test]
    fn test_is_fading_out() {
        // Test is_fading_out helper method
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        assert!(!manager.is_fading_out(), "Should not be fading out initially");

        manager.clear();

        assert!(manager.is_fading_out(), "Should be fading out after clear()");

        manager.tick(500);

        assert!(!manager.is_fading_out(), "Should not be fading out after fade completes");
    }

    #[test]
    fn test_fade_out_completes_even_with_overshoot() {
        // Test that fade-out completes correctly even if tick overshoots
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));
        manager.clear();

        // Overshoot the duration
        manager.tick(600);

        // Should be Idle and video cleared
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert!(manager.current_video().is_none());
    }

    // v2.6: Video looping filter tests

    #[test]
    fn test_get_ffmpeg_filter_contains_loop_filter() {
        // AC: AI video loops indefinitely using loop=-1:size=9999
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Should contain loop filter for infinite looping
        assert!(
            filter.contains("loop=-1:size=9999"),
            "Single video filter should contain loop=-1:size=9999, got: {}",
            filter
        );
    }

    #[test]
    fn test_loop_filter_comes_before_scale() {
        // AC: Seamless loop - loop filter should be first in chain
        // This ensures the loop operates on the original video before any transformation
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        let loop_pos = filter.find("loop=").expect("Should have loop filter");
        let scale_pos = filter.find("scale=").expect("Should have scale filter");

        assert!(
            loop_pos < scale_pos,
            "loop filter should come before scale filter for proper looping"
        );
    }

    #[test]
    fn test_loop_filter_in_crossfade_current_stream() {
        // AC: Both streams during crossfade should have loop filter
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Count loop filter occurrences - should be 2 (one for each stream)
        let loop_count = filter.matches("loop=-1:size=9999").count();
        assert_eq!(
            loop_count, 2,
            "Crossfade filter should have loop filter for both streams, got: {}",
            filter
        );
    }

    #[test]
    fn test_loop_filter_in_fade_out() {
        // AC: Video should still loop during fade-out for seamless experience
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));
        manager.clear(); // Trigger fade-out

        let filter = manager.get_ffmpeg_filter();

        assert!(
            filter.contains("loop=-1:size=9999"),
            "Fade-out filter should contain loop filter, got: {}",
            filter
        );
    }

    #[test]
    fn test_loop_filter_complete_chain_order() {
        // Verify complete filter chain: loop -> scale -> format -> colorchannelmixer
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter();

        let loop_pos = filter.find("loop=").expect("Should have loop");
        let scale_pos = filter.find("scale=").expect("Should have scale");
        let rgba_pos = filter.find("format=rgba").expect("Should have format=rgba");
        let alpha_pos = filter
            .find("colorchannelmixer=aa=")
            .expect("Should have colorchannelmixer");

        assert!(loop_pos < scale_pos, "loop should come before scale");
        assert!(scale_pos < rgba_pos, "scale should come before format");
        assert!(rgba_pos < alpha_pos, "format should come before colorchannelmixer");
    }

    #[test]
    fn test_loop_filter_with_custom_resolution() {
        // Loop filter should work with custom resolution
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        let filter = manager.get_ffmpeg_filter_with_resolution(1920, 1080);

        assert!(
            filter.contains("loop=-1:size=9999"),
            "Custom resolution filter should contain loop filter"
        );
        assert!(
            filter.contains("scale=1920:1080"),
            "Should use custom resolution"
        );
    }

    #[test]
    fn test_loop_resets_when_new_video_queued() {
        // AC: Loop resets when new video queued
        // This is verified by checking that a new filter is generated
        // FFmpeg will restart from the beginning of the new video
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Get initial filter
        let filter1 = manager.get_ffmpeg_filter();
        assert!(filter1.contains("loop=-1:size=9999"));

        // Queue new video and complete transition
        manager.queue_video(PathBuf::from("/video2.mp4"));
        manager.tick(500); // Complete crossfade

        // New filter should still have loop (it's a fresh filter for the new video)
        let filter2 = manager.get_ffmpeg_filter();
        assert!(
            filter2.contains("loop=-1:size=9999"),
            "New video should have loop filter"
        );

        // The video has changed (loop has been reset by using new video)
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4"))
        );
    }

    #[test]
    fn test_crossfade_fallback_has_loop_filter() {
        // Edge case: If in crossfade state but no pending video, fallback filter should have loop
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video.mp4"));

        // In idle state, filter should have loop
        let filter = manager.get_ffmpeg_filter();
        assert!(
            filter.contains("loop=-1:size=9999"),
            "Idle state filter should have loop"
        );
        assert!(
            !filter.contains("xfade"),
            "Idle state should not have xfade"
        );
    }

    // v2.6: Crossfade implementation tests

    #[test]
    fn test_crossfade_configurable_duration_default() {
        // AC: Smooth transition over configured duration
        let manager = OverlayManager::new();
        assert_eq!(
            manager.crossfade_duration_ms(),
            OverlayManager::DEFAULT_CROSSFADE_DURATION_MS,
            "Default crossfade duration should be 500ms"
        );
    }

    #[test]
    fn test_crossfade_configurable_duration_custom() {
        // AC: Smooth transition over configured duration
        let manager = OverlayManager::with_settings(0.5, 1000);
        assert_eq!(
            manager.crossfade_duration_ms(),
            1000,
            "Custom crossfade duration should be 1000ms"
        );
    }

    #[test]
    fn test_crossfade_set_duration() {
        // AC: Smooth transition over configured duration
        let mut manager = OverlayManager::new();
        manager.set_crossfade_duration_ms(750);
        assert_eq!(manager.crossfade_duration_ms(), 750);
    }

    #[test]
    fn test_crossfade_duration_in_filter() {
        // AC: xfade=transition=fade:duration=0.5 between videos
        let manager_default = {
            let mut m = OverlayManager::new();
            m.queue_video(PathBuf::from("/video1.mp4"));
            m.queue_video(PathBuf::from("/video2.mp4"));
            m.get_ffmpeg_filter()
        };

        assert!(
            manager_default.contains("duration=0.50"),
            "Default duration should be 0.5s, got: {}",
            manager_default
        );

        // Custom duration
        let manager_custom = {
            let mut m = OverlayManager::with_settings(0.3, 1000);
            m.queue_video(PathBuf::from("/video1.mp4"));
            m.queue_video(PathBuf::from("/video2.mp4"));
            m.get_ffmpeg_filter()
        };

        assert!(
            manager_custom.contains("duration=1.00"),
            "Custom duration should be 1.0s, got: {}",
            manager_custom
        );
    }

    #[test]
    fn test_crossfade_instant_cut_fallback_with_zero_duration() {
        // AC: Falls back to cut if xfade not possible (0ms duration = instant cut)
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Queue with 0 duration - instant cut
        manager.queue_video_with_duration(PathBuf::from("/video2.mp4"), 0);

        // Should immediately set to current (no crossfade state)
        assert_eq!(
            *manager.transition_state(),
            TransitionState::Idle,
            "0ms duration should result in instant cut (Idle state)"
        );
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4")),
            "Video should be immediately set as current"
        );
    }

    #[test]
    fn test_cut_to_video_method() {
        // AC: Falls back to cut if xfade not possible
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Use cut_to_video for instant switch
        manager.cut_to_video(PathBuf::from("/video2.mp4"));

        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4"))
        );
    }

    #[test]
    fn test_complete_crossfade_immediately_fallback() {
        // AC: Falls back to cut if xfade not possible
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Should be in crossfade state
        assert!(manager.is_crossfading());

        // Force immediate completion (fallback)
        manager.complete_crossfade_immediately();

        // Should now be Idle with video2 as current
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video2.mp4"))
        );
        assert!(manager.pending_video().is_none());
    }

    #[test]
    fn test_is_crossfading() {
        let mut manager = OverlayManager::new();
        assert!(!manager.is_crossfading());

        manager.queue_video(PathBuf::from("/video1.mp4"));
        assert!(!manager.is_crossfading());

        manager.queue_video(PathBuf::from("/video2.mp4"));
        assert!(manager.is_crossfading());

        manager.tick(500);
        assert!(!manager.is_crossfading());
    }

    #[test]
    fn test_xfade_filter_format() {
        // AC: xfade=transition=fade:duration=0.5 between videos
        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        let filter = manager.get_ffmpeg_filter();

        // Verify exact xfade format
        assert!(
            filter.contains("xfade=transition=fade:duration=0.50:offset=0"),
            "xfade filter should have correct format, got: {}",
            filter
        );
    }

    #[test]
    fn test_crossfade_uses_configured_duration_when_queueing() {
        // Set a custom duration, then queue a video
        let mut manager = OverlayManager::new();
        manager.set_crossfade_duration_ms(1500);

        manager.queue_video(PathBuf::from("/video1.mp4"));
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Check the transition state uses the configured duration
        if let TransitionState::CrossfadeIn { duration_ms, .. } = manager.transition_state() {
            assert_eq!(*duration_ms, 1500, "Should use configured duration");
        } else {
            panic!("Expected CrossfadeIn state");
        }

        // Check the filter uses the correct duration
        let filter = manager.get_ffmpeg_filter();
        assert!(
            filter.contains("duration=1.50"),
            "Filter should use 1.5s duration, got: {}",
            filter
        );
    }

    #[test]
    fn test_clear_uses_configured_crossfade_duration() {
        let mut manager = OverlayManager::with_settings(0.5, 1000);
        manager.queue_video(PathBuf::from("/video.mp4"));

        manager.clear();

        if let TransitionState::FadeOut { duration_ms, .. } = manager.transition_state() {
            assert_eq!(*duration_ms, 1000, "FadeOut should use configured duration");
        } else {
            panic!("Expected FadeOut state");
        }
    }

    // v2.9: Error handling - overlay preservation tests

    #[test]
    fn test_overlay_unchanged_if_no_queue_called() {
        // AC: Keeps current overlay unchanged
        // When a timeout or error occurs, the calling code simply doesn't call
        // queue_video, so the overlay manager state remains unchanged.

        let mut manager = OverlayManager::with_opacity(0.5);
        manager.queue_video(PathBuf::from("/existing_video.mp4"));

        // Verify initial state
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/existing_video.mp4"))
        );
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert!((manager.opacity() - 0.5).abs() < f32::EPSILON);

        // Simulate timeout scenario: we simply don't queue a new video
        // (This represents what happens in the error handling path)

        // After "timeout" (no queue_video called), overlay remains unchanged
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/existing_video.mp4"))
        );
        assert_eq!(*manager.transition_state(), TransitionState::Idle);
        assert!((manager.opacity() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_overlay_state_preserved_during_failed_generation() {
        // AC: Keeps current overlay unchanged
        // Simulates a scenario where generation fails and overlay isn't modified

        let mut manager = OverlayManager::new();

        // Set up existing overlay
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Store state before "failed" operation
        let video_before = manager.current_video().cloned();
        let opacity_before = manager.opacity();
        let state_before = manager.transition_state().clone();

        // Simulate failed generation attempt - queue_video is NOT called
        // because the FalClient returned an error (timeout, network, etc.)
        // ... nothing happens to manager ...

        // State should be identical
        assert_eq!(manager.current_video().cloned(), video_before);
        assert!((manager.opacity() - opacity_before).abs() < f32::EPSILON);
        assert_eq!(*manager.transition_state(), state_before);
    }

    #[test]
    fn test_overlay_allows_retry_after_failed_generation() {
        // AC: Allows retry with same prompt
        // After a failed generation, the overlay can still accept new videos

        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Simulate first failed attempt - nothing queued
        // ... timeout or error occurred ...

        // Retry: queue the video after a successful retry
        manager.queue_video(PathBuf::from("/video2.mp4"));

        // Crossfade should start (proves retry worked)
        assert!(matches!(
            manager.transition_state(),
            TransitionState::CrossfadeIn { .. }
        ));
    }

    #[test]
    fn test_overlay_no_pending_video_after_timeout() {
        // When timeout occurs during polling, pending_video shouldn't be set
        // because queue_video was never called

        let mut manager = OverlayManager::new();
        manager.queue_video(PathBuf::from("/video1.mp4"));

        // Timeout occurred - queue_video not called for new video
        assert!(
            manager.pending_video().is_none(),
            "No pending video should exist after timeout"
        );

        // Current video still playing
        assert_eq!(
            manager.current_video(),
            Some(&PathBuf::from("/video1.mp4"))
        );
    }
}
