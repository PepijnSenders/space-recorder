use crate::capture::{AudioCapture, ScreenCapture, WebcamCapture, WindowCapture};
use crate::effects::{build_post_composition_filter, build_webcam_filter_chain, VideoEffect};

/// Build the ghost overlay filter graph for compositing webcam over screen
///
/// # Arguments
/// * `opacity` - Alpha value for the webcam ghost overlay (0.0-1.0)
/// * `mirror` - Whether to apply horizontal flip to webcam
/// * `effect` - Video effect preset to apply to webcam (before alpha blend)
/// * `vignette` - Whether to apply vignette effect after compositing
/// * `grain` - Whether to apply film grain effect after compositing
/// * `live_badge` - Whether to show the LIVE badge overlay
/// * `timestamp` - Whether to show the timestamp overlay
/// * `width` - Output width
/// * `height` - Output height
///
/// # Returns
/// The complete filter_complex string for FFmpeg
#[allow(clippy::too_many_arguments)]
pub fn build_ghost_overlay_filter(
    opacity: f32,
    mirror: bool,
    effect: VideoEffect,
    vignette: bool,
    grain: bool,
    live_badge: bool,
    timestamp: bool,
    width: u32,
    height: u32,
) -> String {
    // Build webcam filter chain with effects applied BEFORE alpha blend
    // Terminal stream remains unmodified (only scaled)
    let webcam_chain = build_webcam_filter_chain(mirror, effect, opacity, width, height);

    // Check if any post-composition effects are enabled
    let has_post_effects = vignette || grain || live_badge || timestamp;

    // Base overlay filter
    let overlay_output = if has_post_effects {
        "[composited]"
    } else {
        "[vout]"
    };
    let mut filter = format!(
        "[0:v]scale={}:{}[screen];[1:v]{}[ghost];[screen][ghost]overlay=0:0:format=auto{}",
        width, height, webcam_chain, overlay_output
    );

    // Add post-composition effects (vignette, grain, live badge, timestamp) if enabled
    if let Some(post_filter) =
        build_post_composition_filter(vignette, grain, live_badge, timestamp)
    {
        filter.push_str(&format!(";[composited]{}[vout]", post_filter));
    }

    filter
}

/// Configuration for the capture pipeline, used to respawn with new settings
pub struct PipelineConfig {
    pub screen_capture: ScreenCapture,
    pub screen_device: String,
    /// Window capture settings (when --window is used)
    pub window_capture: Option<WindowCapture>,
    pub webcam_capture: WebcamCapture,
    pub webcam_device_name: Option<String>,
    pub audio_capture: AudioCapture,
    pub audio_device_name: Option<String>,
    /// Video effect applied to webcam stream only
    pub effect: VideoEffect,
    /// Whether to apply vignette effect to the composited output
    pub vignette: bool,
    /// Whether to apply film grain effect to the composited output
    pub grain: bool,
    /// Whether to show LIVE badge overlay
    pub live_badge: bool,
    /// Whether to show timestamp overlay
    pub timestamp: bool,
    /// Output resolution (width, height)
    pub resolution: (u32, u32),
    /// Output framerate (used by --framerate flag)
    #[allow(dead_code)]
    pub framerate: u32,
    /// AI video overlay path (optional - for fal.ai integration)
    pub ai_video_path: Option<std::path::PathBuf>,
    /// AI video overlay opacity (0.0-1.0)
    pub ai_video_opacity: f32,
}

/// Output mode for FFmpeg pipeline
#[derive(Clone)]
pub enum OutputMode {
    /// Preview only - output to pipe for mpv
    Preview,
    /// Recording only - output to file (no preview)
    #[allow(dead_code)]
    Recording(String),
    /// Both preview and recording (tee output)
    Both(String),
}

impl PipelineConfig {
    /// Build the filter chain for the pipeline
    ///
    /// Layer order: terminal (base) → webcam ghost → AI overlay
    /// All layers are scaled to output resolution.
    pub fn build_filter_chain(&self, opacity: f32) -> String {
        let (width, height) = self.resolution;
        let mut filter_parts = Vec::new();

        // Get crop filter for window capture (if configured)
        let crop_filter = self
            .window_capture
            .as_ref()
            .and_then(|wc| wc.crop_filter());

        // Determine the final output label before post-effects
        let has_post_effects = self.vignette || self.grain || self.live_badge || self.timestamp;
        let has_webcam = self.webcam_device_name.is_some();
        let has_ai_video = self.ai_video_path.is_some();

        // Build screen capture filter chain
        let screen_filter = if let Some(ref crop) = crop_filter {
            format!("[0:v]{},scale={}:{}[screen]", crop, width, height)
        } else {
            format!("[0:v]scale={}:{}[screen]", width, height)
        };

        // Case 1: Screen + Webcam + AI Video (full three-input compositing)
        if has_webcam && has_ai_video {
            // Build webcam filter chain with effects applied BEFORE alpha blend
            let webcam_chain = build_webcam_filter_chain(
                self.webcam_capture.mirror,
                self.effect,
                opacity,
                width,
                height,
            );

            // AI video is the third input (index 2)
            // Audio would be index 3 if present
            let ai_video_filter = format!(
                "[2:v]scale={}:{},format=rgba,colorchannelmixer=aa={:.2}[ai]",
                width, height, self.ai_video_opacity
            );

            // Compose: screen -> webcam ghost overlay -> AI overlay
            let pre_ai_output = if has_post_effects { "[pre_ai]" } else { "" };
            let ai_output = if has_post_effects {
                "[composited]"
            } else {
                "[vout]"
            };

            filter_parts.push(screen_filter);
            filter_parts.push(format!("[1:v]{}[ghost]", webcam_chain));
            filter_parts.push(ai_video_filter);
            filter_parts.push(format!(
                "[screen][ghost]overlay=0:0:format=auto{}",
                if has_ai_video {
                    "[pre_ai]"
                } else {
                    pre_ai_output
                }
            ));
            filter_parts.push(format!(
                "[pre_ai][ai]overlay=0:0:format=auto{}",
                ai_output
            ));

            // Add post-composition effects if enabled
            if let Some(post_filter) = build_post_composition_filter(
                self.vignette,
                self.grain,
                self.live_badge,
                self.timestamp,
            ) {
                filter_parts.push(format!("[composited]{}[vout]", post_filter));
            }
        }
        // Case 2: Screen + AI Video only (no webcam)
        else if has_ai_video && !has_webcam {
            // AI video is the second input (index 1)
            // Audio would be index 2 if present
            let ai_video_filter = format!(
                "[1:v]scale={}:{},format=rgba,colorchannelmixer=aa={:.2}[ai]",
                width, height, self.ai_video_opacity
            );

            let ai_output = if has_post_effects {
                "[composited]"
            } else {
                "[vout]"
            };

            filter_parts.push(screen_filter);
            filter_parts.push(ai_video_filter);
            filter_parts.push(format!(
                "[screen][ai]overlay=0:0:format=auto{}",
                ai_output
            ));

            // Add post-composition effects if enabled
            if let Some(post_filter) = build_post_composition_filter(
                self.vignette,
                self.grain,
                self.live_badge,
                self.timestamp,
            ) {
                filter_parts.push(format!("[composited]{}[vout]", post_filter));
            }
        }
        // Case 3: Screen + Webcam only (existing behavior)
        else if has_webcam {
            // Build the filter with optional window crop before overlay
            let base_filter = build_ghost_overlay_filter(
                opacity,
                self.webcam_capture.mirror,
                self.effect,
                self.vignette,
                self.grain,
                self.live_badge,
                self.timestamp,
                width,
                height,
            );

            if let Some(ref crop) = crop_filter {
                // Insert crop after [0:v] source and before scale
                let scale_pattern = format!("[0:v]scale={}:{}[screen]", width, height);
                let modified_filter = base_filter.replace(
                    &scale_pattern,
                    &format!("[0:v]{},scale={}:{}[screen]", crop, width, height),
                );
                filter_parts.push(modified_filter);
            } else {
                filter_parts.push(base_filter);
            }
        }
        // Case 4: Screen only (no webcam, no AI video)
        else {
            // Build base filter with optional crop
            let screen_filter_base = if let Some(ref crop) = crop_filter {
                format!("[0:v]{},scale={}:{}", crop, width, height)
            } else {
                format!("[0:v]scale={}:{}", width, height)
            };

            if has_post_effects {
                // Apply post-composition effects after scaling
                if let Some(post_filter) = build_post_composition_filter(
                    self.vignette,
                    self.grain,
                    self.live_badge,
                    self.timestamp,
                ) {
                    filter_parts.push(format!("{},{}[vout]", screen_filter_base, post_filter));
                } else {
                    filter_parts.push(format!("{}[vout]", screen_filter_base));
                }
            } else {
                filter_parts.push(format!("{}[vout]", screen_filter_base));
            }
        }

        // Audio filter chain (if audio is enabled)
        // Audio input index depends on number of video inputs:
        // - Screen only: audio is input 1
        // - Screen + webcam: audio is input 2
        // - Screen + AI video: audio is input 2
        // - Screen + webcam + AI video: audio is input 3
        if self.audio_device_name.is_some() {
            let audio_input_index = match (has_webcam, has_ai_video) {
                (true, true) => 3,   // Screen + webcam + AI video + audio
                (true, false) => 2,  // Screen + webcam + audio
                (false, true) => 2,  // Screen + AI video + audio
                (false, false) => 1, // Screen + audio
            };
            if let Some(audio_filter) = self.audio_capture.to_filter() {
                filter_parts.push(format!(
                    "[{}:a]{}[aout]",
                    audio_input_index, audio_filter
                ));
            } else {
                filter_parts.push(format!("[{}:a]anull[aout]", audio_input_index));
            }
        }

        filter_parts.join(";")
    }

    /// Build FFmpeg arguments for the pipeline with the given opacity and output mode
    ///
    /// Input order:
    /// - Input 0: Screen capture (always present)
    /// - Input 1: Webcam (if enabled)
    /// - Input 2 (or 1): AI video (if enabled)
    /// - Last input: Audio (if enabled)
    pub fn build_ffmpeg_args(&self, opacity: f32, output_mode: &OutputMode) -> Vec<String> {
        let mut args = self.screen_capture.to_ffmpeg_args(&self.screen_device);

        // Add webcam input if enabled (input 1)
        if let Some(ref wc_name) = self.webcam_device_name {
            args.extend(self.webcam_capture.to_ffmpeg_args(wc_name));
        }

        // Add AI video input if enabled (input 2 with webcam, input 1 without)
        if let Some(ref ai_video_path) = self.ai_video_path {
            args.extend([
                "-stream_loop".to_string(),
                "-1".to_string(), // Loop indefinitely
                "-i".to_string(),
                ai_video_path.to_string_lossy().to_string(),
            ]);
        }

        // Add audio input if enabled (last input)
        if let Some(ref audio_name) = self.audio_device_name {
            args.extend(self.audio_capture.to_ffmpeg_args(audio_name));
        }

        // Build filter chain
        let filter = self.build_filter_chain(opacity);

        // Add filter_complex
        args.extend(["-filter_complex".to_string(), filter]);

        // Map the video output
        args.extend(["-map".to_string(), "[vout]".to_string()]);

        // Map the audio output if enabled
        if self.audio_device_name.is_some() {
            args.extend(["-map".to_string(), "[aout]".to_string()]);
        }

        match output_mode {
            OutputMode::Preview => {
                // Low-latency encoding settings for preview
                args.extend([
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "ultrafast".to_string(),
                    "-tune".to_string(),
                    "zerolatency".to_string(),
                ]);

                // Audio codec (if audio is enabled)
                if self.audio_device_name.is_some() {
                    args.extend([
                        "-c:a".to_string(),
                        "aac".to_string(),
                        "-b:a".to_string(),
                        "128k".to_string(),
                    ]);
                }

                // Output to NUT format for piping to mpv
                args.extend(["-f".to_string(), "nut".to_string(), "pipe:1".to_string()]);
            }
            OutputMode::Recording(path) => {
                // Quality encoding for recording
                args.extend([
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "medium".to_string(),
                    "-crf".to_string(),
                    "23".to_string(),
                ]);

                // Audio codec (if audio is enabled)
                if self.audio_device_name.is_some() {
                    args.extend([
                        "-c:a".to_string(),
                        "aac".to_string(),
                        "-b:a".to_string(),
                        "128k".to_string(),
                    ]);
                }

                // Output to file
                args.push(path.clone());
            }
            OutputMode::Both(path) => {
                // For dual output (preview + recording), we use tee muxer
                // Low-latency encoding for preview stream
                args.extend([
                    "-c:v".to_string(),
                    "libx264".to_string(),
                    "-preset".to_string(),
                    "ultrafast".to_string(),
                    "-tune".to_string(),
                    "zerolatency".to_string(),
                ]);

                // Audio codec (if audio is enabled)
                if self.audio_device_name.is_some() {
                    args.extend([
                        "-c:a".to_string(),
                        "aac".to_string(),
                        "-b:a".to_string(),
                        "128k".to_string(),
                    ]);
                }

                // Use tee to output to both preview pipe and file
                // The file output will use re-encoding with better quality
                let tee_output = format!("[f=nut]pipe:1|[f=mp4:movflags=+faststart]{}", path);
                args.extend(["-f".to_string(), "tee".to_string(), tee_output]);
            }
        }

        args
    }

    /// Build FFmpeg arguments for preview mode (used in tests)
    #[cfg(test)]
    pub fn build_ffmpeg_args_preview(&self, opacity: f32) -> Vec<String> {
        self.build_ffmpeg_args(opacity, &OutputMode::Preview)
    }

    /// Update the AI video path for dynamic video replacement.
    ///
    /// This allows swapping the AI video without recreating the entire config.
    /// The new video will be used on the next pipeline restart.
    ///
    /// # Arguments
    /// * `video_path` - New video path, or None to disable AI overlay
    ///
    /// # Returns
    /// The previous video path (if any)
    #[allow(dead_code)] // Used when full AI video integration is complete (v2)
    pub fn set_ai_video(
        &mut self,
        video_path: Option<std::path::PathBuf>,
    ) -> Option<std::path::PathBuf> {
        let previous = self.ai_video_path.take();
        self.ai_video_path = video_path;
        previous
    }

    /// Get the current AI video path.
    #[allow(dead_code)] // Used when full AI video integration is complete (v2)
    pub fn ai_video_path(&self) -> Option<&std::path::Path> {
        self.ai_video_path.as_deref()
    }

    /// Update the AI video opacity.
    ///
    /// # Arguments
    /// * `opacity` - New opacity value (0.0-1.0), will be clamped
    ///
    /// # Returns
    /// The previous opacity value
    #[allow(dead_code)] // Used when full AI video integration is complete (v2)
    pub fn set_ai_video_opacity(&mut self, opacity: f32) -> f32 {
        let previous = self.ai_video_opacity;
        self.ai_video_opacity = opacity.clamp(0.0, 1.0);
        previous
    }

    /// Get the current AI video opacity.
    #[allow(dead_code)] // Used when full AI video integration is complete (v2)
    pub fn ai_video_opacity(&self) -> f32 {
        self.ai_video_opacity
    }

    /// Check if AI video overlay is currently enabled.
    #[allow(dead_code)] // Used when full AI video integration is complete (v2)
    pub fn has_ai_video(&self) -> bool {
        self.ai_video_path.is_some()
    }

    /// Get the output resolution.
    #[allow(dead_code)] // Used when full AI video integration is complete (v2)
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::WindowBounds;

    // Ghost overlay filter graph tests

    #[test]
    fn test_build_ghost_overlay_filter_default() {
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::None,
            false,
            false,
            false,
            false,
            1280,
            720,
        );
        // Verify screen is scaled to 1280x720
        assert!(filter.contains("[0:v]scale=1280:720[screen]"));
        // Verify webcam is scaled to 1280x720
        assert!(filter.contains("[1:v]scale=1280:720"));
        // Verify RGBA format conversion for alpha
        assert!(filter.contains("format=rgba"));
        // Verify alpha channel is set via colorchannelmixer
        assert!(filter.contains("colorchannelmixer=aa=0.30"));
        // Verify overlay compositing
        assert!(filter.contains("[screen][ghost]overlay=0:0"));
        // Verify output is [vout]
        assert!(filter.contains("[vout]"));
        // Should NOT contain hflip when mirror is false
        assert!(!filter.contains("hflip"));
        // Should NOT contain effect filters when effect is None
        assert!(!filter.contains("curves="));
        assert!(!filter.contains("eq="));
        // Should NOT contain vignette when disabled
        assert!(!filter.contains("vignette"));
        // Should NOT contain grain when disabled
        assert!(!filter.contains("noise="));
    }

    #[test]
    fn test_build_ghost_overlay_filter_with_mirror() {
        let filter = build_ghost_overlay_filter(
            0.3,
            true,
            VideoEffect::None,
            false,
            false,
            false,
            false,
            1280,
            720,
        );
        // Verify hflip is present for mirror
        assert!(filter.contains("hflip"));
        // Verify rest of chain is still present
        assert!(filter.contains("[0:v]scale=1280:720[screen]"));
        assert!(filter.contains("format=rgba"));
        assert!(filter.contains("colorchannelmixer=aa=0.30"));
    }

    #[test]
    fn test_build_ghost_overlay_filter_opacity_values() {
        // Test various opacity values
        let filter_zero = build_ghost_overlay_filter(
            0.0,
            false,
            VideoEffect::None,
            false,
            false,
            false,
            false,
            1280,
            720,
        );
        assert!(filter_zero.contains("colorchannelmixer=aa=0.00"));

        let filter_full = build_ghost_overlay_filter(
            1.0,
            false,
            VideoEffect::None,
            false,
            false,
            false,
            false,
            1280,
            720,
        );
        assert!(filter_full.contains("colorchannelmixer=aa=1.00"));

        let filter_half = build_ghost_overlay_filter(
            0.5,
            false,
            VideoEffect::None,
            false,
            false,
            false,
            false,
            1280,
            720,
        );
        assert!(filter_half.contains("colorchannelmixer=aa=0.50"));
    }

    #[test]
    fn test_build_ghost_overlay_filter_structure() {
        // Test that the filter has the correct structure (no vignette or grain)
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::None,
            false,
            false,
            false,
            false,
            1280,
            720,
        );

        // Filter should have three parts separated by semicolons:
        // 1. Screen scaling: [0:v]scale=1280:720[screen]
        // 2. Webcam processing: [1:v]scale=1280:720,format=rgba,colorchannelmixer=aa=X[ghost]
        // 3. Overlay: [screen][ghost]overlay=0:0:format=auto[vout]
        let parts: Vec<&str> = filter.split(';').collect();
        assert_eq!(parts.len(), 3);

        // First part: screen scaling
        assert!(parts[0].starts_with("[0:v]"));
        assert!(parts[0].ends_with("[screen]"));

        // Second part: webcam processing chain
        assert!(parts[1].starts_with("[1:v]"));
        assert!(parts[1].ends_with("[ghost]"));

        // Third part: overlay composition
        assert!(parts[2].contains("[screen][ghost]overlay"));
        assert!(parts[2].ends_with("[vout]"));
    }

    #[test]
    fn test_build_ghost_overlay_filter_with_effect() {
        // Test with cyberpunk effect (no vignette or grain)
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::Cyberpunk,
            false,
            false,
            false,
            false,
            1280,
            720,
        );

        // Should contain effect filters
        assert!(filter.contains("curves="));
        assert!(filter.contains("saturation=1.4"));
        assert!(filter.contains("colorbalance="));

        // Terminal stream should remain unmodified (just scaled)
        assert!(filter.contains("[0:v]scale=1280:720[screen]"));
        // Effect should be in webcam chain, before alpha
        assert!(filter.contains("[1:v]"));

        // Verify structure is still correct (3 parts without vignette or grain)
        let parts: Vec<&str> = filter.split(';').collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_build_ghost_overlay_filter_effect_before_alpha() {
        // Effect must be applied BEFORE the alpha blend
        let filter = build_ghost_overlay_filter(
            0.5,
            false,
            VideoEffect::Cyberpunk,
            false,
            false,
            false,
            false,
            1280,
            720,
        );

        // In the webcam chain, effect filters should come before format=rgba
        let webcam_chain_start = filter.find("[1:v]").unwrap();
        let effect_pos = filter.find("saturation=").unwrap();
        let rgba_pos = filter.find("format=rgba").unwrap();
        let alpha_pos = filter.find("colorchannelmixer=aa=").unwrap();

        assert!(
            webcam_chain_start < effect_pos,
            "Effect should be in webcam chain"
        );
        assert!(effect_pos < rgba_pos, "Effect should be before RGBA conversion");
        assert!(rgba_pos < alpha_pos, "RGBA should be before alpha setting");
    }

    // PipelineConfig tests for opacity restart

    #[test]
    fn test_pipeline_config_build_args_screen_only() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);

        // Should have screen input args
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"avfoundation".to_string()));
        assert!(args.contains(&"Capture screen 0".to_string()));

        // Should have scale filter only (no webcam overlay)
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];
        assert!(filter.contains("[0:v]scale=1280:720[vout]"));
        assert!(!filter.contains("ghost")); // No webcam overlay
        assert!(!filter.contains("vignette")); // No vignette
        assert!(!filter.contains("noise=")); // No grain

        // Should have encoding args
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"ultrafast".to_string()));
        assert!(args.contains(&"zerolatency".to_string()));
    }

    #[test]
    fn test_pipeline_config_build_args_with_webcam() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new().with_mirror(true),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args_30 = config.build_ffmpeg_args_preview(0.3);
        let args_70 = config.build_ffmpeg_args_preview(0.7);

        // Should have webcam input args
        assert!(args_30.contains(&"FaceTime HD Camera".to_string()));

        // Should have ghost overlay filter with correct opacity
        let filter_idx_30 = args_30.iter().position(|a| a == "-filter_complex").unwrap();
        let filter_30 = &args_30[filter_idx_30 + 1];
        assert!(filter_30.contains("colorchannelmixer=aa=0.30"));
        assert!(filter_30.contains("[screen][ghost]overlay"));
        assert!(filter_30.contains("hflip")); // Mirror enabled

        let filter_idx_70 = args_70.iter().position(|a| a == "-filter_complex").unwrap();
        let filter_70 = &args_70[filter_idx_70 + 1];
        assert!(filter_70.contains("colorchannelmixer=aa=0.70"));
    }

    #[test]
    fn test_pipeline_config_build_args_with_audio() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::new().with_volume(0.8),
            audio_device_name: Some("MacBook Pro Microphone".to_string()),
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);

        // Should have audio input args
        assert!(args.contains(&":MacBook Pro Microphone".to_string()));

        // Should have audio filter with volume
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];
        assert!(filter.contains("[1:a]volume=0.80[aout]"));

        // Should have audio codec args
        assert!(args.contains(&"aac".to_string()));
        assert!(args.contains(&"128k".to_string()));
    }

    #[test]
    fn test_pipeline_config_build_args_full() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::new(),
            audio_device_name: Some("MacBook Pro Microphone".to_string()),
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.5);

        // Should have all three inputs
        assert!(args.contains(&"Capture screen 0".to_string()));
        assert!(args.contains(&"FaceTime HD Camera".to_string()));
        assert!(args.contains(&":MacBook Pro Microphone".to_string()));

        // Should have proper filter with webcam overlay and audio
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];
        assert!(filter.contains("colorchannelmixer=aa=0.50"));
        assert!(filter.contains("[2:a]")); // Audio is input 2 when webcam present

        // Should output to pipe
        assert!(args.contains(&"pipe:1".to_string()));
        assert!(args.contains(&"nut".to_string()));
    }

    #[test]
    fn test_pipeline_config_opacity_changes_only_filter() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args_low = config.build_ffmpeg_args_preview(0.1);
        let args_high = config.build_ffmpeg_args_preview(0.9);

        // Both should have same number of args
        assert_eq!(args_low.len(), args_high.len());

        // Only the filter_complex value should differ
        let filter_idx_low = args_low.iter().position(|a| a == "-filter_complex").unwrap();
        let filter_idx_high = args_high
            .iter()
            .position(|a| a == "-filter_complex")
            .unwrap();

        // Filters should be different (different opacity)
        assert_ne!(
            args_low[filter_idx_low + 1],
            args_high[filter_idx_high + 1]
        );

        // But filter indices should be the same
        assert_eq!(filter_idx_low, filter_idx_high);

        // Verify opacity values
        assert!(args_low[filter_idx_low + 1].contains("aa=0.10"));
        assert!(args_high[filter_idx_high + 1].contains("aa=0.90"));
    }

    #[test]
    fn test_pipeline_config_build_args_with_effect() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::Cyberpunk,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);

        // Should have effect filters in the filter_complex
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Effect should be applied to webcam only
        assert!(filter.contains("curves="));
        assert!(filter.contains("saturation=1.4"));
        assert!(filter.contains("colorbalance="));

        // Terminal stream should remain unmodified
        assert!(filter.contains("[0:v]scale=1280:720[screen]"));
    }

    #[test]
    fn test_pipeline_config_no_effects() {
        // Test that VideoEffect::None produces no color grading filters
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should NOT have color grading filters
        assert!(
            !filter.contains("curves="),
            "No color curves with no effects"
        );
        assert!(
            !filter.contains("saturation=1.4"),
            "No saturation boost with no effects"
        );
        assert!(
            !filter.contains("colorbalance="),
            "No color balance with no effects"
        );
        assert!(
            !filter.contains("brightness="),
            "No brightness adjustment with no effects"
        );

        // Ghost overlay should still work
        assert!(
            filter.contains("[screen][ghost]overlay"),
            "Ghost overlay still active"
        );
        assert!(
            filter.contains("colorchannelmixer=aa=0.30"),
            "Alpha still applied"
        );
        assert!(filter.contains("format=rgba"), "RGBA conversion still present");
    }

    // Vignette-specific tests

    #[test]
    fn test_build_ghost_overlay_filter_with_vignette() {
        // Test with vignette enabled (no grain)
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::None,
            true,
            false,
            false,
            false,
            1280,
            720,
        );

        // Should contain vignette filter
        assert!(filter.contains("vignette=PI/5"));

        // Should have 4 parts when vignette is enabled:
        // 1. Screen scaling
        // 2. Webcam processing
        // 3. Overlay to [composited]
        // 4. Vignette on [composited] to [vout]
        let parts: Vec<&str> = filter.split(';').collect();
        assert_eq!(parts.len(), 4, "Should have 4 parts with vignette enabled");

        // Overlay should output to [composited] (not [vout])
        assert!(
            parts[2].contains("[composited]"),
            "Overlay should output to [composited]"
        );

        // Vignette should take [composited] and output [vout]
        assert!(
            parts[3].contains("[composited]"),
            "Vignette should read from [composited]"
        );
        assert!(
            parts[3].contains("[vout]"),
            "Vignette should output to [vout]"
        );
        assert!(
            parts[3].contains("vignette=PI/5"),
            "Should apply vignette filter"
        );
    }

    #[test]
    fn test_build_ghost_overlay_filter_vignette_order() {
        // Vignette should be applied AFTER overlay/compositing
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::Cyberpunk,
            true,
            false,
            false,
            false,
            1280,
            720,
        );

        // Verify order: overlay comes before vignette
        let overlay_pos = filter.find("overlay=").unwrap();
        let vignette_pos = filter.find("vignette=").unwrap();
        assert!(
            overlay_pos < vignette_pos,
            "Overlay should come before vignette"
        );

        // Color grading (effect) should come before overlay
        let effect_pos = filter.find("saturation=").unwrap();
        assert!(
            effect_pos < overlay_pos,
            "Color grading should come before overlay"
        );
    }

    #[test]
    fn test_pipeline_config_screen_only_with_vignette() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: true,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should have vignette in the filter even without webcam
        assert!(filter.contains("vignette=PI/5"));
        // Should have scale and vignette
        assert!(filter.contains("scale=1280:720"));
        // Output should be [vout]
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn test_pipeline_config_webcam_with_vignette() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: true,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should have ghost overlay
        assert!(filter.contains("[screen][ghost]overlay"));
        // Should have vignette applied after overlay
        assert!(filter.contains("vignette=PI/5"));
        // Overlay should go to [composited]
        assert!(filter.contains("[composited]"));
    }

    // Film grain tests

    #[test]
    fn test_build_ghost_overlay_filter_with_grain() {
        // Test with grain enabled (no vignette)
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::None,
            false,
            true,
            false,
            false,
            1280,
            720,
        );

        // Should contain noise filter
        assert!(filter.contains("noise=alls=10:allf=t"));

        // Should have 4 parts when grain is enabled:
        // 1. Screen scaling
        // 2. Webcam processing
        // 3. Overlay to [composited]
        // 4. Grain on [composited] to [vout]
        let parts: Vec<&str> = filter.split(';').collect();
        assert_eq!(parts.len(), 4, "Should have 4 parts with grain enabled");

        // Overlay should output to [composited] (not [vout])
        assert!(
            parts[2].contains("[composited]"),
            "Overlay should output to [composited]"
        );

        // Grain should take [composited] and output [vout]
        assert!(
            parts[3].contains("[composited]"),
            "Grain should read from [composited]"
        );
        assert!(parts[3].contains("[vout]"), "Grain should output to [vout]");
        assert!(
            parts[3].contains("noise=alls=10:allf=t"),
            "Should apply grain filter"
        );
    }

    #[test]
    fn test_build_ghost_overlay_filter_with_vignette_and_grain() {
        // Test with both vignette and grain enabled
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::None,
            true,
            true,
            false,
            false,
            1280,
            720,
        );

        // Should contain both filters
        assert!(filter.contains("vignette=PI/5"));
        assert!(filter.contains("noise=alls=10:allf=t"));

        // Should have 4 parts (vignette and grain are combined in post-comp)
        let parts: Vec<&str> = filter.split(';').collect();
        assert_eq!(
            parts.len(),
            4,
            "Should have 4 parts with both effects enabled"
        );

        // Vignette should come before grain
        let vignette_pos = filter.find("vignette=").unwrap();
        let grain_pos = filter.find("noise=").unwrap();
        assert!(
            vignette_pos < grain_pos,
            "Vignette should be applied before grain"
        );
    }

    #[test]
    fn test_build_ghost_overlay_filter_grain_order() {
        // Grain should be applied AFTER overlay/compositing and color grading
        let filter = build_ghost_overlay_filter(
            0.3,
            false,
            VideoEffect::Cyberpunk,
            false,
            true,
            false,
            false,
            1280,
            720,
        );

        // Verify order: color grading -> overlay -> grain
        let effect_pos = filter.find("saturation=").unwrap();
        let overlay_pos = filter.find("overlay=").unwrap();
        let grain_pos = filter.find("noise=").unwrap();

        assert!(
            effect_pos < overlay_pos,
            "Color grading should come before overlay"
        );
        assert!(overlay_pos < grain_pos, "Overlay should come before grain");
    }

    #[test]
    fn test_pipeline_config_screen_only_with_grain() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: true,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should have grain in the filter even without webcam
        assert!(filter.contains("noise=alls=10:allf=t"));
        // Should have scale and grain
        assert!(filter.contains("scale=1280:720"));
        // Output should be [vout]
        assert!(filter.contains("[vout]"));
    }

    #[test]
    fn test_pipeline_config_webcam_with_grain() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: true,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should have ghost overlay
        assert!(filter.contains("[screen][ghost]overlay"));
        // Should have grain applied after overlay
        assert!(filter.contains("noise=alls=10:allf=t"));
        // Overlay should go to [composited]
        assert!(filter.contains("[composited]"));
    }

    #[test]
    fn test_pipeline_config_webcam_with_vignette_and_grain() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: true,
            grain: true,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should have ghost overlay
        assert!(filter.contains("[screen][ghost]overlay"));
        // Should have both vignette and grain applied
        assert!(filter.contains("vignette=PI/5"));
        assert!(filter.contains("noise=alls=10:allf=t"));
        // Vignette should come before grain
        let vignette_pos = filter.find("vignette=").unwrap();
        let grain_pos = filter.find("noise=").unwrap();
        assert!(vignette_pos < grain_pos, "Vignette should be before grain");
    }

    // Window capture tests

    #[test]
    fn test_pipeline_config_with_window_capture_screen_only() {
        // Test window capture with screen only (no webcam)
        let mut window_capture = WindowCapture::new("Terminal");
        window_capture.bounds = Some(WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        });

        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: Some(window_capture),
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should contain crop filter before scale
        assert!(
            filter.contains("crop="),
            "Should contain crop filter for window capture"
        );
        assert!(filter.contains("scale=1280:720"), "Should scale output");
        // Crop should come before scale in the filter chain
        let crop_pos = filter.find("crop=").unwrap();
        let scale_pos = filter.find("scale=").unwrap();
        assert!(crop_pos < scale_pos, "Crop should come before scale");
    }

    #[test]
    fn test_pipeline_config_with_window_capture_and_webcam() {
        // Test window capture with webcam overlay
        let mut window_capture = WindowCapture::new("Terminal");
        window_capture.bounds = Some(WindowBounds {
            x: 100,
            y: 50,
            width: 800,
            height: 600,
        });

        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: Some(window_capture),
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should contain crop filter in screen chain
        assert!(
            filter.contains("crop="),
            "Should contain crop filter for window capture"
        );
        // Should have ghost overlay
        assert!(
            filter.contains("[screen][ghost]overlay"),
            "Should have ghost overlay"
        );
        // Crop should come before scale in the screen filter
        let crop_pos = filter.find("crop=").unwrap();
        let scale_pos = filter.find("scale=1280:720[screen]").unwrap();
        assert!(
            crop_pos < scale_pos,
            "Crop should come before scale in screen chain"
        );
    }

    #[test]
    fn test_pipeline_config_without_window_capture_no_crop() {
        // Test that without window capture, there's no crop filter
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should NOT contain crop filter
        assert!(
            !filter.contains("crop="),
            "Should not contain crop filter without window capture"
        );
    }

    #[test]
    fn test_pipeline_config_window_capture_without_bounds() {
        // WindowCapture without detected bounds should not produce crop filter
        let window_capture = WindowCapture::new("Terminal");
        // bounds is None by default

        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: Some(window_capture),
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should NOT contain crop filter (bounds not set)
        assert!(
            !filter.contains("crop="),
            "Should not contain crop filter when bounds not detected"
        );
    }

    #[test]
    fn test_output_mode_recording_args() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1920, 1080),
            framerate: 60,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args(0.3, &OutputMode::Recording("output.mp4".to_string()));

        // Should have quality encoding for recording
        assert!(
            args.contains(&"medium".to_string()),
            "Should use medium preset"
        );
        assert!(args.contains(&"-crf".to_string()), "Should use CRF");
        assert!(args.contains(&"23".to_string()), "Should use CRF 23");
        // Should output to file
        assert!(
            args.contains(&"output.mp4".to_string()),
            "Should output to file"
        );
        // Should NOT output to pipe
        assert!(!args.contains(&"pipe:1".to_string()));
    }

    #[test]
    fn test_output_mode_both_args() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args(0.3, &OutputMode::Both("recording.mp4".to_string()));

        // Should use tee muxer for dual output
        assert!(args.contains(&"tee".to_string()), "Should use tee muxer");
        // Should have reference to pipe for preview and file for recording
        let tee_output_idx = args
            .iter()
            .position(|a| a.contains("pipe:1") && a.contains("recording.mp4"));
        assert!(
            tee_output_idx.is_some(),
            "Should have tee output with both pipe and file"
        );
    }

    #[test]
    fn test_resolution_affects_filter() {
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1920, 1080),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Filter should use 1920x1080 resolution
        assert!(
            filter.contains("scale=1920:1080"),
            "Filter should use configured resolution"
        );
    }

    // v2.6: Three-input pipeline tests (terminal + webcam + AI video)

    #[test]
    fn test_pipeline_config_with_ai_video_only() {
        // AC: Pipeline accepts ai_video input (without webcam)
        // AC: AI video input is optional (pipeline works without it)
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/ai_video.mp4")),
            ai_video_opacity: 0.4,
        };

        let args = config.build_ffmpeg_args_preview(0.3);

        // Should have AI video input with loop flag
        assert!(
            args.contains(&"-stream_loop".to_string()),
            "Should have stream_loop for AI video"
        );
        assert!(
            args.contains(&"-1".to_string()),
            "Should loop indefinitely"
        );
        assert!(
            args.contains(&"/tmp/ai_video.mp4".to_string()),
            "Should have AI video path"
        );

        // Filter should have AI overlay
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // AC: All layers scaled to output resolution
        assert!(
            filter.contains("scale=1280:720"),
            "AI video should be scaled"
        );
        assert!(
            filter.contains("colorchannelmixer=aa=0.40"),
            "AI video should have correct opacity"
        );
        assert!(
            filter.contains("[screen][ai]overlay"),
            "Should overlay AI on screen"
        );
    }

    #[test]
    fn test_pipeline_config_with_three_inputs() {
        // AC: Pipeline accepts terminal, webcam, AND ai_video inputs
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/ai_video.mp4")),
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.5);

        // Should have all three video inputs
        // Input 0: Screen
        assert!(
            args.contains(&"Capture screen 0".to_string()),
            "Should have screen capture"
        );
        // Input 1: Webcam
        assert!(
            args.contains(&"FaceTime HD Camera".to_string()),
            "Should have webcam"
        );
        // Input 2: AI video
        assert!(
            args.contains(&"/tmp/ai_video.mp4".to_string()),
            "Should have AI video"
        );

        // Filter should have proper compositing
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // AC: Layer order: terminal → webcam ghost → AI overlay
        // Check that screen comes first, then ghost overlay, then AI overlay
        assert!(filter.contains("[0:v]"), "Should have screen as input 0");
        assert!(filter.contains("[1:v]"), "Should have webcam as input 1");
        assert!(filter.contains("[2:v]"), "Should have AI video as input 2");
        assert!(
            filter.contains("[screen][ghost]overlay"),
            "Should overlay ghost on screen"
        );
        assert!(
            filter.contains("[pre_ai][ai]overlay"),
            "Should overlay AI on composited base"
        );
    }

    #[test]
    fn test_pipeline_config_three_inputs_layer_order() {
        // AC: Layer order: terminal → webcam ghost → AI overlay
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/ai_video.mp4")),
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Verify the layer order by checking filter positions
        let screen_ghost_pos = filter
            .find("[screen][ghost]overlay")
            .expect("Should have screen+ghost overlay");
        let pre_ai_pos = filter
            .find("[pre_ai][ai]overlay")
            .expect("Should have pre_ai+ai overlay");

        assert!(
            screen_ghost_pos < pre_ai_pos,
            "Screen+ghost overlay should come before AI overlay"
        );
    }

    #[test]
    fn test_pipeline_config_three_inputs_all_scaled() {
        // AC: All layers scaled to output resolution
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1920, 1080),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/ai_video.mp4")),
            ai_video_opacity: 0.5,
        };

        let args = config.build_ffmpeg_args_preview(0.3);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Count scale=1920:1080 occurrences - should be 3 (screen, webcam, AI video)
        let scale_count = filter.matches("scale=1920:1080").count();
        assert_eq!(
            scale_count, 3,
            "All three inputs should be scaled to output resolution"
        );
    }

    #[test]
    fn test_pipeline_config_three_inputs_with_audio() {
        // Test that audio input index is correct with three video inputs
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::new().with_volume(0.8),
            audio_device_name: Some("MacBook Pro Microphone".to_string()),
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/ai_video.mp4")),
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);

        // Should have audio input
        assert!(
            args.contains(&":MacBook Pro Microphone".to_string()),
            "Should have audio input"
        );

        // Filter should reference audio as input 3 (screen=0, webcam=1, AI=2, audio=3)
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        assert!(
            filter.contains("[3:a]"),
            "Audio should be input 3 with three video inputs"
        );
    }

    #[test]
    fn test_pipeline_config_ai_video_optional() {
        // AC: AI video input is optional (pipeline works without it)
        // Verify existing behavior still works when ai_video_path is None
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None, // No AI video
            ai_video_opacity: 0.3,
        };

        let args = config.build_ffmpeg_args_preview(0.3);

        // Should NOT have stream_loop (no AI video)
        assert!(
            !args.contains(&"-stream_loop".to_string()),
            "Should not have stream_loop without AI video"
        );

        // Filter should still work with just screen + webcam
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        assert!(
            filter.contains("[screen][ghost]overlay"),
            "Should have screen+ghost overlay"
        );
        assert!(
            !filter.contains("[pre_ai]"),
            "Should not have AI layer without AI video"
        );
        assert!(
            !filter.contains("[ai]"),
            "Should not have AI label without AI video"
        );
    }

    #[test]
    fn test_pipeline_config_ai_video_with_effects() {
        // Test AI video with post-composition effects enabled
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::Cyberpunk,
            vignette: true,
            grain: false,
            live_badge: true,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/ai_video.mp4")),
            ai_video_opacity: 0.25,
        };

        let args = config.build_ffmpeg_args_preview(0.4);
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // Should have all compositing layers
        assert!(
            filter.contains("[screen][ghost]overlay"),
            "Should have ghost overlay"
        );
        assert!(
            filter.contains("[pre_ai][ai]overlay"),
            "Should have AI overlay"
        );

        // Should have post-composition effects after AI overlay
        assert!(
            filter.contains("[composited]"),
            "Should have composited intermediate"
        );
        assert!(filter.contains("vignette"), "Should have vignette effect");
        assert!(filter.contains("drawtext"), "Should have LIVE badge text");
        assert!(filter.contains("[vout]"), "Should have final output");
    }

    // v2.6 Dynamic video replacement tests

    #[test]
    fn test_pipeline_config_set_ai_video() {
        // AC: Can swap AI video input
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/video1.mp4")),
            ai_video_opacity: 0.3,
        };

        // Verify initial state
        assert_eq!(
            config.ai_video_path(),
            Some(std::path::Path::new("/tmp/video1.mp4"))
        );

        // Swap to new video
        let previous = config.set_ai_video(Some(std::path::PathBuf::from("/tmp/video2.mp4")));

        // Verify swap occurred
        assert_eq!(
            previous,
            Some(std::path::PathBuf::from("/tmp/video1.mp4"))
        );
        assert_eq!(
            config.ai_video_path(),
            Some(std::path::Path::new("/tmp/video2.mp4"))
        );
    }

    #[test]
    fn test_pipeline_config_set_ai_video_from_none() {
        // AC: Can add AI video when starting from none
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        assert!(!config.has_ai_video());

        // Add AI video
        let previous =
            config.set_ai_video(Some(std::path::PathBuf::from("/tmp/new_video.mp4")));

        assert!(previous.is_none());
        assert!(config.has_ai_video());
        assert_eq!(
            config.ai_video_path(),
            Some(std::path::Path::new("/tmp/new_video.mp4"))
        );
    }

    #[test]
    fn test_pipeline_config_set_ai_video_to_none() {
        // AC: Can remove AI video (set to None)
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/video.mp4")),
            ai_video_opacity: 0.3,
        };

        assert!(config.has_ai_video());

        // Remove AI video
        let previous = config.set_ai_video(None);

        assert_eq!(
            previous,
            Some(std::path::PathBuf::from("/tmp/video.mp4"))
        );
        assert!(!config.has_ai_video());
        assert!(config.ai_video_path().is_none());
    }

    #[test]
    fn test_pipeline_config_set_ai_video_opacity() {
        // AC: Can update AI video opacity
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/video.mp4")),
            ai_video_opacity: 0.3,
        };

        // Verify initial opacity
        assert!((config.ai_video_opacity() - 0.3).abs() < f32::EPSILON);

        // Update opacity
        let previous = config.set_ai_video_opacity(0.5);

        assert!((previous - 0.3).abs() < f32::EPSILON);
        assert!((config.ai_video_opacity() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pipeline_config_set_ai_video_opacity_clamped() {
        // AC: Opacity is clamped to 0.0-1.0 range
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/video.mp4")),
            ai_video_opacity: 0.3,
        };

        // Test clamping above 1.0
        config.set_ai_video_opacity(1.5);
        assert!((config.ai_video_opacity() - 1.0).abs() < f32::EPSILON);

        // Test clamping below 0.0
        config.set_ai_video_opacity(-0.5);
        assert!((config.ai_video_opacity() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pipeline_config_get_resolution() {
        // Test resolution accessor (used by video replacement for format normalization)
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1920, 1080),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        assert_eq!(config.resolution(), (1920, 1080));
    }

    #[test]
    fn test_pipeline_config_filter_after_video_swap() {
        // AC: Filter correctly reflects swapped video
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        // Build filter without AI video
        let filter_without = config.build_filter_chain(0.3);
        assert!(
            !filter_without.contains("[ai]"),
            "Should not have AI layer without video"
        );

        // Swap in AI video
        config.set_ai_video(Some(std::path::PathBuf::from("/tmp/video.mp4")));

        // Build filter with AI video
        let filter_with = config.build_filter_chain(0.3);
        assert!(
            filter_with.contains("[ai]"),
            "Should have AI layer after swap"
        );
        assert!(
            filter_with.contains("[pre_ai][ai]overlay"),
            "Should composite AI layer"
        );
    }

    #[test]
    fn test_pipeline_config_args_after_video_swap() {
        // AC: FFmpeg args correctly reflect swapped video
        let mut config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::new(),
            webcam_device_name: Some("FaceTime HD Camera".to_string()),
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: None,
            ai_video_opacity: 0.3,
        };

        // Build args without AI video
        let args_without = config.build_ffmpeg_args_preview(0.3);
        assert!(!args_without.contains(&"-stream_loop".to_string()));

        // Swap in AI video
        config.set_ai_video(Some(std::path::PathBuf::from("/tmp/video.mp4")));

        // Build args with AI video
        let args_with = config.build_ffmpeg_args_preview(0.3);
        assert!(args_with.contains(&"-stream_loop".to_string()));
        assert!(args_with.contains(&"/tmp/video.mp4".to_string()));
    }

    #[test]
    fn test_pipeline_config_handles_format_differences() {
        // AC: Handles video format differences (resolution, codec)
        // The filter chain scales to output resolution and converts to rgba
        let config = PipelineConfig {
            screen_capture: ScreenCapture::new(0),
            screen_device: "Capture screen 0".to_string(),
            window_capture: None,
            webcam_capture: WebcamCapture::disabled(),
            webcam_device_name: None,
            audio_capture: AudioCapture::disabled(),
            audio_device_name: None,
            effect: VideoEffect::None,
            vignette: false,
            grain: false,
            live_badge: false,
            timestamp: false,
            resolution: (1280, 720),
            framerate: 30,
            ai_video_path: Some(std::path::PathBuf::from("/tmp/any_format_video.mp4")),
            ai_video_opacity: 0.4,
        };

        let filter = config.build_filter_chain(0.3);

        // Filter should scale AI video to output resolution
        assert!(
            filter.contains("scale=1280:720"),
            "Should scale to output resolution"
        );
        // Filter should convert to rgba for alpha blending
        assert!(
            filter.contains("format=rgba"),
            "Should convert to rgba for format handling"
        );
        // Filter should apply opacity
        assert!(
            filter.contains("colorchannelmixer=aa=0.40"),
            "Should apply AI video opacity"
        );
    }
}
