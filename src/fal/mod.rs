//! fal.ai video overlay integration module.
//!
//! This module provides AI-generated video overlay capabilities using fal.ai's
//! text-to-video API. Videos are generated from text prompts, cached locally,
//! and composited as an additional layer in the output stream.

// Allow dead_code during v2 development - will be removed when integrated into main CLI
#![allow(dead_code)]
#![allow(unused_imports)]

mod cache;
mod client;
mod overlay;
mod prompt;
mod video_replacement;

pub use cache::{CacheEntry, VideoCache};
pub use client::{
    validate_prompt, FalClient, FalError, GenerationStatus, QueueResponse, DEFAULT_MODEL,
    FAL_API_BASE_URL, FAL_API_KEY_ENV,
};
pub use overlay::{OverlayManager, TransitionState};
pub use prompt::{PromptCommand, PromptInput};
pub use video_replacement::{
    VideoFormat, VideoReplacement, VideoReplacementError, VideoReplacementManager,
};
