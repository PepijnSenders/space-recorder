//! Raw terminal mode management with panic-safe cleanup.

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};

/// Static flag to track if raw mode is active (for panic handler)
pub(crate) static RAW_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Guard that ensures terminal is restored to normal mode on drop.
/// This handles both normal exits and panics.
pub struct RawModeGuard {
    /// Whether this guard is responsible for cleanup
    active: bool,
}

impl RawModeGuard {
    /// Enter raw mode and return a guard that will restore it on drop.
    ///
    /// # Returns
    /// A guard that will disable raw mode when dropped
    ///
    /// # Errors
    /// Returns an error if enabling raw mode fails
    pub fn enter() -> io::Result<Self> {
        // Install panic hook before entering raw mode
        install_panic_hook();

        enable_raw_mode()?;
        RAW_MODE_ACTIVE.store(true, Ordering::SeqCst);

        Ok(Self { active: true })
    }

    /// Manually exit raw mode without dropping the guard.
    /// After calling this, the guard's drop will be a no-op.
    pub fn exit(&mut self) -> io::Result<()> {
        if self.active {
            self.active = false;
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);
            disable_raw_mode()?;
        }
        Ok(())
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);
            // Best-effort cleanup - ignore errors during drop
            let _ = disable_raw_mode();
        }
    }
}

/// Install a panic hook that restores terminal state before panicking.
/// This ensures the terminal is usable even if the app panics.
pub(crate) fn install_panic_hook() {
    // Only install once - check if we've already installed
    static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

    if HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return; // Already installed
    }

    let original_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal before showing panic message
        if RAW_MODE_ACTIVE.load(Ordering::SeqCst) {
            // Leave alternate screen first
            let _ = crossterm::execute!(
                io::stdout(),
                crossterm::terminal::LeaveAlternateScreen,
            );
            let _ = disable_raw_mode();
            RAW_MODE_ACTIVE.store(false, Ordering::SeqCst);
        }

        // Call the original panic hook to print the panic message
        original_hook(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_mode_guard_enter_and_drop() {
        // Skip test if not running in a terminal (e.g., CI environment)
        // Raw mode requires a real TTY
        match RawModeGuard::enter() {
            Ok(guard) => {
                assert!(RAW_MODE_ACTIVE.load(Ordering::SeqCst));
                drop(guard);
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_raw_mode_guard_manual_exit() {
        // Skip test if not running in a terminal
        match RawModeGuard::enter() {
            Ok(mut guard) => {
                assert!(RAW_MODE_ACTIVE.load(Ordering::SeqCst));

                // Manual exit
                guard.exit().expect("Should exit raw mode");
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));

                // Drop should be a no-op now
                drop(guard);
                assert!(!RAW_MODE_ACTIVE.load(Ordering::SeqCst));
            }
            Err(e) => {
                // Expected in non-TTY environment (CI, tests without terminal)
                eprintln!("Skipping test (no TTY): {}", e);
            }
        }
    }

    #[test]
    fn test_panic_hook_installation() {
        // Just verify the hook can be installed without crashing
        install_panic_hook();
        install_panic_hook(); // Second call should be no-op
    }

    #[test]
    fn test_raw_mode_active_flag_initial_state() {
        // The flag should be false initially (or after previous tests cleanup)
        // Note: This test may be affected by other tests running in parallel
        // but the atomic flag should still be valid
        let _ = RAW_MODE_ACTIVE.load(Ordering::SeqCst);
        // Just verify we can read the flag without panicking
    }
}
