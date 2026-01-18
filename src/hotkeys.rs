//! Global hotkey handling for space-recorder.
//!
//! This module provides global keyboard capture for opacity control hotkeys.
//! Uses rdev for cross-platform (macOS) global key listening.

use rdev::{listen, Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Represents a hotkey event that occurred
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HotkeyEvent {
    /// Increase opacity by 0.1
    IncreaseOpacity,
    /// Decrease opacity by 0.1
    DecreaseOpacity,
}

/// Manages global hotkey listening and opacity state
pub struct HotkeyManager {
    /// Current opacity value (0.0-1.0)
    opacity: Arc<Mutex<f32>>,
    /// Flag indicating if opacity changed since last check
    opacity_changed: Arc<AtomicBool>,
    /// Flag to stop the listener thread
    stop_flag: Arc<AtomicBool>,
    /// Handle to the listener thread
    listener_thread: Option<JoinHandle<()>>,
}

impl HotkeyManager {
    /// Create a new HotkeyManager with the given initial opacity.
    ///
    /// # Arguments
    /// * `initial_opacity` - Starting opacity value (0.0-1.0)
    pub fn new(initial_opacity: f32) -> Self {
        HotkeyManager {
            opacity: Arc::new(Mutex::new(initial_opacity.clamp(0.0, 1.0))),
            opacity_changed: Arc::new(AtomicBool::new(false)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            listener_thread: None,
        }
    }

    /// Start listening for global hotkeys.
    ///
    /// This spawns a background thread that captures global keyboard events.
    /// Returns an error if the listener is already running.
    pub fn start(&mut self) -> Result<(), String> {
        if self.listener_thread.is_some() {
            return Err("Hotkey listener already running".to_string());
        }

        let opacity = self.opacity.clone();
        let opacity_changed = self.opacity_changed.clone();
        let stop_flag = self.stop_flag.clone();

        let handle = thread::spawn(move || {
            let callback = move |event: Event| {
                // Check stop flag periodically
                if stop_flag.load(Ordering::SeqCst) {
                    return;
                }

                if let EventType::KeyPress(key) = event.event_type {
                    let hotkey_event = match key {
                        // '+' or '=' increases opacity
                        Key::Equal => Some(HotkeyEvent::IncreaseOpacity),
                        Key::KeyQ if is_shift_pressed(&event) => Some(HotkeyEvent::IncreaseOpacity), // Shift+= on some keyboards
                        // '-' decreases opacity
                        Key::Minus => Some(HotkeyEvent::DecreaseOpacity),
                        _ => None,
                    };

                    if let Some(hotkey) = hotkey_event {
                        let mut current = opacity.lock().unwrap();
                        let new_opacity = match hotkey {
                            HotkeyEvent::IncreaseOpacity => (*current + 0.1).min(1.0),
                            HotkeyEvent::DecreaseOpacity => (*current - 0.1).max(0.0),
                        };

                        // Only update if changed (floating point comparison with tolerance)
                        if (new_opacity - *current).abs() > 0.001 {
                            *current = new_opacity;
                            opacity_changed.store(true, Ordering::SeqCst);
                            eprintln!("[hotkey] Opacity: {:.0}%", new_opacity * 100.0);
                        }
                    }
                }
            };

            // Start the global listener (blocks until error or stopped)
            // Note: On macOS, this requires Accessibility permissions
            if let Err(e) = listen(callback) {
                eprintln!("[hotkey] Listener error: {:?}", e);
            }
        });

        self.listener_thread = Some(handle);
        Ok(())
    }

    /// Stop the hotkey listener.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        // Note: rdev's listen() doesn't have a clean way to stop,
        // so the thread will continue until the process exits.
        // The stop_flag prevents processing new events.
        self.listener_thread = None;
    }

    /// Get the current opacity value.
    /// Note: Used in task 2.2 (pipeline restart on opacity change)
    #[allow(dead_code)]
    pub fn opacity(&self) -> f32 {
        *self.opacity.lock().unwrap()
    }

    /// Check if opacity has changed since last check, and reset the flag.
    ///
    /// Returns `true` if opacity changed, `false` otherwise.
    /// Note: Used in task 2.2 (pipeline restart on opacity change)
    #[allow(dead_code)]
    pub fn take_opacity_changed(&self) -> bool {
        self.opacity_changed.swap(false, Ordering::SeqCst)
    }

    /// Check if opacity has changed without resetting the flag.
    #[allow(dead_code)]
    pub fn opacity_changed(&self) -> bool {
        self.opacity_changed.load(Ordering::SeqCst)
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Check if shift key is pressed (helper for detecting '+' on some keyboards)
fn is_shift_pressed(_event: &Event) -> bool {
    // rdev doesn't provide easy access to modifier state,
    // so we rely on the Equal key being the primary way to increase
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_manager_new() {
        let manager = HotkeyManager::new(0.3);
        assert!((manager.opacity() - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_hotkey_manager_clamps_initial() {
        let manager = HotkeyManager::new(1.5);
        assert!((manager.opacity() - 1.0).abs() < 0.001);

        let manager2 = HotkeyManager::new(-0.5);
        assert!((manager2.opacity() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_opacity_changed_flag() {
        let manager = HotkeyManager::new(0.5);

        // Initially not changed
        assert!(!manager.opacity_changed());

        // take_opacity_changed should return false and leave it false
        assert!(!manager.take_opacity_changed());
        assert!(!manager.opacity_changed());
    }

    #[test]
    fn test_hotkey_event_equality() {
        assert_eq!(HotkeyEvent::IncreaseOpacity, HotkeyEvent::IncreaseOpacity);
        assert_eq!(HotkeyEvent::DecreaseOpacity, HotkeyEvent::DecreaseOpacity);
        assert_ne!(HotkeyEvent::IncreaseOpacity, HotkeyEvent::DecreaseOpacity);
    }

    #[test]
    fn test_manual_opacity_change() {
        // Test that we can directly modify opacity through the Arc<Mutex>
        let manager = HotkeyManager::new(0.5);

        // Simulate what would happen when a key is pressed
        {
            let mut opacity = manager.opacity.lock().unwrap();
            *opacity = (*opacity + 0.1).min(1.0);
        }
        manager.opacity_changed.store(true, Ordering::SeqCst);

        assert!((manager.opacity() - 0.6).abs() < 0.001);
        assert!(manager.take_opacity_changed());
        assert!(!manager.opacity_changed()); // Should be reset after take
    }

    #[test]
    fn test_opacity_boundaries() {
        let manager = HotkeyManager::new(0.95);

        // Simulate increase at boundary
        {
            let mut opacity = manager.opacity.lock().unwrap();
            *opacity = (*opacity + 0.1).min(1.0);
        }
        assert!((manager.opacity() - 1.0).abs() < 0.001);

        let manager2 = HotkeyManager::new(0.05);

        // Simulate decrease at boundary
        {
            let mut opacity = manager2.opacity.lock().unwrap();
            *opacity = (*opacity - 0.1).max(0.0);
        }
        assert!((manager2.opacity() - 0.0).abs() < 0.001);
    }
}
