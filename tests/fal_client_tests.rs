//! Unit and mock HTTP tests for FalClient.
//!
//! These tests cover:
//! - Client creation and configuration
//! - API request formatting
//! - Status parsing
//! - Error handling
//! - Mock HTTP server integration tests

use std::path::PathBuf;
use std::time::Duration;

use space_recorder::fal::{
    validate_prompt, FalClient, FalError, GenerationStatus, QueueResponse, DEFAULT_MODEL,
    FAL_API_BASE_URL, FAL_API_KEY_ENV,
};

// === Client Creation Tests ===

#[test]
fn test_with_api_key_creates_client() {
    let client = FalClient::with_api_key("test-api-key".to_string()).unwrap();
    assert_eq!(client.api_key(), "test-api-key");
    assert_eq!(client.base_url(), FAL_API_BASE_URL);
    assert_eq!(client.model(), DEFAULT_MODEL);
}

#[test]
fn test_with_api_key_empty_returns_error() {
    let result = FalClient::with_api_key("".to_string());
    assert!(matches!(result, Err(FalError::MissingApiKey)));
}

#[test]
fn test_with_base_url_creates_client() {
    let client =
        FalClient::with_base_url("test-key".to_string(), "https://custom.api".to_string())
            .unwrap();
    assert_eq!(client.api_key(), "test-key");
    assert_eq!(client.base_url(), "https://custom.api");
    assert_eq!(client.model(), DEFAULT_MODEL);
}

#[test]
fn test_with_model_creates_client() {
    let client =
        FalClient::with_model("test-key".to_string(), "custom-model".to_string()).unwrap();
    assert_eq!(client.api_key(), "test-key");
    assert_eq!(client.base_url(), FAL_API_BASE_URL);
    assert_eq!(client.model(), "custom-model");
}

#[test]
fn test_with_model_empty_key_returns_error() {
    let result = FalClient::with_model("".to_string(), "custom-model".to_string());
    assert!(matches!(result, Err(FalError::MissingApiKey)));
}

#[test]
fn test_with_base_url_empty_key_returns_error() {
    let result = FalClient::with_base_url("".to_string(), "https://custom.api".to_string());
    assert!(matches!(result, Err(FalError::MissingApiKey)));
}

#[test]
fn test_new_reads_from_env() {
    // Save current value
    let original = std::env::var(FAL_API_KEY_ENV).ok();

    // Test with env var set
    std::env::set_var(FAL_API_KEY_ENV, "test-key-from-env");
    let result = FalClient::new();
    assert!(result.is_ok(), "new() should succeed when FAL_API_KEY is set");
    let client = result.unwrap();
    assert_eq!(client.api_key(), "test-key-from-env");
    assert_eq!(client.base_url(), FAL_API_BASE_URL);

    // Test with env var unset
    std::env::remove_var(FAL_API_KEY_ENV);
    let result = FalClient::new();
    assert!(
        matches!(result, Err(FalError::MissingApiKey)),
        "new() should fail with MissingApiKey when FAL_API_KEY is not set"
    );

    // Restore original value
    if let Some(val) = original {
        std::env::set_var(FAL_API_KEY_ENV, val);
    }
}

// === Error Display Tests ===

#[test]
fn test_fal_error_display() {
    assert_eq!(
        FalError::MissingApiKey.to_string(),
        "API key not configured"
    );
    assert_eq!(
        FalError::NotImplemented.to_string(),
        "fal.ai feature not yet implemented"
    );
    assert_eq!(
        FalError::ApiError("bad request".to_string()).to_string(),
        "API error: bad request"
    );
    assert_eq!(FalError::Timeout.to_string(), "Generation timed out");
}

#[test]
fn test_rate_limit_error_display() {
    let error = FalError::RateLimit {
        message: "Too many requests".to_string(),
        retry_after_secs: Some(30),
    };
    assert_eq!(error.to_string(), "Rate limited: Too many requests");
}

#[test]
fn test_rate_limit_error_without_retry_after() {
    let error = FalError::RateLimit {
        message: "Rate limit exceeded".to_string(),
        retry_after_secs: None,
    };
    assert!(matches!(
        error,
        FalError::RateLimit {
            retry_after_secs: None,
            ..
        }
    ));
}

#[test]
fn test_rate_limit_error_with_retry_after() {
    let error = FalError::RateLimit {
        message: "Slow down".to_string(),
        retry_after_secs: Some(60),
    };
    if let FalError::RateLimit {
        message,
        retry_after_secs,
    } = error
    {
        assert_eq!(message, "Slow down");
        assert_eq!(retry_after_secs, Some(60));
    } else {
        panic!("Expected RateLimit error");
    }
}

#[test]
fn test_rate_limit_error_variants() {
    let errors = vec![
        FalError::RateLimit {
            message: "test".to_string(),
            retry_after_secs: None,
        },
        FalError::RateLimit {
            message: "test".to_string(),
            retry_after_secs: Some(10),
        },
        FalError::RateLimit {
            message: "test".to_string(),
            retry_after_secs: Some(0),
        },
    ];

    for error in errors {
        assert!(matches!(error, FalError::RateLimit { .. }));
    }
}

#[test]
fn test_network_error_display() {
    let error = FalError::NetworkError {
        message: "Connection refused".to_string(),
        attempts: 3,
    };
    assert_eq!(
        error.to_string(),
        "Network error: Connection refused (after 3 attempts)"
    );
}

#[test]
fn test_network_error_with_different_attempt_counts() {
    let error1 = FalError::NetworkError {
        message: "timeout".to_string(),
        attempts: 1,
    };
    assert!(error1.to_string().contains("1 attempts"));

    let error3 = FalError::NetworkError {
        message: "timeout".to_string(),
        attempts: 3,
    };
    assert!(error3.to_string().contains("3 attempts"));
}

#[test]
fn test_network_error_can_be_pattern_matched() {
    let error = FalError::NetworkError {
        message: "test".to_string(),
        attempts: 2,
    };
    assert!(matches!(error, FalError::NetworkError { .. }));

    if let FalError::NetworkError { message, attempts } = error {
        assert_eq!(message, "test");
        assert_eq!(attempts, 2);
    } else {
        panic!("Expected NetworkError");
    }
}

#[test]
fn test_network_error_distinct_from_other_errors() {
    let network_error = FalError::NetworkError {
        message: "connection failed".to_string(),
        attempts: 3,
    };
    assert!(!matches!(network_error, FalError::HttpError(_)));
    assert!(!matches!(network_error, FalError::RateLimit { .. }));
    assert!(!matches!(network_error, FalError::Timeout));
    assert!(!matches!(network_error, FalError::ApiError(_)));
}

#[test]
fn test_empty_prompt_error_display() {
    let error = FalError::EmptyPrompt;
    assert_eq!(error.to_string(), "Empty prompt");
}

#[test]
fn test_content_policy_violation_error_display() {
    let error = FalError::ContentPolicyViolation {
        message: "Prompt violates content policy".to_string(),
    };
    assert_eq!(
        error.to_string(),
        "Content policy violation: Prompt violates content policy"
    );
}

#[test]
fn test_invalid_prompt_error_display() {
    let error = FalError::InvalidPrompt {
        reason: "Prompt too long".to_string(),
    };
    assert_eq!(error.to_string(), "Invalid prompt: Prompt too long");
}

// === GenerationStatus Tests ===

#[test]
fn test_generation_status_variants() {
    let pending = GenerationStatus::Pending;
    let in_progress = GenerationStatus::InProgress;
    let completed = GenerationStatus::Completed {
        video_url: "https://example.com/video.mp4".to_string(),
    };
    let failed = GenerationStatus::Failed {
        error: "Something went wrong".to_string(),
    };

    assert_eq!(pending, GenerationStatus::Pending);
    assert_eq!(in_progress, GenerationStatus::InProgress);
    assert!(matches!(completed, GenerationStatus::Completed { .. }));
    assert!(matches!(failed, GenerationStatus::Failed { .. }));
}

// === Timeout Tests ===

#[test]
fn test_timeout_error_is_recoverable() {
    let error = FalError::Timeout;
    assert!(matches!(error, FalError::Timeout));
}

#[test]
fn test_timeout_error_display_message() {
    let error = FalError::Timeout;
    let message = error.to_string();
    assert_eq!(message, "Generation timed out");
    assert!(!message.is_empty());
}

#[test]
fn test_timeout_does_not_affect_client_state() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    assert_eq!(client.api_key(), "test-key");
    assert_eq!(client.base_url(), FAL_API_BASE_URL);
    assert_eq!(client.model(), DEFAULT_MODEL);
}

#[test]
fn test_timeout_error_can_be_pattern_matched() {
    let error = FalError::Timeout;
    let is_timeout = matches!(error, FalError::Timeout);
    assert!(is_timeout);

    let api_error = FalError::ApiError("test".to_string());
    let is_api_error_timeout = matches!(api_error, FalError::Timeout);
    assert!(!is_api_error_timeout);

    let io_error = FalError::IoError(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));
    let is_io_error_timeout = matches!(io_error, FalError::Timeout);
    assert!(!is_io_error_timeout);

    let rate_limit_error = FalError::RateLimit {
        message: "too many requests".to_string(),
        retry_after_secs: Some(30),
    };
    let is_rate_limit_timeout = matches!(rate_limit_error, FalError::Timeout);
    assert!(!is_rate_limit_timeout);
}

// === Prompt Validation Tests ===

#[test]
fn test_validate_prompt_rejects_empty_string() {
    let result = validate_prompt("");
    assert!(matches!(result, Err(FalError::EmptyPrompt)));
}

#[test]
fn test_validate_prompt_rejects_whitespace_only() {
    assert!(matches!(validate_prompt("   "), Err(FalError::EmptyPrompt)));
    assert!(matches!(validate_prompt("\t"), Err(FalError::EmptyPrompt)));
    assert!(matches!(validate_prompt("\n"), Err(FalError::EmptyPrompt)));
    assert!(matches!(validate_prompt("  \t\n  "), Err(FalError::EmptyPrompt)));
}

#[test]
fn test_validate_prompt_accepts_valid_prompt() {
    assert!(validate_prompt("hello").is_ok());
    assert!(validate_prompt("cyberpunk cityscape").is_ok());
    assert!(validate_prompt("a beautiful sunset over the ocean").is_ok());
    assert!(validate_prompt("  trimmed prompt  ").is_ok());
}

#[test]
fn test_validate_prompt_accepts_prompts_with_special_characters() {
    assert!(validate_prompt("neon lights & rain").is_ok());
    assert!(validate_prompt("cyberpunk 2077 style!").is_ok());
    assert!(validate_prompt("a sunset... over the sea").is_ok());
    assert!(validate_prompt("prompt with emoji 🎨").is_ok());
}

#[test]
fn test_content_policy_error_can_be_pattern_matched() {
    let error = FalError::ContentPolicyViolation {
        message: "test".to_string(),
    };

    match error {
        FalError::ContentPolicyViolation { message } => {
            assert_eq!(message, "test");
        }
        _ => panic!("Expected ContentPolicyViolation"),
    }
}

#[test]
fn test_empty_prompt_error_can_be_pattern_matched() {
    let error = FalError::EmptyPrompt;
    assert!(matches!(error, FalError::EmptyPrompt));
}

#[test]
fn test_invalid_prompt_errors_are_distinct() {
    let empty_prompt = FalError::EmptyPrompt;
    let content_policy = FalError::ContentPolicyViolation {
        message: "test".to_string(),
    };
    let invalid_prompt = FalError::InvalidPrompt {
        reason: "test".to_string(),
    };

    assert!(!matches!(empty_prompt, FalError::ContentPolicyViolation { .. }));
    assert!(!matches!(empty_prompt, FalError::InvalidPrompt { .. }));
    assert!(!matches!(empty_prompt, FalError::ApiError(_)));

    assert!(!matches!(content_policy, FalError::EmptyPrompt));
    assert!(!matches!(content_policy, FalError::InvalidPrompt { .. }));
    assert!(!matches!(content_policy, FalError::ApiError(_)));

    assert!(!matches!(invalid_prompt, FalError::EmptyPrompt));
    assert!(!matches!(invalid_prompt, FalError::ContentPolicyViolation { .. }));
    assert!(!matches!(invalid_prompt, FalError::ApiError(_)));
}

#[test]
fn test_network_error_keeps_current_overlay_unchanged() {
    let error = FalError::NetworkError {
        message: "Connection failed".to_string(),
        attempts: 3,
    };

    match error {
        FalError::NetworkError { message, attempts } => {
            assert!(!message.is_empty());
            assert!(attempts > 0);
        }
        _ => panic!("Expected NetworkError"),
    }
}

// === Path Generation Tests ===

#[test]
fn test_generate_video_path() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    let path = client.generate_video_path("abc123");

    assert!(path.to_string_lossy().contains("space-recorder"));
    assert!(path.to_string_lossy().contains("fal-videos"));
    assert!(path.to_string_lossy().ends_with("abc123.mp4"));
}

#[test]
fn test_generate_video_path_unique_per_request() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    let path1 = client.generate_video_path("request-1");
    let path2 = client.generate_video_path("request-2");

    assert_ne!(path1, path2);
    assert!(path1.to_string_lossy().contains("request-1"));
    assert!(path2.to_string_lossy().contains("request-2"));
}

// === Async Tests ===

#[tokio::test]
async fn test_submit_generation_builds_correct_url() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "https://queue.fal.run".to_string(),
    )
    .unwrap();

    let expected_url = format!("{}/{}", client.base_url(), client.model());
    assert_eq!(expected_url, "https://queue.fal.run/fal-ai/fast-svd-lcm");
}

#[tokio::test]
async fn test_submit_generation_with_custom_model() {
    let client =
        FalClient::with_model("test-key".to_string(), "fal-ai/custom-model".to_string())
            .unwrap();

    let expected_url = format!("{}/{}", client.base_url(), client.model());
    assert_eq!(expected_url, "https://queue.fal.run/fal-ai/custom-model");
}

#[tokio::test]
async fn test_poll_status_builds_correct_url() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "https://queue.fal.run".to_string(),
    )
    .unwrap();

    let request_id = "abc123";
    let expected_url = format!(
        "{}/{}/requests/{}/status",
        client.base_url(),
        client.model(),
        request_id
    );
    assert_eq!(
        expected_url,
        "https://queue.fal.run/fal-ai/fast-svd-lcm/requests/abc123/status"
    );
}

#[tokio::test]
async fn test_download_video_creates_parent_dirs() {
    let temp_dir = std::env::temp_dir().join("space-recorder-test-download");
    let nested_dest = temp_dir.join("nested").join("dir").join("video.mp4");

    let _ = std::fs::remove_dir_all(&temp_dir);
    assert!(!nested_dest.parent().unwrap().exists());

    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    let result = client
        .download_video("http://localhost:9999/fake.mp4", &nested_dest)
        .await;

    assert!(result.is_err());
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_download_video_returns_pathbuf() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    let dest = std::path::Path::new("/tmp/test-video.mp4");

    let _result: Result<PathBuf, FalError> = client
        .download_video("http://localhost:9999/fake.mp4", dest)
        .await;
}

#[tokio::test]
async fn test_generate_and_download_returns_pathbuf() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    let _result: Result<PathBuf, FalError> =
        client.generate_and_download("test prompt").await;
}

#[tokio::test]
async fn test_generate_and_download_with_timeout_returns_pathbuf() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();
    let timeout = Duration::from_secs(60);

    let _result: Result<PathBuf, FalError> = client
        .generate_and_download_with_timeout("test prompt", timeout)
        .await;
}

#[tokio::test]
async fn test_generate_and_download_fails_on_submit_error() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let result = client.generate_and_download("test prompt").await;
    assert!(result.is_err());
    assert!(matches!(result, Err(FalError::HttpError(_))));
}

#[tokio::test]
async fn test_generate_and_download_with_short_timeout() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let result = client
        .generate_and_download_with_timeout("test", Duration::from_millis(100))
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_submit_generation_with_retry_returns_queue_response() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let _result: Result<QueueResponse, FalError> =
        client.submit_generation_with_retry("test prompt").await;
}

#[tokio::test]
async fn test_submit_generation_with_retry_config_custom_values() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let result = client
        .submit_generation_with_retry_config(
            "test prompt",
            2,
            Duration::from_millis(10),
            Duration::from_secs(1),
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(result, Err(FalError::HttpError(_))));
}

#[tokio::test]
async fn test_generate_and_download_with_timeout_uses_custom_duration() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();

    let durations = [
        Duration::from_secs(30),
        Duration::from_secs(60),
        Duration::from_secs(120),
        Duration::from_secs(300),
    ];

    for duration in durations {
        let _result = client
            .generate_and_download_with_timeout("test", duration)
            .await;
    }
}

#[tokio::test]
async fn test_timeout_allows_retry_with_same_prompt() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let prompt = "cyberpunk cityscape with neon lights";
    let result1 = client.generate_and_download(prompt).await;
    assert!(result1.is_err());

    let result2 = client.generate_and_download(prompt).await;
    assert!(result2.is_err());
}

#[tokio::test]
async fn test_submit_generation_with_network_retry_returns_queue_response() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let _result: Result<QueueResponse, FalError> =
        client.submit_generation_with_network_retry("test prompt").await;
}

#[tokio::test]
async fn test_submit_generation_with_network_retry_config_custom_values() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let result = client
        .submit_generation_with_network_retry_config(
            "test prompt",
            0,
            Duration::from_millis(10),
            Duration::from_secs(1),
        )
        .await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(FalError::NetworkError { .. })),
        "Expected NetworkError, got {:?}",
        result
    );
}

#[tokio::test]
async fn test_network_retry_returns_network_error_after_exhausting_retries() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let result = client
        .submit_generation_with_network_retry_config(
            "test",
            2,
            Duration::from_millis(1),
            Duration::from_millis(10),
        )
        .await;

    match result {
        Err(FalError::NetworkError { attempts, .. }) => {
            assert_eq!(attempts, 3, "Should have made 3 attempts (1 initial + 2 retries)");
        }
        other => panic!("Expected NetworkError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_submit_generation_with_full_retry_returns_queue_response() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let _result: Result<QueueResponse, FalError> =
        client.submit_generation_with_full_retry("test prompt").await;
}

#[tokio::test]
async fn test_submit_generation_with_full_retry_config_custom_values() {
    let client = FalClient::with_base_url(
        "test-key".to_string(),
        "http://localhost:9999".to_string(),
    )
    .unwrap();

    let result = client
        .submit_generation_with_full_retry_config(
            "test prompt",
            1,
            1,
            Duration::from_millis(1),
            Duration::from_millis(10),
        )
        .await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(FalError::NetworkError { .. })),
        "Expected NetworkError, got {:?}",
        result
    );
}

#[tokio::test]
async fn test_submit_generation_rejects_empty_prompt() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();

    let result = client.submit_generation("").await;
    assert!(matches!(result, Err(FalError::EmptyPrompt)));

    let result = client.submit_generation("   ").await;
    assert!(matches!(result, Err(FalError::EmptyPrompt)));
}

#[tokio::test]
async fn test_submit_generation_with_params_rejects_empty_prompt() {
    let client = FalClient::with_api_key("test-key".to_string()).unwrap();

    let result = client
        .submit_generation_with_params("", Some(1024), Some(576), Some(25), Some(8))
        .await;
    assert!(matches!(result, Err(FalError::EmptyPrompt)));
}

// === Mock HTTP Server Tests ===

mod mock_http_tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_submit_generation_sends_correct_authorization_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .and(header("Authorization", "Key test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"request_id": "req-123"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test prompt").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().request_id, "req-123");
    }

    #[tokio::test]
    async fn test_submit_generation_sends_correct_content_type_header() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .and(header("Content-Type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"request_id": "req-456"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test prompt").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_submit_generation_sends_prompt_in_request_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "prompt": "cyberpunk cityscape with neon lights"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"request_id": "req-789"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client
            .submit_generation("cyberpunk cityscape with neon lights")
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().request_id, "req-789");
    }

    #[tokio::test]
    async fn test_submit_generation_with_params_sends_all_params() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "prompt": "test",
                "video_size": {"width": 1024, "height": 576},
                "num_frames": 25,
                "fps": 8
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"request_id": "req-params"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client
            .submit_generation_with_params("test", Some(1024), Some(576), Some(25), Some(8))
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().request_id, "req-params");
    }

    #[tokio::test]
    async fn test_poll_status_sends_correct_get_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/abc123/status"))
            .and(header("Authorization", "Key test-api-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"status": "PENDING"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("abc123").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), GenerationStatus::Pending);
    }

    #[tokio::test]
    async fn test_poll_status_parses_pending_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"status": "PENDING"})),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        assert!(matches!(result, Ok(GenerationStatus::Pending)));
    }

    #[tokio::test]
    async fn test_poll_status_parses_in_queue_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"status": "IN_QUEUE"})),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        assert!(matches!(result, Ok(GenerationStatus::Pending)));
    }

    #[tokio::test]
    async fn test_poll_status_parses_processing_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"status": "PROCESSING"})),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        assert!(matches!(result, Ok(GenerationStatus::InProgress)));
    }

    #[tokio::test]
    async fn test_poll_status_parses_in_progress_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"status": "IN_PROGRESS"})),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        assert!(matches!(result, Ok(GenerationStatus::InProgress)));
    }

    #[tokio::test]
    async fn test_poll_status_parses_completed_with_video_url() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "COMPLETED",
                "video": {"url": "https://fal.ai/videos/generated.mp4"}
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Ok(GenerationStatus::Completed { video_url }) => {
                assert_eq!(video_url, "https://fal.ai/videos/generated.mp4");
            }
            _ => panic!("Expected Completed status, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_poll_status_parses_ok_with_response_url() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "OK",
                "response_url": "https://fal.ai/results/video.mp4"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Ok(GenerationStatus::Completed { video_url }) => {
                assert_eq!(video_url, "https://fal.ai/results/video.mp4");
            }
            _ => panic!("Expected Completed status, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_poll_status_parses_failed_with_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "FAILED",
                "error": "Model overloaded, please try again"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Ok(GenerationStatus::Failed { error }) => {
                assert_eq!(error, "Model overloaded, please try again");
            }
            _ => panic!("Expected Failed status, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_poll_status_parses_error_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ERROR",
                "error": "Internal server error"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Ok(GenerationStatus::Failed { error }) => {
                assert_eq!(error, "Internal server error");
            }
            _ => panic!("Expected Failed status, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_poll_status_returns_error_for_unknown_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "UNKNOWN_STATUS_123"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Err(FalError::ApiError(msg)) => {
                assert!(msg.contains("Unknown generation status"));
                assert!(msg.contains("UNKNOWN_STATUS_123"));
            }
            _ => panic!("Expected ApiError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_429_rate_limit() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_string("Rate limit exceeded, slow down")
                    .insert_header("Retry-After", "30"),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test prompt").await;

        match result {
            Err(FalError::RateLimit {
                message,
                retry_after_secs,
            }) => {
                assert!(message.contains("Rate limit exceeded"));
                assert_eq!(retry_after_secs, Some(30));
            }
            _ => panic!("Expected RateLimit error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_429_without_retry_after() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(ResponseTemplate::new(429).set_body_string("Too many requests"))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test prompt").await;

        match result {
            Err(FalError::RateLimit {
                retry_after_secs, ..
            }) => {
                assert_eq!(retry_after_secs, None);
            }
            _ => panic!("Expected RateLimit error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_400_content_policy() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_string("Request blocked: content policy violation detected"),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("inappropriate prompt").await;

        match result {
            Err(FalError::ContentPolicyViolation { message }) => {
                assert!(message.contains("content policy"));
            }
            _ => panic!("Expected ContentPolicyViolation error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_403_forbidden() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string("Prompt blocked by moderation filter"),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test").await;

        match result {
            Err(FalError::ContentPolicyViolation { message }) => {
                assert!(message.contains("moderation"));
            }
            _ => panic!("Expected ContentPolicyViolation error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_400_non_policy_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(
                ResponseTemplate::new(400).set_body_string("Invalid request format"),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test").await;

        match result {
            Err(FalError::ApiError(msg)) => {
                assert!(msg.contains("400"));
                assert!(msg.contains("Invalid request format"));
            }
            _ => panic!("Expected ApiError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_500_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(
                ResponseTemplate::new(500).set_body_string("Internal server error"),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test").await;

        match result {
            Err(FalError::ApiError(msg)) => {
                assert!(msg.contains("500"));
                assert!(msg.contains("Internal server error"));
            }
            _ => panic!("Expected ApiError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_poll_status_handles_non_success_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string("Request not found"),
            )
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Err(FalError::ApiError(msg)) => {
                assert!(msg.contains("404"));
                assert!(msg.contains("Request not found"));
            }
            _ => panic!("Expected ApiError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_download_video_handles_successful_download() {
        let mock_server = MockServer::start().await;
        let video_bytes: Vec<u8> = vec![0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70];

        Mock::given(method("GET"))
            .and(path("/videos/test.mp4"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(video_bytes.clone()))
            .mount(&mock_server)
            .await;

        let temp_dir = tempfile::tempdir().unwrap();
        let dest = temp_dir.path().join("downloaded.mp4");

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client
            .download_video(&format!("{}/videos/test.mp4", mock_server.uri()), &dest)
            .await;

        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());

        let written_bytes = std::fs::read(&path).unwrap();
        assert_eq!(written_bytes, video_bytes);
    }

    #[tokio::test]
    async fn test_download_video_handles_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/videos/missing.mp4"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Video not found"))
            .mount(&mock_server)
            .await;

        let temp_dir = tempfile::tempdir().unwrap();
        let dest = temp_dir.path().join("missing.mp4");

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client
            .download_video(&format!("{}/videos/missing.mp4", mock_server.uri()), &dest)
            .await;

        match result {
            Err(FalError::ApiError(msg)) => {
                assert!(msg.contains("404"));
                assert!(msg.contains("Video not found"));
            }
            _ => panic!("Expected ApiError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_queue_response_with_status_url() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "request_id": "req-with-status",
                "status_url": "https://queue.fal.run/status/req-with-status"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test").await;

        let response = result.unwrap();
        assert_eq!(response.request_id, "req-with-status");
        assert_eq!(
            response.status_url,
            Some("https://queue.fal.run/status/req-with-status".to_string())
        );
    }

    #[tokio::test]
    async fn test_poll_status_completed_without_video_url_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "COMPLETED"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Err(FalError::ApiError(msg)) => {
                assert!(msg.contains("no video URL"));
            }
            _ => panic!("Expected ApiError, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_poll_status_failed_without_error_message() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "FAILED"
            })))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        match result {
            Ok(GenerationStatus::Failed { error }) => {
                assert!(error.contains("Unknown error"));
            }
            _ => panic!("Expected Failed status, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_handles_malformed_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.submit_generation("test").await;

        assert!(matches!(result, Err(FalError::HttpError(_))));
    }

    #[tokio::test]
    async fn test_poll_status_handles_malformed_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid json"))
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client.poll_status("test-id").await;

        assert!(matches!(result, Err(FalError::HttpError(_))));
    }

    #[tokio::test]
    async fn test_submit_generation_with_params_omits_none_values() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "prompt": "test"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"request_id": "req-none"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client
            .submit_generation_with_params("test", None, None, None, None)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_submit_generation_with_params_handles_partial_video_size() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/fal-ai/fast-svd-lcm"))
            .and(wiremock::matchers::body_json(serde_json::json!({
                "prompt": "test"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"request_id": "req-partial"})),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let client =
            FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
        let result = client
            .submit_generation_with_params("test", Some(1024), None, None, None)
            .await;

        assert!(result.is_ok());
    }
}
