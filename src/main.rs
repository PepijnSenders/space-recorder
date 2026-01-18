mod capture;
mod config;
mod effects;
mod fal;
mod hotkeys;
mod permissions;
mod pipeline;

use capture::{AudioCapture, CaptureError, ScreenCapture, WebcamCapture, WindowCapture};
use clap::{Parser, Subcommand};
use effects::{build_post_composition_filter, build_webcam_filter_chain, VideoEffect};
use hotkeys::HotkeyManager;
use pipeline::{setup_ctrlc_handler, Pipeline, PipelineError};
use std::process::{Command, Stdio};

/// Parse and validate volume (0.0-2.0)
fn parse_volume(s: &str) -> Result<f32, String> {
    let vol: f32 = s.parse().map_err(|_| format!("'{}' is not a valid number", s))?;
    if !(0.0..=2.0).contains(&vol) {
        return Err(format!("Volume must be between 0.0 and 2.0, got {}", vol));
    }
    Ok(vol)
}

/// Parse and validate opacity (0.0-1.0)
fn parse_opacity(s: &str) -> Result<f32, String> {
    let opacity: f32 = s.parse().map_err(|_| format!("'{}' is not a valid number", s))?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(format!("Opacity must be between 0.0 and 1.0, got {}", opacity));
    }
    Ok(opacity)
}

/// Parse video effect preset
fn parse_effect(s: &str) -> Result<VideoEffect, String> {
    VideoEffect::from_str(s).ok_or_else(|| {
        format!(
            "Unknown effect '{}'. Available effects: none, cyberpunk, dark_mode",
            s
        )
    })
}

/// Parse and validate resolution (WIDTHxHEIGHT format)
fn parse_resolution(s: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid resolution format '{}'. Use WIDTHxHEIGHT (e.g., 1920x1080)",
            s
        ));
    }
    let width: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid width '{}' in resolution", parts[0]))?;
    let height: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid height '{}' in resolution", parts[1]))?;
    if width == 0 || height == 0 {
        return Err("Resolution width and height must be greater than 0".to_string());
    }
    if width > 7680 || height > 4320 {
        return Err("Resolution exceeds maximum supported (7680x4320)".to_string());
    }
    Ok((width, height))
}

/// Parse and validate framerate (1-120 fps)
fn parse_framerate(s: &str) -> Result<u32, String> {
    let fps: u32 = s
        .parse()
        .map_err(|_| format!("'{}' is not a valid framerate", s))?;
    if !(1..=120).contains(&fps) {
        return Err(format!(
            "Framerate must be between 1 and 120 fps, got {}",
            fps
        ));
    }
    Ok(fps)
}

/// Parse and validate noise gate threshold (0.0-1.0)
fn parse_noise_gate_threshold(s: &str) -> Result<f32, String> {
    let threshold: f32 = s.parse().map_err(|_| format!("'{}' is not a valid number", s))?;
    if !(0.0..=1.0).contains(&threshold) {
        return Err(format!("Noise gate threshold must be between 0.0 and 1.0, got {}", threshold));
    }
    Ok(threshold)
}

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
fn build_ghost_overlay_filter(opacity: f32, mirror: bool, effect: VideoEffect, vignette: bool, grain: bool, live_badge: bool, timestamp: bool, width: u32, height: u32) -> String {
    // Build webcam filter chain with effects applied BEFORE alpha blend
    // Terminal stream remains unmodified (only scaled)
    let webcam_chain = build_webcam_filter_chain(mirror, effect, opacity, width, height);

    // Check if any post-composition effects are enabled
    let has_post_effects = vignette || grain || live_badge || timestamp;

    // Base overlay filter
    let overlay_output = if has_post_effects { "[composited]" } else { "[vout]" };
    let mut filter = format!(
        "[0:v]scale={}:{}[screen];[1:v]{}[ghost];[screen][ghost]overlay=0:0:format=auto{}",
        width, height,
        webcam_chain,
        overlay_output
    );

    // Add post-composition effects (vignette, grain, live badge, timestamp) if enabled
    if let Some(post_filter) = build_post_composition_filter(vignette, grain, live_badge, timestamp) {
        filter.push_str(&format!(";[composited]{}[vout]", post_filter));
    }

    filter
}

/// space-recorder: Video compositor for coding streams
#[derive(Parser)]
#[command(name = "space-recorder")]
#[command(version, about = "Video compositor for coding streams")]
#[command(long_about = "Composite terminal windows with a ghostly webcam overlay for \
    screen sharing in video calls. Supports visual effects, text overlays, \
    and real-time opacity control via hotkeys.")]
#[command(after_help = "EXAMPLES:
    # Start with Terminal window capture
    space-recorder start --window Terminal

    # Custom opacity and cyberpunk effect
    space-recorder start -W Terminal -o 0.4 -e cyberpunk

    # Full screen capture with LIVE badge
    space-recorder start --screen 0 --live

    # Screen only, no webcam
    space-recorder start --no-webcam

    # Enable fal.ai overlay mode (type prompts during stream)
    space-recorder start --fal --window Terminal

    # Pre-generate AI video before streaming
    space-recorder fal-generate \"cyberpunk cityscape\"

    # List available devices
    space-recorder list-devices

For more information, see: https://github.com/username/space-recorder")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List available video and audio capture devices
    #[command(after_help = "EXAMPLES:
    space-recorder list-devices          # List all devices
    space-recorder list-devices --video  # List only video devices
    space-recorder list-devices --audio  # List only audio devices")]
    ListDevices {
        /// Show only video devices (cameras and screens)
        #[arg(long)]
        video: bool,
        /// Show only audio devices (microphones)
        #[arg(long)]
        audio: bool,
    },

    /// Manage fal.ai video cache
    #[command(after_help = "EXAMPLES:
    space-recorder fal-cache list        # List all cached videos
    space-recorder fal-cache clear       # Remove all cached videos
    space-recorder fal-cache clear abc123  # Remove specific cached video by hash")]
    FalCache {
        #[command(subcommand)]
        action: FalCacheAction,
    },

    /// Pre-generate AI video from a text prompt
    ///
    /// Generates a video using fal.ai and caches it locally.
    /// Useful for pre-warming the cache before starting a stream.
    #[command(after_help = "EXAMPLES:
    space-recorder fal-generate \"cyberpunk cityscape\"
    space-recorder fal-generate \"abstract particles flowing\"
    space-recorder fal-generate --batch prompts.txt

ENVIRONMENT:
    FAL_API_KEY    Required. Your fal.ai API key.")]
    FalGenerate {
        /// The text prompt describing the video to generate
        #[arg(required_unless_present = "batch")]
        prompt: Option<String>,

        /// Path to a file containing prompts (one per line) for batch generation
        #[arg(long, short = 'b', conflicts_with = "prompt")]
        batch: Option<std::path::PathBuf>,
    },

    /// Start the compositor and preview in mpv
    #[command(after_help = "EXAMPLES:
    # Capture Terminal window with default settings
    space-recorder start --window Terminal

    # Custom opacity (40%) with cyberpunk color grading
    space-recorder start -W Code -o 0.4 -e cyberpunk

    # Full screen capture with LIVE badge and timestamp
    space-recorder start --screen 0 --live --timestamp

    # Disable webcam for screen-only capture
    space-recorder start --no-webcam --vignette

    # Enable audio processing
    space-recorder start -W Terminal --noise-gate --compressor

    # Enable fal.ai overlay mode (type prompts during stream)
    space-recorder start --fal -W Terminal

    # fal.ai mode with custom AI overlay opacity
    space-recorder start --fal --fal-opacity 0.5 -W Terminal

HOTKEYS (while running):
    +/=    Increase ghost opacity
    -      Decrease ghost opacity
    Ctrl+C Quit

FAL.AI COMMANDS (when --fal is enabled):
    <prompt>     Generate and overlay AI video from text prompt
    /clear       Remove current AI video overlay
    /opacity N   Set AI overlay opacity (0.0-1.0)")]
    Start {
        /// Capture specific window by application name (e.g., "Terminal", "Code")
        /// Overrides full-screen capture
        #[arg(long, short = 'W')]
        window: Option<String>,

        /// Screen device index to capture (default: auto-detect first screen)
        #[arg(long, short = 's')]
        screen: Option<usize>,

        /// Webcam device to use (by name or index, auto-detects if not specified)
        #[arg(long, short = 'w')]
        webcam: Option<String>,

        /// Disable webcam capture
        #[arg(long)]
        no_webcam: bool,

        /// Mirror (horizontally flip) the webcam
        #[arg(long)]
        mirror: bool,

        /// Ghost overlay opacity (0.0 = invisible, 1.0 = fully visible)
        /// Default: 0.3 (or from config file)
        #[arg(long, short = 'o', value_parser = parse_opacity)]
        opacity: Option<f32>,

        /// Audio volume level (0.0 = mute, 1.0 = normal, 2.0 = double)
        /// Default: 1.0 (or from config file)
        #[arg(long, short = 'v', value_parser = parse_volume)]
        volume: Option<f32>,

        /// Disable audio capture
        #[arg(long)]
        no_audio: bool,

        /// Video effect preset to apply to webcam (none, cyberpunk, dark_mode)
        /// Default: none (or from config file)
        #[arg(long, short = 'e', value_parser = parse_effect)]
        effect: Option<VideoEffect>,

        /// Disable all video effects (overrides --effect)
        #[arg(long)]
        no_effects: bool,

        /// Enable vignette effect (subtle darkening around frame edges)
        /// Default: on when any effect is enabled, off when --no-effects is used
        #[arg(long)]
        vignette: bool,

        /// Disable vignette effect (overrides default vignette with effects)
        #[arg(long)]
        no_vignette: bool,

        /// Enable film grain effect (subtle noise texture for cinematic look)
        #[arg(long)]
        grain: bool,

        /// Enable noise gate to reduce background noise when not speaking
        #[arg(long)]
        noise_gate: bool,

        /// Noise gate threshold (0.0-1.0, default 0.01). Lower = more aggressive gating.
        #[arg(long, value_parser = parse_noise_gate_threshold)]
        noise_gate_threshold: Option<f32>,

        /// Enable compressor to even out volume levels and prevent clipping
        #[arg(long)]
        compressor: bool,

        /// Show LIVE badge overlay (red badge with white text at top-left)
        #[arg(long)]
        live: bool,

        /// Hide LIVE badge overlay (overrides --live)
        #[arg(long)]
        no_live_badge: bool,

        /// Show timestamp overlay (HH:MM:SS at top-right, updates in real-time)
        #[arg(long)]
        timestamp: bool,

        /// Hide timestamp overlay (overrides --timestamp)
        #[arg(long)]
        no_timestamp: bool,

        /// Output file path for recording (e.g., recording.mp4)
        /// When specified, records to file in addition to preview
        #[arg(long, short = 'O')]
        output: Option<String>,

        /// Output resolution (WIDTHxHEIGHT, e.g., 1920x1080)
        /// Defaults to 1280x720 if not specified
        #[arg(long, short = 'r', value_parser = parse_resolution)]
        resolution: Option<(u32, u32)>,

        /// Output framerate (1-120 fps, default: 30)
        #[arg(long, short = 'f', value_parser = parse_framerate)]
        framerate: Option<u32>,

        /// Custom config file path (default: ~/.config/space-recorder/config.toml)
        #[arg(long, short = 'c')]
        config: Option<String>,

        /// Enable fal.ai video overlay mode
        /// Type prompts during streaming to generate AI videos as overlay layers.
        /// Requires FAL_API_KEY environment variable to be set.
        #[arg(long)]
        fal: bool,

        /// AI video overlay opacity (0.0 = invisible, 1.0 = fully visible)
        /// Defaults to webcam opacity if not specified.
        /// Can be changed during stream via /opacity command.
        #[arg(long, value_parser = parse_opacity)]
        fal_opacity: Option<f32>,
    },
}

#[derive(Subcommand)]
enum FalCacheAction {
    /// List all cached videos with prompts and sizes
    List,
    /// Clear cached videos (all or by specific hash)
    Clear {
        /// Specific video hash to clear (clears all if not provided)
        hash: Option<String>,
    },
}


#[derive(Debug, Clone)]
struct Device {
    index: usize,
    name: String,
}

#[derive(Debug)]
struct DeviceList {
    video_devices: Vec<Device>,
    audio_devices: Vec<Device>,
}

/// Run ffmpeg to list available AVFoundation devices
fn list_avfoundation_devices() -> Result<DeviceList, String> {
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

/// Parse FFmpeg's device list output
fn parse_device_list(stderr: &str) -> Result<DeviceList, String> {
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

/// Parse a single device line from FFmpeg output
fn parse_device_line(line: &str) -> Option<Device> {
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

fn print_devices(devices: &DeviceList, show_video: bool, show_audio: bool) {
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

/// Display formatted startup status showing current settings
#[allow(clippy::too_many_arguments)]
fn print_startup_status(
    window_app: Option<&str>,
    screen_device: &str,
    screen_index: usize,
    webcam_device: Option<&str>,
    opacity: f32,
    effect: VideoEffect,
    vignette: bool,
    grain: bool,
    live_badge: bool,
    timestamp: bool,
    audio_device: Option<&str>,
    volume: f32,
    noise_gate: bool,
    compressor: bool,
) {
    println!();
    println!("┌─────────────────────────────────────────┐");
    println!("│         space-recorder v{}          │", env!("CARGO_PKG_VERSION"));
    println!("├─────────────────────────────────────────┤");

    // Capture source
    if let Some(app) = window_app {
        println!("│  Window:   {:<28}│", app);
    } else {
        println!("│  Screen:   {:<28}│", format!("{} (index {})", screen_device, screen_index));
    }

    // Webcam and overlay settings
    if let Some(wc) = webcam_device {
        println!("│  Webcam:   {:<28}│", wc);
        println!("│  Opacity:  {:<28}│", format!("{:.0}%", opacity * 100.0));
        println!("│  Effect:   {:<28}│", effect);
    } else {
        println!("│  Webcam:   {:<28}│", "disabled");
    }

    // Visual effects
    let mut effects_list = Vec::new();
    if vignette { effects_list.push("vignette"); }
    if grain { effects_list.push("grain"); }
    if live_badge { effects_list.push("LIVE"); }
    if timestamp { effects_list.push("timestamp"); }
    let effects_str = if effects_list.is_empty() { "none".to_string() } else { effects_list.join(", ") };
    println!("│  Effects:  {:<28}│", effects_str);

    // Audio settings
    if let Some(_audio) = audio_device {
        let mut audio_effects = Vec::new();
        if noise_gate { audio_effects.push("gate"); }
        if compressor { audio_effects.push("comp"); }
        let audio_fx = if audio_effects.is_empty() { "".to_string() } else { format!(" [{}]", audio_effects.join("+")) };
        println!("│  Audio:    {:<28}│", format!("{:.0}%{}", volume * 100.0, audio_fx));
    } else {
        println!("│  Audio:    {:<28}│", "disabled");
    }

    println!("├─────────────────────────────────────────┤");
    println!("│  HOTKEYS                                │");
    println!("│    +/=     Increase opacity             │");
    println!("│    -       Decrease opacity             │");
    println!("│    Ctrl+C  Quit                         │");
    println!("└─────────────────────────────────────────┘");
    println!();
}

/// Configuration for the capture pipeline, used to respawn with new settings
struct PipelineConfig {
    screen_capture: ScreenCapture,
    screen_device: String,
    /// Window capture settings (when --window is used)
    window_capture: Option<WindowCapture>,
    webcam_capture: WebcamCapture,
    webcam_device_name: Option<String>,
    audio_capture: AudioCapture,
    audio_device_name: Option<String>,
    /// Video effect applied to webcam stream only
    effect: VideoEffect,
    /// Whether to apply vignette effect to the composited output
    vignette: bool,
    /// Whether to apply film grain effect to the composited output
    grain: bool,
    /// Whether to show LIVE badge overlay
    live_badge: bool,
    /// Whether to show timestamp overlay
    timestamp: bool,
    /// Output resolution (width, height)
    resolution: (u32, u32),
    /// Output framerate (used by --framerate flag, wired in task 3.4.2)
    #[allow(dead_code)]
    framerate: u32,
    /// AI video overlay path (optional - for fal.ai integration)
    ai_video_path: Option<std::path::PathBuf>,
    /// AI video overlay opacity (0.0-1.0)
    ai_video_opacity: f32,
}

/// Output mode for FFmpeg pipeline
#[derive(Clone)]
enum OutputMode {
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
    fn build_filter_chain(&self, opacity: f32) -> String {
        let (width, height) = self.resolution;
        let mut filter_parts = Vec::new();

        // Get crop filter for window capture (if configured)
        let crop_filter = self.window_capture.as_ref().and_then(|wc| wc.crop_filter());

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
            let ai_output = if has_post_effects { "[composited]" } else { "[vout]" };

            filter_parts.push(screen_filter);
            filter_parts.push(format!("[1:v]{}[ghost]", webcam_chain));
            filter_parts.push(ai_video_filter);
            filter_parts.push(format!(
                "[screen][ghost]overlay=0:0:format=auto{}",
                if has_ai_video { "[pre_ai]" } else { pre_ai_output }
            ));
            filter_parts.push(format!("[pre_ai][ai]overlay=0:0:format=auto{}", ai_output));

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

            let ai_output = if has_post_effects { "[composited]" } else { "[vout]" };

            filter_parts.push(screen_filter);
            filter_parts.push(ai_video_filter);
            filter_parts.push(format!("[screen][ai]overlay=0:0:format=auto{}", ai_output));

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
                if let Some(post_filter) =
                    build_post_composition_filter(self.vignette, self.grain, self.live_badge, self.timestamp)
                {
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
                filter_parts.push(format!("[{}:a]{}[aout]", audio_input_index, audio_filter));
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
    fn build_ffmpeg_args(&self, opacity: f32, output_mode: &OutputMode) -> Vec<String> {
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
                    args.extend(["-c:a".to_string(), "aac".to_string(), "-b:a".to_string(), "128k".to_string()]);
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
                    args.extend(["-c:a".to_string(), "aac".to_string(), "-b:a".to_string(), "128k".to_string()]);
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
                    args.extend(["-c:a".to_string(), "aac".to_string(), "-b:a".to_string(), "128k".to_string()]);
                }

                // Use tee to output to both preview pipe and file
                // The file output will use re-encoding with better quality
                let tee_output = format!(
                    "[f=nut]pipe:1|[f=mp4:movflags=+faststart]{}",
                    path
                );
                args.extend(["-f".to_string(), "tee".to_string(), tee_output]);
            }
        }

        args
    }

    /// Build FFmpeg arguments for preview mode (used in tests)
    #[cfg(test)]
    fn build_ffmpeg_args_preview(&self, opacity: f32) -> Vec<String> {
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
    pub fn set_ai_video(&mut self, video_path: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
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

/// Spawn mpv for preview playback
fn spawn_mpv() -> Result<std::process::Child, PipelineError> {
    Command::new("mpv")
        .args([
            "--no-cache",
            "--untimed",
            "--no-terminal",
            "--force-seekable=no",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PipelineError::ProcessFailed {
                    exit_code: None,
                    stderr: "mpv not found. Please install it with:\n\n    brew install mpv\n"
                        .to_string(),
                }
            } else {
                PipelineError::ProcessFailed {
                    exit_code: None,
                    stderr: format!("Failed to spawn mpv: {}", e),
                }
            }
        })
}

/// Spawn the FFmpeg pipeline with the given configuration, opacity, and output mode
fn spawn_pipeline(
    config: &PipelineConfig,
    opacity: f32,
    output_mode: &OutputMode,
) -> Result<(Pipeline, Option<std::process::Child>), PipelineError> {
    let args = config.build_ffmpeg_args(opacity, output_mode);
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    match output_mode {
        OutputMode::Preview | OutputMode::Both(_) => {
            // Need to pipe to mpv for preview
            let mut mpv = spawn_mpv()?;
            let mpv_stdin = mpv.stdin.take().expect("Failed to get mpv stdin");
            let pipeline = Pipeline::spawn_with_stdout(&args_ref, mpv_stdin)?;
            Ok((pipeline, Some(mpv)))
        }
        OutputMode::Recording(_) => {
            // Recording only, no preview
            let pipeline = Pipeline::spawn(&args_ref)?;
            Ok((pipeline, None))
        }
    }
}

/// Format a CaptureError for user-friendly display
fn format_capture_error(e: &CaptureError) -> String {
    e.to_string()
}

/// Format bytes as human-readable string (KB, MB, GB)
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Run fal-cache subcommand
fn run_fal_cache(action: FalCacheAction) -> Result<(), String> {
    let cache = fal::VideoCache::with_default_dir()
        .map_err(|e| format!("Failed to access cache directory: {}", e))?;

    match action {
        FalCacheAction::List => {
            let entries = cache
                .list_entries()
                .map_err(|e| format!("Failed to list cache entries: {}", e))?;

            if entries.is_empty() {
                println!("Cache is empty.");
                return Ok(());
            }

            println!("Cached videos:\n");
            for entry in &entries {
                let prompt_display = entry
                    .prompt
                    .as_ref()
                    .map(|p| {
                        // Truncate long prompts for display
                        if p.len() > 50 {
                            format!("{}...", &p[..47])
                        } else {
                            p.clone()
                        }
                    })
                    .unwrap_or_else(|| "(no prompt data)".to_string());

                println!(
                    "  {} {} \"{}\"",
                    entry.hash,
                    format_size(entry.size_bytes),
                    prompt_display
                );
            }

            let total_size = cache
                .total_size_bytes()
                .map_err(|e| format!("Failed to calculate total size: {}", e))?;
            println!("\nTotal: {} videos, {}", entries.len(), format_size(total_size));

            Ok(())
        }
        FalCacheAction::Clear { hash } => {
            match hash {
                Some(h) => {
                    // Clear specific video by hash
                    let removed = cache
                        .remove(&h)
                        .map_err(|e| format!("Failed to remove cached video: {}", e))?;

                    if removed {
                        println!("Removed cached video: {}", h);
                    } else {
                        println!("No cached video found with hash: {}", h);
                    }
                }
                None => {
                    // Clear all cached videos
                    let count = cache
                        .clear_all()
                        .map_err(|e| format!("Failed to clear cache: {}", e))?;

                    if count == 0 {
                        println!("Cache is already empty.");
                    } else {
                        println!("Removed {} cached video{}.", count, if count == 1 { "" } else { "s" });
                    }
                }
            }
            Ok(())
        }
    }
}

/// Run fal-generate command to pre-generate AI video from a text prompt
fn run_fal_generate(prompt: &str) -> Result<(), String> {
    // Check if already cached
    let cache = fal::VideoCache::with_default_dir_initialized()
        .map_err(|e| format!("Failed to initialize cache: {}", e))?;

    if let Some(cached_path) = cache.get(prompt) {
        println!("Found in cache: {}", cached_path.display());
        println!("Hash: {}", fal::VideoCache::hash_prompt(prompt));
        return Ok(());
    }

    // Create the async runtime and run the generation
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create async runtime: {}", e))?;

    rt.block_on(async {
        // Create the fal client (checks for API key)
        let client = fal::FalClient::new().map_err(|e| match e {
            fal::FalError::MissingApiKey => {
                "FAL_API_KEY environment variable is not set.\n\n\
                To use fal.ai features, add your API key to a .env file:\n\
                    echo 'FAL_API_KEY=your-api-key-here' >> .env\n\n\
                Or set it as an environment variable:\n\
                    export FAL_API_KEY=\"your-api-key-here\"\n\n\
                Get your API key at: https://fal.ai/".to_string()
            }
            _ => format!("Failed to create fal.ai client: {}", e),
        })?;

        println!("Generating video for: \"{}\"", prompt);
        println!();

        // Step 1: Submit generation request
        print!("Submitting to fal.ai... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let queue_response = client.submit_generation(prompt).await.map_err(|e| {
            format!("Failed to submit generation request: {}", e)
        })?;
        println!("done");
        println!("  Request ID: {}", queue_response.request_id);

        // Step 2: Poll for completion
        print!("Generating");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let video_url = loop {
            let status = client.poll_status(&queue_response.request_id).await.map_err(|e| {
                format!("\nFailed to check generation status: {}", e)
            })?;

            match status {
                fal::GenerationStatus::Pending => {
                    print!(".");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
                fal::GenerationStatus::InProgress => {
                    print!(".");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
                fal::GenerationStatus::Completed { video_url } => {
                    println!(" done");
                    break video_url;
                }
                fal::GenerationStatus::Failed { error } => {
                    return Err(format!("\nGeneration failed: {}", error));
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        };

        // Step 3: Download the video
        print!("Downloading video... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let temp_path = std::env::temp_dir()
            .join("space-recorder")
            .join(format!("{}.mp4", queue_response.request_id));

        client.download_video(&video_url, &temp_path).await.map_err(|e| {
            format!("Failed to download video: {}", e)
        })?;
        println!("done");

        // Step 4: Store in cache
        print!("Caching video... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let cached_path = cache.store_with_metadata(prompt, &temp_path)
            .map_err(|e| format!("Failed to cache video: {}", e))?;
        println!("done");

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        println!();
        println!("Video ready!");
        println!("  Path: {}", cached_path.display());
        println!("  Hash: {}", fal::VideoCache::hash_prompt(prompt));

        Ok(())
    })
}

/// Run fal-generate --batch command to pre-generate AI videos from a file of prompts
fn run_fal_generate_batch(batch_file: &std::path::Path) -> Result<(), String> {
    // Read prompts from file
    let contents = std::fs::read_to_string(batch_file)
        .map_err(|e| format!("Failed to read batch file '{}': {}", batch_file.display(), e))?;

    // Parse prompts (one per line, skip empty lines and comments)
    let prompts: Vec<&str> = contents
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    if prompts.is_empty() {
        return Err(format!(
            "No prompts found in batch file '{}'. Expected one prompt per line.",
            batch_file.display()
        ));
    }

    println!("Batch generation: {} prompts from '{}'", prompts.len(), batch_file.display());
    println!();

    // Initialize cache once for all generations
    let cache = fal::VideoCache::with_default_dir_initialized()
        .map_err(|e| format!("Failed to initialize cache: {}", e))?;

    // Track results
    let mut completed = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let total = prompts.len();

    // Create async runtime once
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create async runtime: {}", e))?;

    // Process each prompt sequentially
    for (i, prompt) in prompts.iter().enumerate() {
        let progress = format!("[{}/{}]", i + 1, total);

        // Check if already cached
        if cache.get(prompt).is_some() {
            println!("{} Skipped (cached): \"{}\"", progress, prompt);
            skipped += 1;
            continue;
        }

        println!("{} Generating: \"{}\"", progress, prompt);

        // Generate this prompt
        let result = rt.block_on(async {
            generate_single_video(&cache, prompt).await
        });

        match result {
            Ok(path) => {
                println!("    Cached: {}", path.display());
                completed += 1;
            }
            Err(e) => {
                eprintln!("    Failed: {}", e);
                failed += 1;
            }
        }
    }

    // Summary
    println!();
    println!("Batch complete:");
    println!("  Generated: {}", completed);
    println!("  Skipped (cached): {}", skipped);
    if failed > 0 {
        println!("  Failed: {}", failed);
    }

    if failed > 0 && completed == 0 && skipped == 0 {
        Err("All prompts failed to generate".to_string())
    } else {
        Ok(())
    }
}

/// Generate a single video and cache it (helper for batch processing)
async fn generate_single_video(cache: &fal::VideoCache, prompt: &str) -> Result<std::path::PathBuf, String> {
    // Create the fal client
    let client = fal::FalClient::new().map_err(|e| match e {
        fal::FalError::MissingApiKey => {
            "FAL_API_KEY environment variable is not set".to_string()
        }
        _ => format!("Failed to create fal.ai client: {}", e),
    })?;

    // Submit generation request
    let queue_response = client.submit_generation(prompt).await.map_err(|e| {
        format!("Submit failed: {}", e)
    })?;

    // Poll for completion
    let video_url = loop {
        let status = client.poll_status(&queue_response.request_id).await.map_err(|e| {
            format!("Poll failed: {}", e)
        })?;

        match status {
            fal::GenerationStatus::Pending | fal::GenerationStatus::InProgress => {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            fal::GenerationStatus::Completed { video_url } => {
                break video_url;
            }
            fal::GenerationStatus::Failed { error } => {
                return Err(format!("Generation failed: {}", error));
            }
        }
    };

    // Download the video
    let temp_path = std::env::temp_dir()
        .join("space-recorder")
        .join(format!("{}.mp4", queue_response.request_id));

    client.download_video(&video_url, &temp_path).await.map_err(|e| {
        format!("Download failed: {}", e)
    })?;

    // Store in cache
    let cached_path = cache.store_with_metadata(prompt, &temp_path)
        .map_err(|e| format!("Cache failed: {}", e))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);

    Ok(cached_path)
}

/// Run the start command with screen, webcam, and audio capture
#[allow(clippy::too_many_arguments)] // Direct mapping from CLI args
fn run_start(
    window_app: Option<String>,
    screen_index: Option<usize>,
    webcam_device: Option<String>,
    no_webcam: bool,
    mirror: bool,
    opacity: f32,
    volume: f32,
    no_audio: bool,
    effect: VideoEffect,
    vignette: bool,
    grain: bool,
    noise_gate: bool,
    noise_gate_threshold: f32,
    compressor: bool,
    live_badge: bool,
    timestamp: bool,
    output_file: Option<String>,
    resolution: (u32, u32),
    framerate: u32,
    fal_enabled: bool,
    fal_opacity: f32,
) -> Result<(), PipelineError> {
    // Verify macOS permissions before capture starts
    let need_webcam = !no_webcam;
    let need_audio = !no_audio;
    let need_accessibility = window_app.is_some();

    let permission_errors = permissions::verify_permissions(
        true, // Always need screen recording
        need_webcam,
        need_audio,
        need_accessibility,
    );

    if !permission_errors.is_empty() {
        permissions::print_permission_errors(&permission_errors);
        return Err(PipelineError::ProcessFailed {
            exit_code: None,
            stderr: format!(
                "{} permission(s) missing. Please grant the required permissions and try again.",
                permission_errors.len()
            ),
        });
    }

    // Set up Ctrl+C handler
    if let Err(e) = setup_ctrlc_handler() {
        eprintln!("Warning: Could not set up Ctrl+C handler: {}", e);
    }

    // Set up hotkey manager for opacity control
    let mut hotkey_manager = HotkeyManager::new(opacity);
    let hotkeys_enabled = hotkey_manager.start().is_ok();
    if !hotkeys_enabled {
        eprintln!("Warning: Could not start hotkey listener. Opacity hotkeys (+/-) will not work.");
        eprintln!("On macOS, ensure Accessibility permission is granted.\n");
    }

    // Configure screen capture
    let screen_capture = ScreenCapture::new(screen_index.unwrap_or(0))
        .with_framerate(framerate);

    // Find the screen device
    let screen_device = screen_capture.find_screen_device().map_err(|e| {
        PipelineError::ProcessFailed {
            exit_code: None,
            stderr: e.to_string(),
        }
    })?;

    // Configure window capture (if --window is specified)
    let window_capture = if let Some(ref app_name) = window_app {
        let mut wc = WindowCapture::new(app_name);
        match wc.detect_bounds() {
            Ok(_bounds) => Some(wc),
            Err(e) => {
                return Err(PipelineError::ProcessFailed {
                    exit_code: None,
                    stderr: format_capture_error(&e),
                });
            }
        }
    } else {
        None
    };

    // Configure webcam capture
    let webcam_capture = if no_webcam {
        WebcamCapture::disabled()
    } else {
        let mut wc = WebcamCapture::new()
            .with_mirror(mirror)
            .with_framerate(framerate);
        if let Some(device) = webcam_device {
            wc = wc.with_device(device);
        }
        wc
    };

    // Find webcam device if enabled
    let webcam_device_name = if webcam_capture.enabled {
        match webcam_capture.find_webcam_device() {
            Ok(name) => Some(name),
            Err(e) => {
                eprintln!("Warning: {}", e);
                eprintln!("Continuing without webcam.\n");
                None
            }
        }
    } else {
        None
    };

    // Configure audio capture
    let audio_capture = if no_audio {
        AudioCapture::disabled()
    } else {
        let mut ac = AudioCapture::new().with_volume(volume);
        if noise_gate {
            ac = ac.with_noise_gate_threshold(noise_gate_threshold);
        }
        if compressor {
            ac = ac.with_compressor();
        }
        ac
    };

    // Find audio device if enabled
    let audio_device_name = if audio_capture.enabled {
        match audio_capture.find_audio_device() {
            Ok(name) => Some(name),
            Err(e) => {
                eprintln!("Warning: {}", e);
                eprintln!("Continuing without audio.\n");
                None
            }
        }
    } else {
        None
    };

    // Display status with current settings
    print_startup_status(
        window_app.as_deref(),
        &screen_device,
        screen_capture.screen_index,
        webcam_device_name.as_deref(),
        opacity,
        effect,
        vignette,
        grain,
        live_badge,
        timestamp,
        audio_device_name.as_deref(),
        audio_capture.volume,
        audio_capture.noise_gate.enabled,
        audio_capture.compressor.enabled,
    );

    // Create pipeline configuration for respawning
    let config = PipelineConfig {
        screen_capture,
        screen_device,
        window_capture,
        webcam_capture,
        webcam_device_name,
        audio_capture,
        audio_device_name,
        effect,
        vignette,
        grain,
        live_badge,
        timestamp,
        resolution,
        framerate,
        ai_video_path: None,      // AI video will be set when fal.ai integration is enabled via --fal
        ai_video_opacity: fal_opacity,
    };

    // Determine output mode
    let output_mode = match output_file {
        Some(path) => OutputMode::Both(path.clone()),
        None => OutputMode::Preview,
    };

    // Track current opacity for restarts
    let mut current_opacity = opacity;

    // Start fal.ai prompt input listener if --fal is enabled
    let prompt_receiver = if fal_enabled {
        let (_prompt_input, receiver) = fal::PromptInput::spawn_listener();
        Some(receiver)
    } else {
        None
    };

    // Spawn initial pipeline
    let (mut pipeline, mut mpv) = spawn_pipeline(&config, current_opacity, &output_mode)?;

    let has_preview = mpv.is_some();
    if has_preview {
        println!("Streaming... (preview window opened)");
    } else {
        println!("Recording...");
    }
    if let OutputMode::Both(ref path) = output_mode {
        println!("Recording to: {}", path);
    } else if let OutputMode::Recording(ref path) = output_mode {
        println!("Recording to: {}", path);
    }

    // Log fal.ai instructions if enabled
    if fal_enabled {
        println!();
        println!("fal.ai overlay mode enabled. Type prompts to generate AI videos:");
        println!("  - Enter a prompt to generate video (e.g., \"cyberpunk cityscape\")");
        println!("  - /clear    - Remove current AI overlay");
        println!("  - /opacity <value> - Set AI overlay opacity (0.0-1.0)");
        println!();
    }

    // Wait for either process to exit, Ctrl+C, or opacity change
    loop {
        // Check if Ctrl+C was received
        if pipeline::ctrlc_received() {
            println!("\nShutting down...");
            pipeline.shutdown()?;
            if let Some(ref mut mpv_proc) = mpv {
                let _ = mpv_proc.kill();
            }
            break;
        }

        // Check if opacity changed (hotkey pressed)
        if hotkey_manager.take_opacity_changed() {
            let new_opacity = hotkey_manager.opacity();

            // Only restart if opacity actually changed
            if (new_opacity - current_opacity).abs() > 0.001 {
                eprintln!("[restart] Restarting pipeline with opacity {:.0}%...", new_opacity * 100.0);

                // Measure restart time
                let restart_start = std::time::Instant::now();

                // Shut down current pipeline
                let _ = pipeline.shutdown();
                if let Some(ref mut mpv_proc) = mpv {
                    let _ = mpv_proc.kill();
                    let _ = mpv_proc.wait(); // Ensure mpv is fully stopped
                }

                // Spawn new pipeline with updated opacity
                match spawn_pipeline(&config, new_opacity, &output_mode) {
                    Ok((new_pipeline, new_mpv)) => {
                        pipeline = new_pipeline;
                        mpv = new_mpv;
                        current_opacity = new_opacity;

                        let restart_duration = restart_start.elapsed();
                        eprintln!("[restart] Pipeline restarted in {:?}", restart_duration);
                    }
                    Err(e) => {
                        eprintln!("[restart] Failed to restart pipeline: {}", e);
                        // Try to recover with original opacity
                        match spawn_pipeline(&config, current_opacity, &output_mode) {
                            Ok((new_pipeline, new_mpv)) => {
                                pipeline = new_pipeline;
                                mpv = new_mpv;
                                eprintln!("[restart] Recovered with previous opacity");
                            }
                            Err(e2) => {
                                eprintln!("[restart] Recovery failed: {}", e2);
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }

        // Check for fal.ai prompt commands (non-blocking)
        if let Some(ref receiver) = prompt_receiver {
            // Non-blocking check for prompt commands
            while let Ok(cmd) = receiver.try_recv() {
                match cmd {
                    fal::PromptCommand::Generate(prompt) => {
                        // Log that we received the prompt (actual video generation will be implemented in later tasks)
                        fal::PromptInput::print_generating(&prompt);
                        eprintln!("[fal] Video generation not yet fully integrated. Prompt received: \"{}\"", prompt);
                    }
                    fal::PromptCommand::Clear => {
                        fal::PromptInput::print_overlay_cleared();
                        eprintln!("[fal] Clear overlay requested (full integration coming in later tasks)");
                    }
                    fal::PromptCommand::SetOpacity(value) => {
                        fal::PromptInput::print_opacity_set(value);
                        eprintln!("[fal] AI overlay opacity set to {:.0}% (full integration coming in later tasks)", value * 100.0);
                    }
                }
            }
        }

        // Check if FFmpeg is still running
        if !pipeline.is_running() {
            let status = pipeline.wait()?;
            if let Some(ref mut mpv_proc) = mpv {
                let _ = mpv_proc.kill();
            }
            if !status.success() {
                let stderr_lines = pipeline.take_stderr_output();
                return Err(PipelineError::ProcessFailed {
                    exit_code: status.code(),
                    stderr: stderr_lines.join("\n"),
                });
            }
            break;
        }

        // Check if mpv is still running (when in preview mode)
        if let Some(ref mut mpv_proc) = mpv {
            match mpv_proc.try_wait() {
                Ok(Some(_)) => {
                    // mpv exited, shut down FFmpeg
                    println!("\nPreview window closed.");
                    pipeline.shutdown()?;
                    break;
                }
                Ok(None) => {} // Still running
                Err(_) => break,
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("Capture stopped.");
    Ok(())
}

/// Load .env file and check for FAL_API_KEY
///
/// Loads environment variables from .env file in the project root.
/// Does not override existing environment variables.
/// Logs a warning if FAL_API_KEY is not set.
fn load_env() {
    // Load .env file, don't override existing env vars
    // dotenv::dotenv() returns Err if .env doesn't exist, which is fine
    let _ = dotenv::dotenv();

    // Check for FAL_API_KEY and warn if not set
    if std::env::var("FAL_API_KEY").is_err() {
        eprintln!("Warning: FAL_API_KEY environment variable not set.");
        eprintln!("         fal.ai video overlay features will be disabled.");
        eprintln!("         Set FAL_API_KEY in .env or environment to enable.\n");
    }
}

fn main() {
    // Load .env file before anything else
    load_env();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::ListDevices { video, audio }) => {
            match list_avfoundation_devices() {
                Ok(devices) => print_devices(&devices, video, audio),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::FalCache { action }) => {
            if let Err(e) = run_fal_cache(action) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::FalGenerate { prompt, batch }) => {
            let result = if let Some(batch_file) = batch {
                run_fal_generate_batch(&batch_file)
            } else if let Some(p) = prompt {
                run_fal_generate(&p)
            } else {
                Err("Either a prompt or --batch file must be provided".to_string())
            };
            if let Err(e) = result {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Start { window, screen, webcam, no_webcam, mirror, opacity, volume, no_audio, effect, no_effects, vignette, no_vignette, grain, noise_gate, noise_gate_threshold, compressor, live, no_live_badge, timestamp, no_timestamp, output, resolution, framerate, config: config_path, fal, fal_opacity }) => {
            // Load config file
            // If --config is specified, require the file to exist
            // Otherwise, fall back to defaults if default config not found
            let cfg = if let Some(ref path) = config_path {
                let path = std::path::PathBuf::from(path);
                match config::Config::load_from_explicit(path.clone()) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match config::Config::load() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Warning: Failed to load config file: {}", e);
                        eprintln!("Using default settings.\n");
                        config::Config::default()
                    }
                }
            };

            // Merge settings: CLI args > config file > built-in defaults
            // Window: CLI > config
            let window = window.or(cfg.capture.window.app_name);

            // Screen: CLI > config
            let screen = screen.or(cfg.capture.screen.device);

            // Webcam settings
            // no_webcam CLI flag takes precedence, otherwise check config
            let no_webcam = no_webcam || cfg.capture.webcam.enabled.map(|e| !e).unwrap_or(false);
            let mirror = mirror || cfg.capture.webcam.mirror.unwrap_or(false);
            let webcam = webcam.or(cfg.capture.webcam.device);

            // Opacity: CLI > config > default (0.3)
            let opacity = opacity
                .or(cfg.compositor.opacity)
                .unwrap_or(0.3);

            // Effect: CLI > config > default (none)
            let effect = effect
                .or_else(|| {
                    cfg.effects.preset.as_ref().and_then(|p| VideoEffect::from_str(p))
                })
                .unwrap_or(VideoEffect::None);

            // --no-effects overrides --effect
            let effect = if no_effects { VideoEffect::None } else { effect };

            // Vignette: CLI flag > config > default (on when effects are enabled)
            let vignette = if no_vignette {
                false
            } else if vignette {
                true
            } else if let Some(v) = cfg.effects.vignette {
                v
            } else {
                // Default: on when any effect is enabled (not none and not --no-effects)
                effect != VideoEffect::None
            };

            // Grain: CLI flag > config > default (false)
            let grain = grain || cfg.effects.grain.unwrap_or(false);

            // LIVE badge: CLI flags > config > default (false)
            // --no-live-badge takes precedence over --live and config
            let live_badge = if no_live_badge {
                false
            } else if live {
                true
            } else {
                cfg.effects.overlays.live_badge.unwrap_or(false)
            };

            // Timestamp: CLI flags > config > default (false)
            // --no-timestamp takes precedence over --timestamp and config
            let timestamp = if no_timestamp {
                false
            } else if timestamp {
                true
            } else {
                cfg.effects.overlays.timestamp.unwrap_or(false)
            };

            // Audio settings
            // no_audio CLI flag takes precedence, otherwise check config
            let no_audio = no_audio || cfg.audio.enabled.map(|e| !e).unwrap_or(false);

            // Volume: CLI > config > default (1.0)
            let volume = volume
                .or(cfg.audio.volume)
                .unwrap_or(1.0);

            // Noise gate: CLI flag > config > default (false)
            let noise_gate = noise_gate || cfg.audio.processing.noise_gate.unwrap_or(false);

            // Noise gate threshold: CLI > config > default (0.01)
            let noise_gate_threshold = noise_gate_threshold
                .or(cfg.audio.processing.noise_gate_threshold)
                .unwrap_or(0.01);

            // Compressor: CLI flag > config > default (false)
            let compressor = compressor || cfg.audio.processing.compressor.unwrap_or(false);

            // Resolution: CLI > config > default (1280x720)
            let resolution = resolution
                .or_else(|| cfg.output.resolution.map(|r| (r[0], r[1])))
                .unwrap_or((1280, 720));

            // Framerate: CLI > config > default (30)
            let framerate = framerate
                .or(cfg.output.framerate)
                .unwrap_or(30);

            // Check FAL_API_KEY if --fal is enabled
            // If API key is missing, warn and continue with fal features disabled
            let fal = if fal && std::env::var("FAL_API_KEY").is_err() {
                eprintln!("Warning: FAL_API_KEY environment variable not set.");
                eprintln!("         fal.ai video overlay features will be disabled.");
                eprintln!();
                eprintln!("To enable fal.ai video overlay, add your API key to a .env file:");
                eprintln!("  echo 'FAL_API_KEY=your-api-key-here' >> .env");
                eprintln!();
                eprintln!("Or set it as an environment variable:");
                eprintln!("  export FAL_API_KEY=\"your-api-key-here\"");
                eprintln!();
                eprintln!("Get your API key at: https://fal.ai/");
                eprintln!();
                false // Disable fal features but continue
            } else {
                fal
            };

            // fal_opacity: CLI > webcam opacity (defaults to same as webcam opacity)
            let fal_opacity = fal_opacity.unwrap_or(opacity);

            if let Err(e) = run_start(window, screen, webcam, no_webcam, mirror, opacity, volume, no_audio, effect, vignette, grain, noise_gate, noise_gate_threshold, compressor, live_badge, timestamp, output, resolution, framerate, fal, fal_opacity) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            // Show brief help when no command is provided
            println!("space-recorder {}", env!("CARGO_PKG_VERSION"));
            println!("Video compositor for coding streams\n");
            println!("USAGE:");
            println!("    space-recorder <COMMAND>\n");
            println!("COMMANDS:");
            println!("    start         Start the compositor and preview in mpv");
            println!("    list-devices  List available video and audio capture devices");
            println!("    fal-cache     Manage fal.ai video cache");
            println!("    help          Print this message or the help of a subcommand\n");
            println!("Run 'space-recorder --help' for more details and examples.");
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

    // Opacity parsing tests

    #[test]
    fn test_parse_opacity_valid() {
        assert_eq!(parse_opacity("0.3").unwrap(), 0.3);
        assert_eq!(parse_opacity("0.0").unwrap(), 0.0);
        assert_eq!(parse_opacity("1.0").unwrap(), 1.0);
        assert_eq!(parse_opacity("0.5").unwrap(), 0.5);
    }

    #[test]
    fn test_parse_opacity_boundaries() {
        // At boundaries should work
        assert!(parse_opacity("0.0").is_ok());
        assert!(parse_opacity("1.0").is_ok());
        // Just outside boundaries should fail
        assert!(parse_opacity("-0.1").is_err());
        assert!(parse_opacity("1.1").is_err());
    }

    #[test]
    fn test_parse_opacity_invalid_input() {
        assert!(parse_opacity("not_a_number").is_err());
        assert!(parse_opacity("").is_err());
        assert!(parse_opacity("abc").is_err());
    }

    #[test]
    fn test_parse_opacity_out_of_range() {
        let err = parse_opacity("2.0").unwrap_err();
        assert!(err.contains("must be between 0.0 and 1.0"));
        assert!(err.contains("2"));
    }

    // Noise gate threshold parsing tests

    #[test]
    fn test_parse_noise_gate_threshold_valid() {
        assert_eq!(parse_noise_gate_threshold("0.01").unwrap(), 0.01);
        assert_eq!(parse_noise_gate_threshold("0.0").unwrap(), 0.0);
        assert_eq!(parse_noise_gate_threshold("1.0").unwrap(), 1.0);
        assert_eq!(parse_noise_gate_threshold("0.05").unwrap(), 0.05);
    }

    #[test]
    fn test_parse_noise_gate_threshold_boundaries() {
        // At boundaries should work
        assert!(parse_noise_gate_threshold("0.0").is_ok());
        assert!(parse_noise_gate_threshold("1.0").is_ok());
        // Just outside boundaries should fail
        assert!(parse_noise_gate_threshold("-0.1").is_err());
        assert!(parse_noise_gate_threshold("1.1").is_err());
    }

    #[test]
    fn test_parse_noise_gate_threshold_invalid_input() {
        assert!(parse_noise_gate_threshold("not_a_number").is_err());
        assert!(parse_noise_gate_threshold("").is_err());
        assert!(parse_noise_gate_threshold("abc").is_err());
    }

    #[test]
    fn test_parse_noise_gate_threshold_out_of_range() {
        let err = parse_noise_gate_threshold("2.0").unwrap_err();
        assert!(err.contains("must be between 0.0 and 1.0"));
        assert!(err.contains("2"));
    }

    // Ghost overlay filter graph tests

    #[test]
    fn test_build_ghost_overlay_filter_default() {
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::None, false, false, false, false, 1280, 720);
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
        let filter = build_ghost_overlay_filter(0.3, true, VideoEffect::None, false, false, false, false, 1280, 720);
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
        let filter_zero = build_ghost_overlay_filter(0.0, false, VideoEffect::None, false, false, false, false, 1280, 720);
        assert!(filter_zero.contains("colorchannelmixer=aa=0.00"));

        let filter_full = build_ghost_overlay_filter(1.0, false, VideoEffect::None, false, false, false, false, 1280, 720);
        assert!(filter_full.contains("colorchannelmixer=aa=1.00"));

        let filter_half = build_ghost_overlay_filter(0.5, false, VideoEffect::None, false, false, false, false, 1280, 720);
        assert!(filter_half.contains("colorchannelmixer=aa=0.50"));
    }

    #[test]
    fn test_build_ghost_overlay_filter_structure() {
        // Test that the filter has the correct structure (no vignette or grain)
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::None, false, false, false, false, 1280, 720);

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
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::Cyberpunk, false, false, false, false, 1280, 720);

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
        let filter = build_ghost_overlay_filter(0.5, false, VideoEffect::Cyberpunk, false, false, false, false, 1280, 720);

        // In the webcam chain, effect filters should come before format=rgba
        let webcam_chain_start = filter.find("[1:v]").unwrap();
        let effect_pos = filter.find("saturation=").unwrap();
        let rgba_pos = filter.find("format=rgba").unwrap();
        let alpha_pos = filter.find("colorchannelmixer=aa=").unwrap();

        assert!(webcam_chain_start < effect_pos, "Effect should be in webcam chain");
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
        let filter_idx_high = args_high.iter().position(|a| a == "-filter_complex").unwrap();

        // Filters should be different (different opacity)
        assert_ne!(args_low[filter_idx_low + 1], args_high[filter_idx_high + 1]);

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
        assert!(!filter.contains("curves="), "No color curves with no effects");
        assert!(!filter.contains("saturation=1.4"), "No saturation boost with no effects");
        assert!(!filter.contains("colorbalance="), "No color balance with no effects");
        assert!(!filter.contains("brightness="), "No brightness adjustment with no effects");

        // Ghost overlay should still work
        assert!(filter.contains("[screen][ghost]overlay"), "Ghost overlay still active");
        assert!(filter.contains("colorchannelmixer=aa=0.30"), "Alpha still applied");
        assert!(filter.contains("format=rgba"), "RGBA conversion still present");
    }

    #[test]
    fn test_no_effects_overrides_effect() {
        // Simulate the behavior in main: --no-effects should override --effect
        let original_effect = VideoEffect::Cyberpunk;
        let no_effects = true;

        // This mirrors the logic in main()
        let final_effect = if no_effects { VideoEffect::None } else { original_effect };

        assert_eq!(final_effect, VideoEffect::None);
    }

    // Vignette-specific tests

    #[test]
    fn test_build_ghost_overlay_filter_with_vignette() {
        // Test with vignette enabled (no grain)
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::None, true, false, false, false, 1280, 720);

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
        assert!(parts[2].contains("[composited]"), "Overlay should output to [composited]");

        // Vignette should take [composited] and output [vout]
        assert!(parts[3].contains("[composited]"), "Vignette should read from [composited]");
        assert!(parts[3].contains("[vout]"), "Vignette should output to [vout]");
        assert!(parts[3].contains("vignette=PI/5"), "Should apply vignette filter");
    }

    #[test]
    fn test_build_ghost_overlay_filter_vignette_order() {
        // Vignette should be applied AFTER overlay/compositing
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::Cyberpunk, true, false, false, false, 1280, 720);

        // Verify order: overlay comes before vignette
        let overlay_pos = filter.find("overlay=").unwrap();
        let vignette_pos = filter.find("vignette=").unwrap();
        assert!(overlay_pos < vignette_pos, "Overlay should come before vignette");

        // Color grading (effect) should come before overlay
        let effect_pos = filter.find("saturation=").unwrap();
        assert!(effect_pos < overlay_pos, "Color grading should come before overlay");
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

    #[test]
    fn test_vignette_default_logic() {
        // Test the vignette defaulting logic (mirrors main() behavior)

        // When effect is None, no_vignette=false, no explicit vignette -> false
        let effect = VideoEffect::None;
        let no_vignette = false;
        let explicit_vignette = false;
        let vignette = if no_vignette { false } else if explicit_vignette { true } else { effect != VideoEffect::None };
        assert!(!vignette, "Vignette should be off when effect is None");

        // When effect is Cyberpunk, no_vignette=false, no explicit vignette -> true
        let effect = VideoEffect::Cyberpunk;
        let vignette = if no_vignette { false } else if explicit_vignette { true } else { effect != VideoEffect::None };
        assert!(vignette, "Vignette should be on when effect is Cyberpunk");

        // When no_vignette=true, even with effect -> false
        let no_vignette = true;
        let vignette = if no_vignette { false } else if explicit_vignette { true } else { effect != VideoEffect::None };
        assert!(!vignette, "Vignette should be off when --no-vignette is set");

        // When explicit vignette=true, even without effect -> true
        let no_vignette = false;
        let explicit_vignette = true;
        let effect = VideoEffect::None;
        let vignette = if no_vignette { false } else if explicit_vignette { true } else { effect != VideoEffect::None };
        assert!(vignette, "Vignette should be on when explicitly enabled");
    }

    // Film grain tests

    #[test]
    fn test_build_ghost_overlay_filter_with_grain() {
        // Test with grain enabled (no vignette)
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::None, false, true, false, false, 1280, 720);

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
        assert!(parts[2].contains("[composited]"), "Overlay should output to [composited]");

        // Grain should take [composited] and output [vout]
        assert!(parts[3].contains("[composited]"), "Grain should read from [composited]");
        assert!(parts[3].contains("[vout]"), "Grain should output to [vout]");
        assert!(parts[3].contains("noise=alls=10:allf=t"), "Should apply grain filter");
    }

    #[test]
    fn test_build_ghost_overlay_filter_with_vignette_and_grain() {
        // Test with both vignette and grain enabled
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::None, true, true, false, false, 1280, 720);

        // Should contain both filters
        assert!(filter.contains("vignette=PI/5"));
        assert!(filter.contains("noise=alls=10:allf=t"));

        // Should have 4 parts (vignette and grain are combined in post-comp)
        let parts: Vec<&str> = filter.split(';').collect();
        assert_eq!(parts.len(), 4, "Should have 4 parts with both effects enabled");

        // Vignette should come before grain
        let vignette_pos = filter.find("vignette=").unwrap();
        let grain_pos = filter.find("noise=").unwrap();
        assert!(vignette_pos < grain_pos, "Vignette should be applied before grain");
    }

    #[test]
    fn test_build_ghost_overlay_filter_grain_order() {
        // Grain should be applied AFTER overlay/compositing and color grading
        let filter = build_ghost_overlay_filter(0.3, false, VideoEffect::Cyberpunk, false, true, false, false, 1280, 720);

        // Verify order: color grading -> overlay -> grain
        let effect_pos = filter.find("saturation=").unwrap();
        let overlay_pos = filter.find("overlay=").unwrap();
        let grain_pos = filter.find("noise=").unwrap();

        assert!(effect_pos < overlay_pos, "Color grading should come before overlay");
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

    // Overlay toggle tests

    #[test]
    fn test_no_live_badge_overrides_live() {
        // Simulate the behavior in main(): --no-live-badge overrides --live
        let live = true;
        let no_live_badge = true;

        // This mirrors the logic in main()
        let live_badge = live && !no_live_badge;

        assert!(!live_badge, "--no-live-badge should disable LIVE badge");
    }

    #[test]
    fn test_live_badge_enabled_when_live_set() {
        // --live enables it, no --no-live-badge
        let live = true;
        let no_live_badge = false;

        let live_badge = live && !no_live_badge;

        assert!(live_badge, "LIVE badge should be enabled when --live is set");
    }

    #[test]
    fn test_live_badge_disabled_by_default() {
        // Neither --live nor --no-live-badge
        let live = false;
        let no_live_badge = false;

        let live_badge = live && !no_live_badge;

        assert!(!live_badge, "LIVE badge should be off by default");
    }

    #[test]
    fn test_no_timestamp_overrides_timestamp() {
        // Simulate the behavior in main(): --no-timestamp overrides --timestamp
        let timestamp_flag = true;
        let no_timestamp = true;

        // This mirrors the logic in main()
        let timestamp = timestamp_flag && !no_timestamp;

        assert!(!timestamp, "--no-timestamp should disable timestamp");
    }

    #[test]
    fn test_timestamp_enabled_when_flag_set() {
        // --timestamp enables it, no --no-timestamp
        let timestamp_flag = true;
        let no_timestamp = false;

        let timestamp = timestamp_flag && !no_timestamp;

        assert!(timestamp, "Timestamp should be enabled when --timestamp is set");
    }

    #[test]
    fn test_timestamp_disabled_by_default() {
        // Neither --timestamp nor --no-timestamp
        let timestamp_flag = false;
        let no_timestamp = false;

        let timestamp = timestamp_flag && !no_timestamp;

        assert!(!timestamp, "Timestamp should be off by default");
    }

    #[test]
    fn test_overlay_toggles_independent() {
        // Test that LIVE badge and timestamp toggles work independently
        // --live enabled, --no-timestamp enabled
        let live = true;
        let no_live_badge = false;
        let timestamp_flag = true;
        let no_timestamp = true;

        let live_badge = live && !no_live_badge;
        let timestamp = timestamp_flag && !no_timestamp;

        assert!(live_badge, "LIVE badge should be enabled");
        assert!(!timestamp, "Timestamp should be disabled by --no-timestamp");
    }

    // Window capture tests

    #[test]
    fn test_pipeline_config_with_window_capture_screen_only() {
        use capture::WindowBounds;
        // Test window capture with screen only (no webcam)
        let mut window_capture = capture::WindowCapture::new("Terminal");
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
        assert!(filter.contains("crop="), "Should contain crop filter for window capture");
        assert!(filter.contains("scale=1280:720"), "Should scale output");
        // Crop should come before scale in the filter chain
        let crop_pos = filter.find("crop=").unwrap();
        let scale_pos = filter.find("scale=").unwrap();
        assert!(crop_pos < scale_pos, "Crop should come before scale");
    }

    #[test]
    fn test_pipeline_config_with_window_capture_and_webcam() {
        use capture::WindowBounds;
        // Test window capture with webcam overlay
        let mut window_capture = capture::WindowCapture::new("Terminal");
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
        assert!(filter.contains("crop="), "Should contain crop filter for window capture");
        // Should have ghost overlay
        assert!(filter.contains("[screen][ghost]overlay"), "Should have ghost overlay");
        // Crop should come before scale in the screen filter
        let crop_pos = filter.find("crop=").unwrap();
        let scale_pos = filter.find("scale=1280:720[screen]").unwrap();
        assert!(crop_pos < scale_pos, "Crop should come before scale in screen chain");
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
        assert!(!filter.contains("crop="), "Should not contain crop filter without window capture");
    }

    #[test]
    fn test_pipeline_config_window_capture_without_bounds() {
        // WindowCapture without detected bounds should not produce crop filter
        let window_capture = capture::WindowCapture::new("Terminal");
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
        assert!(!filter.contains("crop="), "Should not contain crop filter when bounds not detected");
    }

    // Resolution and framerate parsing tests

    #[test]
    fn test_parse_resolution_valid() {
        assert_eq!(parse_resolution("1920x1080").unwrap(), (1920, 1080));
        assert_eq!(parse_resolution("1280x720").unwrap(), (1280, 720));
        assert_eq!(parse_resolution("3840x2160").unwrap(), (3840, 2160));
    }

    #[test]
    fn test_parse_resolution_invalid_format() {
        assert!(parse_resolution("1920").is_err());
        assert!(parse_resolution("1920:1080").is_err());
        assert!(parse_resolution("1920-1080").is_err());
        assert!(parse_resolution("widthxheight").is_err());
    }

    #[test]
    fn test_parse_resolution_zero_values() {
        assert!(parse_resolution("0x1080").is_err());
        assert!(parse_resolution("1920x0").is_err());
    }

    #[test]
    fn test_parse_resolution_too_large() {
        assert!(parse_resolution("10000x10000").is_err());
    }

    #[test]
    fn test_parse_framerate_valid() {
        assert_eq!(parse_framerate("30").unwrap(), 30);
        assert_eq!(parse_framerate("60").unwrap(), 60);
        assert_eq!(parse_framerate("1").unwrap(), 1);
        assert_eq!(parse_framerate("120").unwrap(), 120);
    }

    #[test]
    fn test_parse_framerate_invalid() {
        assert!(parse_framerate("0").is_err());
        assert!(parse_framerate("121").is_err());
        assert!(parse_framerate("-1").is_err());
        assert!(parse_framerate("abc").is_err());
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
        assert!(args.contains(&"medium".to_string()), "Should use medium preset");
        assert!(args.contains(&"-crf".to_string()), "Should use CRF");
        assert!(args.contains(&"23".to_string()), "Should use CRF 23");
        // Should output to file
        assert!(args.contains(&"output.mp4".to_string()), "Should output to file");
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
        let tee_output_idx = args.iter().position(|a| a.contains("pipe:1") && a.contains("recording.mp4"));
        assert!(tee_output_idx.is_some(), "Should have tee output with both pipe and file");
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
        assert!(filter.contains("scale=1920:1080"), "Filter should use configured resolution");
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
        assert!(args.contains(&"-stream_loop".to_string()), "Should have stream_loop for AI video");
        assert!(args.contains(&"-1".to_string()), "Should loop indefinitely");
        assert!(args.contains(&"/tmp/ai_video.mp4".to_string()), "Should have AI video path");

        // Filter should have AI overlay
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // AC: All layers scaled to output resolution
        assert!(filter.contains("scale=1280:720"), "AI video should be scaled");
        assert!(filter.contains("colorchannelmixer=aa=0.40"), "AI video should have correct opacity");
        assert!(filter.contains("[screen][ai]overlay"), "Should overlay AI on screen");
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
        assert!(args.contains(&"Capture screen 0".to_string()), "Should have screen capture");
        // Input 1: Webcam
        assert!(args.contains(&"FaceTime HD Camera".to_string()), "Should have webcam");
        // Input 2: AI video
        assert!(args.contains(&"/tmp/ai_video.mp4".to_string()), "Should have AI video");

        // Filter should have proper compositing
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        // AC: Layer order: terminal → webcam ghost → AI overlay
        // Check that screen comes first, then ghost overlay, then AI overlay
        assert!(filter.contains("[0:v]"), "Should have screen as input 0");
        assert!(filter.contains("[1:v]"), "Should have webcam as input 1");
        assert!(filter.contains("[2:v]"), "Should have AI video as input 2");
        assert!(filter.contains("[screen][ghost]overlay"), "Should overlay ghost on screen");
        assert!(filter.contains("[pre_ai][ai]overlay"), "Should overlay AI on composited base");
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
        let screen_ghost_pos = filter.find("[screen][ghost]overlay").expect("Should have screen+ghost overlay");
        let pre_ai_pos = filter.find("[pre_ai][ai]overlay").expect("Should have pre_ai+ai overlay");

        assert!(screen_ghost_pos < pre_ai_pos, "Screen+ghost overlay should come before AI overlay");
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
        assert_eq!(scale_count, 3, "All three inputs should be scaled to output resolution");
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
        assert!(args.contains(&":MacBook Pro Microphone".to_string()), "Should have audio input");

        // Filter should reference audio as input 3 (screen=0, webcam=1, AI=2, audio=3)
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        assert!(filter.contains("[3:a]"), "Audio should be input 3 with three video inputs");
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
        assert!(!args.contains(&"-stream_loop".to_string()), "Should not have stream_loop without AI video");

        // Filter should still work with just screen + webcam
        let filter_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let filter = &args[filter_idx + 1];

        assert!(filter.contains("[screen][ghost]overlay"), "Should have screen+ghost overlay");
        assert!(!filter.contains("[pre_ai]"), "Should not have AI layer without AI video");
        assert!(!filter.contains("[ai]"), "Should not have AI label without AI video");
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
        assert!(filter.contains("[screen][ghost]overlay"), "Should have ghost overlay");
        assert!(filter.contains("[pre_ai][ai]overlay"), "Should have AI overlay");

        // Should have post-composition effects after AI overlay
        assert!(filter.contains("[composited]"), "Should have composited intermediate");
        assert!(filter.contains("vignette"), "Should have vignette effect");
        assert!(filter.contains("drawtext"), "Should have LIVE badge text");
        assert!(filter.contains("[vout]"), "Should have final output");
    }

    // .env file loading tests

    #[test]
    fn test_env_var_accessible_after_dotenv() {
        // Test that dotenv::dotenv() doesn't panic and env vars are accessible
        // Note: dotenv::dotenv() returns Err if .env doesn't exist, which is fine
        let _ = dotenv::dotenv();

        // After dotenv loads, std::env::var should work
        // (may or may not find FAL_API_KEY depending on test environment)
        let _result = std::env::var("FAL_API_KEY");
        // We just verify it doesn't panic - Ok or Err are both valid
    }

    #[test]
    fn test_env_var_not_overridden() {
        // Set an env var before loading dotenv
        std::env::set_var("TEST_EXISTING_VAR", "original_value");

        // Load dotenv (uses default behavior which doesn't override existing vars)
        let _ = dotenv::dotenv();

        // Verify existing env var was not overridden
        assert_eq!(
            std::env::var("TEST_EXISTING_VAR").unwrap(),
            "original_value",
            "Existing env vars should not be overridden by dotenv"
        );

        // Clean up
        std::env::remove_var("TEST_EXISTING_VAR");
    }

    #[test]
    fn test_fal_api_key_detection() {
        // Test that we can check for FAL_API_KEY presence
        // First ensure it's not set
        std::env::remove_var("FAL_API_KEY");

        // Check returns Err when not set
        assert!(
            std::env::var("FAL_API_KEY").is_err(),
            "FAL_API_KEY should not be set initially"
        );

        // Set it and verify we can read it
        std::env::set_var("FAL_API_KEY", "test_key_12345");
        assert_eq!(
            std::env::var("FAL_API_KEY").unwrap(),
            "test_key_12345",
            "FAL_API_KEY should be readable after setting"
        );

        // Clean up
        std::env::remove_var("FAL_API_KEY");
    }

    #[test]
    fn test_fal_disabled_when_api_key_missing() {
        // AC: fal features disabled but app continues when FAL_API_KEY not set
        // This tests the logic that disables fal when API key is missing

        // Save original value
        let original = std::env::var("FAL_API_KEY").ok();

        // Remove API key
        std::env::remove_var("FAL_API_KEY");

        // Simulate the logic from main.rs: if --fal is enabled but key is missing, fal is disabled
        let fal_requested = true;
        let fal_enabled = if fal_requested && std::env::var("FAL_API_KEY").is_err() {
            false // Disable fal features but continue
        } else {
            fal_requested
        };

        assert!(!fal_enabled, "fal should be disabled when API key is missing");

        // Now test with API key set
        std::env::set_var("FAL_API_KEY", "test_key");
        let fal_enabled = if fal_requested && std::env::var("FAL_API_KEY").is_err() {
            false
        } else {
            fal_requested
        };

        assert!(fal_enabled, "fal should be enabled when API key is set");

        // Restore original value
        if let Some(val) = original {
            std::env::set_var("FAL_API_KEY", val);
        } else {
            std::env::remove_var("FAL_API_KEY");
        }
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
        assert_eq!(previous, Some(std::path::PathBuf::from("/tmp/video1.mp4")));
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
        let previous = config.set_ai_video(Some(std::path::PathBuf::from("/tmp/new_video.mp4")));

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

        assert_eq!(previous, Some(std::path::PathBuf::from("/tmp/video.mp4")));
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
        assert!(!filter_without.contains("[ai]"), "Should not have AI layer without video");

        // Swap in AI video
        config.set_ai_video(Some(std::path::PathBuf::from("/tmp/video.mp4")));

        // Build filter with AI video
        let filter_with = config.build_filter_chain(0.3);
        assert!(filter_with.contains("[ai]"), "Should have AI layer after swap");
        assert!(filter_with.contains("[pre_ai][ai]overlay"), "Should composite AI layer");
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
        assert!(filter.contains("scale=1280:720"), "Should scale to output resolution");
        // Filter should convert to rgba for alpha blending
        assert!(filter.contains("format=rgba"), "Should convert to rgba for format handling");
        // Filter should apply opacity
        assert!(filter.contains("colorchannelmixer=aa=0.40"), "Should apply AI video opacity");
    }
}
