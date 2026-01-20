//! Keyboard input handling and PTY byte conversion.
//!
//! This module handles:
//! - Converting crossterm KeyEvents to bytes for PTY transmission
//! - Processing hotkeys (Alt+C, Alt+P, etc.) before forwarding to PTY

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::terminal::CameraModal;

/// Result of handling a key event.
pub enum KeyAction {
    /// Key was handled as a hotkey (don't forward to PTY)
    Handled,
    /// Key should be forwarded to PTY
    Forward(Vec<u8>),
    /// No action needed
    None,
}

/// Handle a key event, checking for hotkeys first.
///
/// Hotkeys intercepted (not forwarded to PTY):
/// - Alt+C: Toggle camera visibility
/// - Alt+P: Cycle position
/// - Alt+S: Cycle size
/// - Alt+A: Cycle charset
/// - Alt+T: Cycle transparency
pub fn handle_key_event(event: KeyEvent, modal: &mut CameraModal) -> KeyAction {
    let KeyEvent {
        code, modifiers, ..
    } = event;

    // Check for Alt+key hotkeys first
    if modifiers.contains(KeyModifiers::ALT) {
        match code {
            KeyCode::Char('c') | KeyCode::Char('C') => {
                modal.toggle();
                return KeyAction::Handled;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                modal.cycle_position();
                return KeyAction::Handled;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                modal.cycle_size();
                return KeyAction::Handled;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                modal.cycle_charset();
                return KeyAction::Handled;
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                modal.cycle_transparency();
                return KeyAction::Handled;
            }
            _ => {
                // Other Alt+key combinations - forward to PTY
            }
        }
    }

    // Convert to bytes for PTY
    match key_event_to_bytes(event) {
        Some(bytes) => KeyAction::Forward(bytes),
        None => KeyAction::None,
    }
}

/// Convert a crossterm KeyEvent to bytes that can be sent to the PTY.
pub fn key_event_to_bytes(event: KeyEvent) -> Option<Vec<u8>> {
    let KeyEvent {
        code, modifiers, ..
    } = event;

    // Handle Ctrl+key combinations
    if modifiers.contains(KeyModifiers::CONTROL) {
        return match code {
            // Ctrl+A through Ctrl+Z map to 0x01-0x1A
            KeyCode::Char(c) if c.is_ascii_alphabetic() => {
                let ctrl_char = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                Some(vec![ctrl_char])
            }
            // Ctrl+[ is ESC (0x1B)
            KeyCode::Char('[') => Some(vec![0x1B]),
            // Ctrl+\ is 0x1C
            KeyCode::Char('\\') => Some(vec![0x1C]),
            // Ctrl+] is 0x1D
            KeyCode::Char(']') => Some(vec![0x1D]),
            // Ctrl+^ is 0x1E
            KeyCode::Char('^') => Some(vec![0x1E]),
            // Ctrl+_ is 0x1F
            KeyCode::Char('_') => Some(vec![0x1F]),
            // Ctrl+Space is NUL (0x00)
            KeyCode::Char(' ') => Some(vec![0x00]),
            _ => None,
        };
    }

    // Handle Alt+key combinations (send ESC prefix)
    if modifiers.contains(KeyModifiers::ALT) {
        return match code {
            KeyCode::Char(c) => Some(vec![0x1B, c as u8]),
            _ => None,
        };
    }

    // Handle regular keys and special keys
    match code {
        KeyCode::Char(c) => Some(c.to_string().into_bytes()),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7F]), // DEL character
        KeyCode::Esc => Some(vec![0x1B]),
        // Arrow keys - ANSI escape sequences
        KeyCode::Up => Some(b"\x1B[A".to_vec()),
        KeyCode::Down => Some(b"\x1B[B".to_vec()),
        KeyCode::Right => Some(b"\x1B[C".to_vec()),
        KeyCode::Left => Some(b"\x1B[D".to_vec()),
        // Home/End
        KeyCode::Home => Some(b"\x1B[H".to_vec()),
        KeyCode::End => Some(b"\x1B[F".to_vec()),
        // Page Up/Down
        KeyCode::PageUp => Some(b"\x1B[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1B[6~".to_vec()),
        // Insert/Delete
        KeyCode::Insert => Some(b"\x1B[2~".to_vec()),
        KeyCode::Delete => Some(b"\x1B[3~".to_vec()),
        // Function keys F1-F12
        KeyCode::F(1) => Some(b"\x1BOP".to_vec()),
        KeyCode::F(2) => Some(b"\x1BOQ".to_vec()),
        KeyCode::F(3) => Some(b"\x1BOR".to_vec()),
        KeyCode::F(4) => Some(b"\x1BOS".to_vec()),
        KeyCode::F(5) => Some(b"\x1B[15~".to_vec()),
        KeyCode::F(6) => Some(b"\x1B[17~".to_vec()),
        KeyCode::F(7) => Some(b"\x1B[18~".to_vec()),
        KeyCode::F(8) => Some(b"\x1B[19~".to_vec()),
        KeyCode::F(9) => Some(b"\x1B[20~".to_vec()),
        KeyCode::F(10) => Some(b"\x1B[21~".to_vec()),
        KeyCode::F(11) => Some(b"\x1B[23~".to_vec()),
        KeyCode::F(12) => Some(b"\x1B[24~".to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_event_to_bytes_regular_char() {
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![b'a']));
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_c() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x03])); // ETX
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_d() {
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x04])); // EOT
    }

    #[test]
    fn test_key_event_to_bytes_ctrl_z() {
        let event = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x1A])); // SUB
    }

    #[test]
    fn test_key_event_to_bytes_enter() {
        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![b'\r']));
    }

    #[test]
    fn test_key_event_to_bytes_backspace() {
        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x7F]));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_up() {
        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[A".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_down() {
        let event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[B".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_left() {
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[D".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_arrow_right() {
        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(b"\x1B[C".to_vec()));
    }

    #[test]
    fn test_key_event_to_bytes_escape() {
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x1B]));
    }

    #[test]
    fn test_key_event_to_bytes_tab() {
        let event = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(key_event_to_bytes(event), Some(vec![b'\t']));
    }

    #[test]
    fn test_key_event_to_bytes_alt_c() {
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
        assert_eq!(key_event_to_bytes(event), Some(vec![0x1B, b'c']));
    }

    // ==================== Hotkey Handling Tests ====================

    #[test]
    fn test_handle_key_event_alt_c_toggles_visibility() {
        let mut modal = CameraModal::new();
        assert!(!modal.visible);

        // Alt+C should toggle visibility
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert!(modal.visible);

        // Alt+C again should toggle back
        let action2 = handle_key_event(event, &mut modal);
        assert!(matches!(action2, KeyAction::Handled));
        assert!(!modal.visible);
    }

    #[test]
    fn test_handle_key_event_alt_c_uppercase() {
        let mut modal = CameraModal::new();
        assert!(!modal.visible);

        // Alt+C (uppercase) should also work
        let event = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert!(modal.visible);
    }

    #[test]
    fn test_handle_key_event_alt_p_cycles_position() {
        use crate::terminal::ModalPosition;

        let mut modal = CameraModal::new();
        assert_eq!(modal.position, ModalPosition::BottomRight);

        // Alt+P should cycle position
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert_eq!(modal.position, ModalPosition::BottomLeft);
    }

    #[test]
    fn test_handle_key_event_alt_s_cycles_size() {
        use crate::terminal::ModalSize;

        let mut modal = CameraModal::new();
        assert_eq!(modal.size, ModalSize::Small);

        // Alt+S should cycle size
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert_eq!(modal.size, ModalSize::Medium);
    }

    #[test]
    fn test_handle_key_event_alt_a_cycles_charset() {
        use crate::ascii::CharSet;

        let mut modal = CameraModal::new();
        assert_eq!(modal.charset, CharSet::Standard);

        // Alt+A should cycle charset
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        assert!(matches!(action, KeyAction::Handled));
        assert_eq!(modal.charset, CharSet::Blocks);
    }

    #[test]
    fn test_handle_key_event_other_alt_keys_forwarded() {
        let mut modal = CameraModal::new();

        // Alt+X (not a hotkey) should be forwarded to PTY
        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        let action = handle_key_event(event, &mut modal);
        match action {
            KeyAction::Forward(bytes) => {
                assert_eq!(bytes, vec![0x1B, b'x']); // ESC + x
            }
            _ => panic!("Expected Forward action for Alt+X"),
        }
    }

    #[test]
    fn test_handle_key_event_regular_keys_forwarded() {
        let mut modal = CameraModal::new();

        // Regular 'a' (no modifier) should be forwarded
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_key_event(event, &mut modal);
        match action {
            KeyAction::Forward(bytes) => {
                assert_eq!(bytes, vec![b'a']);
            }
            _ => panic!("Expected Forward action for regular 'a'"),
        }
    }

    #[test]
    fn test_handle_key_event_ctrl_keys_forwarded() {
        let mut modal = CameraModal::new();

        // Ctrl+C should be forwarded (not our hotkey)
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = handle_key_event(event, &mut modal);
        match action {
            KeyAction::Forward(bytes) => {
                assert_eq!(bytes, vec![0x03]); // ETX (Ctrl+C)
            }
            _ => panic!("Expected Forward action for Ctrl+C"),
        }
    }
}
