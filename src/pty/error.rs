//! PTY error types

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
