//! PromptInput - CLI prompt input handler during stream.
//!
//! Handles runtime input during streaming for controlling the fal.ai overlay.
//! Supports text prompts for video generation and special commands for
//! controlling overlay behavior.

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;

/// Commands that can be sent through the prompt input channel.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptCommand {
    /// Generate a video from the given text prompt.
    Generate(String),
    /// Clear the current AI overlay.
    Clear,
    /// Set the AI overlay opacity (0.0-1.0).
    SetOpacity(f32),
}

/// CLI prompt input handler for runtime video generation requests.
///
/// Listens for user input on stdin and parses it into commands.
/// Regular text is treated as a video generation prompt, while
/// special commands like `/clear` and `/opacity` control the overlay.
pub struct PromptInput {
    tx: mpsc::Sender<PromptCommand>,
}

impl PromptInput {
    /// Start listening for prompt input on stdin.
    ///
    /// Spawns a background thread that reads lines from stdin and parses
    /// them into `PromptCommand` values. The thread is non-blocking and
    /// sends parsed commands through the returned channel.
    ///
    /// # Returns
    /// A tuple of:
    /// - `PromptInput` - Handle for sending commands programmatically
    /// - `Receiver<PromptCommand>` - Channel receiver for processed commands
    ///
    /// # Input Format
    /// - Regular text: Treated as `Generate(text)` command
    /// - `/clear`: Parsed as `Clear` command
    /// - `/opacity <value>`: Parsed as `SetOpacity(value)` command
    /// - Empty input: Ignored
    pub fn spawn_listener() -> (Self, mpsc::Receiver<PromptCommand>) {
        let (tx, rx) = mpsc::channel();
        let tx_clone = tx.clone();

        thread::spawn(move || {
            let stdin = io::stdin();
            let handle = stdin.lock();

            // Show initial prompt
            Self::print_prompt();

            for line in handle.lines() {
                match line {
                    Ok(input) => {
                        if let Some(cmd) = Self::parse_input(&input) {
                            if tx_clone.send(cmd).is_err() {
                                break; // Channel closed
                            }
                        }
                        // Show prompt for next input
                        Self::print_prompt();
                    }
                    Err(_) => break, // EOF or read error
                }
            }
        });

        (Self { tx }, rx)
    }

    /// Parse a line of input into a PromptCommand.
    ///
    /// # Arguments
    /// * `input` - Raw input string from stdin
    ///
    /// # Returns
    /// - `Some(PromptCommand)` if input is valid
    /// - `None` if input is empty or invalid
    ///
    /// # Parsing Rules
    /// - Empty/whitespace-only input is ignored
    /// - `/clear` → `Clear` command
    /// - `/opacity <value>` → `SetOpacity(value)` command (value must be 0.0-1.0)
    /// - Any other text → `Generate(text)` command
    pub fn parse_input(input: &str) -> Option<PromptCommand> {
        let trimmed = input.trim();

        // Ignore empty input
        if trimmed.is_empty() {
            return None;
        }

        // Check for slash commands
        if trimmed.starts_with('/') {
            return Self::parse_command(trimmed);
        }

        // Regular text is a generate command
        Some(PromptCommand::Generate(trimmed.to_string()))
    }

    /// Parse a slash command.
    ///
    /// # Arguments
    /// * `input` - Input string starting with '/'
    ///
    /// # Returns
    /// - `Some(PromptCommand)` for valid commands
    /// - `None` for invalid commands (logs warning)
    fn parse_command(input: &str) -> Option<PromptCommand> {
        let parts: Vec<&str> = input.split_whitespace().collect();

        if parts.is_empty() {
            return None;
        }

        match parts[0].to_lowercase().as_str() {
            "/clear" => Some(PromptCommand::Clear),
            "/opacity" => {
                if parts.len() < 2 {
                    Self::print_status("Usage: /opacity <0.0-1.0>");
                    return None;
                }
                match parts[1].parse::<f32>() {
                    Ok(value) => {
                        if !(0.0..=1.0).contains(&value) {
                            Self::print_status("Opacity must be between 0.0 and 1.0");
                            return None;
                        }
                        Some(PromptCommand::SetOpacity(value))
                    }
                    Err(_) => {
                        Self::print_status("Invalid opacity value. Usage: /opacity <0.0-1.0>");
                        None
                    }
                }
            }
            _ => {
                Self::print_status(&format!("Unknown command: {}", parts[0]));
                Self::print_status("Available commands: /clear, /opacity <value>");
                None
            }
        }
    }

    /// Send a command programmatically (for testing or automation).
    ///
    /// # Arguments
    /// * `command` - The command to send
    ///
    /// # Returns
    /// - `Ok(())` if command was sent successfully
    /// - `Err(SendError)` if channel is closed
    pub fn send(&self, command: PromptCommand) -> Result<(), mpsc::SendError<PromptCommand>> {
        self.tx.send(command)
    }

    /// Print the input prompt.
    ///
    /// Shows `> ` to indicate ready for input.
    pub fn print_prompt() {
        print!("> ");
        // Flush to ensure prompt is visible before reading
        let _ = io::stdout().flush();
    }

    /// Print a status message.
    ///
    /// Status messages are printed with a newline and don't interfere
    /// with the input prompt.
    ///
    /// # Arguments
    /// * `message` - The status message to display
    pub fn print_status(message: &str) {
        println!("{}", message);
    }

    /// Print a generating status message.
    pub fn print_generating(prompt: &str) {
        Self::print_status(&format!("Generating video... (prompt: \"{}\")", prompt));
    }

    /// Print a cache hit message.
    pub fn print_cache_hit(prompt: &str) {
        Self::print_status(&format!("Found in cache: \"{}\"", prompt));
    }

    /// Print a cache miss message.
    pub fn print_cache_miss() {
        Self::print_status("Cache miss, calling fal.ai...");
    }

    /// Print a video ready message.
    pub fn print_video_ready() {
        Self::print_status("Video ready, crossfading in...");
    }

    /// Print an overlay cleared message.
    pub fn print_overlay_cleared() {
        Self::print_status("AI overlay cleared.");
    }

    /// Print an opacity changed message.
    pub fn print_opacity_set(opacity: f32) {
        Self::print_status(&format!("AI overlay opacity set to {}%", (opacity * 100.0) as u32));
    }

    /// Print an error message.
    pub fn print_error(message: &str) {
        Self::print_status(&format!("Error: {}", message));
    }

    /// Print a timeout error message.
    ///
    /// Informs the user that video generation timed out and they can retry
    /// with the same prompt.
    pub fn print_timeout_error(prompt: &str) {
        Self::print_status(&format!(
            "Generation timed out for prompt: \"{}\". You can retry with the same prompt.",
            prompt
        ));
    }

    /// Print a network error message.
    pub fn print_network_error(message: &str) {
        Self::print_status(&format!("Network error: {}. Retrying...", message));
    }

    /// Print a generation failed message.
    pub fn print_generation_failed(reason: &str) {
        Self::print_status(&format!("Generation failed: {}", reason));
    }

    /// Print a warning for an empty prompt.
    ///
    /// Used when a prompt is detected as empty after parsing but before
    /// sending to the API.
    pub fn print_empty_prompt_warning() {
        Self::print_status("Warning: Empty prompt ignored.");
    }

    /// Print a warning for an invalid prompt.
    ///
    /// Used when a prompt is rejected due to content policy or other
    /// validation failures.
    ///
    /// # Arguments
    /// * `reason` - The reason the prompt was rejected
    pub fn print_invalid_prompt_warning(reason: &str) {
        Self::print_status(&format!("Warning: Invalid prompt - {}", reason));
    }

    /// Print a warning for API content policy rejection.
    ///
    /// Used when the fal.ai API rejects a prompt due to content policy
    /// violations.
    ///
    /// # Arguments
    /// * `details` - Optional details about the rejection
    pub fn print_content_policy_warning(details: Option<&str>) {
        match details {
            Some(d) => Self::print_status(&format!("Warning: Prompt rejected by content policy - {}", d)),
            None => Self::print_status("Warning: Prompt rejected by content policy."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // v2.5.1: PromptInput struct and PromptCommand enum tests

    #[test]
    fn test_prompt_command_generate_variant() {
        // AC: PromptCommand enum: Generate(String)
        let cmd = PromptCommand::Generate("test prompt".to_string());
        assert!(matches!(cmd, PromptCommand::Generate(s) if s == "test prompt"));
    }

    #[test]
    fn test_prompt_command_clear_variant() {
        // AC: PromptCommand enum: Clear
        let cmd = PromptCommand::Clear;
        assert!(matches!(cmd, PromptCommand::Clear));
    }

    #[test]
    fn test_prompt_command_set_opacity_variant() {
        // AC: PromptCommand enum: SetOpacity(f32)
        let cmd = PromptCommand::SetOpacity(0.5);
        assert!(matches!(cmd, PromptCommand::SetOpacity(v) if (v - 0.5).abs() < f32::EPSILON));
    }

    #[test]
    fn test_prompt_command_equality() {
        // Test PartialEq implementation
        assert_eq!(PromptCommand::Clear, PromptCommand::Clear);
        assert_eq!(
            PromptCommand::Generate("test".to_string()),
            PromptCommand::Generate("test".to_string())
        );
        assert_eq!(PromptCommand::SetOpacity(0.5), PromptCommand::SetOpacity(0.5));

        assert_ne!(PromptCommand::Clear, PromptCommand::SetOpacity(0.0));
        assert_ne!(
            PromptCommand::Generate("a".to_string()),
            PromptCommand::Generate("b".to_string())
        );
    }

    #[test]
    fn test_prompt_command_clone() {
        // Test Clone implementation
        let cmd = PromptCommand::Generate("test".to_string());
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
    }

    #[test]
    fn test_prompt_command_debug() {
        // Test Debug implementation
        let cmd = PromptCommand::Generate("test".to_string());
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("Generate"));
        assert!(debug_str.contains("test"));
    }

    // v2.5.3: Prompt parsing tests

    #[test]
    fn test_parse_input_regular_text_as_generate() {
        // AC: Regular text treated as Generate(text) command
        let cmd = PromptInput::parse_input("cyberpunk cityscape");
        assert_eq!(cmd, Some(PromptCommand::Generate("cyberpunk cityscape".to_string())));
    }

    #[test]
    fn test_parse_input_clear_command() {
        // AC: /clear parsed as Clear command
        let cmd = PromptInput::parse_input("/clear");
        assert_eq!(cmd, Some(PromptCommand::Clear));
    }

    #[test]
    fn test_parse_input_clear_command_case_insensitive() {
        // /clear should be case insensitive
        assert_eq!(PromptInput::parse_input("/CLEAR"), Some(PromptCommand::Clear));
        assert_eq!(PromptInput::parse_input("/Clear"), Some(PromptCommand::Clear));
    }

    #[test]
    fn test_parse_input_opacity_command() {
        // AC: /opacity 0.5 parsed as SetOpacity(0.5) command
        let cmd = PromptInput::parse_input("/opacity 0.5");
        assert_eq!(cmd, Some(PromptCommand::SetOpacity(0.5)));
    }

    #[test]
    fn test_parse_input_opacity_command_various_values() {
        // Test various valid opacity values
        assert_eq!(PromptInput::parse_input("/opacity 0.0"), Some(PromptCommand::SetOpacity(0.0)));
        assert_eq!(PromptInput::parse_input("/opacity 1.0"), Some(PromptCommand::SetOpacity(1.0)));
        assert_eq!(PromptInput::parse_input("/opacity 0.25"), Some(PromptCommand::SetOpacity(0.25)));
        assert_eq!(PromptInput::parse_input("/opacity 0"), Some(PromptCommand::SetOpacity(0.0)));
        assert_eq!(PromptInput::parse_input("/opacity 1"), Some(PromptCommand::SetOpacity(1.0)));
    }

    #[test]
    fn test_parse_input_opacity_command_out_of_range() {
        // AC: Opacity must be 0.0-1.0
        assert_eq!(PromptInput::parse_input("/opacity 1.5"), None);
        assert_eq!(PromptInput::parse_input("/opacity -0.1"), None);
        assert_eq!(PromptInput::parse_input("/opacity 2"), None);
    }

    #[test]
    fn test_parse_input_opacity_command_invalid_value() {
        // Invalid opacity values should return None
        assert_eq!(PromptInput::parse_input("/opacity abc"), None);
        assert_eq!(PromptInput::parse_input("/opacity"), None);
    }

    #[test]
    fn test_parse_input_empty_ignored() {
        // AC: Empty input ignored
        assert_eq!(PromptInput::parse_input(""), None);
    }

    #[test]
    fn test_parse_input_whitespace_only_ignored() {
        // Whitespace-only input should be ignored
        assert_eq!(PromptInput::parse_input("   "), None);
        assert_eq!(PromptInput::parse_input("\t"), None);
        assert_eq!(PromptInput::parse_input("\n"), None);
    }

    #[test]
    fn test_parse_input_trims_whitespace() {
        // AC: Trims whitespace from prompts
        let cmd = PromptInput::parse_input("  hello world  ");
        assert_eq!(cmd, Some(PromptCommand::Generate("hello world".to_string())));
    }

    #[test]
    fn test_parse_input_unknown_command() {
        // Unknown slash commands should return None
        assert_eq!(PromptInput::parse_input("/unknown"), None);
        assert_eq!(PromptInput::parse_input("/foo bar"), None);
    }

    #[test]
    fn test_parse_input_slash_only() {
        // Just a slash should be treated as a generate command
        let cmd = PromptInput::parse_input("/");
        assert_eq!(cmd, None); // Empty command after slash
    }

    #[test]
    fn test_parse_input_preserves_prompt_content() {
        // Prompts with special characters should be preserved
        let cmd = PromptInput::parse_input("neon lights & rain, cyberpunk 2077 style");
        assert_eq!(
            cmd,
            Some(PromptCommand::Generate("neon lights & rain, cyberpunk 2077 style".to_string()))
        );
    }

    #[test]
    fn test_parse_input_multiword_prompt() {
        // Multi-word prompts should be preserved as single Generate command
        let cmd = PromptInput::parse_input("a beautiful sunset over the ocean with palm trees");
        assert_eq!(
            cmd,
            Some(PromptCommand::Generate(
                "a beautiful sunset over the ocean with palm trees".to_string()
            ))
        );
    }

    #[test]
    fn test_parse_input_opacity_with_extra_whitespace() {
        // Opacity command with extra whitespace
        let cmd = PromptInput::parse_input("/opacity   0.75");
        assert_eq!(cmd, Some(PromptCommand::SetOpacity(0.75)));
    }

    #[test]
    fn test_parse_input_clear_with_extra_args_ignored() {
        // /clear with extra arguments - args are ignored, clear is executed
        let cmd = PromptInput::parse_input("/clear now please");
        assert_eq!(cmd, Some(PromptCommand::Clear));
    }

    // v2.5.2: spawn_listener tests (channel functionality)

    #[test]
    fn test_send_command_programmatically() {
        // AC: Can send commands programmatically via send()
        let (tx, _rx) = mpsc::channel();
        let input = PromptInput { tx };

        // Should not error when channel is open
        assert!(input.send(PromptCommand::Clear).is_ok());
        assert!(input.send(PromptCommand::Generate("test".to_string())).is_ok());
        assert!(input.send(PromptCommand::SetOpacity(0.5)).is_ok());
    }

    #[test]
    fn test_send_command_receives_on_channel() {
        // AC: Sends commands through channel
        let (tx, rx) = mpsc::channel();
        let input = PromptInput { tx };

        input.send(PromptCommand::Clear).unwrap();
        input.send(PromptCommand::Generate("test".to_string())).unwrap();
        input.send(PromptCommand::SetOpacity(0.7)).unwrap();

        assert_eq!(rx.recv().unwrap(), PromptCommand::Clear);
        assert_eq!(rx.recv().unwrap(), PromptCommand::Generate("test".to_string()));
        assert_eq!(rx.recv().unwrap(), PromptCommand::SetOpacity(0.7));
    }

    #[test]
    fn test_send_command_fails_when_channel_closed() {
        // Channel closed should return error
        let (tx, rx) = mpsc::channel::<PromptCommand>();
        let input = PromptInput { tx };

        // Drop receiver to close channel
        drop(rx);

        // Send should fail
        assert!(input.send(PromptCommand::Clear).is_err());
    }

    // v2.5.4: Display function tests

    #[test]
    fn test_status_message_functions_exist() {
        // Verify all status message functions are callable
        // These test that the functions compile and can be called
        // Actual output goes to stdout, not tested here

        // These should not panic
        PromptInput::print_status("test message");
        PromptInput::print_generating("test prompt");
        PromptInput::print_cache_hit("test prompt");
        PromptInput::print_cache_miss();
        PromptInput::print_video_ready();
        PromptInput::print_overlay_cleared();
        PromptInput::print_opacity_set(0.5);
        PromptInput::print_error("test error");
        PromptInput::print_timeout_error("test prompt");
        PromptInput::print_network_error("connection failed");
        PromptInput::print_generation_failed("API error");
    }

    // v2.9: Timeout and error handling display tests

    #[test]
    fn test_print_timeout_error_callable() {
        // AC: Logs timeout error to user
        // print_timeout_error should be callable and not panic
        PromptInput::print_timeout_error("cyberpunk cityscape");
    }

    #[test]
    fn test_print_network_error_callable() {
        // print_network_error should be callable and not panic
        PromptInput::print_network_error("connection timed out");
    }

    #[test]
    fn test_print_generation_failed_callable() {
        // print_generation_failed should be callable and not panic
        PromptInput::print_generation_failed("invalid prompt");
    }

    // v2.9: Invalid prompt handling display tests

    #[test]
    fn test_print_empty_prompt_warning_callable() {
        // AC: Detects empty prompts (ignores) - logs warning
        // Should not panic when called
        PromptInput::print_empty_prompt_warning();
    }

    #[test]
    fn test_print_invalid_prompt_warning_callable() {
        // AC: Handles API rejection (content policy, etc.) - logs warning with reason
        // Should not panic when called
        PromptInput::print_invalid_prompt_warning("Prompt too short");
    }

    #[test]
    fn test_print_content_policy_warning_callable() {
        // AC: Handles API rejection (content policy, etc.) - logs warning with reason
        // Should not panic when called with various inputs
        PromptInput::print_content_policy_warning(None);
        PromptInput::print_content_policy_warning(Some("Violates terms of service"));
    }

    #[test]
    fn test_invalid_prompt_warning_functions_exist_in_public_api() {
        // Verify all invalid prompt warning functions are accessible
        // These should not panic
        PromptInput::print_empty_prompt_warning();
        PromptInput::print_invalid_prompt_warning("reason");
        PromptInput::print_content_policy_warning(None);
        PromptInput::print_content_policy_warning(Some("details"));
    }
}
