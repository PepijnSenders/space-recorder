//! FFmpeg pipeline management for space-recorder.
//!
//! This module handles spawning, monitoring, and terminating FFmpeg processes.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// Errors that can occur during pipeline operations
#[derive(Debug)]
pub enum PipelineError {
    /// FFmpeg executable not found
    FfmpegNotFound,
    /// Failed to spawn FFmpeg process
    SpawnFailed(std::io::Error),
    /// FFmpeg process exited with non-zero status
    ProcessFailed { exit_code: Option<i32>, stderr: String },
    /// Pipeline was interrupted (e.g., by SIGINT)
    #[allow(dead_code)]
    Interrupted,
    /// I/O error during pipeline operation
    IoError(std::io::Error),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::FfmpegNotFound => {
                write!(
                    f,
                    "FFmpeg not found. Please install it with:\n\n    brew install ffmpeg\n"
                )
            }
            PipelineError::SpawnFailed(e) => write!(f, "Failed to spawn FFmpeg: {}", e),
            PipelineError::ProcessFailed { exit_code, stderr } => {
                write!(
                    f,
                    "FFmpeg exited with code {:?}\n{}",
                    exit_code,
                    stderr
                )
            }
            PipelineError::Interrupted => write!(f, "Pipeline interrupted"),
            PipelineError::IoError(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<std::io::Error> for PipelineError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            PipelineError::FfmpegNotFound
        } else {
            PipelineError::IoError(e)
        }
    }
}

/// Represents a running FFmpeg pipeline
pub struct Pipeline {
    /// The FFmpeg child process
    child: Child,
    /// Flag indicating if shutdown has been requested
    shutdown_flag: Arc<AtomicBool>,
    /// Handle for the stderr reader thread
    stderr_thread: Option<JoinHandle<Vec<String>>>,
    /// Handle for the stdout pipe thread (when piping to another process)
    #[allow(dead_code)]
    pipe_thread: Option<JoinHandle<()>>,
}

impl Pipeline {
    /// Spawn a new FFmpeg process with the given arguments.
    ///
    /// # Arguments
    /// * `args` - FFmpeg command-line arguments (excluding the `ffmpeg` command itself)
    ///
    /// # Returns
    /// A running `Pipeline` or an error
    #[allow(dead_code)] // Used in tests and for future recording mode
    pub fn spawn(args: &[&str]) -> Result<Self, PipelineError> {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PipelineError::FfmpegNotFound
            } else {
                PipelineError::SpawnFailed(e)
            }
        })?;

        let shutdown_flag = Arc::new(AtomicBool::new(false));

        // Spawn a thread to read stderr
        let stderr = child.stderr.take();
        let stderr_thread = stderr.map(|stderr| {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let mut lines = Vec::new();
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            // Log stderr output (for debugging)
                            eprintln!("[ffmpeg] {}", l);
                            lines.push(l);
                        }
                        Err(_) => break,
                    }
                }
                lines
            })
        });

        Ok(Pipeline {
            child,
            shutdown_flag,
            stderr_thread,
            pipe_thread: None,
        })
    }

    /// Spawn a new FFmpeg process and pipe its stdout to a target stdin.
    ///
    /// This is used for preview mode where FFmpeg output is piped to mpv.
    ///
    /// # Arguments
    /// * `args` - FFmpeg command-line arguments (excluding the `ffmpeg` command itself)
    /// * `target_stdin` - The stdin of the target process (e.g., mpv)
    ///
    /// # Returns
    /// A running `Pipeline` or an error
    pub fn spawn_with_stdout(args: &[&str], target_stdin: ChildStdin) -> Result<Self, PipelineError> {
        let mut cmd = Command::new("ffmpeg");
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                PipelineError::FfmpegNotFound
            } else {
                PipelineError::SpawnFailed(e)
            }
        })?;

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let shutdown_flag_clone = shutdown_flag.clone();

        // Spawn a thread to read stderr
        let stderr = child.stderr.take();
        let stderr_thread = stderr.map(|stderr| {
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let mut lines = Vec::new();
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            // Log stderr output (for debugging)
                            eprintln!("[ffmpeg] {}", l);
                            lines.push(l);
                        }
                        Err(_) => break,
                    }
                }
                lines
            })
        });

        // Spawn a thread to pipe stdout to target stdin
        let stdout = child.stdout.take();
        let pipe_thread = stdout.map(|mut stdout| {
            let mut target = target_stdin;
            thread::spawn(move || {
                let mut buf = [0u8; 65536]; // 64KB buffer for low latency
                loop {
                    if shutdown_flag_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    match std::io::Read::read(&mut stdout, &mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            if target.write_all(&buf[..n]).is_err() {
                                break; // Target closed
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
        });

        Ok(Pipeline {
            child,
            shutdown_flag,
            stderr_thread,
            pipe_thread,
        })
    }

    /// Get a reference to the stdout pipe for reading output.
    #[allow(dead_code)]
    pub fn stdout(&mut self) -> Option<&mut std::process::ChildStdout> {
        self.child.stdout.as_mut()
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Wait for the process to complete and return the exit status.
    pub fn wait(&mut self) -> Result<ExitStatus, PipelineError> {
        self.child.wait().map_err(PipelineError::IoError)
    }

    /// Request a graceful shutdown of the pipeline.
    ///
    /// This sends SIGINT to FFmpeg and waits for it to terminate.
    /// If FFmpeg doesn't exit within the timeout, SIGKILL is sent.
    pub fn shutdown(&mut self) -> Result<ExitStatus, PipelineError> {
        self.shutdown_flag.store(true, Ordering::SeqCst);

        // Send SIGINT (equivalent to Ctrl+C) to FFmpeg
        #[cfg(unix)]
        {
            unsafe {
                let pid = self.child.id() as i32;
                libc::kill(pid, libc::SIGINT);
            }
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, just kill the process
            let _ = self.child.kill();
        }

        // Wait for FFmpeg to exit gracefully (up to 2 seconds)
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(2);

        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        // Timeout exceeded, force kill
                        let _ = self.child.kill();
                        return self.child.wait().map_err(PipelineError::IoError);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => return Err(PipelineError::IoError(e)),
            }
        }
    }

    /// Check if shutdown has been requested.
    #[allow(dead_code)]
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_flag.load(Ordering::SeqCst)
    }

    /// Get the collected stderr output after the process has finished.
    pub fn take_stderr_output(&mut self) -> Vec<String> {
        self.stderr_thread
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default()
    }

    /// Get the process ID of the FFmpeg process.
    #[allow(dead_code)]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // Ensure the process is terminated when Pipeline is dropped
        if self.is_running() {
            let _ = self.shutdown();
        }
    }
}

/// Global flag for handling Ctrl+C across the application
static CTRLC_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Check if Ctrl+C has been received.
#[allow(dead_code)]
pub fn ctrlc_received() -> bool {
    CTRLC_RECEIVED.load(Ordering::SeqCst)
}

/// Set up the Ctrl+C handler.
///
/// This should be called once at program startup.
pub fn setup_ctrlc_handler() -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        CTRLC_RECEIVED.store(true, Ordering::SeqCst);
        eprintln!("\nReceived Ctrl+C, shutting down...");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_ffmpeg_version() {
        // Simple test: spawn ffmpeg -version which exits immediately
        let result = Pipeline::spawn(&["-version"]);
        assert!(result.is_ok(), "Should be able to spawn ffmpeg -version");

        let mut pipeline = result.unwrap();
        let status = pipeline.wait().unwrap();
        assert!(status.success(), "ffmpeg -version should succeed");
    }

    #[test]
    fn test_spawn_ffmpeg_invalid_args() {
        // Test with invalid arguments - ffmpeg should fail
        let result = Pipeline::spawn(&["-invalid_nonexistent_flag_xyz"]);
        assert!(result.is_ok(), "Should be able to spawn ffmpeg even with invalid args");

        let mut pipeline = result.unwrap();
        let status = pipeline.wait().unwrap();
        assert!(!status.success(), "ffmpeg with invalid args should fail");
    }

    #[test]
    fn test_stderr_capture() {
        // ffmpeg -version outputs to stdout, but -help outputs to stderr
        let result = Pipeline::spawn(&["-version"]);
        assert!(result.is_ok());

        let mut pipeline = result.unwrap();
        let _ = pipeline.wait();
        let stderr = pipeline.take_stderr_output();
        // stderr may or may not have content depending on ffmpeg version
        // Just verify we can collect it without panicking
        let _ = stderr;
    }

    #[test]
    fn test_shutdown_nonrunning_process() {
        // Spawn a process that exits immediately
        let result = Pipeline::spawn(&["-version"]);
        assert!(result.is_ok());

        let mut pipeline = result.unwrap();
        // Wait for it to finish first
        let _ = pipeline.wait();
        // Shutdown should handle already-exited process gracefully
        let shutdown_result = pipeline.shutdown();
        // This might error since process already exited, but shouldn't panic
        let _ = shutdown_result;
    }

    #[test]
    fn test_pipeline_error_display() {
        let err = PipelineError::FfmpegNotFound;
        let msg = format!("{}", err);
        assert!(msg.contains("FFmpeg not found"));
        assert!(msg.contains("brew install ffmpeg"));
    }

    #[test]
    fn test_process_failed_error() {
        let err = PipelineError::ProcessFailed {
            exit_code: Some(1),
            stderr: "Error message".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("1"));
        assert!(msg.contains("Error message"));
    }
}
