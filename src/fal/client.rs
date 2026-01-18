//! FalClient - handles communication with fal.ai API.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use super::retry::{
    calculate_backoff, is_transient_network_error, parse_retry_after, DEFAULT_BACKOFF_BASE,
    DEFAULT_BACKOFF_MAX, DEFAULT_MAX_RETRIES, DEFAULT_NETWORK_RETRIES,
};

/// The environment variable name for the fal.ai API key.
pub const FAL_API_KEY_ENV: &str = "FAL_API_KEY";

/// Default base URL for the fal.ai API.
pub const FAL_API_BASE_URL: &str = "https://queue.fal.run";

/// Default model for video generation.
pub const DEFAULT_MODEL: &str = "fal-ai/fast-svd-lcm";

/// Default timeout for HTTP requests (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default connection timeout (10 seconds).
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default timeout for video generation (120 seconds).
const DEFAULT_GENERATION_TIMEOUT: Duration = Duration::from_secs(120);

/// Default polling interval for status checks (2 seconds).
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);


/// HTTP status code for rate limiting.
const HTTP_STATUS_TOO_MANY_REQUESTS: u16 = 429;

/// HTTP status code for bad request (often content policy).
const HTTP_STATUS_BAD_REQUEST: u16 = 400;

/// HTTP status code for forbidden (content policy violation).
const HTTP_STATUS_FORBIDDEN: u16 = 403;

/// Keywords that indicate a content policy violation in error messages.
const CONTENT_POLICY_KEYWORDS: &[&str] = &[
    "content policy",
    "policy violation",
    "inappropriate",
    "not allowed",
    "prohibited",
    "blocked",
    "unsafe",
    "violates",
    "moderation",
    "nsfw",
];


/// Check if an error message indicates a content policy violation.
///
/// Searches for common keywords that indicate content policy issues.
fn is_content_policy_error(error_text: &str) -> bool {
    let lower = error_text.to_lowercase();
    CONTENT_POLICY_KEYWORDS.iter().any(|keyword| lower.contains(keyword))
}

/// Validate a prompt before sending to the API.
///
/// Checks for:
/// - Empty or whitespace-only prompts
///
/// # Arguments
/// * `prompt` - The prompt to validate
///
/// # Returns
/// `Ok(())` if the prompt is valid, `Err(FalError)` otherwise.
pub fn validate_prompt(prompt: &str) -> Result<(), FalError> {
    let trimmed = prompt.trim();

    // Check for empty prompt
    if trimmed.is_empty() {
        return Err(FalError::EmptyPrompt);
    }

    Ok(())
}


/// Request body for video generation.
#[derive(Debug, Serialize)]
struct GenerateRequest {
    /// The text prompt to generate video from.
    prompt: String,
    /// Video width in pixels (default 1024).
    #[serde(skip_serializing_if = "Option::is_none")]
    video_size: Option<VideoSize>,
    /// Number of frames to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    num_frames: Option<u32>,
    /// Frames per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    fps: Option<u32>,
}

/// Video size configuration.
#[derive(Debug, Serialize)]
struct VideoSize {
    width: u32,
    height: u32,
}

/// Response from queue submission.
#[derive(Debug, Deserialize)]
pub struct QueueResponse {
    /// The unique request ID for polling.
    pub request_id: String,
    /// URL to check status (optional).
    #[serde(default)]
    pub status_url: Option<String>,
}

/// Response from status polling endpoint.
#[derive(Debug, Deserialize)]
struct StatusResponse {
    /// The status of the generation request.
    status: String,
    /// Response ID (same as request_id).
    #[serde(default)]
    response_url: Option<String>,
    /// Video output when completed (nested in "video" field).
    #[serde(default)]
    video: Option<VideoOutput>,
    /// Error message if generation failed.
    #[serde(default)]
    error: Option<String>,
}

/// Video output from successful generation.
#[derive(Debug, Deserialize)]
struct VideoOutput {
    /// URL to download the generated video.
    url: String,
}

/// Status of a video generation request.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerationStatus {
    /// Request is queued for processing.
    Pending,
    /// Video is being generated.
    InProgress,
    /// Generation completed successfully.
    Completed { video_url: String },
    /// Generation failed with an error.
    Failed { error: String },
}

/// Client for communicating with the fal.ai API.
pub struct FalClient {
    api_key: String,
    base_url: String,
    model: String,
    http_client: reqwest::Client,
}

impl FalClient {
    /// Create a new FalClient by reading API key from environment.
    ///
    /// Reads the `FAL_API_KEY` environment variable and creates an HTTP client
    /// with reasonable timeouts.
    ///
    /// # Errors
    ///
    /// Returns `FalError::MissingApiKey` if the `FAL_API_KEY` environment
    /// variable is not set.
    pub fn new() -> Result<Self, FalError> {
        let api_key = std::env::var(FAL_API_KEY_ENV).map_err(|_| FalError::MissingApiKey)?;
        Self::with_api_key(api_key)
    }

    /// Create a new FalClient with an explicit API key.
    ///
    /// This is useful for testing or when the API key is obtained from
    /// a source other than environment variables.
    pub fn with_api_key(api_key: String) -> Result<Self, FalError> {
        if api_key.is_empty() {
            return Err(FalError::MissingApiKey);
        }

        let http_client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            api_key,
            base_url: FAL_API_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            http_client,
        })
    }

    /// Create a new FalClient with a custom base URL.
    ///
    /// Useful for testing against a mock server.
    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self, FalError> {
        if api_key.is_empty() {
            return Err(FalError::MissingApiKey);
        }

        let http_client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            api_key,
            base_url,
            model: DEFAULT_MODEL.to_string(),
            http_client,
        })
    }

    /// Create a new FalClient with a custom model.
    pub fn with_model(api_key: String, model: String) -> Result<Self, FalError> {
        if api_key.is_empty() {
            return Err(FalError::MissingApiKey);
        }

        let http_client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .build()?;

        Ok(Self {
            api_key,
            base_url: FAL_API_BASE_URL.to_string(),
            model,
            http_client,
        })
    }

    /// Get the API key.
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the model.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Submit a video generation request to the fal.ai queue.
    ///
    /// Sends a POST request to the fal.ai queue API with the given prompt
    /// and video parameters. Returns a request ID that can be used to poll
    /// for completion status.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    ///
    /// # Returns
    ///
    /// A `QueueResponse` containing the request ID for polling.
    ///
    /// # Errors
    ///
    /// Returns `FalError::EmptyPrompt` if the prompt is empty,
    /// `FalError::ContentPolicyViolation` if the API rejects the prompt for content policy,
    /// `FalError::RateLimit` if the API returns a 429 status code,
    /// `FalError::ApiError` if the API returns another error response,
    /// or `FalError::HttpError` if the request fails.
    pub async fn submit_generation(&self, prompt: &str) -> Result<QueueResponse, FalError> {
        // Validate prompt before sending to API
        validate_prompt(prompt)?;

        let url = format!("{}/{}", self.base_url, self.model);

        let request_body = GenerateRequest {
            prompt: prompt.to_string(),
            video_size: None,
            num_frames: None,
            fps: None,
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Key {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();

            // Check for rate limit (429 Too Many Requests)
            if status.as_u16() == HTTP_STATUS_TOO_MANY_REQUESTS {
                let retry_after_secs = parse_retry_after(&response);
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Rate limit exceeded".to_string());
                log::warn!(
                    "Rate limited by fal.ai API. Retry-After: {:?} seconds",
                    retry_after_secs
                );
                return Err(FalError::RateLimit {
                    message: error_text,
                    retry_after_secs,
                });
            }

            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Check for content policy violations (400 Bad Request or 403 Forbidden)
            if (status.as_u16() == HTTP_STATUS_BAD_REQUEST || status.as_u16() == HTTP_STATUS_FORBIDDEN)
                && is_content_policy_error(&error_text)
            {
                log::warn!("Prompt rejected by content policy: {}", error_text);
                return Err(FalError::ContentPolicyViolation {
                    message: error_text,
                });
            }

            return Err(FalError::ApiError(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        let queue_response: QueueResponse = response.json().await?;
        Ok(queue_response)
    }

    /// Submit a video generation request with automatic retry on rate limit.
    ///
    /// This method wraps `submit_generation` and automatically retries with
    /// exponential backoff when the API returns a 429 rate limit response.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    ///
    /// # Returns
    ///
    /// A `QueueResponse` containing the request ID for polling.
    ///
    /// # Errors
    ///
    /// Returns `FalError::RateLimit` if all retry attempts are exhausted,
    /// `FalError::ApiError` for other API errors, or `FalError::HttpError`
    /// if the request fails.
    pub async fn submit_generation_with_retry(
        &self,
        prompt: &str,
    ) -> Result<QueueResponse, FalError> {
        self.submit_generation_with_retry_config(
            prompt,
            DEFAULT_MAX_RETRIES,
            DEFAULT_BACKOFF_BASE,
            DEFAULT_BACKOFF_MAX,
        )
        .await
    }

    /// Submit a video generation request with custom retry configuration.
    ///
    /// Like `submit_generation_with_retry`, but allows customizing the retry
    /// behavior including max attempts and backoff timing.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    /// * `max_retries` - Maximum number of retry attempts
    /// * `backoff_base` - Base delay for exponential backoff
    /// * `backoff_max` - Maximum delay cap for backoff
    pub async fn submit_generation_with_retry_config(
        &self,
        prompt: &str,
        max_retries: u32,
        backoff_base: Duration,
        backoff_max: Duration,
    ) -> Result<QueueResponse, FalError> {
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match self.submit_generation(prompt).await {
                Ok(response) => return Ok(response),
                Err(FalError::RateLimit {
                    message,
                    retry_after_secs,
                }) => {
                    last_error = Some(FalError::RateLimit {
                        message: message.clone(),
                        retry_after_secs,
                    });

                    if attempt >= max_retries {
                        log::error!(
                            "Rate limit exceeded after {} attempts. Giving up.",
                            attempt + 1
                        );
                        break;
                    }

                    // Calculate delay: use Retry-After header if provided, else exponential backoff
                    let delay = if let Some(retry_secs) = retry_after_secs {
                        Duration::from_secs(retry_secs).min(backoff_max)
                    } else {
                        calculate_backoff(attempt, backoff_base, backoff_max)
                    };

                    log::info!(
                        "Rate limited (attempt {}/{}). Retrying in {:?}...",
                        attempt + 1,
                        max_retries + 1,
                        delay
                    );

                    tokio::time::sleep(delay).await;
                }
                Err(e) => {
                    // Non-rate-limit errors are not retried
                    return Err(e);
                }
            }
        }

        // Return the last rate limit error
        Err(last_error.unwrap_or_else(|| FalError::RateLimit {
            message: "Rate limit exceeded".to_string(),
            retry_after_secs: None,
        }))
    }

    /// Submit a video generation request with automatic retry on network errors.
    ///
    /// This method wraps `submit_generation` and automatically retries with
    /// exponential backoff when transient network errors occur (connection
    /// failures, timeouts, etc.).
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    ///
    /// # Returns
    ///
    /// A `QueueResponse` containing the request ID for polling.
    ///
    /// # Errors
    ///
    /// Returns `FalError::NetworkError` if all retry attempts are exhausted,
    /// `FalError::RateLimit` for rate limiting (not retried by this method),
    /// `FalError::ApiError` for other API errors, or `FalError::HttpError`
    /// for non-transient HTTP errors.
    pub async fn submit_generation_with_network_retry(
        &self,
        prompt: &str,
    ) -> Result<QueueResponse, FalError> {
        self.submit_generation_with_network_retry_config(
            prompt,
            DEFAULT_NETWORK_RETRIES,
            DEFAULT_BACKOFF_BASE,
            DEFAULT_BACKOFF_MAX,
        )
        .await
    }

    /// Submit a video generation request with custom network retry configuration.
    ///
    /// Like `submit_generation_with_network_retry`, but allows customizing the retry
    /// behavior including max attempts and backoff timing.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    /// * `max_retries` - Maximum number of retry attempts (default 3)
    /// * `backoff_base` - Base delay for exponential backoff
    /// * `backoff_max` - Maximum delay cap for backoff
    pub async fn submit_generation_with_network_retry_config(
        &self,
        prompt: &str,
        max_retries: u32,
        backoff_base: Duration,
        backoff_max: Duration,
    ) -> Result<QueueResponse, FalError> {
        let mut last_network_error_msg = String::new();
        let mut attempt_count = 0u32;

        for attempt in 0..=max_retries {
            attempt_count = attempt + 1;

            match self.submit_generation(prompt).await {
                Ok(response) => return Ok(response),
                Err(FalError::HttpError(ref http_err)) if is_transient_network_error(http_err) => {
                    last_network_error_msg = http_err.to_string();

                    if attempt >= max_retries {
                        log::error!(
                            "Network error after {} attempts. Giving up. Error: {}",
                            attempt + 1,
                            http_err
                        );
                        break;
                    }

                    let delay = calculate_backoff(attempt, backoff_base, backoff_max);

                    log::warn!(
                        "Network error (attempt {}/{}): {}. Retrying in {:?}...",
                        attempt + 1,
                        max_retries + 1,
                        http_err,
                        delay
                    );

                    tokio::time::sleep(delay).await;
                }
                Err(e) => {
                    // Non-transient errors (rate limits, API errors, etc.) are not retried
                    return Err(e);
                }
            }
        }

        // Return a clear network error with attempt count
        Err(FalError::NetworkError {
            message: if last_network_error_msg.is_empty() {
                "Connection failed".to_string()
            } else {
                last_network_error_msg
            },
            attempts: attempt_count,
        })
    }

    /// Submit a video generation request with retry on both network errors and rate limits.
    ///
    /// This is the most resilient method, combining retries for transient network
    /// errors AND rate limiting responses. Use this for maximum reliability.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    ///
    /// # Errors
    ///
    /// Returns `FalError::NetworkError` if all network retry attempts are exhausted,
    /// `FalError::RateLimit` if all rate limit retry attempts are exhausted,
    /// or `FalError::ApiError` for non-retryable API errors.
    pub async fn submit_generation_with_full_retry(&self, prompt: &str) -> Result<QueueResponse, FalError> {
        self.submit_generation_with_full_retry_config(
            prompt,
            DEFAULT_NETWORK_RETRIES,
            DEFAULT_MAX_RETRIES,
            DEFAULT_BACKOFF_BASE,
            DEFAULT_BACKOFF_MAX,
        )
        .await
    }

    /// Submit a video generation request with custom retry configuration for both
    /// network errors and rate limits.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    /// * `network_retries` - Maximum retry attempts for network errors
    /// * `rate_limit_retries` - Maximum retry attempts for rate limiting
    /// * `backoff_base` - Base delay for exponential backoff
    /// * `backoff_max` - Maximum delay cap for backoff
    pub async fn submit_generation_with_full_retry_config(
        &self,
        prompt: &str,
        network_retries: u32,
        rate_limit_retries: u32,
        backoff_base: Duration,
        backoff_max: Duration,
    ) -> Result<QueueResponse, FalError> {
        let mut last_error: Option<FalError>;
        let mut network_attempt = 0u32;
        let mut rate_limit_attempt = 0u32;

        loop {
            match self.submit_generation(prompt).await {
                Ok(response) => return Ok(response),

                // Handle transient network errors
                Err(FalError::HttpError(ref http_err)) if is_transient_network_error(http_err) => {
                    network_attempt += 1;
                    last_error = Some(FalError::NetworkError {
                        message: http_err.to_string(),
                        attempts: network_attempt,
                    });

                    if network_attempt > network_retries {
                        log::error!(
                            "Network error after {} attempts. Giving up. Error: {}",
                            network_attempt,
                            http_err
                        );
                        break;
                    }

                    let delay = calculate_backoff(network_attempt - 1, backoff_base, backoff_max);

                    log::warn!(
                        "Network error (attempt {}/{}): {}. Retrying in {:?}...",
                        network_attempt,
                        network_retries + 1,
                        http_err,
                        delay
                    );

                    tokio::time::sleep(delay).await;
                }

                // Handle rate limiting
                Err(FalError::RateLimit {
                    ref message,
                    retry_after_secs,
                }) => {
                    rate_limit_attempt += 1;
                    last_error = Some(FalError::RateLimit {
                        message: message.clone(),
                        retry_after_secs,
                    });

                    if rate_limit_attempt > rate_limit_retries {
                        log::error!(
                            "Rate limit exceeded after {} attempts. Giving up.",
                            rate_limit_attempt
                        );
                        break;
                    }

                    let delay = if let Some(retry_secs) = retry_after_secs {
                        Duration::from_secs(retry_secs).min(backoff_max)
                    } else {
                        calculate_backoff(rate_limit_attempt - 1, backoff_base, backoff_max)
                    };

                    log::info!(
                        "Rate limited (attempt {}/{}). Retrying in {:?}...",
                        rate_limit_attempt,
                        rate_limit_retries + 1,
                        delay
                    );

                    tokio::time::sleep(delay).await;
                }

                // Non-retryable errors
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // Return the last error
        Err(last_error.unwrap_or_else(|| FalError::NetworkError {
            message: "Request failed".to_string(),
            attempts: network_attempt.max(1),
        }))
    }

    /// Submit a video generation request with custom parameters.
    ///
    /// Like `submit_generation`, but allows specifying video dimensions,
    /// frame count, and FPS.
    ///
    /// # Errors
    ///
    /// Returns `FalError::EmptyPrompt` if the prompt is empty,
    /// `FalError::ContentPolicyViolation` if the API rejects the prompt for content policy,
    /// `FalError::RateLimit` if the API returns a 429 status code,
    /// `FalError::ApiError` if the API returns another error response,
    /// or `FalError::HttpError` if the request fails.
    pub async fn submit_generation_with_params(
        &self,
        prompt: &str,
        width: Option<u32>,
        height: Option<u32>,
        num_frames: Option<u32>,
        fps: Option<u32>,
    ) -> Result<QueueResponse, FalError> {
        // Validate prompt before sending to API
        validate_prompt(prompt)?;

        let url = format!("{}/{}", self.base_url, self.model);

        let video_size = match (width, height) {
            (Some(w), Some(h)) => Some(VideoSize { width: w, height: h }),
            _ => None,
        };

        let request_body = GenerateRequest {
            prompt: prompt.to_string(),
            video_size,
            num_frames,
            fps,
        };

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Key {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();

            // Check for rate limit (429 Too Many Requests)
            if status.as_u16() == HTTP_STATUS_TOO_MANY_REQUESTS {
                let retry_after_secs = parse_retry_after(&response);
                let error_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Rate limit exceeded".to_string());
                log::warn!(
                    "Rate limited by fal.ai API. Retry-After: {:?} seconds",
                    retry_after_secs
                );
                return Err(FalError::RateLimit {
                    message: error_text,
                    retry_after_secs,
                });
            }

            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Check for content policy violations (400 Bad Request or 403 Forbidden)
            if (status.as_u16() == HTTP_STATUS_BAD_REQUEST || status.as_u16() == HTTP_STATUS_FORBIDDEN)
                && is_content_policy_error(&error_text)
            {
                log::warn!("Prompt rejected by content policy: {}", error_text);
                return Err(FalError::ContentPolicyViolation {
                    message: error_text,
                });
            }

            return Err(FalError::ApiError(format!(
                "API request failed with status {}: {}",
                status, error_text
            )));
        }

        let queue_response: QueueResponse = response.json().await?;
        Ok(queue_response)
    }

    /// Generate video from text prompt and download to a local file.
    ///
    /// This is the main end-to-end method that combines all steps:
    /// 1. Submits the generation request to the fal.ai queue
    /// 2. Polls the status until completion (or timeout)
    /// 3. Downloads the generated video to disk
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    ///
    /// # Returns
    ///
    /// The path to the downloaded video file on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The API request fails (`FalError::ApiError` or `FalError::HttpError`)
    /// - Generation times out after 120 seconds (`FalError::Timeout`)
    /// - Generation fails on the server (`FalError::ApiError`)
    /// - Video download fails (`FalError::HttpError` or `FalError::IoError`)
    pub async fn generate_and_download(&self, prompt: &str) -> Result<PathBuf, FalError> {
        self.generate_and_download_with_timeout(prompt, DEFAULT_GENERATION_TIMEOUT)
            .await
    }

    /// Generate video from text prompt with a custom timeout.
    ///
    /// Same as `generate_and_download`, but allows specifying a custom timeout
    /// for the entire generation process.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The text prompt describing the video to generate
    /// * `timeout` - Maximum duration to wait for generation to complete
    ///
    /// # Returns
    ///
    /// The path to the downloaded video file on success.
    pub async fn generate_and_download_with_timeout(
        &self,
        prompt: &str,
        timeout: Duration,
    ) -> Result<PathBuf, FalError> {
        use tokio::time::Instant;

        log::info!("Starting video generation for prompt: {}", prompt);

        // Step 1: Submit generation request
        log::info!("Submitting generation request...");
        let queue_response = self.submit_generation(prompt).await?;
        let request_id = &queue_response.request_id;
        log::info!("Generation submitted, request_id: {}", request_id);

        // Step 2: Poll for completion with timeout
        log::info!("Polling for completion (timeout: {:?})...", timeout);
        let start_time = Instant::now();
        let video_url = loop {
            // Check timeout
            if start_time.elapsed() > timeout {
                log::error!("Generation timed out after {:?}", timeout);
                return Err(FalError::Timeout);
            }

            // Poll status
            let status = self.poll_status(request_id).await?;
            match status {
                GenerationStatus::Pending => {
                    log::debug!("Status: pending, waiting...");
                }
                GenerationStatus::InProgress => {
                    log::info!("Status: generating...");
                }
                GenerationStatus::Completed { video_url } => {
                    log::info!("Generation complete!");
                    break video_url;
                }
                GenerationStatus::Failed { error } => {
                    log::error!("Generation failed: {}", error);
                    return Err(FalError::ApiError(format!("Generation failed: {}", error)));
                }
            }

            // Wait before next poll
            tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
        };

        // Step 3: Download video
        log::info!("Downloading video from: {}", video_url);
        let dest_path = self.generate_video_path(request_id);
        let downloaded_path = self.download_video(&video_url, &dest_path).await?;
        log::info!("Video downloaded to: {:?}", downloaded_path);

        Ok(downloaded_path)
    }

    /// Generate a unique file path for a video based on request ID.
    pub fn generate_video_path(&self, request_id: &str) -> PathBuf {
        let cache_dir = std::env::temp_dir().join("space-recorder").join("fal-videos");
        cache_dir.join(format!("{}.mp4", request_id))
    }

    /// Check generation status by polling the fal.ai status endpoint.
    ///
    /// Queries the status of a previously submitted generation request using
    /// the request ID returned from `submit_generation`.
    ///
    /// # Arguments
    ///
    /// * `request_id` - The unique request ID from `QueueResponse`
    ///
    /// # Returns
    ///
    /// A `GenerationStatus` indicating the current state:
    /// - `Pending` - Request is queued
    /// - `InProgress` - Video is being generated
    /// - `Completed { video_url }` - Generation finished, video ready to download
    /// - `Failed { error }` - Generation failed with an error message
    ///
    /// # Errors
    ///
    /// Returns `FalError::HttpError` if the request fails, or `FalError::ApiError`
    /// if the API returns an error response.
    pub async fn poll_status(&self, request_id: &str) -> Result<GenerationStatus, FalError> {
        // fal.ai status endpoint: GET /{model}/requests/{request_id}/status
        let url = format!(
            "{}/{}/requests/{}/status",
            self.base_url, self.model, request_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Key {}", self.api_key))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(FalError::ApiError(format!(
                "Status check failed with status {}: {}",
                status, error_text
            )));
        }

        let status_response: StatusResponse = response.json().await?;

        // Map fal.ai status to GenerationStatus enum
        match status_response.status.to_uppercase().as_str() {
            "PENDING" | "IN_QUEUE" => Ok(GenerationStatus::Pending),
            "PROCESSING" | "IN_PROGRESS" => Ok(GenerationStatus::InProgress),
            "COMPLETED" | "OK" => {
                // Extract video URL from response
                if let Some(video) = status_response.video {
                    Ok(GenerationStatus::Completed {
                        video_url: video.url,
                    })
                } else if let Some(response_url) = status_response.response_url {
                    // Some endpoints use response_url instead of video.url
                    Ok(GenerationStatus::Completed {
                        video_url: response_url,
                    })
                } else {
                    Err(FalError::ApiError(
                        "Generation completed but no video URL in response".to_string(),
                    ))
                }
            }
            "FAILED" | "ERROR" => {
                let error_message = status_response
                    .error
                    .unwrap_or_else(|| "Unknown error occurred during generation".to_string());
                Ok(GenerationStatus::Failed {
                    error: error_message,
                })
            }
            unknown => Err(FalError::ApiError(format!(
                "Unknown generation status: {}",
                unknown
            ))),
        }
    }

    /// Download a video file from a URL to disk.
    ///
    /// Streams the download to disk without loading the full video into memory.
    /// This is important for large video files to avoid memory exhaustion.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to download the video from
    /// * `dest` - The destination path where the video file will be saved
    ///
    /// # Returns
    ///
    /// The path to the downloaded video file on success.
    ///
    /// # Errors
    ///
    /// Returns `FalError::HttpError` if the download request fails,
    /// `FalError::IoError` if writing to disk fails, or `FalError::ApiError`
    /// if the server returns an error response.
    pub async fn download_video(&self, url: &str, dest: &Path) -> Result<PathBuf, FalError> {
        // Create parent directories if they don't exist
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Start the download request
        let response = self
            .http_client
            .get(url)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(FalError::ApiError(format!(
                "Video download failed with status {}: {}",
                status, error_text
            )));
        }

        // Stream the response body to disk
        let mut file = tokio::fs::File::create(dest).await?;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
        }

        file.flush().await?;

        Ok(dest.to_path_buf())
    }
}

/// Errors that can occur during fal.ai operations.
#[derive(Debug, thiserror::Error)]
pub enum FalError {
    #[error("fal.ai feature not yet implemented")]
    NotImplemented,

    #[error("API key not configured")]
    MissingApiKey,

    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("API error: {0}")]
    ApiError(String),

    #[error("Generation timed out")]
    Timeout,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Rate limited: {message}")]
    RateLimit {
        /// Human-readable rate limit message
        message: String,
        /// Retry-After header value in seconds, if provided
        retry_after_secs: Option<u64>,
    },

    #[error("Network error: {message} (after {attempts} attempts)")]
    NetworkError {
        /// Human-readable network error message
        message: String,
        /// Number of retry attempts made before giving up
        attempts: u32,
    },

    #[error("Content policy violation: {message}")]
    ContentPolicyViolation {
        /// Human-readable explanation of the policy violation
        message: String,
    },

    #[error("Empty prompt")]
    EmptyPrompt,

    #[error("Invalid prompt: {reason}")]
    InvalidPrompt {
        /// Reason the prompt was deemed invalid
        reason: String,
    },
}
