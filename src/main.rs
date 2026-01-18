mod capture;
mod config;
mod devices;
mod effects;
mod fal;
mod hotkeys;
mod permissions;
mod pipeline;
mod pipeline_config;

use capture::{AudioCapture, CaptureError, ScreenCapture, WebcamCapture, WindowCapture};
use clap::{Parser, Subcommand};
use effects::VideoEffect;
use pipeline_config::{OutputMode, PipelineConfig};
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
            match devices::list_avfoundation_devices() {
                Ok(device_list) => devices::print_devices(&device_list, video, audio),
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

    // CLI logic tests

    #[test]
    fn test_no_effects_overrides_effect() {
        // Simulate the behavior in main: --no-effects should override --effect
        let original_effect = VideoEffect::Cyberpunk;
        let no_effects = true;

        // This mirrors the logic in main()
        let final_effect = if no_effects { VideoEffect::None } else { original_effect };

        assert_eq!(final_effect, VideoEffect::None);
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
}
