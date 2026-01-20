//! End-to-end tests for ASCII rendering functionality.
//!
//! These tests verify the Phase 3 milestone acceptance criteria:
//! - Camera frame converts to ASCII
//! - Face recognizable in output (verified by meaningful character variation)
//! - Different charsets produce different looks
//! - Performance: <10ms per frame

use space_recorder::ascii::{
    BLOCKS_CHARSET, MINIMAL_CHARSET, STANDARD_CHARSET, apply_edge_detection, calculate_dimensions,
    downsample, map_to_chars, render_braille, to_grayscale,
};
use space_recorder::camera::{CameraCapture, CameraSettings, Frame, FrameFormat, list_devices};
use std::collections::HashSet;
use std::thread;
use std::time::{Duration, Instant};

/// Helper to create a test frame with specified pattern.
fn make_test_frame(pattern: &str, width: u32, height: u32) -> Frame {
    let pixel_count = (width * height) as usize;
    let data = match pattern {
        "gradient_h" => {
            // Horizontal gradient: left dark, right bright
            let mut data = Vec::with_capacity(pixel_count * 3);
            for y in 0..height {
                for x in 0..width {
                    let brightness = ((x as f32 / width as f32) * 255.0) as u8;
                    data.extend_from_slice(&[brightness, brightness, brightness]);
                    let _ = y; // suppress unused warning
                }
            }
            data
        }
        "gradient_v" => {
            // Vertical gradient: top dark, bottom bright
            let mut data = Vec::with_capacity(pixel_count * 3);
            for y in 0..height {
                let brightness = ((y as f32 / height as f32) * 255.0) as u8;
                for _x in 0..width {
                    data.extend_from_slice(&[brightness, brightness, brightness]);
                }
            }
            data
        }
        "face_like" => {
            // Simulate a face-like pattern: bright center (face), dark edges (background)
            let mut data = Vec::with_capacity(pixel_count * 3);
            let center_x = width as f32 / 2.0;
            let center_y = height as f32 / 2.0;
            let max_dist = (center_x.powi(2) + center_y.powi(2)).sqrt();

            for y in 0..height {
                for x in 0..width {
                    let dist =
                        ((x as f32 - center_x).powi(2) + (y as f32 - center_y).powi(2)).sqrt();
                    // Bright at center, darker at edges (simulates face in frame)
                    let brightness = (255.0 * (1.0 - dist / max_dist)).max(0.0) as u8;
                    data.extend_from_slice(&[brightness, brightness, brightness]);
                }
            }
            data
        }
        "checkerboard" => {
            // Checkerboard pattern for testing edge detection
            let mut data = Vec::with_capacity(pixel_count * 3);
            for y in 0..height {
                for x in 0..width {
                    let brightness = if (x + y) % 2 == 0 { 0 } else { 255 };
                    data.extend_from_slice(&[brightness, brightness, brightness]);
                }
            }
            data
        }
        "uniform" => {
            // Uniform mid-gray
            vec![128u8; pixel_count * 3]
        }
        _ => panic!("Unknown pattern: {}", pattern),
    };

    Frame {
        data,
        width,
        height,
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    }
}

/// Full rendering pipeline: frame -> grayscale -> downsample -> map to chars.
fn render_frame_to_ascii(
    frame: &Frame,
    char_width: u16,
    char_height: u16,
    charset: &[char],
    invert: bool,
    edge_detection: bool,
) -> Vec<char> {
    // Step 1: Convert to grayscale
    let gray = to_grayscale(frame);

    // Step 2: Optional edge detection
    let gray = if edge_detection {
        apply_edge_detection(&gray, frame.width, frame.height)
    } else {
        gray
    };

    // Step 3: Downsample to character grid
    let brightness = downsample(&gray, frame.width, frame.height, char_width, char_height);

    // Step 4: Map to characters
    map_to_chars(&brightness, charset, invert)
}

/// Convert ASCII chars to a displayable string with newlines.
fn chars_to_string(chars: &[char], width: u16) -> String {
    chars
        .chunks(width as usize)
        .map(|row| row.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

// ====================
// Test: Camera frame converts to ASCII
// ====================

#[test]
fn test_frame_converts_to_ascii() {
    // Create a test frame with a gradient pattern
    let frame = make_test_frame("gradient_h", 640, 480);

    // Render to ASCII
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 40, 20);
    let chars = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);

    // Verify output dimensions
    assert_eq!(
        chars.len(),
        (char_w * char_h) as usize,
        "Output should have char_w * char_h characters"
    );

    // Verify output contains valid characters from charset
    for c in &chars {
        assert!(
            STANDARD_CHARSET.contains(c),
            "Character '{}' should be from charset",
            c
        );
    }

    // Print for visual inspection
    println!("Horizontal gradient (40x{} chars):", char_h);
    println!("{}", chars_to_string(&chars, char_w));
}

#[test]
fn test_frame_converts_to_ascii_with_real_camera() {
    let devices = list_devices().expect("Should be able to list devices");

    if devices.is_empty() {
        println!("SKIP: No cameras available for this test");
        return;
    }

    let settings = CameraSettings::default();
    let mut camera = CameraCapture::open(settings).expect("Should open camera");
    camera.start().expect("Should start capture");

    // Wait for a frame
    let mut attempts = 0;
    while camera.get_frame().is_none() && attempts < 100 {
        thread::sleep(Duration::from_millis(50));
        attempts += 1;
    }

    let frame = camera.get_frame();
    assert!(frame.is_some(), "Should have captured at least one frame");

    let frame = frame.unwrap();
    println!(
        "Captured frame: {}x{}, {} bytes",
        frame.width,
        frame.height,
        frame.data.len()
    );

    // Render to ASCII
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 60, 20);
    let chars = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);

    // Verify output dimensions
    assert_eq!(chars.len(), (char_w * char_h) as usize);

    // Print for visual inspection
    println!("Real camera frame ({}x{} chars):", char_w, char_h);
    println!("{}", chars_to_string(&chars, char_w));

    camera.stop();
}

// ====================
// Test: Face recognizable in output (verified by character variation)
// ====================

#[test]
fn test_face_pattern_produces_recognizable_output() {
    // Create a face-like pattern (bright center, dark edges)
    let frame = make_test_frame("face_like", 640, 480);

    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 40, 20);
    let chars = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);

    // The face pattern should produce variation in the output
    let unique_chars: HashSet<char> = chars.iter().copied().collect();
    assert!(
        unique_chars.len() >= 3,
        "Face-like pattern should produce at least 3 different characters, got {}",
        unique_chars.len()
    );

    // Center should be brighter than corners
    let center_idx = (char_h / 2) as usize * char_w as usize + (char_w / 2) as usize;
    let corner_idx = 0; // top-left corner

    let center_char = chars[center_idx];
    let corner_char = chars[corner_idx];

    // Get brightness levels from charset position
    let center_brightness = STANDARD_CHARSET
        .iter()
        .position(|&c| c == center_char)
        .unwrap_or(0);
    let corner_brightness = STANDARD_CHARSET
        .iter()
        .position(|&c| c == corner_char)
        .unwrap_or(0);

    assert!(
        center_brightness > corner_brightness,
        "Center ({} at idx {}) should be brighter than corner ({} at idx {})",
        center_char,
        center_brightness,
        corner_char,
        corner_brightness
    );

    println!("Face-like pattern ({}x{} chars):", char_w, char_h);
    println!("{}", chars_to_string(&chars, char_w));
    println!(
        "Center char: '{}' (level {}), Corner char: '{}' (level {})",
        center_char, center_brightness, corner_char, corner_brightness
    );
}

#[test]
fn test_gradient_produces_progressive_characters() {
    // Horizontal gradient should produce characters that progress through charset
    let frame = make_test_frame("gradient_h", 640, 480);

    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 40, 20);
    let chars = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);

    // Check middle row for progression
    let mid_row_start = (char_h / 2) as usize * char_w as usize;
    let mid_row_end = mid_row_start + char_w as usize;
    let mid_row: Vec<char> = chars[mid_row_start..mid_row_end].to_vec();

    // Left side should be darker than right side
    let left_char = mid_row[0];
    let right_char = mid_row[char_w as usize - 1];

    let left_brightness = STANDARD_CHARSET
        .iter()
        .position(|&c| c == left_char)
        .unwrap_or(0);
    let right_brightness = STANDARD_CHARSET
        .iter()
        .position(|&c| c == right_char)
        .unwrap_or(0);

    assert!(
        right_brightness > left_brightness,
        "Right side ({} at idx {}) should be brighter than left ({} at idx {})",
        right_char,
        right_brightness,
        left_char,
        left_brightness
    );

    println!(
        "Gradient: left='{}' (lvl {}), right='{}' (lvl {})",
        left_char, left_brightness, right_char, right_brightness
    );
}

// ====================
// Test: Different charsets produce different looks
// ====================

#[test]
fn test_different_charsets_produce_different_output() {
    let frame = make_test_frame("face_like", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 40, 20);

    // Render with each charset
    let standard_chars =
        render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    let blocks_chars = render_frame_to_ascii(&frame, char_w, char_h, BLOCKS_CHARSET, false, false);
    let minimal_chars =
        render_frame_to_ascii(&frame, char_w, char_h, MINIMAL_CHARSET, false, false);

    // All outputs should have same dimensions
    assert_eq!(standard_chars.len(), blocks_chars.len());
    assert_eq!(standard_chars.len(), minimal_chars.len());

    // But different character sets
    let standard_set: HashSet<char> = standard_chars.iter().copied().collect();
    let blocks_set: HashSet<char> = blocks_chars.iter().copied().collect();
    let minimal_set: HashSet<char> = minimal_chars.iter().copied().collect();

    // Standard charset has unique ASCII chars
    assert!(
        standard_set.iter().any(|c| ".:=+*#%@".contains(*c)),
        "Standard charset should contain ASCII density chars"
    );

    // Blocks charset has block characters
    assert!(
        blocks_set.iter().any(|c| "░▒▓█".contains(*c)),
        "Blocks charset should contain block characters"
    );

    // Minimal is a subset of standard
    assert!(
        minimal_set.iter().all(|c| ".: #".contains(*c)),
        "Minimal charset should only contain space, dot, colon, hash"
    );

    println!("Standard charset output:");
    println!("{}", chars_to_string(&standard_chars, char_w));
    println!("\nBlocks charset output:");
    println!("{}", chars_to_string(&blocks_chars, char_w));
    println!("\nMinimal charset output:");
    println!("{}", chars_to_string(&minimal_chars, char_w));
}

#[test]
fn test_braille_charset_produces_output() {
    let frame = make_test_frame("face_like", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 40, 20);

    // Render with braille
    let gray = to_grayscale(&frame);
    let braille_chars =
        render_braille(&gray, frame.width, frame.height, char_w, char_h, 128, false);

    // Verify output dimensions
    assert_eq!(braille_chars.len(), (char_w * char_h) as usize);

    // All characters should be in braille range (U+2800 to U+28FF)
    for c in &braille_chars {
        let code = *c as u32;
        assert!(
            (0x2800..=0x28FF).contains(&code),
            "Character U+{:04X} should be in braille range",
            code
        );
    }

    // Should have variation (not all same character)
    let unique_braille: HashSet<char> = braille_chars.iter().copied().collect();
    assert!(
        unique_braille.len() > 1,
        "Braille output should have variation, got {} unique chars",
        unique_braille.len()
    );

    println!("Braille charset output ({}x{}):", char_w, char_h);
    println!("{}", chars_to_string(&braille_chars, char_w));
}

#[test]
fn test_invert_option_flips_brightness() {
    let frame = make_test_frame("gradient_h", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 40, 10);

    // Render normal and inverted
    let normal = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    let inverted = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, true, false);

    // Middle row: left char should flip brightness
    let mid_row_start = (char_h / 2) as usize * char_w as usize;
    let normal_left = normal[mid_row_start];
    let inverted_left = inverted[mid_row_start];

    let normal_left_lvl = STANDARD_CHARSET
        .iter()
        .position(|&c| c == normal_left)
        .unwrap();
    let inverted_left_lvl = STANDARD_CHARSET
        .iter()
        .position(|&c| c == inverted_left)
        .unwrap();

    // Dark becomes bright when inverted
    assert!(
        inverted_left_lvl > normal_left_lvl,
        "Inverted should flip brightness: normal '{}' (lvl {}) vs inverted '{}' (lvl {})",
        normal_left,
        normal_left_lvl,
        inverted_left,
        inverted_left_lvl
    );
}

#[test]
fn test_edge_detection_produces_different_output() {
    let frame = make_test_frame("checkerboard", 100, 100);
    let char_w = 20;
    let char_h = 10;

    // Render with and without edge detection
    let normal = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    let with_edges = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, true);

    // Edge detection should change the output
    assert_ne!(
        normal, with_edges,
        "Edge detection should produce different output"
    );

    println!("Without edge detection:");
    println!("{}", chars_to_string(&normal, char_w));
    println!("\nWith edge detection:");
    println!("{}", chars_to_string(&with_edges, char_w));
}

// ====================
// Test: Performance <10ms per frame
// ====================

#[test]
fn test_performance_under_10ms() {
    // Create a realistic camera frame (640x480)
    let frame = make_test_frame("face_like", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 80, 24);

    // Warm up
    for _ in 0..5 {
        let _ = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    }

    // Measure rendering time
    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    }

    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

    println!(
        "Average render time: {:.3}ms per frame ({} iterations)",
        avg_ms, iterations
    );
    println!("Output dimensions: {}x{} chars", char_w, char_h);

    // In release mode, should be <10ms. Debug builds are slower.
    // AC requires <10ms in production use.
    #[cfg(debug_assertions)]
    let threshold = 50.0; // More lenient for debug builds
    #[cfg(not(debug_assertions))]
    let threshold = 10.0;

    assert!(
        avg_ms < threshold,
        "Rendering should take <{}ms, took {:.3}ms",
        threshold,
        avg_ms
    );
}

#[test]
fn test_performance_with_edge_detection() {
    // Edge detection adds overhead - verify it's still acceptable
    let frame = make_test_frame("face_like", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 80, 24);

    // Warm up
    for _ in 0..5 {
        let _ = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, true);
    }

    // Measure rendering time with edge detection
    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, true);
    }

    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

    println!(
        "Average render time with edge detection: {:.3}ms per frame ({} iterations)",
        avg_ms, iterations
    );

    // Edge detection is an optional feature that adds overhead.
    // In release mode, should be <15ms. Debug builds are slower.
    #[cfg(debug_assertions)]
    let threshold = 100.0; // More lenient for debug builds
    #[cfg(not(debug_assertions))]
    let threshold = 15.0;

    assert!(
        avg_ms < threshold,
        "Rendering with edge detection should take <{}ms, took {:.3}ms",
        threshold,
        avg_ms
    );
}

#[test]
fn test_performance_braille_rendering() {
    // Braille rendering has more computation per character
    let frame = make_test_frame("face_like", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 80, 24);
    let gray = to_grayscale(&frame);

    // Warm up
    for _ in 0..5 {
        let _ = render_braille(&gray, frame.width, frame.height, char_w, char_h, 128, false);
    }

    // Measure rendering time
    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = render_braille(&gray, frame.width, frame.height, char_w, char_h, 128, false);
    }

    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

    println!(
        "Average braille render time: {:.3}ms per frame ({} iterations)",
        avg_ms, iterations
    );

    // In release mode, should be <10ms. Debug builds are slower.
    #[cfg(debug_assertions)]
    let threshold = 50.0;
    #[cfg(not(debug_assertions))]
    let threshold = 10.0;

    assert!(
        avg_ms < threshold,
        "Braille rendering should take <{}ms, took {:.3}ms",
        threshold,
        avg_ms
    );
}

#[test]
fn test_performance_with_real_camera_frame() {
    let devices = list_devices().expect("Should be able to list devices");

    if devices.is_empty() {
        println!("SKIP: No cameras available for this test");
        return;
    }

    let settings = CameraSettings::default();
    let mut camera = CameraCapture::open(settings).expect("Should open camera");
    camera.start().expect("Should start capture");

    // Wait for a frame
    let mut attempts = 0;
    while camera.get_frame().is_none() && attempts < 100 {
        thread::sleep(Duration::from_millis(50));
        attempts += 1;
    }

    let frame = camera.get_frame();
    assert!(frame.is_some(), "Should have captured at least one frame");

    let frame = frame.unwrap();
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 80, 24);

    // Warm up
    for _ in 0..5 {
        let _ = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    }

    // Measure rendering time
    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = render_frame_to_ascii(&frame, char_w, char_h, STANDARD_CHARSET, false, false);
    }

    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

    println!(
        "Real camera frame {}x{} -> {}x{} chars: {:.3}ms average",
        frame.width, frame.height, char_w, char_h, avg_ms
    );

    camera.stop();

    // In release mode, should be <10ms. Debug builds are slower.
    #[cfg(debug_assertions)]
    let threshold = 50.0;
    #[cfg(not(debug_assertions))]
    let threshold = 10.0;

    assert!(
        avg_ms < threshold,
        "Real camera frame rendering should take <{}ms, took {:.3}ms",
        threshold,
        avg_ms
    );
}

// ====================
// Test: Aspect ratio preservation
// ====================

#[test]
fn test_aspect_ratio_preserved() {
    // A 640x480 (4:3) frame should render with correct proportions
    let frame = make_test_frame("face_like", 640, 480);
    let (char_w, char_h) = calculate_dimensions(frame.width, frame.height, 80, 40);

    // The displayed aspect ratio (accounting for ~2:1 char aspect) should match image
    let img_aspect = 640.0 / 480.0; // 1.333...
    let char_aspect = 2.0;
    let displayed_aspect = (char_w as f32) / (char_h as f32 * char_aspect);

    let ratio = displayed_aspect / img_aspect;
    assert!(
        (0.90..=1.10).contains(&ratio),
        "Aspect ratio should be preserved within 10%: img={:.3}, displayed={:.3}, ratio={:.3}",
        img_aspect,
        displayed_aspect,
        ratio
    );

    println!(
        "640x480 -> {}x{} chars, aspect ratio preserved: {:.3}",
        char_w, char_h, ratio
    );
}
