//! Unit tests for terminal module types (ModalPosition, ModalSize, CameraModal, StatusBar).
//!
//! These tests cover the pure logic of modal positioning, sizing, and state management
//! without requiring a real terminal.

use ratatui::layout::Rect;
use space_recorder::ascii::CharSet;
use space_recorder::terminal::{AsciiFrame, CameraModal, ModalPosition, ModalSize, StatusBar};

// ==================== ModalPosition Tests ====================

#[test]
fn test_modal_position_default() {
    let pos = ModalPosition::default();
    assert_eq!(pos, ModalPosition::BottomRight);
}

#[test]
fn test_modal_position_top_left() {
    let container = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let rect = ModalPosition::TopLeft.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 1);
    assert_eq!(rect.y, 1);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.height, 10);
}

#[test]
fn test_modal_position_top_right() {
    let container = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let rect = ModalPosition::TopRight.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 59);
    assert_eq!(rect.y, 1);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.height, 10);
}

#[test]
fn test_modal_position_bottom_left() {
    let container = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let rect = ModalPosition::BottomLeft.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 1);
    assert_eq!(rect.y, 13);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.height, 10);
}

#[test]
fn test_modal_position_bottom_right() {
    let container = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let rect = ModalPosition::BottomRight.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 59);
    assert_eq!(rect.y, 13);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.height, 10);
}

#[test]
fn test_modal_position_center() {
    let container = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let rect = ModalPosition::Center.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 30);
    assert_eq!(rect.y, 7);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.height, 10);
}

#[test]
fn test_modal_position_with_offset_container() {
    let container = Rect {
        x: 10,
        y: 5,
        width: 80,
        height: 24,
    };
    let rect = ModalPosition::TopLeft.calculate_rect(container, 20, 10);
    assert_eq!(rect.x, 11);
    assert_eq!(rect.y, 6);
}

#[test]
fn test_modal_position_clamps_to_container() {
    let container = Rect {
        x: 0,
        y: 0,
        width: 10,
        height: 5,
    };
    let rect = ModalPosition::BottomRight.calculate_rect(container, 20, 10);
    assert!(rect.width <= container.width);
    assert!(rect.height <= container.height);
    assert_eq!(rect.width, 8);
    assert_eq!(rect.height, 3);
}

#[test]
fn test_modal_position_next_cycle() {
    assert_eq!(ModalPosition::TopLeft.next(), ModalPosition::TopRight);
    assert_eq!(ModalPosition::TopRight.next(), ModalPosition::BottomRight);
    assert_eq!(ModalPosition::BottomRight.next(), ModalPosition::BottomLeft);
    assert_eq!(ModalPosition::BottomLeft.next(), ModalPosition::Center);
    assert_eq!(ModalPosition::Center.next(), ModalPosition::TopLeft);
}

#[test]
fn test_modal_position_names() {
    assert_eq!(ModalPosition::TopLeft.name(), "top-left");
    assert_eq!(ModalPosition::TopRight.name(), "top-right");
    assert_eq!(ModalPosition::BottomLeft.name(), "bottom-left");
    assert_eq!(ModalPosition::BottomRight.name(), "bottom-right");
    assert_eq!(ModalPosition::Center.name(), "center");
}

// ==================== ModalSize Tests ====================

#[test]
fn test_modal_size_default() {
    let size = ModalSize::default();
    assert_eq!(size, ModalSize::Small);
}

#[test]
fn test_modal_size_dimensions() {
    assert_eq!(ModalSize::Small.dimensions(), (22, 12));
    assert_eq!(ModalSize::Medium.dimensions(), (42, 22));
    assert_eq!(ModalSize::Large.dimensions(), (62, 32));
}

#[test]
fn test_modal_size_inner_dimensions() {
    assert_eq!(ModalSize::Small.inner_dimensions(), (20, 10));
    assert_eq!(ModalSize::Medium.inner_dimensions(), (40, 20));
    assert_eq!(ModalSize::Large.inner_dimensions(), (60, 30));
}

#[test]
fn test_modal_size_next_cycle() {
    assert_eq!(ModalSize::Small.next(), ModalSize::Medium);
    assert_eq!(ModalSize::Medium.next(), ModalSize::Large);
    assert_eq!(ModalSize::Large.next(), ModalSize::XLarge);
    assert_eq!(ModalSize::XLarge.next(), ModalSize::Huge);
    assert_eq!(ModalSize::Huge.next(), ModalSize::Small);
}

#[test]
fn test_modal_size_names() {
    assert_eq!(ModalSize::Small.name(), "small");
    assert_eq!(ModalSize::Medium.name(), "medium");
    assert_eq!(ModalSize::Large.name(), "large");
    assert_eq!(ModalSize::XLarge.name(), "xlarge");
    assert_eq!(ModalSize::Huge.name(), "huge");
}

// ==================== CameraModal Tests ====================

#[test]
fn test_camera_modal_new() {
    let modal = CameraModal::new();
    assert!(!modal.visible);
    assert_eq!(modal.position, ModalPosition::BottomRight);
    assert_eq!(modal.size, ModalSize::Small);
    assert!(modal.frame.is_none());
    assert!(!modal.border);
    assert_eq!(modal.charset, CharSet::Standard);
}

#[test]
fn test_camera_modal_default() {
    let modal = CameraModal::default();
    assert!(!modal.visible);
    assert_eq!(modal.position, ModalPosition::BottomRight);
    assert_eq!(modal.size, ModalSize::Small);
    assert_eq!(modal.charset, CharSet::Standard);
}

#[test]
fn test_camera_modal_toggle() {
    let mut modal = CameraModal::new();
    assert!(!modal.visible);
    modal.toggle();
    assert!(modal.visible);
    modal.toggle();
    assert!(!modal.visible);
}

#[test]
fn test_camera_modal_cycle_position() {
    let mut modal = CameraModal::new();
    assert_eq!(modal.position, ModalPosition::BottomRight);
    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::BottomLeft);
    modal.cycle_position();
    assert_eq!(modal.position, ModalPosition::Center);
}

#[test]
fn test_camera_modal_cycle_size() {
    let mut modal = CameraModal::new();
    assert_eq!(modal.size, ModalSize::Small);
    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Medium);
    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Large);
    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::XLarge);
    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Huge);
    modal.cycle_size();
    assert_eq!(modal.size, ModalSize::Small);
}

#[test]
fn test_camera_modal_cycle_charset() {
    let mut modal = CameraModal::new();
    assert_eq!(modal.charset, CharSet::Standard);
    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Blocks);
    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Minimal);
    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Braille);
    modal.cycle_charset();
    assert_eq!(modal.charset, CharSet::Standard);
}

#[test]
fn test_camera_modal_calculate_rect() {
    let modal = CameraModal::new();
    let container = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    let rect = modal.calculate_rect(container);
    assert_eq!(rect.x, 57);
    assert_eq!(rect.y, 11);
    assert_eq!(rect.width, 22);
    assert_eq!(rect.height, 12);
}

#[test]
fn test_camera_modal_set_frame() {
    let mut modal = CameraModal::new();
    assert!(modal.frame.is_none());

    let frame = AsciiFrame::from_chars(vec!['#'; 6], 3, 2);
    modal.set_frame(frame);

    assert!(modal.frame.is_some());
    let f = modal.frame.as_ref().unwrap();
    assert_eq!(f.width, 3);
    assert_eq!(f.height, 2);
}

#[test]
fn test_camera_modal_clear_frame() {
    let mut modal = CameraModal::new();
    modal.set_frame(AsciiFrame::from_chars(vec!['#'; 6], 3, 2));
    assert!(modal.frame.is_some());

    modal.clear_frame();
    assert!(modal.frame.is_none());
}

#[test]
fn test_camera_modal_with_frame_visible() {
    let mut modal = CameraModal::new();
    modal.visible = true;
    modal.set_frame(AsciiFrame::from_chars(vec!['@'; 200], 20, 10));

    let f = modal.frame.as_ref().unwrap();
    assert_eq!(f.chars.len(), 200);
    assert!(f.chars.iter().all(|&c| c == '@'));
}

// ==================== StatusBar Tests ====================

#[test]
fn test_status_bar_new() {
    let sb = StatusBar::new();
    assert!(sb.visible);
}

#[test]
fn test_status_bar_default() {
    let sb = StatusBar::default();
    assert!(sb.visible);
}

#[test]
fn test_status_bar_with_visibility_true() {
    let sb = StatusBar::with_visibility(true);
    assert!(sb.visible);
}

#[test]
fn test_status_bar_with_visibility_false() {
    let sb = StatusBar::with_visibility(false);
    assert!(!sb.visible);
}

#[test]
fn test_status_bar_toggle() {
    let mut sb = StatusBar::new();
    assert!(sb.visible);
    sb.toggle();
    assert!(!sb.visible);
    sb.toggle();
    assert!(sb.visible);
}

#[test]
fn test_status_bar_format_camera_on() {
    let sb = StatusBar::new();
    let mut modal = CameraModal::new();
    modal.visible = true;

    let text = sb.format(&modal);
    assert!(text.contains("cam:on"));
    assert!(text.contains("bottom-right"));
    assert!(text.contains("small"));
    assert!(text.contains("standard"));
}

#[test]
fn test_status_bar_format_camera_off() {
    let sb = StatusBar::new();
    let modal = CameraModal::new();

    let text = sb.format(&modal);
    assert!(text.contains("cam:off"));
}

#[test]
fn test_status_bar_format_reflects_position() {
    let sb = StatusBar::new();
    let mut modal = CameraModal::new();

    modal.position = ModalPosition::TopLeft;
    assert!(sb.format(&modal).contains("top-left"));

    modal.position = ModalPosition::TopRight;
    assert!(sb.format(&modal).contains("top-right"));

    modal.position = ModalPosition::BottomLeft;
    assert!(sb.format(&modal).contains("bottom-left"));

    modal.position = ModalPosition::Center;
    assert!(sb.format(&modal).contains("center"));
}

#[test]
fn test_status_bar_format_reflects_size() {
    let sb = StatusBar::new();
    let mut modal = CameraModal::new();

    modal.size = ModalSize::Small;
    assert!(sb.format(&modal).contains("small"));

    modal.size = ModalSize::Medium;
    assert!(sb.format(&modal).contains("medium"));

    modal.size = ModalSize::Large;
    assert!(sb.format(&modal).contains("large"));
}

#[test]
fn test_status_bar_format_reflects_charset() {
    let sb = StatusBar::new();
    let mut modal = CameraModal::new();

    modal.charset = CharSet::Standard;
    assert!(sb.format(&modal).contains("standard"));

    modal.charset = CharSet::Blocks;
    assert!(sb.format(&modal).contains("blocks"));

    modal.charset = CharSet::Minimal;
    assert!(sb.format(&modal).contains("minimal"));

    modal.charset = CharSet::Braille;
    assert!(sb.format(&modal).contains("braille"));
}

#[test]
fn test_status_bar_format_has_separators() {
    let sb = StatusBar::new();
    let modal = CameraModal::new();

    let text = sb.format(&modal);
    assert_eq!(text.matches('|').count(), 3);
}

#[test]
fn test_status_bar_format_has_padding() {
    let sb = StatusBar::new();
    let modal = CameraModal::new();

    let text = sb.format(&modal);
    assert!(text.starts_with(' '));
    assert!(text.ends_with(' '));
}
