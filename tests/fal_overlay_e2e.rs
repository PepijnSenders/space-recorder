//! End-to-end tests for fal overlay streaming.
//!
//! Tests the complete fal.ai overlay workflow:
//! - AC: Start stream with `--fal` flag
//! - AC: Enter prompt, video generates and appears
//! - AC: Enter new prompt, crossfade occurs
//! - AC: `/clear` removes overlay
//! - AC: `/opacity 0.5` changes opacity
//! - AC: Cache persists across restarts

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use space_recorder::fal::{
    FalClient, OverlayManager, PromptCommand, PromptInput, TransitionState, VideoCache,
};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a mock fal.ai API server that simulates video generation.
async fn setup_mock_fal_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Mock video generation submission endpoint
    // POST /{model} - returns a queue response with request_id
    Mock::given(method("POST"))
        .and(path_regex(r"^/fal-ai/.*"))
        .and(header("Authorization", "Key test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "request_id": "test-request-123"
        })))
        .mount(&mock_server)
        .await;

    // Mock status polling endpoint - immediately returns completed
    Mock::given(method("GET"))
        .and(path_regex(r"^/fal-ai/.*/requests/.*/status$"))
        .and(header("Authorization", "Key test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "COMPLETED",
            "video": {
                "url": "http://localhost:9999/test-video.mp4"
            }
        })))
        .mount(&mock_server)
        .await;

    mock_server
}

/// Create a mock server for video downloads.
async fn setup_mock_video_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Mock video download endpoint - returns fake video bytes
    Mock::given(method("GET"))
        .and(path_regex(r".*\.mp4$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(b"fake-video-content-for-testing".to_vec())
                .insert_header("content-type", "video/mp4"),
        )
        .mount(&mock_server)
        .await;

    mock_server
}

/// Create a test FalClient pointing to a mock server.
fn create_test_client(base_url: &str) -> FalClient {
    FalClient::with_base_url("test-api-key".to_string(), base_url.to_string())
        .expect("Failed to create test client")
}

/// Simulate the complete video generation and caching flow.
/// Returns the path to the cached video.
async fn generate_and_cache_video(
    prompt: &str,
    cache: &VideoCache,
    client: &FalClient,
    video_server_uri: &str,
    temp_dir: &std::path::Path,
) -> PathBuf {
    // Submit generation request
    let queue_response = client
        .submit_generation(prompt)
        .await
        .expect("Failed to submit generation");

    // Poll for completion
    let _status = client
        .poll_status(&queue_response.request_id)
        .await
        .expect("Failed to poll status");

    // Download video
    let download_path = temp_dir.join(format!("{}.mp4", VideoCache::hash_prompt(prompt)));
    let download_url = format!("{}/video.mp4", video_server_uri);
    client
        .download_video(&download_url, &download_path)
        .await
        .expect("Failed to download video");

    // Store in cache
    cache
        .store_with_metadata(prompt, &download_path)
        .expect("Failed to cache video")
}

// ============================================================================
// E2E Test: Fal Overlay Streaming
// ============================================================================

/// E2E Test: Complete fal overlay streaming workflow.
///
/// This test simulates a complete streaming session with fal.ai overlay:
/// 1. Start stream with --fal flag (simulated via PromptInput)
/// 2. Enter prompt, video generates and appears
/// 3. Enter new prompt, crossfade occurs
/// 4. /clear removes overlay
/// 5. /opacity 0.5 changes opacity
/// 6. Cache persists across restarts
#[tokio::test]
async fn test_e2e_fal_overlay_streaming() {
    // ===== Setup =====
    let api_server = setup_mock_fal_server().await;
    let video_server = setup_mock_video_server().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create cache in temp directory to test persistence
    let cache_dir = temp_dir.path().join("cache");
    let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Failed to create cache");

    // Create FalClient pointing to mock server
    let client = create_test_client(&api_server.uri());

    // Create OverlayManager (simulates the overlay state during streaming)
    let mut overlay = OverlayManager::new();

    // ===== AC: Start stream with --fal flag =====
    // Simulated by creating a channel for PromptCommand (as done in main.rs when --fal is enabled)
    // TestPromptInput simulates user input without spawning a thread
    let (tx, rx) = mpsc::channel::<PromptCommand>();
    let prompt_input = TestPromptInput::new(tx);

    // Verify initial overlay state (no video, idle)
    assert!(overlay.current_video().is_none(), "Overlay should start with no video");
    assert_eq!(*overlay.transition_state(), TransitionState::Idle);

    // ===== AC: Enter prompt, video generates and appears =====
    let prompt1 = "cyberpunk cityscape with neon lights";

    // Simulate user entering prompt
    prompt_input.send(PromptCommand::Generate(prompt1.to_string())).unwrap();

    // Receive and process the command
    let cmd = rx.recv_timeout(Duration::from_secs(1)).expect("Should receive command");
    assert!(matches!(cmd, PromptCommand::Generate(_)));

    // Check cache first (should miss)
    assert!(cache.get(prompt1).is_none(), "Cache should miss for new prompt");

    // Generate and cache video
    let video_path1 = generate_and_cache_video(
        prompt1,
        &cache,
        &client,
        &video_server.uri(),
        temp_dir.path(),
    )
    .await;

    // Queue video to overlay
    overlay.queue_video(video_path1.clone());

    // Verify video is now current (first video sets directly, no crossfade)
    assert_eq!(overlay.current_video(), Some(&video_path1));
    assert_eq!(*overlay.transition_state(), TransitionState::Idle);

    // ===== AC: Enter new prompt, crossfade occurs =====
    let prompt2 = "abstract particles flowing in space";

    // Simulate user entering second prompt
    prompt_input.send(PromptCommand::Generate(prompt2.to_string())).unwrap();

    let cmd = rx.recv_timeout(Duration::from_secs(1)).expect("Should receive command");
    assert!(matches!(cmd, PromptCommand::Generate(_)));

    // Generate and cache second video
    let video_path2 = generate_and_cache_video(
        prompt2,
        &cache,
        &client,
        &video_server.uri(),
        temp_dir.path(),
    )
    .await;

    // Queue second video - should trigger crossfade
    overlay.queue_video(video_path2.clone());

    // Verify crossfade state
    assert!(
        matches!(overlay.transition_state(), TransitionState::CrossfadeIn { .. }),
        "Should be in crossfade state after queueing second video"
    );
    assert_eq!(overlay.current_video(), Some(&video_path1), "Current should still be first video during crossfade");

    // Verify crossfade FFmpeg filter is generated correctly
    let filter = overlay.get_ffmpeg_filter();
    assert!(filter.contains("xfade=transition=fade"), "Crossfade filter should contain xfade");
    assert!(filter.contains("[ai_current]"), "Filter should reference current video");
    assert!(filter.contains("[ai_pending]"), "Filter should reference pending video");

    // Complete crossfade transition
    overlay.tick(500); // Complete the 500ms default crossfade

    // Verify crossfade completed
    assert_eq!(*overlay.transition_state(), TransitionState::Idle);
    assert_eq!(overlay.current_video(), Some(&video_path2), "Current should now be second video");

    // ===== AC: /opacity 0.5 changes opacity =====
    // Simulate /opacity command
    prompt_input.send(PromptCommand::SetOpacity(0.5)).unwrap();

    let cmd = rx.recv_timeout(Duration::from_secs(1)).expect("Should receive command");
    assert_eq!(cmd, PromptCommand::SetOpacity(0.5));

    // Apply opacity change
    overlay.set_opacity(0.5);

    // Verify opacity changed
    assert!((overlay.opacity() - 0.5).abs() < f32::EPSILON, "Opacity should be 0.5");

    // Verify filter reflects new opacity
    let filter = overlay.get_ffmpeg_filter();
    assert!(
        filter.contains("colorchannelmixer=aa=0.50"),
        "Filter should reflect new opacity 0.50, got: {}",
        filter
    );

    // ===== AC: /clear removes overlay =====
    // Simulate /clear command
    prompt_input.send(PromptCommand::Clear).unwrap();

    let cmd = rx.recv_timeout(Duration::from_secs(1)).expect("Should receive command");
    assert_eq!(cmd, PromptCommand::Clear);

    // Apply clear command
    overlay.clear();

    // Verify fade-out transition started
    assert!(
        matches!(overlay.transition_state(), TransitionState::FadeOut { .. }),
        "Should be in fade-out state after clear"
    );

    // Complete fade-out
    overlay.tick(500);

    // Verify overlay is cleared
    assert!(overlay.current_video().is_none(), "Overlay should be cleared after fade-out");
    assert_eq!(*overlay.transition_state(), TransitionState::Idle);

    // Verify filter is empty when no video
    let filter = overlay.get_ffmpeg_filter();
    assert!(filter.is_empty(), "Filter should be empty when no video");

    // ===== AC: Cache persists across restarts =====
    // Drop the original cache and create a new one pointing to same directory
    drop(cache);

    // Simulate restart by creating new cache instance
    let cache_restarted = VideoCache::new_initialized(cache_dir.clone())
        .expect("Failed to create cache after restart");

    // Verify both videos are still cached
    let cached_path1 = cache_restarted.get(prompt1);
    assert!(
        cached_path1.is_some(),
        "First prompt should still be cached after restart"
    );
    assert!(
        cached_path1.unwrap().exists(),
        "Cached video file should exist"
    );

    let cached_path2 = cache_restarted.get(prompt2);
    assert!(
        cached_path2.is_some(),
        "Second prompt should still be cached after restart"
    );
    assert!(
        cached_path2.unwrap().exists(),
        "Cached video file should exist"
    );

    // Verify prompt metadata is preserved
    let hash1 = VideoCache::hash_prompt(prompt1);
    assert_eq!(
        cache_restarted.get_prompt(&hash1),
        Some(prompt1.to_string()),
        "Prompt metadata should persist"
    );

    let hash2 = VideoCache::hash_prompt(prompt2);
    assert_eq!(
        cache_restarted.get_prompt(&hash2),
        Some(prompt2.to_string()),
        "Prompt metadata should persist"
    );

    // Verify list_entries works after restart
    let entries = cache_restarted.list_entries().expect("Should list entries");
    assert_eq!(entries.len(), 2, "Should have 2 cached videos");

    // ===== Simulate using cached video on restart (cache hit) =====
    let mut overlay_restarted = OverlayManager::new();

    // User enters same prompt - should hit cache
    let cached_path = cache_restarted.get(prompt1).expect("Should find in cache");
    assert!(cached_path.exists(), "Cached file should exist");

    // Queue cached video
    overlay_restarted.queue_video(cached_path.clone());

    // Verify video loaded immediately (no API call needed)
    assert_eq!(overlay_restarted.current_video(), Some(&cached_path));
    assert_eq!(*overlay_restarted.transition_state(), TransitionState::Idle);
}

/// Test wrapper for PromptInput that doesn't spawn a thread.
/// Used to simulate user input during testing.
struct TestPromptInput {
    tx: mpsc::Sender<PromptCommand>,
}

impl TestPromptInput {
    /// Create a new test prompt input with the given channel.
    fn new(tx: mpsc::Sender<PromptCommand>) -> Self {
        Self { tx }
    }

    fn send(&self, cmd: PromptCommand) -> Result<(), mpsc::SendError<PromptCommand>> {
        self.tx.send(cmd)
    }
}

// ============================================================================
// Additional E2E Tests for Edge Cases
// ============================================================================

/// Test: Opacity command edge cases during streaming.
#[tokio::test]
async fn test_e2e_opacity_edge_cases() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let mut overlay = OverlayManager::new();

    // Create a test video file
    let video_path = temp_dir.path().join("test.mp4");
    std::fs::write(&video_path, b"fake-video").expect("Failed to write test video");

    // Queue video
    overlay.queue_video(video_path);

    // Test: Parse /opacity 0.0 (minimum)
    let cmd = PromptInput::parse_input("/opacity 0.0");
    assert_eq!(cmd, Some(PromptCommand::SetOpacity(0.0)));

    overlay.set_opacity(0.0);
    let filter = overlay.get_ffmpeg_filter();
    assert!(filter.contains("colorchannelmixer=aa=0.00"), "Should handle 0.0 opacity");

    // Test: Parse /opacity 1.0 (maximum)
    let cmd = PromptInput::parse_input("/opacity 1.0");
    assert_eq!(cmd, Some(PromptCommand::SetOpacity(1.0)));

    overlay.set_opacity(1.0);
    let filter = overlay.get_ffmpeg_filter();
    assert!(filter.contains("colorchannelmixer=aa=1.00"), "Should handle 1.0 opacity");

    // Test: Invalid opacity values are rejected
    assert_eq!(PromptInput::parse_input("/opacity 1.5"), None, "Should reject >1.0");
    assert_eq!(PromptInput::parse_input("/opacity -0.1"), None, "Should reject <0.0");
    assert_eq!(PromptInput::parse_input("/opacity abc"), None, "Should reject non-numeric");
    assert_eq!(PromptInput::parse_input("/opacity"), None, "Should reject missing value");
}

/// Test: Clear command during different overlay states.
#[tokio::test]
async fn test_e2e_clear_during_different_states() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create test video files
    let video1 = temp_dir.path().join("video1.mp4");
    let video2 = temp_dir.path().join("video2.mp4");
    std::fs::write(&video1, b"fake-video-1").expect("Failed to write");
    std::fs::write(&video2, b"fake-video-2").expect("Failed to write");

    // Test 1: Clear when idle with video
    {
        let mut overlay = OverlayManager::new();
        overlay.queue_video(video1.clone());
        assert!(overlay.current_video().is_some());

        let cmd = PromptInput::parse_input("/clear");
        assert_eq!(cmd, Some(PromptCommand::Clear));

        overlay.clear();
        assert!(matches!(overlay.transition_state(), TransitionState::FadeOut { .. }));

        overlay.tick(500);
        assert!(overlay.current_video().is_none());
    }

    // Test 2: Clear during crossfade
    {
        let mut overlay = OverlayManager::new();
        overlay.queue_video(video1.clone());
        overlay.queue_video(video2.clone());
        assert!(matches!(overlay.transition_state(), TransitionState::CrossfadeIn { .. }));

        overlay.clear();
        // Should cancel crossfade and start fade-out
        assert!(overlay.pending_video().is_none(), "Pending should be cleared");
        assert!(matches!(overlay.transition_state(), TransitionState::FadeOut { .. }));

        overlay.tick(500);
        assert!(overlay.current_video().is_none());
    }

    // Test 3: Clear when no video (noop)
    {
        let mut overlay = OverlayManager::new();
        assert!(overlay.current_video().is_none());

        overlay.clear();
        assert_eq!(*overlay.transition_state(), TransitionState::Idle);
        assert!(overlay.current_video().is_none());
    }
}

/// Test: Cache persistence and retrieval after multiple operations.
#[tokio::test]
async fn test_e2e_cache_persistence_comprehensive() {
    let api_server = setup_mock_fal_server().await;
    let video_server = setup_mock_video_server().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let cache_dir = temp_dir.path().join("persistent_cache");

    // Session 1: Generate multiple videos
    {
        let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Create cache");
        let client = create_test_client(&api_server.uri());

        let prompts = vec![
            "neon grid horizon",
            "abstract smoke particles",
            "futuristic city skyline",
        ];

        for prompt in &prompts {
            let _ = generate_and_cache_video(
                prompt,
                &cache,
                &client,
                &video_server.uri(),
                temp_dir.path(),
            )
            .await;
        }

        // Verify all cached
        for prompt in &prompts {
            assert!(cache.get(prompt).is_some(), "Should be cached: {}", prompt);
        }

        let entries = cache.list_entries().unwrap();
        assert_eq!(entries.len(), 3);
    }

    // Session 2: Restart and verify persistence
    {
        let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Reopen cache");

        // All entries should persist
        let entries = cache.list_entries().unwrap();
        assert_eq!(entries.len(), 3, "All entries should persist across restart");

        // Verify each prompt's metadata
        for entry in &entries {
            assert!(entry.prompt.is_some(), "Prompt metadata should persist");
            assert!(entry.path.exists(), "Video file should exist");
        }

        // Verify specific lookups work
        assert!(cache.get("neon grid horizon").is_some());
        assert!(cache.get("abstract smoke particles").is_some());
        assert!(cache.get("futuristic city skyline").is_some());
    }

    // Session 3: Clear specific entry and verify
    {
        let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Reopen cache");

        // Remove one entry
        let hash = VideoCache::hash_prompt("neon grid horizon");
        let removed = cache.remove(&hash).unwrap();
        assert!(removed, "Should remove entry");

        // Verify it's gone but others remain
        assert!(cache.get("neon grid horizon").is_none());
        assert!(cache.get("abstract smoke particles").is_some());
        assert!(cache.get("futuristic city skyline").is_some());

        let entries = cache.list_entries().unwrap();
        assert_eq!(entries.len(), 2);
    }

    // Session 4: Verify partial cache persists
    {
        let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Reopen cache");

        let entries = cache.list_entries().unwrap();
        assert_eq!(entries.len(), 2, "Should have 2 entries after removal");

        // The removed one should still be gone
        assert!(cache.get("neon grid horizon").is_none());
    }

    // Session 5: Clear all and verify
    {
        let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Reopen cache");

        let cleared = cache.clear_all().unwrap();
        assert_eq!(cleared, 2, "Should clear 2 videos");

        let entries = cache.list_entries().unwrap();
        assert!(entries.is_empty(), "Cache should be empty after clear_all");
    }

    // Session 6: Empty cache persists (directory exists but empty)
    {
        let cache = VideoCache::new_initialized(cache_dir.clone()).expect("Reopen cache");

        let entries = cache.list_entries().unwrap();
        assert!(entries.is_empty(), "Empty cache should persist as empty");

        // Can still add new entries
        assert!(cache.cache_dir().exists());
    }
}

/// Test: Crossfade transitions with different durations.
#[tokio::test]
async fn test_e2e_crossfade_durations() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let video1 = temp_dir.path().join("video1.mp4");
    let video2 = temp_dir.path().join("video2.mp4");
    std::fs::write(&video1, b"fake-video-1").expect("Failed to write");
    std::fs::write(&video2, b"fake-video-2").expect("Failed to write");

    // Test default duration (500ms)
    {
        let mut overlay = OverlayManager::new();
        assert_eq!(overlay.crossfade_duration_ms(), 500);

        overlay.queue_video(video1.clone());
        overlay.queue_video(video2.clone());

        if let TransitionState::CrossfadeIn { duration_ms, .. } = overlay.transition_state() {
            assert_eq!(*duration_ms, 500);
        } else {
            panic!("Expected CrossfadeIn state");
        }

        let filter = overlay.get_ffmpeg_filter();
        assert!(filter.contains("duration=0.50"), "Default duration should be 0.5s");
    }

    // Test custom duration (1000ms)
    {
        let mut overlay = OverlayManager::with_settings(0.3, 1000);
        assert_eq!(overlay.crossfade_duration_ms(), 1000);

        overlay.queue_video(video1.clone());
        overlay.queue_video(video2.clone());

        if let TransitionState::CrossfadeIn { duration_ms, .. } = overlay.transition_state() {
            assert_eq!(*duration_ms, 1000);
        } else {
            panic!("Expected CrossfadeIn state");
        }

        let filter = overlay.get_ffmpeg_filter();
        assert!(filter.contains("duration=1.00"), "Custom duration should be 1.0s");

        // Verify partial progress
        overlay.tick(500); // 50% through 1000ms
        assert!(matches!(overlay.transition_state(), TransitionState::CrossfadeIn { .. }));

        overlay.tick(500); // Complete
        assert_eq!(*overlay.transition_state(), TransitionState::Idle);
    }

    // Test instant cut (0ms duration)
    {
        let mut overlay = OverlayManager::new();
        overlay.queue_video(video1.clone());

        // Use instant cut
        overlay.queue_video_with_duration(video2.clone(), 0);

        // Should skip to Idle immediately
        assert_eq!(*overlay.transition_state(), TransitionState::Idle);
        assert_eq!(overlay.current_video(), Some(&video2));
    }
}

/// Test: Prompt parsing edge cases.
#[test]
fn test_e2e_prompt_parsing_edge_cases() {
    // Regular prompts
    assert_eq!(
        PromptInput::parse_input("cyberpunk city"),
        Some(PromptCommand::Generate("cyberpunk city".to_string()))
    );

    // Prompts with special characters
    assert_eq!(
        PromptInput::parse_input("neon lights & rain, cyberpunk 2077 style"),
        Some(PromptCommand::Generate("neon lights & rain, cyberpunk 2077 style".to_string()))
    );

    // Commands - case insensitive
    assert_eq!(PromptInput::parse_input("/clear"), Some(PromptCommand::Clear));
    assert_eq!(PromptInput::parse_input("/CLEAR"), Some(PromptCommand::Clear));
    assert_eq!(PromptInput::parse_input("/Clear"), Some(PromptCommand::Clear));

    // Opacity with various formats
    assert_eq!(PromptInput::parse_input("/opacity 0.5"), Some(PromptCommand::SetOpacity(0.5)));
    assert_eq!(PromptInput::parse_input("/opacity 0"), Some(PromptCommand::SetOpacity(0.0)));
    assert_eq!(PromptInput::parse_input("/opacity 1"), Some(PromptCommand::SetOpacity(1.0)));
    assert_eq!(PromptInput::parse_input("/OPACITY 0.75"), Some(PromptCommand::SetOpacity(0.75)));

    // Empty/whitespace - ignored
    assert_eq!(PromptInput::parse_input(""), None);
    assert_eq!(PromptInput::parse_input("   "), None);
    assert_eq!(PromptInput::parse_input("\t"), None);

    // Unknown commands
    assert_eq!(PromptInput::parse_input("/unknown"), None);
    assert_eq!(PromptInput::parse_input("/help"), None);

    // Whitespace trimming
    assert_eq!(
        PromptInput::parse_input("  hello world  "),
        Some(PromptCommand::Generate("hello world".to_string()))
    );
}
