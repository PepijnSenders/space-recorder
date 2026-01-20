//! PTY host implementation - spawns user's shell and relays I/O

use portable_pty::{Child, CommandBuilder, MasterPty, native_pty_system};
use std::io::{Read, Write};

use super::error::PtyError;
use super::size::PtySize;

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

#[cfg(test)]
mod tests {
    use super::*;

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
