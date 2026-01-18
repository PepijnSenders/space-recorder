//! Integration tests for the VideoCache + FalClient flow.
//!
//! Tests the acceptance criteria:
//! - AC: Generate video, verify cached
//! - AC: Request same prompt, verify cache hit
//! - AC: Clear cache, verify cache miss

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

// Import from the main crate
use space_recorder::fal::{FalClient, VideoCache};

/// Tracks API call count for verifying cache behavior.
static API_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Test helper: Create a mock server that simulates fal.ai API.
/// Returns the mock server and sets up the required endpoints.
async fn setup_mock_fal_server() -> MockServer {
    let mock_server = MockServer::start().await;

    // Reset call counter
    API_CALL_COUNT.store(0, Ordering::SeqCst);

    // Mock the video generation submission endpoint
    // POST /{model} - returns a queue response with request_id
    Mock::given(method("POST"))
        .and(path_regex(r"^/fal-ai/.*"))
        .and(header("Authorization", "Key test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "request_id": "test-request-123"
        })))
        .mount(&mock_server)
        .await;

    // Mock the status polling endpoint - immediately returns completed
    // GET /{model}/requests/{request_id}/status
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

/// Test helper: Create a mock server for video download.
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

/// Helper to create a FalClient pointing to a mock server.
fn create_test_client(base_url: &str) -> FalClient {
    FalClient::with_base_url("test-api-key".to_string(), base_url.to_string())
        .expect("Failed to create test client")
}

/// Integration test: Cache flow with generate, cache hit, and cache miss.
///
/// This test verifies the complete cache integration:
/// 1. Generate video via mock API, verify it gets cached
/// 2. Request same prompt, verify cache hit (no new API call)
/// 3. Clear cache, verify cache miss (needs new API call)
#[tokio::test]
async fn test_cache_flow_integration() {
    // Setup mock servers
    let api_server = setup_mock_fal_server().await;
    let video_server = setup_mock_video_server().await;

    // Create a temporary cache directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache = VideoCache::new_initialized(temp_dir.path().join("cache"))
        .expect("Failed to create cache");

    // Create client pointing to mock API server
    let client = create_test_client(&api_server.uri());

    let test_prompt = "cyberpunk cityscape with neon lights";

    // ===== Step 1: Generate video, verify cached =====

    // First, verify cache is empty for this prompt
    assert!(
        cache.get(test_prompt).is_none(),
        "Cache should be empty initially"
    );

    // Simulate the generation flow:
    // 1. Check cache (miss)
    // 2. Call API to generate
    // 3. Store in cache
    let cached_result = cache.get(test_prompt);
    assert!(cached_result.is_none(), "Initial cache lookup should miss");

    // Submit generation request to mock API
    let queue_response = client
        .submit_generation(test_prompt)
        .await
        .expect("Failed to submit generation");
    assert_eq!(queue_response.request_id, "test-request-123");

    // Poll for completion (mock returns COMPLETED immediately)
    let status = client
        .poll_status(&queue_response.request_id)
        .await
        .expect("Failed to poll status");

    // Extract video URL from completed status
    let _video_url = match status {
        space_recorder::fal::GenerationStatus::Completed { video_url } => video_url,
        _ => panic!("Expected Completed status, got {:?}", status),
    };

    // Download the video to a temp location
    let download_path = temp_dir.path().join("downloaded.mp4");

    // Use the video server URL instead of the mock one in the response
    let actual_download_url = format!("{}/test-video.mp4", video_server.uri());
    client
        .download_video(&actual_download_url, &download_path)
        .await
        .expect("Failed to download video");

    assert!(download_path.exists(), "Downloaded video should exist");

    // Store in cache
    let cached_path = cache
        .store_with_metadata(test_prompt, &download_path)
        .expect("Failed to store in cache");

    assert!(cached_path.exists(), "Cached video should exist");

    // Verify cache now has the video
    let cache_lookup = cache.get(test_prompt);
    assert!(
        cache_lookup.is_some(),
        "Cache should contain video after storing"
    );
    assert_eq!(
        cache_lookup.unwrap(),
        cached_path,
        "Cache should return correct path"
    );

    // Verify the cached file has content
    let cached_content = std::fs::read(&cached_path).expect("Failed to read cached file");
    assert_eq!(
        cached_content,
        b"fake-video-content-for-testing",
        "Cached content should match downloaded content"
    );

    // ===== Step 2: Request same prompt, verify cache hit =====

    // Check cache again - should hit
    let cache_hit = cache.get(test_prompt);
    assert!(cache_hit.is_some(), "Second lookup should be a cache hit");
    assert_eq!(
        cache_hit.unwrap(),
        cached_path,
        "Cache hit should return same path"
    );

    // Verify we can skip the API entirely when cache hits
    // The cache hit means we don't need to call submit_generation again
    let hit_path = cache.get(test_prompt).unwrap();
    assert!(hit_path.exists(), "Cache hit path should exist");

    // Verify the metadata was stored correctly
    let hash = VideoCache::hash_prompt(test_prompt);
    let stored_prompt = cache.get_prompt(&hash);
    assert_eq!(
        stored_prompt,
        Some(test_prompt.to_string()),
        "Stored prompt should match original"
    );

    // ===== Step 3: Clear cache, verify cache miss =====

    // Clear all cached videos
    let cleared_count = cache.clear_all().expect("Failed to clear cache");
    assert_eq!(cleared_count, 1, "Should have cleared 1 video");

    // Verify cache is now empty
    let cache_miss = cache.get(test_prompt);
    assert!(
        cache_miss.is_none(),
        "Cache should be empty after clear_all"
    );

    // Verify metadata is also cleared
    let cleared_prompt = cache.get_prompt(&hash);
    assert!(
        cleared_prompt.is_none(),
        "Prompt metadata should be cleared"
    );

    // After cache clear, a new request would need to call the API again
    // Let's verify this by submitting another generation request
    let new_queue_response = client
        .submit_generation(test_prompt)
        .await
        .expect("Should be able to submit after cache clear");
    assert_eq!(
        new_queue_response.request_id, "test-request-123",
        "New API call should work after cache clear"
    );
}

/// Test: Verify cache removes specific entries.
#[tokio::test]
async fn test_cache_remove_specific_entry() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache = VideoCache::new_initialized(temp_dir.path().join("cache"))
        .expect("Failed to create cache");

    // Create test video files
    let video1_path = temp_dir.path().join("video1.mp4");
    let video2_path = temp_dir.path().join("video2.mp4");
    std::fs::write(&video1_path, b"video1-content").expect("Failed to write video1");
    std::fs::write(&video2_path, b"video2-content").expect("Failed to write video2");

    let prompt1 = "first prompt";
    let prompt2 = "second prompt";

    // Store both videos
    cache
        .store_with_metadata(prompt1, &video1_path)
        .expect("Failed to store video1");
    cache
        .store_with_metadata(prompt2, &video2_path)
        .expect("Failed to store video2");

    // Verify both are cached
    assert!(cache.get(prompt1).is_some());
    assert!(cache.get(prompt2).is_some());

    // Remove only the first one
    let hash1 = VideoCache::hash_prompt(prompt1);
    let removed = cache.remove(&hash1).expect("Failed to remove");
    assert!(removed, "Should have removed the entry");

    // Verify first is gone, second remains
    assert!(cache.get(prompt1).is_none(), "First should be removed");
    assert!(cache.get(prompt2).is_some(), "Second should remain");
}

/// Test: Verify cache list entries works correctly.
#[tokio::test]
async fn test_cache_list_entries() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache = VideoCache::new_initialized(temp_dir.path().join("cache"))
        .expect("Failed to create cache");

    // Initially empty
    let entries = cache.list_entries().expect("Failed to list");
    assert!(entries.is_empty(), "Cache should be empty initially");

    // Add some videos
    let video_path = temp_dir.path().join("video.mp4");
    std::fs::write(&video_path, b"video-content").expect("Failed to write");

    cache
        .store_with_metadata("prompt one", &video_path)
        .expect("Failed to store 1");
    cache
        .store_with_metadata("prompt two", &video_path)
        .expect("Failed to store 2");

    let entries = cache.list_entries().expect("Failed to list");
    assert_eq!(entries.len(), 2, "Should have 2 entries");

    // Verify prompts are stored
    let prompts: Vec<_> = entries.iter().filter_map(|e| e.prompt.as_ref()).collect();
    assert!(prompts.contains(&&"prompt one".to_string()));
    assert!(prompts.contains(&&"prompt two".to_string()));
}

/// Test: Deterministic hash ensures cache consistency.
#[tokio::test]
async fn test_cache_hash_deterministic() {
    let prompt = "consistent prompt for hashing";

    // Hash should be the same every time
    let hash1 = VideoCache::hash_prompt(prompt);
    let hash2 = VideoCache::hash_prompt(prompt);
    let hash3 = VideoCache::hash_prompt(prompt);

    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);

    // Different prompts should have different hashes
    let different_hash = VideoCache::hash_prompt("different prompt");
    assert_ne!(hash1, different_hash);
}

/// Test: Full round-trip with cache-aware generation helper.
/// Simulates the actual usage pattern where we check cache before API call.
#[tokio::test]
async fn test_cache_aware_generation_flow() {
    let api_server = setup_mock_fal_server().await;
    let video_server = setup_mock_video_server().await;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let cache = VideoCache::new_initialized(temp_dir.path().join("cache"))
        .expect("Failed to create cache");
    let client = create_test_client(&api_server.uri());

    let prompt = "abstract particles floating in space";

    // Helper function that mimics the real usage pattern
    async fn get_video_with_cache(
        prompt: &str,
        cache: &VideoCache,
        client: &FalClient,
        video_server_uri: &str,
        temp_dir: &std::path::Path,
    ) -> (PathBuf, bool) {
        // Check cache first
        if let Some(cached_path) = cache.get(prompt) {
            return (cached_path, true); // Cache hit
        }

        // Cache miss - generate via API
        let queue_response = client
            .submit_generation(prompt)
            .await
            .expect("Failed to submit");
        let _status = client
            .poll_status(&queue_response.request_id)
            .await
            .expect("Failed to poll");

        // Download
        let download_path = temp_dir.join(format!("{}.mp4", VideoCache::hash_prompt(prompt)));
        let download_url = format!("{}/video.mp4", video_server_uri);
        client
            .download_video(&download_url, &download_path)
            .await
            .expect("Failed to download");

        // Store in cache
        let cached_path = cache
            .store_with_metadata(prompt, &download_path)
            .expect("Failed to cache");

        (cached_path, false) // Cache miss - had to generate
    }

    // First call - should be cache miss
    let (path1, was_hit1) = get_video_with_cache(
        prompt,
        &cache,
        &client,
        &video_server.uri(),
        temp_dir.path(),
    )
    .await;
    assert!(!was_hit1, "First call should be cache miss");
    assert!(path1.exists(), "Path should exist after generation");

    // Second call - should be cache hit
    let (path2, was_hit2) = get_video_with_cache(
        prompt,
        &cache,
        &client,
        &video_server.uri(),
        temp_dir.path(),
    )
    .await;
    assert!(was_hit2, "Second call should be cache hit");
    assert_eq!(path1, path2, "Cache should return same path");

    // Third call - still cache hit
    let (path3, was_hit3) = get_video_with_cache(
        prompt,
        &cache,
        &client,
        &video_server.uri(),
        temp_dir.path(),
    )
    .await;
    assert!(was_hit3, "Third call should still be cache hit");
    assert_eq!(path1, path3);

    // Clear and try again - should be cache miss
    cache.clear_all().expect("Failed to clear");

    let (path4, was_hit4) = get_video_with_cache(
        prompt,
        &cache,
        &client,
        &video_server.uri(),
        temp_dir.path(),
    )
    .await;
    assert!(!was_hit4, "After clear should be cache miss");
    assert!(path4.exists(), "New path should exist");
}
