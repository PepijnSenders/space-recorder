//! PTY host module - spawns user's shell and relays I/O

use portable_pty::{
    Child, CommandBuilder, MasterPty, PtySize as PortablePtySize, native_pty_system,
};
use std::io::{Read, Write};

/// Error type for PTY operations
#[derive(Debug)]
pub enum PtyError {
    /// Failed to create PTY pair
    PtyCreationFailed(Box<dyn std::error::Error + Send + Sync>),
    /// Failed to spawn shell
    SpawnFailed(Box<dyn std::error::Error + Send + Sync>),
    /// Failed to get reader from PTY
    ReaderFailed(Box<dyn std::error::Error + Send + Sync>),
    /// Failed to get writer from PTY
    WriterFailed(Box<dyn std::error::Error + Send + Sync>),
    /// PTY I/O error
    IoError(std::io::Error),
    /// Failed to resize PTY
    ResizeFailed(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtyError::PtyCreationFailed(e) => write!(f, "Failed to create PTY: {}", e),
            PtyError::SpawnFailed(e) => write!(f, "Failed to spawn shell: {}", e),
            PtyError::ReaderFailed(e) => write!(f, "Failed to get PTY reader: {}", e),
            PtyError::WriterFailed(e) => write!(f, "Failed to get PTY writer: {}", e),
            PtyError::IoError(e) => write!(f, "PTY I/O error: {}", e),
            PtyError::ResizeFailed(e) => write!(f, "Failed to resize PTY: {}", e),
        }
    }
}

impl std::error::Error for PtyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PtyError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PtyError {
    fn from(err: std::io::Error) -> Self {
        PtyError::IoError(err)
    }
}

/// Terminal size configuration
#[derive(Debug, Clone, Copy)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl From<PtySize> for PortablePtySize {
    fn from(size: PtySize) -> Self {
        PortablePtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: size.pixel_width,
            pixel_height: size.pixel_height,
        }
    }
}

/// PTY host that manages a shell process
pub struct PtyHost {
    /// The PTY master handle (kept for resize operations)
    master: Box<dyn MasterPty + Send>,
    /// Child process handle
    child: Box<dyn Child + Send + Sync>,
    /// Reader for shell output
    reader: Box<dyn Read + Send>,
    /// Writer for shell input
    writer: Box<dyn Write + Send>,
}

impl PtyHost {
    /// Spawn a new shell in a PTY
    ///
    /// # Arguments
    /// * `shell` - Path to the shell to spawn (e.g., "/bin/zsh")
    /// * `size` - Initial terminal size
    ///
    /// # Returns
    /// A PtyHost instance with read/write handles to the shell
    pub fn spawn(shell: &str, size: PtySize) -> Result<Self, PtyError> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(size.into())
            .map_err(|e| PtyError::PtyCreationFailed(e.into()))?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::SpawnFailed(e.into()))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::ReaderFailed(e.into()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::WriterFailed(e.into()))?;

        Ok(Self {
            master: pair.master,
            child,
            reader,
            writer,
        })
    }

    /// Resize the PTY (call on terminal resize)
    pub fn resize(&self, size: PtySize) -> Result<(), PtyError> {
        self.master
            .resize(size.into())
            .map_err(|e| PtyError::ResizeFailed(e.into()))
    }

    /// Write bytes to the shell's stdin
    pub fn write(&mut self, data: &[u8]) -> Result<usize, PtyError> {
        let n = self.writer.write(data)?;
        self.writer.flush()?;
        Ok(n)
    }

    /// Read available bytes from the shell's stdout
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, PtyError> {
        Ok(self.reader.read(buf)?)
    }

    /// Check if the shell process has exited
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>, PtyError> {
        Ok(self.child.try_wait()?)
    }

    /// Kill the shell process
    pub fn kill(&mut self) -> Result<(), PtyError> {
        Ok(self.child.kill()?)
    }

    /// Get a reference to the reader
    pub fn reader(&mut self) -> &mut (dyn Read + Send) {
        &mut *self.reader
    }

    /// Get a reference to the writer
    pub fn writer(&mut self) -> &mut (dyn Write + Send) {
        &mut *self.writer
    }

    /// Take ownership of the reader for use in a separate thread
    pub fn take_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        // We can't easily take from the Box, so we use Option pattern
        // This method should only be called once
        None // Placeholder - reader is not Option yet
    }
}

/// A version of PtyHost that separates the reader for multi-threaded use
pub struct PtyHostSplit {
    /// The PTY master handle (kept for resize operations)
    master: Box<dyn MasterPty + Send>,
    /// Child process handle
    child: Box<dyn Child + Send + Sync>,
    /// Writer for shell input
    writer: Box<dyn Write + Send>,
}

impl PtyHostSplit {
    /// Check if the shell process has exited
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>, PtyError> {
        Ok(self.child.try_wait()?)
    }

    /// Kill the shell process
    pub fn kill(&mut self) -> Result<(), PtyError> {
        Ok(self.child.kill()?)
    }

    /// Write bytes to the shell's stdin
    pub fn write(&mut self, data: &[u8]) -> Result<usize, PtyError> {
        let n = self.writer.write(data)?;
        self.writer.flush()?;
        Ok(n)
    }

    /// Resize the PTY (call on terminal resize)
    pub fn resize(&self, size: PtySize) -> Result<(), PtyError> {
        self.master
            .resize(size.into())
            .map_err(|e| PtyError::ResizeFailed(e.into()))
    }
}

impl PtyHost {
    /// Split the PtyHost into a reader and the rest, for multi-threaded use.
    /// The reader can be moved to a background thread while the main struct
    /// handles writing and process management.
    pub fn split(self) -> (Box<dyn Read + Send>, PtyHostSplit) {
        (
            self.reader,
            PtyHostSplit {
                master: self.master,
                child: self.child,
                writer: self.writer,
            },
        )
    }
}

/// Select shell based on priority:
/// 1. CLI argument (if provided)
/// 2. $SHELL environment variable
/// 3. /bin/zsh (macOS default fallback)
///
/// # Arguments
/// * `cli_shell` - Optional shell path from --shell CLI argument
///
/// # Returns
/// Path to the shell to use
pub fn select_shell(cli_shell: Option<&str>) -> String {
    if let Some(shell) = cli_shell {
        return shell.to_string();
    }

    if let Ok(shell) = std::env::var("SHELL") {
        return shell;
    }

    "/bin/zsh".to_string()
}

/// Get the default shell based on environment (deprecated, use select_shell)
///
/// Priority:
/// 1. $SHELL environment variable
/// 2. /bin/zsh (macOS default)
/// 3. /bin/bash (fallback)
pub fn default_shell() -> String {
    select_shell(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_shell_with_cli_arg() {
        // CLI arg takes highest priority
        let shell = select_shell(Some("/bin/fish"));
        assert_eq!(shell, "/bin/fish");
    }

    #[test]
    fn test_select_shell_without_cli_falls_back_to_env() {
        // Without CLI arg, should use $SHELL (which is typically set)
        let shell = select_shell(None);
        // On most systems $SHELL is set, so result should match $SHELL or fallback
        assert!(
            shell.starts_with('/'),
            "Shell path should be absolute: {}",
            shell
        );
    }

    #[test]
    fn test_select_shell_fallback_to_zsh() {
        // Test the fallback by temporarily clearing SHELL env
        // SAFETY: This test runs in a single thread and restores the var immediately
        let original_shell = std::env::var("SHELL").ok();

        unsafe { std::env::remove_var("SHELL") };
        let shell = select_shell(None);
        assert_eq!(shell, "/bin/zsh", "Should fallback to /bin/zsh");

        // Restore original SHELL
        if let Some(s) = original_shell {
            unsafe { std::env::set_var("SHELL", s) };
        }
    }

    #[test]
    fn test_default_shell_returns_valid_path() {
        let shell = default_shell();
        assert!(
            shell.starts_with('/'),
            "Shell path should be absolute: {}",
            shell
        );
    }

    #[test]
    fn test_pty_size_default() {
        let size = PtySize::default();
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn test_spawn_echo() {
        let size = PtySize::default();
        let pty = PtyHost::spawn("/bin/echo", size);
        assert!(pty.is_ok(), "Should spawn /bin/echo successfully");
    }

    #[test]
    fn test_spawn_and_wait() {
        let size = PtySize::default();
        let mut pty = PtyHost::spawn("/bin/echo", size).expect("Should spawn");

        // Poll for process exit with timeout
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(2);

        loop {
            if let Some(_status) = pty.try_wait().expect("Should check status") {
                return; // Test passed - process exited
            }
            if start.elapsed() > timeout {
                panic!("Echo process did not exit within timeout");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
