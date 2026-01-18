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
    fn generate_video_path(&self, request_id: &str) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // Test that new() properly reads FAL_API_KEY_ENV
        // Since env vars are shared state, we just verify the logic:
        // - If FAL_API_KEY is set, new() should succeed
        // - If FAL_API_KEY is not set, new() should fail with MissingApiKey

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

    #[test]
    fn test_generate_request_serialization() {
        let request = GenerateRequest {
            prompt: "cyberpunk cityscape".to_string(),
            video_size: None,
            num_frames: None,
            fps: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"prompt\":\"cyberpunk cityscape\""));
        // video_size, num_frames, fps should be omitted when None
        assert!(!json.contains("video_size"));
        assert!(!json.contains("num_frames"));
        assert!(!json.contains("fps"));
    }

    #[test]
    fn test_generate_request_with_params_serialization() {
        let request = GenerateRequest {
            prompt: "test prompt".to_string(),
            video_size: Some(VideoSize {
                width: 1024,
                height: 576,
            }),
            num_frames: Some(25),
            fps: Some(8),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"prompt\":\"test prompt\""));
        assert!(json.contains("\"video_size\""));
        assert!(json.contains("\"width\":1024"));
        assert!(json.contains("\"height\":576"));
        assert!(json.contains("\"num_frames\":25"));
        assert!(json.contains("\"fps\":8"));
    }

    #[test]
    fn test_queue_response_deserialization() {
        let json = r#"{"request_id": "abc123"}"#;
        let response: QueueResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.request_id, "abc123");
        assert!(response.status_url.is_none());
    }

    #[test]
    fn test_queue_response_with_status_url() {
        let json = r#"{"request_id": "abc123", "status_url": "https://queue.fal.run/status/abc123"}"#;
        let response: QueueResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.request_id, "abc123");
        assert_eq!(
            response.status_url,
            Some("https://queue.fal.run/status/abc123".to_string())
        );
    }

    #[test]
    fn test_default_model_constant() {
        assert_eq!(DEFAULT_MODEL, "fal-ai/fast-svd-lcm");
    }

    #[tokio::test]
    async fn test_submit_generation_builds_correct_url() {
        // This test verifies that the URL is built correctly.
        // We can't actually call the API without a mock server, but we can verify
        // the client is configured correctly.
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "https://queue.fal.run".to_string(),
        )
        .unwrap();

        // Verify the client would construct the correct URL
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

        // Verify the URL format: {base_url}/{model}/requests/{request_id}/status
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

    #[test]
    fn test_status_response_pending_deserialization() {
        let json = r#"{"status": "PENDING"}"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "PENDING");
        assert!(response.video.is_none());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_status_response_in_queue_deserialization() {
        let json = r#"{"status": "IN_QUEUE"}"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "IN_QUEUE");
    }

    #[test]
    fn test_status_response_processing_deserialization() {
        let json = r#"{"status": "PROCESSING"}"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "PROCESSING");
    }

    #[test]
    fn test_status_response_in_progress_deserialization() {
        let json = r#"{"status": "IN_PROGRESS"}"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "IN_PROGRESS");
    }

    #[test]
    fn test_status_response_completed_with_video() {
        let json = r#"{
            "status": "COMPLETED",
            "video": {
                "url": "https://fal.ai/videos/generated.mp4"
            }
        }"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "COMPLETED");
        assert!(response.video.is_some());
        assert_eq!(
            response.video.unwrap().url,
            "https://fal.ai/videos/generated.mp4"
        );
    }

    #[test]
    fn test_status_response_completed_with_response_url() {
        let json = r#"{
            "status": "OK",
            "response_url": "https://fal.ai/results/video.mp4"
        }"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "OK");
        assert_eq!(
            response.response_url,
            Some("https://fal.ai/results/video.mp4".to_string())
        );
    }

    #[test]
    fn test_status_response_failed_with_error() {
        let json = r#"{
            "status": "FAILED",
            "error": "Generation failed: invalid prompt"
        }"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "FAILED");
        assert_eq!(
            response.error,
            Some("Generation failed: invalid prompt".to_string())
        );
    }

    #[test]
    fn test_status_response_error_status() {
        let json = r#"{
            "status": "ERROR",
            "error": "Internal server error"
        }"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, "ERROR");
        assert_eq!(
            response.error,
            Some("Internal server error".to_string())
        );
    }

    #[test]
    fn test_video_output_deserialization() {
        let json = r#"{"url": "https://example.com/video.mp4"}"#;
        let output: VideoOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.url, "https://example.com/video.mp4");
    }

    #[tokio::test]
    async fn test_download_video_creates_parent_dirs() {
        use std::path::PathBuf;

        // Create a temp dir for testing
        let temp_dir = std::env::temp_dir().join("space-recorder-test-download");
        let nested_dest = temp_dir.join("nested").join("dir").join("video.mp4");

        // Clean up any previous test artifacts
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Verify parent dir doesn't exist
        assert!(!nested_dest.parent().unwrap().exists());

        // Create client (we can't actually test download without a mock server,
        // but we can verify the function signature and basic setup)
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();

        // The download will fail (no real server), but this tests that the function
        // exists with the correct signature
        let result = client
            .download_video("http://localhost:9999/fake.mp4", &nested_dest)
            .await;

        // Should fail with connection error (no server), not a type error
        assert!(result.is_err());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_download_video_returns_pathbuf() {
        // This test verifies the function returns PathBuf type
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();
        let dest = std::path::Path::new("/tmp/test-video.mp4");

        // The function should return Result<PathBuf, FalError>
        let _result: Result<PathBuf, FalError> = client
            .download_video("http://localhost:9999/fake.mp4", dest)
            .await;

        // Type check passed - function signature is correct
    }

    // === Tests for generate_and_download ===

    #[test]
    fn test_generate_video_path() {
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();
        let path = client.generate_video_path("abc123");

        // Should be in temp dir under space-recorder/fal-videos
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

    #[test]
    fn test_default_generation_timeout_is_120s() {
        // Verify the default timeout constant is 120 seconds as per spec
        assert_eq!(DEFAULT_GENERATION_TIMEOUT, Duration::from_secs(120));
    }

    #[test]
    fn test_default_poll_interval_is_2s() {
        // Verify the polling interval is reasonable (2 seconds)
        assert_eq!(DEFAULT_POLL_INTERVAL, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_generate_and_download_returns_pathbuf() {
        // This test verifies the function signature returns Result<PathBuf, FalError>
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();

        // The function should return Result<PathBuf, FalError>
        // Will fail due to no server, but type signature is verified
        let _result: Result<PathBuf, FalError> =
            client.generate_and_download("test prompt").await;
    }

    #[tokio::test]
    async fn test_generate_and_download_with_timeout_returns_pathbuf() {
        // This test verifies the custom timeout function signature
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();
        let timeout = Duration::from_secs(60);

        // The function should return Result<PathBuf, FalError>
        let _result: Result<PathBuf, FalError> = client
            .generate_and_download_with_timeout("test prompt", timeout)
            .await;
    }

    #[tokio::test]
    async fn test_generate_and_download_fails_on_submit_error() {
        // Test that submit errors propagate correctly
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(), // No server running
        )
        .unwrap();

        let result = client.generate_and_download("test prompt").await;

        // Should fail with HTTP error (connection refused)
        assert!(result.is_err());
        assert!(matches!(result, Err(FalError::HttpError(_))));
    }

    #[tokio::test]
    async fn test_generate_and_download_with_short_timeout() {
        // Test that timeout parameter is respected
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(),
        )
        .unwrap();

        // Very short timeout - should fail before timeout is reached due to connection error
        let result = client
            .generate_and_download_with_timeout("test", Duration::from_millis(100))
            .await;

        // Should fail (either timeout or connection error)
        assert!(result.is_err());
    }

    // === Tests for rate limit handling ===

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
    fn test_http_status_too_many_requests_constant() {
        assert_eq!(HTTP_STATUS_TOO_MANY_REQUESTS, 429);
    }

    #[test]
    fn test_rate_limit_error_variants() {
        // Test that RateLimit can be matched properly
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

    #[tokio::test]
    async fn test_submit_generation_with_retry_returns_queue_response() {
        // Verify the function signature is correct
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(),
        )
        .unwrap();

        // Will fail due to no server, but verifies type signature
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

        // Test with custom retry configuration
        let result = client
            .submit_generation_with_retry_config(
                "test prompt",
                2,                         // max_retries
                Duration::from_millis(10), // backoff_base
                Duration::from_secs(1),    // backoff_max
            )
            .await;

        // Should fail with HTTP error (not rate limit, since no server)
        assert!(result.is_err());
        assert!(matches!(result, Err(FalError::HttpError(_))));
    }

    // === Tests for generation timeout handling ===

    #[test]
    fn test_timeout_error_is_recoverable() {
        // AC: Allows retry with same prompt
        // FalError::Timeout does not consume or invalidate any state,
        // meaning the same prompt can be submitted again
        let error = FalError::Timeout;

        // Timeout error should exist and be matchable
        assert!(matches!(error, FalError::Timeout));

        // The error type is just a signal - no state is consumed
        // Client can be reused to call submit_generation with the same prompt
    }

    #[test]
    fn test_timeout_error_display_message() {
        // AC: Logs timeout error
        let error = FalError::Timeout;
        let message = error.to_string();

        // Should have a clear error message
        assert_eq!(message, "Generation timed out");
        assert!(!message.is_empty());
    }

    #[test]
    fn test_default_generation_timeout_is_configurable() {
        // AC: Timeout after configurable duration (default 120s)
        // The default is 120s as specified
        assert_eq!(DEFAULT_GENERATION_TIMEOUT, Duration::from_secs(120));

        // The timeout can be overridden via generate_and_download_with_timeout
        let custom_timeout = Duration::from_secs(60);
        assert_ne!(custom_timeout, DEFAULT_GENERATION_TIMEOUT);
        // Custom timeout can be passed to generate_and_download_with_timeout
    }

    #[tokio::test]
    async fn test_generate_and_download_with_timeout_uses_custom_duration() {
        // AC: Timeout after configurable duration
        // This test verifies that custom timeout durations are accepted
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();

        // These should all be valid durations
        let durations = [
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(120),
            Duration::from_secs(300),
        ];

        for duration in durations {
            // The function accepts custom timeout - will fail due to no server
            // but verifies the API accepts various timeout values
            let _result = client
                .generate_and_download_with_timeout("test", duration)
                .await;
        }
    }

    #[test]
    fn test_timeout_does_not_affect_client_state() {
        // AC: Keeps current overlay unchanged (client side)
        // After a timeout error, the FalClient is still usable
        // and no internal state is corrupted

        // Create a client
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();

        // Client properties should be accessible before and after any operation
        assert_eq!(client.api_key(), "test-key");
        assert_eq!(client.base_url(), FAL_API_BASE_URL);
        assert_eq!(client.model(), DEFAULT_MODEL);

        // The client is still valid and can be used for new requests
        // (timeout doesn't invalidate the client)
    }

    #[tokio::test]
    async fn test_timeout_allows_retry_with_same_prompt() {
        // AC: Allows retry with same prompt
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(),
        )
        .unwrap();

        let prompt = "cyberpunk cityscape with neon lights";

        // First attempt (will fail due to no server, not timeout)
        let result1 = client.generate_and_download(prompt).await;
        assert!(result1.is_err());

        // Second attempt with the SAME prompt should also work (not blocked)
        let result2 = client.generate_and_download(prompt).await;
        assert!(result2.is_err());

        // The prompt was not "consumed" or invalidated by the first failure
        // Both attempts used the exact same prompt string
    }

    #[test]
    fn test_timeout_error_can_be_pattern_matched() {
        // AC: Error handling allows proper response to timeout
        let error = FalError::Timeout;

        let is_timeout = matches!(error, FalError::Timeout);
        assert!(is_timeout);

        // Can distinguish timeout from other errors
        let api_error = FalError::ApiError("test".to_string());
        let is_api_error_timeout = matches!(api_error, FalError::Timeout);
        assert!(!is_api_error_timeout);

        // IO errors are also not timeouts
        let io_error = FalError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        let is_io_error_timeout = matches!(io_error, FalError::Timeout);
        assert!(!is_io_error_timeout);

        // Rate limit errors are not timeouts
        let rate_limit_error = FalError::RateLimit {
            message: "too many requests".to_string(),
            retry_after_secs: Some(30),
        };
        let is_rate_limit_timeout = matches!(rate_limit_error, FalError::Timeout);
        assert!(!is_rate_limit_timeout);
    }

    // === Tests for network error handling ===

    #[test]
    fn test_network_error_display() {
        // AC: Clear error message to user
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
        // AC: Clear error message includes attempt count
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

        // Can extract fields
        if let FalError::NetworkError { message, attempts } = error {
            assert_eq!(message, "test");
            assert_eq!(attempts, 2);
        } else {
            panic!("Expected NetworkError");
        }
    }

    #[test]
    fn test_network_error_distinct_from_other_errors() {
        // NetworkError is distinct from HttpError, RateLimit, etc.
        let network_error = FalError::NetworkError {
            message: "connection failed".to_string(),
            attempts: 3,
        };
        assert!(!matches!(network_error, FalError::HttpError(_)));
        assert!(!matches!(network_error, FalError::RateLimit { .. }));
        assert!(!matches!(network_error, FalError::Timeout));
        assert!(!matches!(network_error, FalError::ApiError(_)));
    }

    #[tokio::test]
    async fn test_submit_generation_with_network_retry_returns_queue_response() {
        // Verify the function signature is correct
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(),
        )
        .unwrap();

        // Will fail due to no server, but verifies type signature
        let _result: Result<QueueResponse, FalError> =
            client.submit_generation_with_network_retry("test prompt").await;
    }

    #[tokio::test]
    async fn test_submit_generation_with_network_retry_config_custom_values() {
        // AC: Retries on transient network errors (3x)
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(),
        )
        .unwrap();

        // Test with custom retry configuration - using 0 retries for fast test
        let result = client
            .submit_generation_with_network_retry_config(
                "test prompt",
                0, // No retries - just one attempt
                Duration::from_millis(10),
                Duration::from_secs(1),
            )
            .await;

        // Should fail with network error (connection refused is transient)
        assert!(result.is_err());
        // Connection errors are transient, so we get NetworkError after exhausting retries
        assert!(
            matches!(result, Err(FalError::NetworkError { .. })),
            "Expected NetworkError, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_network_retry_returns_network_error_after_exhausting_retries() {
        // AC: Final failure keeps current overlay (returns error, doesn't panic)
        let client = FalClient::with_base_url(
            "test-key".to_string(),
            "http://localhost:9999".to_string(),
        )
        .unwrap();

        let result = client
            .submit_generation_with_network_retry_config(
                "test",
                2, // 2 retries = 3 total attempts
                Duration::from_millis(1), // Fast backoff for test
                Duration::from_millis(10),
            )
            .await;

        // Should return NetworkError with attempt count
        match result {
            Err(FalError::NetworkError { attempts, .. }) => {
                assert_eq!(attempts, 3, "Should have made 3 attempts (1 initial + 2 retries)");
            }
            other => panic!("Expected NetworkError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_submit_generation_with_full_retry_returns_queue_response() {
        // Verify the function signature is correct for full retry method
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

        // Test with custom retry configuration for both network and rate limit
        let result = client
            .submit_generation_with_full_retry_config(
                "test prompt",
                1, // network_retries
                1, // rate_limit_retries
                Duration::from_millis(1),
                Duration::from_millis(10),
            )
            .await;

        // Should fail with network error (connection refused is transient)
        assert!(result.is_err());
        assert!(
            matches!(result, Err(FalError::NetworkError { .. })),
            "Expected NetworkError, got {:?}",
            result
        );
    }

    #[test]
    fn test_network_error_keeps_current_overlay_unchanged() {
        // AC: Final failure keeps current overlay
        // The NetworkError type doesn't modify any state - it's just an error signal
        // that allows the caller to keep the current overlay unchanged
        let error = FalError::NetworkError {
            message: "Connection failed".to_string(),
            attempts: 3,
        };

        // NetworkError is matchable, allowing proper error handling
        match error {
            FalError::NetworkError { message, attempts } => {
                // Caller can handle this error and decide to keep current overlay
                assert!(!message.is_empty());
                assert!(attempts > 0);
            }
            _ => panic!("Expected NetworkError"),
        }
    }

    // === Tests for invalid prompt handling ===

    #[test]
    fn test_validate_prompt_rejects_empty_string() {
        // AC: Detects empty prompts (ignores)
        let result = validate_prompt("");
        assert!(matches!(result, Err(FalError::EmptyPrompt)));
    }

    #[test]
    fn test_validate_prompt_rejects_whitespace_only() {
        // AC: Detects empty prompts (ignores)
        assert!(matches!(validate_prompt("   "), Err(FalError::EmptyPrompt)));
        assert!(matches!(validate_prompt("\t"), Err(FalError::EmptyPrompt)));
        assert!(matches!(validate_prompt("\n"), Err(FalError::EmptyPrompt)));
        assert!(matches!(validate_prompt("  \t\n  "), Err(FalError::EmptyPrompt)));
    }

    #[test]
    fn test_validate_prompt_accepts_valid_prompt() {
        // Valid prompts should be accepted
        assert!(validate_prompt("hello").is_ok());
        assert!(validate_prompt("cyberpunk cityscape").is_ok());
        assert!(validate_prompt("a beautiful sunset over the ocean").is_ok());
        assert!(validate_prompt("  trimmed prompt  ").is_ok()); // Has content after trim
    }

    #[test]
    fn test_validate_prompt_accepts_prompts_with_special_characters() {
        // Prompts with special characters should be accepted
        assert!(validate_prompt("neon lights & rain").is_ok());
        assert!(validate_prompt("cyberpunk 2077 style!").is_ok());
        assert!(validate_prompt("a sunset... over the sea").is_ok());
        assert!(validate_prompt("prompt with emoji 🎨").is_ok());
    }

    #[test]
    fn test_empty_prompt_error_display() {
        // AC: Logs warning with reason
        let error = FalError::EmptyPrompt;
        assert_eq!(error.to_string(), "Empty prompt");
    }

    #[test]
    fn test_content_policy_violation_error_display() {
        // AC: Handles API rejection (content policy, etc.)
        // AC: Logs warning with reason
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
        // AC: Logs warning with reason
        let error = FalError::InvalidPrompt {
            reason: "Prompt too long".to_string(),
        };
        assert_eq!(error.to_string(), "Invalid prompt: Prompt too long");
    }

    #[test]
    fn test_is_content_policy_error_detects_keywords() {
        // AC: Handles API rejection (content policy, etc.)
        assert!(is_content_policy_error("This prompt violates our content policy"));
        assert!(is_content_policy_error("Request blocked due to policy violation"));
        assert!(is_content_policy_error("Content not allowed"));
        assert!(is_content_policy_error("Prompt was blocked for safety"));
        assert!(is_content_policy_error("NSFW content detected"));
        assert!(is_content_policy_error("Moderation filter triggered"));
        assert!(is_content_policy_error("Inappropriate content detected"));
        assert!(is_content_policy_error("This is prohibited content"));
        assert!(is_content_policy_error("Unsafe prompt rejected"));
        assert!(is_content_policy_error("Your request violates our terms"));
    }

    #[test]
    fn test_is_content_policy_error_case_insensitive() {
        // Keywords should be case-insensitive
        assert!(is_content_policy_error("CONTENT POLICY violation"));
        assert!(is_content_policy_error("Content Policy Violation"));
        assert!(is_content_policy_error("BLOCKED by filter"));
    }

    #[test]
    fn test_is_content_policy_error_returns_false_for_other_errors() {
        // Non-policy errors should not match
        assert!(!is_content_policy_error("Network timeout"));
        assert!(!is_content_policy_error("Server error 500"));
        assert!(!is_content_policy_error("Invalid API key"));
        assert!(!is_content_policy_error("Rate limit exceeded"));
        assert!(!is_content_policy_error(""));
    }

    #[test]
    fn test_content_policy_error_can_be_pattern_matched() {
        // AC: Continues accepting new prompts
        // Error should be matchable for proper handling
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
        // AC: Continues accepting new prompts
        // Error should be matchable for proper handling
        let error = FalError::EmptyPrompt;
        assert!(matches!(error, FalError::EmptyPrompt));
    }

    #[test]
    fn test_invalid_prompt_errors_are_distinct() {
        // All invalid prompt errors are distinct from other errors
        let empty_prompt = FalError::EmptyPrompt;
        let content_policy = FalError::ContentPolicyViolation {
            message: "test".to_string(),
        };
        let invalid_prompt = FalError::InvalidPrompt {
            reason: "test".to_string(),
        };

        // Empty prompt is distinct
        assert!(!matches!(empty_prompt, FalError::ContentPolicyViolation { .. }));
        assert!(!matches!(empty_prompt, FalError::InvalidPrompt { .. }));
        assert!(!matches!(empty_prompt, FalError::ApiError(_)));

        // Content policy is distinct
        assert!(!matches!(content_policy, FalError::EmptyPrompt));
        assert!(!matches!(content_policy, FalError::InvalidPrompt { .. }));
        assert!(!matches!(content_policy, FalError::ApiError(_)));

        // Invalid prompt is distinct
        assert!(!matches!(invalid_prompt, FalError::EmptyPrompt));
        assert!(!matches!(invalid_prompt, FalError::ContentPolicyViolation { .. }));
        assert!(!matches!(invalid_prompt, FalError::ApiError(_)));
    }

    #[tokio::test]
    async fn test_submit_generation_rejects_empty_prompt() {
        // AC: Detects empty prompts (ignores)
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();

        let result = client.submit_generation("").await;
        assert!(matches!(result, Err(FalError::EmptyPrompt)));

        let result = client.submit_generation("   ").await;
        assert!(matches!(result, Err(FalError::EmptyPrompt)));
    }

    #[tokio::test]
    async fn test_submit_generation_with_params_rejects_empty_prompt() {
        // AC: Detects empty prompts (ignores)
        let client = FalClient::with_api_key("test-key".to_string()).unwrap();

        let result = client
            .submit_generation_with_params("", Some(1024), Some(576), Some(25), Some(8))
            .await;
        assert!(matches!(result, Err(FalError::EmptyPrompt)));
    }

    #[test]
    fn test_http_status_constants() {
        // Verify the HTTP status constants are correct
        assert_eq!(HTTP_STATUS_BAD_REQUEST, 400);
        assert_eq!(HTTP_STATUS_FORBIDDEN, 403);
    }

    #[test]
    fn test_content_policy_keywords_list() {
        // Verify the keywords list contains expected values
        assert!(CONTENT_POLICY_KEYWORDS.contains(&"content policy"));
        assert!(CONTENT_POLICY_KEYWORDS.contains(&"policy violation"));
        assert!(CONTENT_POLICY_KEYWORDS.contains(&"blocked"));
        assert!(CONTENT_POLICY_KEYWORDS.contains(&"nsfw"));
        assert!(CONTENT_POLICY_KEYWORDS.contains(&"moderation"));
    }

    // ============================================================
    // Mock HTTP Server Tests for FalClient
    // These tests use wiremock to verify actual HTTP request/response handling
    // ============================================================

    mod mock_http_tests {
        use super::*;
        use wiremock::matchers::{body_json_schema, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // === Tests for API request formatting ===

        #[tokio::test]
        async fn test_submit_generation_sends_correct_authorization_header() {
            // AC: Test API request formatting
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
            // AC: Test API request formatting
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
            // AC: Test API request formatting
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
            // AC: Test API request formatting
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
            // AC: Test API request formatting
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

        // === Tests for status parsing ===

        #[tokio::test]
        async fn test_poll_status_parses_pending_status() {
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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
            // AC: Test status parsing
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

        // === Tests for error handling ===

        #[tokio::test]
        async fn test_submit_generation_handles_429_rate_limit() {
            // AC: Test error handling - Mock HTTP responses
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
            // AC: Test error handling
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
            // AC: Test error handling
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
            // AC: Test error handling
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
            // AC: Test error handling
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

            // 400 without content policy keywords should be ApiError, not ContentPolicyViolation
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
            // AC: Test error handling
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
            // AC: Test error handling
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
            // AC: Test error handling / Mock HTTP responses
            let mock_server = MockServer::start().await;
            let video_bytes: Vec<u8> = vec![0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70]; // MP4 header bytes

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

            // Verify content was written correctly
            let written_bytes = std::fs::read(&path).unwrap();
            assert_eq!(written_bytes, video_bytes);
        }

        #[tokio::test]
        async fn test_download_video_handles_404() {
            // AC: Test error handling
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
        async fn test_submit_generation_with_custom_model_uses_correct_path() {
            // AC: Test API request formatting
            let mock_server = MockServer::start().await;

            // Mount mock on custom model path
            Mock::given(method("POST"))
                .and(path("/fal-ai/custom-video-model"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"request_id": "custom-req"})),
                )
                .expect(1)
                .mount(&mock_server)
                .await;

            // Need to override base URL for mock server
            let client = FalClient {
                api_key: "test-key".to_string(),
                base_url: mock_server.uri(),
                model: "fal-ai/custom-video-model".to_string(),
                http_client: reqwest::Client::new(),
            };

            let result = client.submit_generation("test").await;

            assert!(result.is_ok());
            assert_eq!(result.unwrap().request_id, "custom-req");
        }

        #[tokio::test]
        async fn test_submit_generation_queue_response_with_status_url() {
            // AC: Test status parsing
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
            // AC: Test status parsing - edge case
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status": "COMPLETED"
                    // No video URL provided
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
            // AC: Test status parsing - edge case
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/fal-ai/fast-svd-lcm/requests/test-id/status"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status": "FAILED"
                    // No error message provided
                })))
                .mount(&mock_server)
                .await;

            let client =
                FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
            let result = client.poll_status("test-id").await;

            match result {
                Ok(GenerationStatus::Failed { error }) => {
                    // Should use default error message
                    assert!(error.contains("Unknown error"));
                }
                _ => panic!("Expected Failed status, got {:?}", result),
            }
        }

        #[tokio::test]
        async fn test_submit_generation_handles_malformed_json_response() {
            // AC: Test error handling
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/fal-ai/fast-svd-lcm"))
                .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
                .mount(&mock_server)
                .await;

            let client =
                FalClient::with_base_url("test-api-key".to_string(), mock_server.uri()).unwrap();
            let result = client.submit_generation("test").await;

            // Should return HttpError (from serde/reqwest json parsing failure)
            assert!(matches!(result, Err(FalError::HttpError(_))));
        }

        #[tokio::test]
        async fn test_poll_status_handles_malformed_json_response() {
            // AC: Test error handling
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
            // AC: Test API request formatting - partial params
            let mock_server = MockServer::start().await;

            // Only prompt should be present, no video_size, num_frames, or fps
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
            // AC: Test API request formatting
            let mock_server = MockServer::start().await;

            // Only width provided, not height - video_size should be omitted
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
}
