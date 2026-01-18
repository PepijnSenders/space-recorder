//! Effects module for video effects, color grading, and overlays.
//!
//! Color grading effects are applied to the webcam stream only, keeping the terminal readable.
//! These effects are applied BEFORE the alpha/ghost blend in the filter chain.
//!
//! Post-composition effects (like vignette) are applied AFTER the ghost overlay blend
//! to affect the entire composited frame.
//!
//! Text overlays (LIVE badge, timestamp) are applied last in the post-composition chain.

/// Path to Helvetica font on macOS
const HELVETICA_FONT: &str = "/System/Library/Fonts/Helvetica.ttc";

/// Video effect preset that can be applied to the webcam stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoEffect {
    /// No color grading, just the raw webcam feed
    #[default]
    None,
    /// Cyberpunk: Blue/magenta color shift, increased saturation, slight contrast boost
    Cyberpunk,
    /// Dark mode friendly: Subtle brightness/contrast adjustment, preserves readability
    DarkMode,
}

impl VideoEffect {
    /// Parse effect name from string
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" => Some(Self::None),
            "cyberpunk" => Some(Self::Cyberpunk),
            "dark_mode" | "darkmode" | "dark-mode" => Some(Self::DarkMode),
            _ => None,
        }
    }

    /// Get the FFmpeg filter string for this effect
    ///
    /// Returns None if no effect is applied (None variant)
    pub fn to_filter(self) -> Option<String> {
        match self {
            Self::None => None,
            Self::Cyberpunk => Some(
                // Blue/magenta color shift with increased saturation and contrast
                // Applied as: curves for color shift, eq for saturation/contrast, colorbalance for tint
                "curves=r='0/0 0.25/0.2 0.5/0.45 0.75/0.8 1/1':g='0/0 0.25/0.25 0.5/0.5 0.75/0.75 1/1':b='0/0 0.25/0.3 0.5/0.6 0.75/0.85 1/1',eq=saturation=1.4:contrast=1.1,colorbalance=rs=0.1:gs=-0.05:bs=0.2:rm=0.1:gm=-0.1:bm=0.15".to_string()
            ),
            Self::DarkMode => Some(
                // Subtle brightness/contrast adjustment that preserves terminal readability
                "eq=brightness=0.05:contrast=1.05:saturation=1.1,unsharp=5:5:0.5:5:5:0".to_string()
            ),
        }
    }

    /// Check if this effect applies any processing
    #[allow(dead_code)] // Useful for future conditional logic
    pub fn is_active(&self) -> bool {
        *self != Self::None
    }
}

impl std::fmt::Display for VideoEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Cyberpunk => write!(f, "cyberpunk"),
            Self::DarkMode => write!(f, "dark_mode"),
        }
    }
}

/// Build the webcam filter chain including optional effects
///
/// The chain applies effects BEFORE the alpha/ghost blend:
/// 1. Mirror (hflip) if enabled
/// 2. Scale to target resolution
/// 3. Apply color grading effect (if any)
/// 4. Convert to RGBA and set alpha for ghost overlay
///
/// # Arguments
/// * `mirror` - Whether to apply horizontal flip
/// * `effect` - The video effect preset to apply
/// * `opacity` - Alpha value for the ghost overlay (0.0-1.0)
/// * `width` - Target width
/// * `height` - Target height
///
/// # Returns
/// The FFmpeg filter chain string for the webcam input
pub fn build_webcam_filter_chain(
    mirror: bool,
    effect: VideoEffect,
    opacity: f32,
    width: u32,
    height: u32,
) -> String {
    let mut filters = Vec::new();

    // 1. Mirror if enabled (first in chain)
    if mirror {
        filters.push("hflip".to_string());
    }

    // 2. Scale to target resolution
    filters.push(format!("scale={}:{}", width, height));

    // 3. Apply effect (color grading) BEFORE alpha blend
    if let Some(effect_filter) = effect.to_filter() {
        filters.push(effect_filter);
    }

    // 4. Convert to RGBA and set alpha for ghost overlay (last in chain)
    filters.push("format=rgba".to_string());
    filters.push(format!("colorchannelmixer=aa={:.2}", opacity));

    filters.join(",")
}

/// Build the LIVE badge overlay filter
///
/// Creates a red badge with white "LIVE" text positioned at the top-left corner.
///
/// # Specs
/// - Red box background at 0.8 alpha
/// - White text, 24pt Helvetica font
/// - Position: top-left with 20px margin
/// - Box border width: 8px (padding around text)
///
/// # Returns
/// The FFmpeg drawtext filter string for the LIVE badge
pub fn build_live_badge_filter() -> String {
    format!(
        "drawtext=text='LIVE':\
         fontfile={}:\
         fontsize=24:\
         fontcolor=white:\
         box=1:\
         boxcolor=red@0.8:\
         boxborderw=8:\
         x=20:\
         y=20",
        HELVETICA_FONT
    )
}

/// Build the timestamp overlay filter
///
/// Creates a real-time updating timestamp positioned at the top-right corner.
///
/// # Specs
/// - Shows current time in HH:MM:SS format
/// - White text at 0.8 alpha (semi-transparent)
/// - 18pt Helvetica font
/// - Position: top-right with 20px margin
/// - Updates in real-time via FFmpeg's localtime expansion
///
/// # Returns
/// The FFmpeg drawtext filter string for the timestamp
pub fn build_timestamp_filter() -> String {
    // Note: FFmpeg drawtext requires escaping colons in the time format
    // %{localtime\:%H\\\:%M\\\:%S} expands to HH:MM:SS
    format!(
        "drawtext=text='%{{localtime\\:%H\\\\\\:%M\\\\\\:%S}}':\
         fontfile={}:\
         fontsize=18:\
         fontcolor=white@0.8:\
         x=w-tw-20:\
         y=20",
        HELVETICA_FONT
    )
}

/// Build the post-composition filter chain for effects applied after ghost overlay
///
/// These effects are applied to the entire composited frame (terminal + webcam).
/// Order: vignette -> grain -> text overlays (LIVE badge, timestamp)
///
/// # Arguments
/// * `vignette` - Whether to apply vignette effect (subtle darkening around edges)
/// * `grain` - Whether to apply film grain effect (subtle noise texture)
/// * `live_badge` - Whether to show the LIVE badge overlay
/// * `timestamp` - Whether to show the timestamp overlay
///
/// # Returns
/// Optional FFmpeg filter string, or None if no post-composition effects are enabled
pub fn build_post_composition_filter(vignette: bool, grain: bool, live_badge: bool, timestamp: bool) -> Option<String> {
    let mut filters = Vec::new();

    // Vignette: subtle darkening around frame edges
    // Uses PI/5 for a subtle effect (spec says PI/5 for coding stream preset)
    if vignette {
        filters.push("vignette=PI/5".to_string());
    }

    // Film grain: subtle noise texture for cinematic look
    // Uses noise filter with alls=10 (strength 10) and allf=t (temporal noise for variation)
    if grain {
        filters.push("noise=alls=10:allf=t".to_string());
    }

    // LIVE badge: red badge with white text at top-left
    // Applied after visual effects so text is always readable
    if live_badge {
        filters.push(build_live_badge_filter());
    }

    // Timestamp: current time at top-right
    // Applied last so it's always visible on top of other effects
    if timestamp {
        filters.push(build_timestamp_filter());
    }

    if filters.is_empty() {
        None
    } else {
        Some(filters.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // VideoEffect tests

    #[test]
    fn test_video_effect_from_str() {
        assert_eq!(VideoEffect::from_str("none"), Some(VideoEffect::None));
        assert_eq!(VideoEffect::from_str("cyberpunk"), Some(VideoEffect::Cyberpunk));
        assert_eq!(VideoEffect::from_str("CYBERPUNK"), Some(VideoEffect::Cyberpunk));
        assert_eq!(VideoEffect::from_str("dark_mode"), Some(VideoEffect::DarkMode));
        assert_eq!(VideoEffect::from_str("darkmode"), Some(VideoEffect::DarkMode));
        assert_eq!(VideoEffect::from_str("dark-mode"), Some(VideoEffect::DarkMode));
        assert_eq!(VideoEffect::from_str("invalid"), None);
    }

    #[test]
    fn test_video_effect_to_filter_none() {
        let effect = VideoEffect::None;
        assert!(effect.to_filter().is_none());
    }

    #[test]
    fn test_video_effect_to_filter_cyberpunk() {
        let effect = VideoEffect::Cyberpunk;
        let filter = effect.to_filter().unwrap();
        // Should contain curves for color shift
        assert!(filter.contains("curves="));
        // Should contain saturation boost (1.4x)
        assert!(filter.contains("saturation=1.4"));
        // Should contain contrast boost
        assert!(filter.contains("contrast=1.1"));
        // Should contain colorbalance for tint
        assert!(filter.contains("colorbalance="));
    }

    #[test]
    fn test_video_effect_to_filter_dark_mode() {
        let effect = VideoEffect::DarkMode;
        let filter = effect.to_filter().unwrap();
        // Should contain brightness adjustment
        assert!(filter.contains("brightness=0.05"));
        // Should contain contrast adjustment
        assert!(filter.contains("contrast=1.05"));
        // Should contain saturation
        assert!(filter.contains("saturation=1.1"));
        // Should contain unsharp for sharpening
        assert!(filter.contains("unsharp="));
    }

    #[test]
    fn test_video_effect_is_active() {
        assert!(!VideoEffect::None.is_active());
        assert!(VideoEffect::Cyberpunk.is_active());
        assert!(VideoEffect::DarkMode.is_active());
    }

    #[test]
    fn test_video_effect_display() {
        assert_eq!(format!("{}", VideoEffect::None), "none");
        assert_eq!(format!("{}", VideoEffect::Cyberpunk), "cyberpunk");
        assert_eq!(format!("{}", VideoEffect::DarkMode), "dark_mode");
    }

    #[test]
    fn test_video_effect_default() {
        let effect = VideoEffect::default();
        assert_eq!(effect, VideoEffect::None);
    }

    // Webcam filter chain tests

    #[test]
    fn test_build_webcam_filter_chain_no_effects() {
        let chain = build_webcam_filter_chain(false, VideoEffect::None, 0.3, 1280, 720);
        // Should contain scale
        assert!(chain.contains("scale=1280:720"));
        // Should contain format=rgba
        assert!(chain.contains("format=rgba"));
        // Should contain alpha setting
        assert!(chain.contains("colorchannelmixer=aa=0.30"));
        // Should NOT contain hflip
        assert!(!chain.contains("hflip"));
        // Should NOT contain effect filters
        assert!(!chain.contains("curves="));
        assert!(!chain.contains("eq="));
    }

    #[test]
    fn test_build_webcam_filter_chain_with_mirror() {
        let chain = build_webcam_filter_chain(true, VideoEffect::None, 0.3, 1280, 720);
        // Should start with hflip
        assert!(chain.starts_with("hflip,"));
        // Should contain scale
        assert!(chain.contains("scale=1280:720"));
    }

    #[test]
    fn test_build_webcam_filter_chain_with_cyberpunk() {
        let chain = build_webcam_filter_chain(false, VideoEffect::Cyberpunk, 0.3, 1280, 720);
        // Should contain scale
        assert!(chain.contains("scale=1280:720"));
        // Should contain effect filters (curves, saturation, colorbalance)
        assert!(chain.contains("curves="));
        assert!(chain.contains("saturation=1.4"));
        assert!(chain.contains("colorbalance="));
        // Effect should be BEFORE format=rgba (alpha blend)
        let effect_pos = chain.find("saturation=").unwrap();
        let rgba_pos = chain.find("format=rgba").unwrap();
        assert!(effect_pos < rgba_pos, "Effect should be applied before alpha blend");
    }

    #[test]
    fn test_build_webcam_filter_chain_with_dark_mode() {
        let chain = build_webcam_filter_chain(false, VideoEffect::DarkMode, 0.5, 1280, 720);
        // Should contain dark mode filters
        assert!(chain.contains("brightness=0.05"));
        assert!(chain.contains("contrast=1.05"));
        // Should contain alpha setting
        assert!(chain.contains("colorchannelmixer=aa=0.50"));
    }

    #[test]
    fn test_build_webcam_filter_chain_full() {
        let chain = build_webcam_filter_chain(true, VideoEffect::Cyberpunk, 0.7, 1920, 1080);
        // Should contain all components in order: hflip, scale, effect, format, alpha
        assert!(chain.starts_with("hflip,"));
        assert!(chain.contains("scale=1920:1080"));
        assert!(chain.contains("saturation=1.4"));
        assert!(chain.contains("format=rgba"));
        assert!(chain.contains("colorchannelmixer=aa=0.70"));

        // Verify order: hflip -> scale -> effect -> format -> alpha
        let hflip_pos = chain.find("hflip").unwrap();
        let scale_pos = chain.find("scale=").unwrap();
        let effect_pos = chain.find("saturation=").unwrap();
        let rgba_pos = chain.find("format=rgba").unwrap();
        let alpha_pos = chain.find("colorchannelmixer=aa=").unwrap();

        assert!(hflip_pos < scale_pos);
        assert!(scale_pos < effect_pos);
        assert!(effect_pos < rgba_pos);
        assert!(rgba_pos < alpha_pos);
    }

    #[test]
    fn test_build_webcam_filter_chain_opacity_values() {
        let chain_zero = build_webcam_filter_chain(false, VideoEffect::None, 0.0, 1280, 720);
        assert!(chain_zero.contains("colorchannelmixer=aa=0.00"));

        let chain_full = build_webcam_filter_chain(false, VideoEffect::None, 1.0, 1280, 720);
        assert!(chain_full.contains("colorchannelmixer=aa=1.00"));
    }

    // Post-composition filter tests (vignette and grain)

    #[test]
    fn test_build_post_composition_filter_vignette_enabled() {
        let filter = build_post_composition_filter(true, false, false, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain vignette with PI/5 value
        assert!(filter_str.contains("vignette=PI/5"));
    }

    #[test]
    fn test_build_post_composition_filter_vignette_disabled() {
        let filter = build_post_composition_filter(false, false, false, false);
        // Should return None when no post-composition effects are enabled
        assert!(filter.is_none());
    }

    #[test]
    fn test_build_post_composition_filter_exact_value() {
        let filter = build_post_composition_filter(true, false, false, false).unwrap();
        // Verify exact filter string for vignette only
        assert_eq!(filter, "vignette=PI/5");
    }

    // Film grain tests

    #[test]
    fn test_build_post_composition_filter_grain_enabled() {
        let filter = build_post_composition_filter(false, true, false, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain noise filter with correct parameters
        assert!(filter_str.contains("noise=alls=10:allf=t"));
    }

    #[test]
    fn test_build_post_composition_filter_grain_exact_value() {
        let filter = build_post_composition_filter(false, true, false, false).unwrap();
        // Verify exact filter string for grain only
        assert_eq!(filter, "noise=alls=10:allf=t");
    }

    #[test]
    fn test_build_post_composition_filter_vignette_and_grain() {
        let filter = build_post_composition_filter(true, true, false, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain both effects
        assert!(filter_str.contains("vignette=PI/5"));
        assert!(filter_str.contains("noise=alls=10:allf=t"));
        // Vignette should come before grain (order matters)
        let vignette_pos = filter_str.find("vignette").unwrap();
        let grain_pos = filter_str.find("noise").unwrap();
        assert!(vignette_pos < grain_pos, "Vignette should be applied before grain");
    }

    #[test]
    fn test_build_post_composition_filter_vignette_and_grain_exact() {
        let filter = build_post_composition_filter(true, true, false, false).unwrap();
        // Verify exact filter string with both effects
        assert_eq!(filter, "vignette=PI/5,noise=alls=10:allf=t");
    }

    #[test]
    fn test_build_post_composition_filter_neither_enabled() {
        let filter = build_post_composition_filter(false, false, false, false);
        // Should return None when no effects are enabled
        assert!(filter.is_none());
    }

    // LIVE badge tests

    #[test]
    fn test_build_live_badge_filter() {
        let filter = build_live_badge_filter();
        // Should contain drawtext with LIVE text
        assert!(filter.contains("drawtext=text='LIVE'"));
        // Should use Helvetica font
        assert!(filter.contains("fontfile=/System/Library/Fonts/Helvetica.ttc"));
        // Should have 24pt font size
        assert!(filter.contains("fontsize=24"));
        // Should have white text
        assert!(filter.contains("fontcolor=white"));
        // Should have red box with 0.8 alpha
        assert!(filter.contains("box=1"));
        assert!(filter.contains("boxcolor=red@0.8"));
        // Should have 8px box border (padding)
        assert!(filter.contains("boxborderw=8"));
        // Should be positioned at top-left with 20px margin
        assert!(filter.contains("x=20"));
        assert!(filter.contains("y=20"));
    }

    #[test]
    fn test_build_post_composition_filter_live_badge_enabled() {
        let filter = build_post_composition_filter(false, false, true, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain the LIVE badge drawtext filter
        assert!(filter_str.contains("drawtext=text='LIVE'"));
        assert!(filter_str.contains("boxcolor=red@0.8"));
    }

    #[test]
    fn test_build_post_composition_filter_live_badge_only() {
        let filter = build_post_composition_filter(false, false, true, false).unwrap();
        // Should only contain the drawtext filter
        assert!(filter.starts_with("drawtext="));
        // Should not contain vignette or grain
        assert!(!filter.contains("vignette"));
        assert!(!filter.contains("noise="));
    }

    #[test]
    fn test_build_post_composition_filter_all_effects() {
        let filter = build_post_composition_filter(true, true, true, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain all effects
        assert!(filter_str.contains("vignette=PI/5"));
        assert!(filter_str.contains("noise=alls=10:allf=t"));
        assert!(filter_str.contains("drawtext=text='LIVE'"));
        // Order should be: vignette -> grain -> live badge
        let vignette_pos = filter_str.find("vignette").unwrap();
        let grain_pos = filter_str.find("noise").unwrap();
        let live_pos = filter_str.find("drawtext").unwrap();
        assert!(vignette_pos < grain_pos, "Vignette should be before grain");
        assert!(grain_pos < live_pos, "Grain should be before LIVE badge");
    }

    #[test]
    fn test_build_post_composition_filter_vignette_and_live_badge() {
        let filter = build_post_composition_filter(true, false, true, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain vignette and live badge
        assert!(filter_str.contains("vignette=PI/5"));
        assert!(filter_str.contains("drawtext=text='LIVE'"));
        // Should not contain grain
        assert!(!filter_str.contains("noise="));
        // Vignette should come before live badge
        let vignette_pos = filter_str.find("vignette").unwrap();
        let live_pos = filter_str.find("drawtext").unwrap();
        assert!(vignette_pos < live_pos, "Vignette should be before LIVE badge");
    }

    #[test]
    fn test_build_post_composition_filter_grain_and_live_badge() {
        let filter = build_post_composition_filter(false, true, true, false);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain grain and live badge
        assert!(filter_str.contains("noise=alls=10:allf=t"));
        assert!(filter_str.contains("drawtext=text='LIVE'"));
        // Should not contain vignette
        assert!(!filter_str.contains("vignette"));
        // Grain should come before live badge
        let grain_pos = filter_str.find("noise").unwrap();
        let live_pos = filter_str.find("drawtext").unwrap();
        assert!(grain_pos < live_pos, "Grain should be before LIVE badge");
    }

    // Timestamp overlay tests

    #[test]
    fn test_build_timestamp_filter() {
        let filter = build_timestamp_filter();
        // Should contain drawtext with localtime expansion for HH:MM:SS
        assert!(filter.contains("drawtext="));
        assert!(filter.contains("localtime"));
        // Should use Helvetica font
        assert!(filter.contains("fontfile=/System/Library/Fonts/Helvetica.ttc"));
        // Should have 18pt font size
        assert!(filter.contains("fontsize=18"));
        // Should have semi-transparent white text (0.8 alpha)
        assert!(filter.contains("fontcolor=white@0.8"));
        // Should be positioned at top-right with 20px margin
        assert!(filter.contains("x=w-tw-20"));
        assert!(filter.contains("y=20"));
    }

    #[test]
    fn test_build_post_composition_filter_timestamp_enabled() {
        let filter = build_post_composition_filter(false, false, false, true);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain the timestamp drawtext filter
        assert!(filter_str.contains("drawtext="));
        assert!(filter_str.contains("localtime"));
        assert!(filter_str.contains("fontcolor=white@0.8"));
    }

    #[test]
    fn test_build_post_composition_filter_timestamp_only() {
        let filter = build_post_composition_filter(false, false, false, true).unwrap();
        // Should only contain the drawtext filter
        assert!(filter.starts_with("drawtext="));
        // Should not contain vignette or grain or LIVE badge
        assert!(!filter.contains("vignette"));
        assert!(!filter.contains("noise="));
        assert!(!filter.contains("'LIVE'"));
    }

    #[test]
    fn test_build_post_composition_filter_live_badge_and_timestamp() {
        let filter = build_post_composition_filter(false, false, true, true);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain both LIVE badge and timestamp
        assert!(filter_str.contains("drawtext=text='LIVE'"));
        assert!(filter_str.contains("localtime"));
        // LIVE badge should come before timestamp (order matters for layering)
        let live_pos = filter_str.find("'LIVE'").unwrap();
        let timestamp_pos = filter_str.find("localtime").unwrap();
        assert!(live_pos < timestamp_pos, "LIVE badge should be before timestamp");
    }

    #[test]
    fn test_build_post_composition_filter_all_effects_with_timestamp() {
        let filter = build_post_composition_filter(true, true, true, true);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain all effects
        assert!(filter_str.contains("vignette=PI/5"));
        assert!(filter_str.contains("noise=alls=10:allf=t"));
        assert!(filter_str.contains("drawtext=text='LIVE'"));
        assert!(filter_str.contains("localtime"));
        // Order should be: vignette -> grain -> live badge -> timestamp
        let vignette_pos = filter_str.find("vignette").unwrap();
        let grain_pos = filter_str.find("noise").unwrap();
        let live_pos = filter_str.find("'LIVE'").unwrap();
        let timestamp_pos = filter_str.find("localtime").unwrap();
        assert!(vignette_pos < grain_pos, "Vignette should be before grain");
        assert!(grain_pos < live_pos, "Grain should be before LIVE badge");
        assert!(live_pos < timestamp_pos, "LIVE badge should be before timestamp");
    }

    #[test]
    fn test_build_post_composition_filter_vignette_and_timestamp() {
        let filter = build_post_composition_filter(true, false, false, true);
        assert!(filter.is_some());
        let filter_str = filter.unwrap();
        // Should contain vignette and timestamp
        assert!(filter_str.contains("vignette=PI/5"));
        assert!(filter_str.contains("localtime"));
        // Should not contain grain or LIVE badge
        assert!(!filter_str.contains("noise="));
        assert!(!filter_str.contains("'LIVE'"));
    }
}
