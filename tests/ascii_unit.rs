//! Unit tests for the ASCII renderer module.
//!
//! These tests verify the core ASCII rendering algorithms:
//! - Grayscale conversion
//! - Downsampling
//! - Character mapping
//! - Braille rendering
//! - Aspect ratio calculations

use space_recorder::ascii::*;
use space_recorder::camera::{Frame, FrameFormat};
use std::time::Instant;

fn make_frame(data: Vec<u8>, width: u32, height: u32) -> Frame {
    Frame {
        data,
        width,
        height,
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    }
}

// ==================== Grayscale Conversion Tests ====================

#[test]
fn test_grayscale_pure_red() {
    // Pure red pixel: R=255, G=0, B=0
    // Luminance = 0.299 * 255 = 76.245 ≈ 76
    let frame = make_frame(vec![255, 0, 0], 1, 1);
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 1);
    assert_eq!(gray[0], 76); // 299 * 255 / 1000 = 76
}

#[test]
fn test_grayscale_pure_green() {
    // Pure green pixel: R=0, G=255, B=0
    // Luminance = 0.587 * 255 = 149.685 ≈ 149
    let frame = make_frame(vec![0, 255, 0], 1, 1);
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 1);
    assert_eq!(gray[0], 149); // 587 * 255 / 1000 = 149
}

#[test]
fn test_grayscale_pure_blue() {
    // Pure blue pixel: R=0, G=0, B=255
    // Luminance = 0.114 * 255 = 29.07 ≈ 29
    let frame = make_frame(vec![0, 0, 255], 1, 1);
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 1);
    assert_eq!(gray[0], 29); // 114 * 255 / 1000 = 29
}

#[test]
fn test_grayscale_white() {
    // White pixel: R=255, G=255, B=255
    // Luminance = (299 + 587 + 114) * 255 / 1000 = 255000/1000 = 255
    let frame = make_frame(vec![255, 255, 255], 1, 1);
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 1);
    assert_eq!(gray[0], 255);
}

#[test]
fn test_grayscale_black() {
    // Black pixel: R=0, G=0, B=0
    let frame = make_frame(vec![0, 0, 0], 1, 1);
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 1);
    assert_eq!(gray[0], 0);
}

#[test]
fn test_grayscale_luminance_order() {
    // Green should produce highest luminance, then red, then blue
    // This matches human perception
    let red = make_frame(vec![255, 0, 0], 1, 1);
    let green = make_frame(vec![0, 255, 0], 1, 1);
    let blue = make_frame(vec![0, 0, 255], 1, 1);

    let r = to_grayscale(&red)[0];
    let g = to_grayscale(&green)[0];
    let b = to_grayscale(&blue)[0];

    assert!(g > r, "green ({}) should be brighter than red ({})", g, r);
    assert!(r > b, "red ({}) should be brighter than blue ({})", r, b);
}

#[test]
fn test_grayscale_multiple_pixels() {
    // 3x1 image: Red, Green, Blue pixels
    let frame = make_frame(
        vec![
            255, 0, 0, // Red
            0, 255, 0, // Green
            0, 0, 255, // Blue
        ],
        3,
        1,
    );
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 3);
    assert_eq!(gray[0], 76); // Red luminance
    assert_eq!(gray[1], 149); // Green luminance
    assert_eq!(gray[2], 29); // Blue luminance
}

#[test]
fn test_grayscale_2x2_grid() {
    // 2x2 image
    let frame = make_frame(
        vec![
            255, 0, 0, 0, 255, 0, // Row 0: Red, Green
            0, 0, 255, 128, 128, 128, // Row 1: Blue, Gray
        ],
        2,
        2,
    );
    let gray = to_grayscale(&frame);
    assert_eq!(gray.len(), 4);
    assert_eq!(gray[0], 76); // Red
    assert_eq!(gray[1], 149); // Green
    assert_eq!(gray[2], 29); // Blue
    // Gray: (299*128 + 587*128 + 114*128) / 1000 = 128000/1000 = 128
    assert_eq!(gray[3], 128);
}

#[test]
fn test_grayscale_into_reuses_buffer() {
    let frame = make_frame(vec![255, 0, 0, 0, 255, 0], 2, 1);
    let mut buffer = Vec::with_capacity(100); // Pre-allocated

    let count = to_grayscale_into(&frame, &mut buffer);
    assert_eq!(count, 2);
    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer[0], 76);
    assert_eq!(buffer[1], 149);

    // Reuse buffer with different frame
    let frame2 = make_frame(vec![0, 0, 255], 1, 1);
    let count2 = to_grayscale_into(&frame2, &mut buffer);
    assert_eq!(count2, 1);
    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer[0], 29);
}

#[test]
fn test_grayscale_empty_frame() {
    let frame = make_frame(vec![], 0, 0);
    let gray = to_grayscale(&frame);
    assert!(gray.is_empty());
}

#[test]
fn test_grayscale_mid_gray() {
    // Mid-gray: R=128, G=128, B=128
    // (299*128 + 587*128 + 114*128) / 1000 = 128000/1000 = 128
    let frame = make_frame(vec![128, 128, 128], 1, 1);
    let gray = to_grayscale(&frame);
    assert_eq!(gray[0], 128);
}

// ==================== Downsampling Tests ====================

#[test]
fn test_downsample_1to1() {
    // 1x1 image to 1x1 character - no actual downsampling
    let gray = vec![128];
    let result = downsample(&gray, 1, 1, 1, 1);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], 128);
}

#[test]
fn test_downsample_2x2_to_1x1() {
    // 2x2 image averaged into single character
    // Pixels: 0, 100, 200, 56 → average = 356/4 = 89
    let gray = vec![0, 100, 200, 56];
    let result = downsample(&gray, 2, 2, 1, 1);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], 89);
}

#[test]
fn test_downsample_4x4_to_2x2() {
    // 4x4 image to 2x2 character grid
    // Each 2x2 block becomes one character
    #[rustfmt::skip]
    let gray = vec![
        10, 20,  30, 40,   // Row 0
        50, 60,  70, 80,   // Row 1
        90, 100, 110, 120, // Row 2
        130, 140, 150, 160 // Row 3
    ];
    let result = downsample(&gray, 4, 4, 2, 2);
    assert_eq!(result.len(), 4);

    // Top-left 2x2: (10+20+50+60)/4 = 140/4 = 35
    assert_eq!(result[0], 35);
    // Top-right 2x2: (30+40+70+80)/4 = 220/4 = 55
    assert_eq!(result[1], 55);
    // Bottom-left 2x2: (90+100+130+140)/4 = 460/4 = 115
    assert_eq!(result[2], 115);
    // Bottom-right 2x2: (110+120+150+160)/4 = 540/4 = 135
    assert_eq!(result[3], 135);
}

#[test]
fn test_downsample_preserves_order() {
    // Verify row-major output order
    // 6x2 image to 3x1 (each 2x2 block → one char)
    #[rustfmt::skip]
    let gray = vec![
        0, 0,    100, 100, 200, 200,
        0, 0,    100, 100, 200, 200,
    ];
    let result = downsample(&gray, 6, 2, 3, 1);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], 0);
    assert_eq!(result[1], 100);
    assert_eq!(result[2], 200);
}

#[test]
fn test_downsample_uniform_image() {
    // All pixels same value should result in same value everywhere
    let gray = vec![128; 16]; // 4x4 image
    let result = downsample(&gray, 4, 4, 2, 2);
    assert_eq!(result.len(), 4);
    assert!(result.iter().all(|&v| v == 128));
}

#[test]
fn test_downsample_empty_input() {
    let gray: Vec<u8> = vec![];
    let result = downsample(&gray, 0, 0, 10, 10);
    assert!(result.is_empty());
}

#[test]
fn test_downsample_zero_output_width() {
    let gray = vec![128; 4];
    let result = downsample(&gray, 2, 2, 0, 2);
    assert!(result.is_empty());
}

#[test]
fn test_downsample_zero_output_height() {
    let gray = vec![128; 4];
    let result = downsample(&gray, 2, 2, 2, 0);
    assert!(result.is_empty());
}

#[test]
fn test_downsample_configurable_dimensions() {
    // Test that output dimensions are configurable
    let gray = vec![100; 100]; // 10x10 image

    // Different output sizes
    let r1 = downsample(&gray, 10, 10, 5, 5);
    assert_eq!(r1.len(), 25);

    let r2 = downsample(&gray, 10, 10, 2, 2);
    assert_eq!(r2.len(), 4);

    let r3 = downsample(&gray, 10, 10, 10, 1);
    assert_eq!(r3.len(), 10);

    let r4 = downsample(&gray, 10, 10, 1, 10);
    assert_eq!(r4.len(), 10);
}

#[test]
fn test_downsample_non_divisible_dimensions() {
    // 5x5 image to 2x2 - cells have different sizes
    let gray = vec![100; 25]; // 5x5 uniform
    let result = downsample(&gray, 5, 5, 2, 2);
    assert_eq!(result.len(), 4);
    // All cells should still average to 100
    assert!(result.iter().all(|&v| v == 100));
}

#[test]
fn test_downsample_gradient_horizontal() {
    // Horizontal gradient: left side dark, right side bright
    // 4x2 image: [0, 0, 255, 255] in each row
    #[rustfmt::skip]
    let gray = vec![
        0, 0, 255, 255,
        0, 0, 255, 255,
    ];
    let result = downsample(&gray, 4, 2, 2, 1);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], 0); // Left half: all 0s
    assert_eq!(result[1], 255); // Right half: all 255s
}

#[test]
fn test_downsample_gradient_vertical() {
    // Vertical gradient: top dark, bottom bright
    // 2x4 image
    #[rustfmt::skip]
    let gray = vec![
        0, 0,
        0, 0,
        255, 255,
        255, 255,
    ];
    let result = downsample(&gray, 2, 4, 1, 2);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], 0); // Top half: all 0s
    assert_eq!(result[1], 255); // Bottom half: all 255s
}

#[test]
fn test_downsample_realistic_camera_resolution() {
    // Simulate downsampling 640x480 to 40x20
    let gray = vec![128; 640 * 480];
    let result = downsample(&gray, 640, 480, 40, 20);
    assert_eq!(result.len(), 40 * 20);
    assert!(result.iter().all(|&v| v == 128));
}

#[test]
fn test_downsample_into_basic() {
    let gray = vec![0, 100, 200, 56];
    let mut buffer = Vec::new();
    let count = downsample_into(&gray, 2, 2, 1, 1, &mut buffer);
    assert_eq!(count, 1);
    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer[0], 89);
}

#[test]
fn test_downsample_into_reuses_buffer() {
    let gray1 = vec![100; 4]; // 2x2
    let gray2 = vec![200; 9]; // 3x3

    let mut buffer = Vec::with_capacity(100);

    let count1 = downsample_into(&gray1, 2, 2, 1, 1, &mut buffer);
    assert_eq!(count1, 1);
    assert_eq!(buffer[0], 100);

    // Reuse buffer with different dimensions
    let count2 = downsample_into(&gray2, 3, 3, 1, 1, &mut buffer);
    assert_eq!(count2, 1);
    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer[0], 200);
}

#[test]
fn test_downsample_into_empty() {
    let gray: Vec<u8> = vec![];
    let mut buffer = vec![1, 2, 3]; // Pre-existing data
    let count = downsample_into(&gray, 0, 0, 10, 10, &mut buffer);
    assert_eq!(count, 0);
    assert!(buffer.is_empty()); // Buffer should be cleared
}

#[test]
fn test_downsample_checkerboard() {
    // Checkerboard pattern: alternating 0 and 255
    // 4x4 image to 2x2 - each 2x2 block has mixed values
    #[rustfmt::skip]
    let gray = vec![
        0, 255, 0, 255,
        255, 0, 255, 0,
        0, 255, 0, 255,
        255, 0, 255, 0,
    ];
    let result = downsample(&gray, 4, 4, 2, 2);
    assert_eq!(result.len(), 4);
    // Each 2x2 block: (0+255+255+0)/4 = 510/4 = 127
    assert!(result.iter().all(|&v| v == 127));
}

// ==================== Character Mapping Tests ====================

#[test]
fn test_map_standard_charset_has_10_levels() {
    assert_eq!(STANDARD_CHARSET.len(), 10);
    assert_eq!(STANDARD_CHARSET[0], ' '); // darkest
    assert_eq!(STANDARD_CHARSET[9], '@'); // brightest
}

#[test]
fn test_map_brightness_extremes() {
    // Brightness 0 -> first char (darkest)
    // Brightness 255 -> last char (brightest)
    let brightness = vec![0, 255];
    let chars = map_to_chars(&brightness, STANDARD_CHARSET, false);
    assert_eq!(chars[0], ' '); // darkest
    assert_eq!(chars[1], '@'); // brightest
}

#[test]
fn test_map_brightness_to_index() {
    // Standard charset: [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@']
    // Index formula: (b * 9) / 255
    // b=0   -> idx=0  -> ' '
    // b=28  -> idx=0  -> ' '  (28*9/255=0)
    // b=29  -> idx=1  -> '.'  (29*9/255=1)
    // b=127 -> idx=4  -> '='  (127*9/255=4)
    // b=255 -> idx=9  -> '@'  (255*9/255=9)
    let brightness = vec![0, 28, 29, 127, 255];
    let chars = map_to_chars(&brightness, STANDARD_CHARSET, false);

    assert_eq!(chars[0], ' '); // idx 0
    assert_eq!(chars[1], ' '); // idx 0
    assert_eq!(chars[2], '.'); // idx 1
    assert_eq!(chars[3], '='); // idx 4
    assert_eq!(chars[4], '@'); // idx 9
}

#[test]
fn test_map_invert_option() {
    // With invert=true, brightness 0 becomes 255 and vice versa
    let brightness = vec![0, 255];
    let chars = map_to_chars(&brightness, STANDARD_CHARSET, true);
    assert_eq!(chars[0], '@'); // 255-0=255 -> brightest
    assert_eq!(chars[1], ' '); // 255-255=0 -> darkest
}

#[test]
fn test_map_invert_mid_brightness() {
    // Mid brightness should stay roughly mid regardless of invert
    let brightness = vec![127];
    let normal = map_to_chars(&brightness, STANDARD_CHARSET, false);
    let inverted = map_to_chars(&brightness, STANDARD_CHARSET, true);

    // 127 -> idx 4 ('=')
    // 255-127=128 -> idx 4 ('=')
    assert_eq!(normal[0], '=');
    assert_eq!(inverted[0], '=');
}

#[test]
fn test_map_custom_charset() {
    // Test with a custom 3-level charset
    let charset = &['.', 'o', 'O'];
    let brightness = vec![0, 128, 255];
    let chars = map_to_chars(&brightness, charset, false);
    assert_eq!(chars[0], '.'); // idx 0: (0 * 2) / 255 = 0
    assert_eq!(chars[1], 'o'); // idx 1: (128 * 2) / 255 = 256 / 255 = 1
    assert_eq!(chars[2], 'O'); // idx 2: (255 * 2) / 255 = 2
}

#[test]
fn test_map_empty_brightness() {
    let brightness: Vec<u8> = vec![];
    let chars = map_to_chars(&brightness, STANDARD_CHARSET, false);
    assert!(chars.is_empty());
}

#[test]
fn test_map_empty_charset() {
    // Edge case: empty charset should return spaces
    let brightness = vec![0, 127, 255];
    let chars = map_to_chars(&brightness, &[], false);
    assert_eq!(chars.len(), 3);
    assert!(chars.iter().all(|&c| c == ' '));
}

#[test]
fn test_map_single_char_charset() {
    // Single character charset: all brightness maps to same char
    let charset = &['#'];
    let brightness = vec![0, 127, 255];
    let chars = map_to_chars(&brightness, charset, false);
    assert!(chars.iter().all(|&c| c == '#'));
}

#[test]
fn test_map_into_basic() {
    let brightness = vec![0, 127, 255];
    let mut buffer = Vec::new();
    let count = map_to_chars_into(&brightness, STANDARD_CHARSET, false, &mut buffer);
    assert_eq!(count, 3);
    assert_eq!(buffer.len(), 3);
    assert_eq!(buffer[0], ' ');
    assert_eq!(buffer[2], '@');
}

#[test]
fn test_map_into_reuses_buffer() {
    let brightness1 = vec![0, 255];
    let brightness2 = vec![255, 0, 127];

    let mut buffer = Vec::with_capacity(100);

    let count1 = map_to_chars_into(&brightness1, STANDARD_CHARSET, false, &mut buffer);
    assert_eq!(count1, 2);
    assert_eq!(buffer, vec![' ', '@']);

    // Reuse buffer
    let count2 = map_to_chars_into(&brightness2, STANDARD_CHARSET, false, &mut buffer);
    assert_eq!(count2, 3);
    assert_eq!(buffer.len(), 3);
    assert_eq!(buffer[0], '@');
    assert_eq!(buffer[1], ' ');
}

#[test]
fn test_map_into_with_invert() {
    let brightness = vec![0, 255];
    let mut buffer = Vec::new();
    map_to_chars_into(&brightness, STANDARD_CHARSET, true, &mut buffer);
    assert_eq!(buffer[0], '@'); // inverted: 0 -> 255 -> brightest
    assert_eq!(buffer[1], ' '); // inverted: 255 -> 0 -> darkest
}

#[test]
fn test_map_full_range_coverage() {
    // Test that all brightness levels map to valid characters
    let brightness: Vec<u8> = (0..=255).collect();
    let chars = map_to_chars(&brightness, STANDARD_CHARSET, false);
    assert_eq!(chars.len(), 256);

    // All chars should be from the charset
    for c in &chars {
        assert!(STANDARD_CHARSET.contains(c));
    }

    // First should be darkest, last should be brightest
    assert_eq!(chars[0], ' ');
    assert_eq!(chars[255], '@');
}

#[test]
fn test_map_gradient_produces_ordered_chars() {
    // Monotonically increasing brightness should produce monotonically
    // increasing (or equal) character indices
    let brightness: Vec<u8> = (0..=255).step_by(10).collect();
    let chars = map_to_chars(&brightness, STANDARD_CHARSET, false);

    let mut prev_idx = 0;
    for c in &chars {
        let idx = STANDARD_CHARSET.iter().position(|&x| x == *c).unwrap();
        assert!(idx >= prev_idx, "Character indices should be non-decreasing");
        prev_idx = idx;
    }
}

// ==================== Blocks Charset Tests ====================

#[test]
fn test_blocks_charset_has_5_levels() {
    assert_eq!(BLOCKS_CHARSET.len(), 5);
    assert_eq!(BLOCKS_CHARSET[0], ' '); // darkest
    assert_eq!(BLOCKS_CHARSET[4], '█'); // brightest (full block)
}

#[test]
fn test_blocks_charset_contains_unicode_blocks() {
    // Verify block characters are present
    assert!(BLOCKS_CHARSET.contains(&'░')); // light shade
    assert!(BLOCKS_CHARSET.contains(&'▒')); // medium shade
    assert!(BLOCKS_CHARSET.contains(&'▓')); // dark shade
    assert!(BLOCKS_CHARSET.contains(&'█')); // full block
}

#[test]
fn test_map_with_blocks_charset() {
    let brightness = vec![0, 64, 128, 192, 255];
    let chars = map_to_chars(&brightness, BLOCKS_CHARSET, false);
    assert_eq!(chars[0], ' '); // darkest
    assert_eq!(chars[4], '█'); // brightest
}

// ==================== Minimal Charset Tests ====================

#[test]
fn test_minimal_charset_has_4_levels() {
    assert_eq!(MINIMAL_CHARSET.len(), 4);
    assert_eq!(MINIMAL_CHARSET[0], ' '); // darkest
    assert_eq!(MINIMAL_CHARSET[3], '#'); // brightest
}

#[test]
fn test_map_with_minimal_charset() {
    let brightness = vec![0, 85, 170, 255];
    let chars = map_to_chars(&brightness, MINIMAL_CHARSET, false);
    assert_eq!(chars[0], ' '); // darkest
    assert_eq!(chars[3], '#'); // brightest
}

// ==================== Braille Tests ====================

#[test]
fn test_braille_base_char() {
    assert_eq!(BRAILLE_BASE, '\u{2800}');
}

#[test]
fn test_grid_to_braille_empty() {
    // All false = empty braille (base character)
    let grid = [[false; 4]; 2];
    assert_eq!(grid_to_braille(grid), '\u{2800}');
}

#[test]
fn test_grid_to_braille_full() {
    // All true = all 8 dots lit (0xFF offset)
    let grid = [[true; 4]; 2];
    assert_eq!(grid_to_braille(grid), '\u{28FF}');
}

#[test]
fn test_grid_to_braille_single_dots() {
    // Test individual dot positions
    // [0,0] = bit 0 (0x01)
    let mut grid = [[false; 4]; 2];
    grid[0][0] = true;
    assert_eq!(grid_to_braille(grid), '\u{2801}');

    // [0,1] = bit 1 (0x02)
    grid = [[false; 4]; 2];
    grid[0][1] = true;
    assert_eq!(grid_to_braille(grid), '\u{2802}');

    // [0,2] = bit 2 (0x04)
    grid = [[false; 4]; 2];
    grid[0][2] = true;
    assert_eq!(grid_to_braille(grid), '\u{2804}');

    // [0,3] = bit 6 (0x40)
    grid = [[false; 4]; 2];
    grid[0][3] = true;
    assert_eq!(grid_to_braille(grid), '\u{2840}');

    // [1,0] = bit 3 (0x08)
    grid = [[false; 4]; 2];
    grid[1][0] = true;
    assert_eq!(grid_to_braille(grid), '\u{2808}');

    // [1,1] = bit 4 (0x10)
    grid = [[false; 4]; 2];
    grid[1][1] = true;
    assert_eq!(grid_to_braille(grid), '\u{2810}');

    // [1,2] = bit 5 (0x20)
    grid = [[false; 4]; 2];
    grid[1][2] = true;
    assert_eq!(grid_to_braille(grid), '\u{2820}');

    // [1,3] = bit 7 (0x80)
    grid = [[false; 4]; 2];
    grid[1][3] = true;
    assert_eq!(grid_to_braille(grid), '\u{2880}');
}

#[test]
fn test_render_braille_empty_input() {
    let gray: Vec<u8> = vec![];
    let result = render_braille(&gray, 0, 0, 10, 10, 128, false);
    assert!(result.is_empty());
}

#[test]
fn test_render_braille_zero_output() {
    let gray = vec![128; 100];
    let result = render_braille(&gray, 10, 10, 0, 0, 128, false);
    assert!(result.is_empty());
}

#[test]
fn test_render_braille_output_dimensions() {
    // 8x8 image to 4x2 braille characters
    let gray = vec![200; 64]; // All bright
    let result = render_braille(&gray, 8, 8, 4, 2, 128, false);
    assert_eq!(result.len(), 8); // 4 * 2 = 8 characters
}

#[test]
fn test_render_braille_all_bright() {
    // All pixels above threshold -> all dots lit
    let gray = vec![255; 16]; // 4x4 all white
    let result = render_braille(&gray, 4, 4, 2, 1, 128, false);
    assert_eq!(result.len(), 2);
    // All dots should be lit (full braille)
    assert!(result.iter().all(|&c| c == '\u{28FF}'));
}

#[test]
fn test_render_braille_all_dark() {
    // All pixels below threshold -> empty braille
    let gray = vec![0; 16]; // 4x4 all black
    let result = render_braille(&gray, 4, 4, 2, 1, 128, false);
    assert_eq!(result.len(), 2);
    // All dots should be off (empty braille)
    assert!(result.iter().all(|&c| c == '\u{2800}'));
}

#[test]
fn test_render_braille_threshold() {
    // Test that threshold properly separates bright from dark
    let gray = vec![100, 200, 100, 200]; // 2x2 alternating

    // With threshold 150: only 200s are bright
    let result1 = render_braille(&gray, 2, 2, 1, 1, 150, false);
    assert_eq!(result1.len(), 1);
    // Some dots lit, some not
    assert!(result1[0] != '\u{2800}' && result1[0] != '\u{28FF}');

    // With threshold 50: all are bright
    let result2 = render_braille(&gray, 2, 2, 1, 1, 50, false);
    assert_eq!(result2.len(), 1);
}

#[test]
fn test_render_braille_invert() {
    let gray = vec![200; 16]; // All bright

    // Without invert, above threshold (128) -> dots on
    let normal = render_braille(&gray, 4, 4, 2, 1, 128, false);
    // With invert, 255-200=55, below threshold -> dots off
    let inverted = render_braille(&gray, 4, 4, 2, 1, 128, true);

    assert_ne!(normal, inverted);
}

#[test]
fn test_render_braille_into_reuses_buffer() {
    let gray1 = vec![255; 16];
    let gray2 = vec![0; 16];
    let mut buffer = Vec::with_capacity(100);

    let count1 = render_braille_into(&gray1, 4, 4, 2, 1, 128, false, &mut buffer);
    assert_eq!(count1, 2);
    assert!(buffer.iter().all(|&c| c == '\u{28FF}'));

    // Reuse buffer with different data
    let count2 = render_braille_into(&gray2, 4, 4, 2, 1, 128, false, &mut buffer);
    assert_eq!(count2, 2);
    assert!(buffer.iter().all(|&c| c == '\u{2800}'));
}

#[test]
fn test_render_braille_realistic_resolution() {
    // Simulate rendering 640x480 to 80x20 braille characters
    // This gives effective resolution of 160x80 dots
    let gray = vec![128; 640 * 480];
    let result = render_braille(&gray, 640, 480, 80, 20, 127, false);
    assert_eq!(result.len(), 80 * 20);
}

// ==================== Aspect Ratio Correction Tests ====================

#[test]
fn test_default_char_aspect_ratio() {
    assert_eq!(DEFAULT_CHAR_ASPECT_RATIO, 2.0);
}

#[test]
fn test_calculate_dimensions_zero_inputs() {
    // Zero image dimensions
    assert_eq!(calculate_dimensions(0, 480, 80, 24), (0, 0));
    assert_eq!(calculate_dimensions(640, 0, 80, 24), (0, 0));

    // Zero max dimensions
    assert_eq!(calculate_dimensions(640, 480, 0, 24), (0, 0));
    assert_eq!(calculate_dimensions(640, 480, 80, 0), (0, 0));

    // All zeros
    assert_eq!(calculate_dimensions(0, 0, 0, 0), (0, 0));
}

#[test]
fn test_calculate_dimensions_square_image() {
    // A 100x100 (1:1) image with char aspect 2.0
    // To display as square: char_width / char_height = 2.0
    // So if width=80, height should be 40
    let (w, h) = calculate_dimensions(100, 100, 80, 40);
    assert_eq!(w, 80);
    assert_eq!(h, 40);
}

#[test]
fn test_calculate_dimensions_4_3_image() {
    // A 640x480 (4:3) image
    // img_aspect = 4/3 = 1.333...
    // target_char_aspect = 1.333... * 2.0 = 2.666...
    // If width=80, height = 80 / 2.666... = 30
    let (w, h) = calculate_dimensions(640, 480, 80, 40);
    assert_eq!(w, 80);
    assert_eq!(h, 30);
}

#[test]
fn test_calculate_dimensions_16_9_image() {
    // A 1920x1080 (16:9) image
    // img_aspect = 16/9 = 1.777...
    // target_char_aspect = 1.777... * 2.0 = 3.555...
    // If width=80, height = 80 / 3.555... ≈ 22.5 → rounds to 22 or 23
    let (w, h) = calculate_dimensions(1920, 1080, 80, 40);
    assert_eq!(w, 80);
    // 80 / (16/9 * 2) = 80 * 9 / 32 = 22.5 → 22 or 23
    assert!(h == 22 || h == 23);
}

#[test]
fn test_calculate_dimensions_portrait_image() {
    // A 480x640 (3:4) portrait image
    // img_aspect = 3/4 = 0.75
    // target_char_aspect = 0.75 * 2.0 = 1.5
    // If width=80, height = 80 / 1.5 = 53.33... → rounds to 53
    // But max_height is 40, so we're height-constrained
    // width = 40 * 1.5 = 60
    let (w, h) = calculate_dimensions(480, 640, 80, 40);
    assert_eq!(h, 40);
    assert_eq!(w, 60);
}

#[test]
fn test_calculate_dimensions_very_wide_image() {
    // A 1000x100 (10:1) ultra-wide image
    // img_aspect = 10.0
    // target_char_aspect = 10.0 * 2.0 = 20.0
    // If width=80, height = 80 / 20.0 = 4
    let (w, h) = calculate_dimensions(1000, 100, 80, 40);
    assert_eq!(w, 80);
    assert_eq!(h, 4);
}

#[test]
fn test_calculate_dimensions_very_tall_image() {
    // A 100x1000 (1:10) ultra-tall image
    // img_aspect = 0.1
    // target_char_aspect = 0.1 * 2.0 = 0.2
    // If width=80, height = 80 / 0.2 = 400 (way over max)
    // So use height-constrained: width = 40 * 0.2 = 8
    let (w, h) = calculate_dimensions(100, 1000, 80, 40);
    assert_eq!(h, 40);
    assert_eq!(w, 8);
}

#[test]
fn test_calculate_dimensions_fits_within_max() {
    // Result should always fit within max dimensions
    let test_cases = [
        (640, 480, 80, 24),
        (1920, 1080, 120, 30),
        (480, 640, 60, 40),
        (100, 100, 50, 50),
        (1000, 100, 100, 100),
        (100, 1000, 100, 100),
    ];

    for (img_w, img_h, max_w, max_h) in test_cases {
        let (w, h) = calculate_dimensions(img_w, img_h, max_w, max_h);
        assert!(
            w <= max_w,
            "Width {} exceeds max {} for image {}x{} in {}x{}",
            w,
            max_w,
            img_w,
            img_h,
            max_w,
            max_h
        );
        assert!(
            h <= max_h,
            "Height {} exceeds max {} for image {}x{} in {}x{}",
            h,
            max_h,
            img_w,
            img_h,
            max_w,
            max_h
        );
    }
}

#[test]
fn test_calculate_dimensions_nonzero_result() {
    // Result should be at least 1x1 for valid inputs
    let test_cases = [
        (1, 1, 1, 1),
        (10000, 1, 80, 24),
        (1, 10000, 80, 24),
        (640, 480, 1, 1),
    ];

    for (img_w, img_h, max_w, max_h) in test_cases {
        let (w, h) = calculate_dimensions(img_w, img_h, max_w, max_h);
        assert!(
            w >= 1 && h >= 1,
            "Got {}x{} for image {}x{} in {}x{}",
            w,
            h,
            img_w,
            img_h,
            max_w,
            max_h
        );
    }
}

#[test]
fn test_calculate_dimensions_with_aspect_custom() {
    // Test with char_aspect = 1.0 (square chars like some bitmap fonts)
    // For a 100x100 image, with square chars, output should also be 1:1
    let (w, h) = calculate_dimensions_with_aspect(100, 100, 80, 40, 1.0);
    // target_char_aspect = 1.0 * 1.0 = 1.0, so width=height
    // Constrained by height (40), so width=40
    assert_eq!(w, 40);
    assert_eq!(h, 40);
}

#[test]
fn test_calculate_dimensions_with_aspect_narrow_chars() {
    // Test with char_aspect = 3.0 (very tall, narrow chars)
    // For a 100x100 image
    // target_char_aspect = 1.0 * 3.0 = 3.0
    // If width=80, height = 80 / 3.0 = 26.67 → 27
    let (w, h) = calculate_dimensions_with_aspect(100, 100, 80, 40, 3.0);
    assert_eq!(w, 80);
    assert_eq!(h, 27);
}

#[test]
fn test_calculate_dimensions_preserves_aspect_ratio() {
    // Verify that the displayed aspect ratio matches the image aspect ratio
    // For a 640x480 (4:3) image displayed with char_aspect 2.0:
    // Displayed aspect = (char_width * 1) / (char_height * char_aspect)
    //                  = char_width / (char_height * 2.0)
    // This should equal img_width / img_height = 4/3

    let img_w = 640.0;
    let img_h = 480.0;
    let char_aspect = 2.0;
    let img_aspect = img_w / img_h;

    let (w, h) = calculate_dimensions(640, 480, 80, 60);

    // Displayed aspect ratio accounting for char proportions
    let displayed_aspect = (w as f32) / (h as f32 * char_aspect);

    // Should be within ~5% of the original due to rounding
    let ratio = displayed_aspect / img_aspect;
    assert!(
        (0.95..=1.05).contains(&ratio),
        "Aspect ratio not preserved: displayed={}, expected={}, ratio={}",
        displayed_aspect,
        img_aspect,
        ratio
    );
}

#[test]
fn test_calculate_dimensions_camera_to_modal() {
    // Real-world test: 640x480 camera in an 80x24 terminal
    // This should produce dimensions that fit in a typical modal
    let (w, h) = calculate_dimensions(640, 480, 40, 12);
    assert!(w <= 40);
    assert!(h <= 12);

    // The 4:3 image should use width efficiently
    // target_char_aspect = (4/3) * 2 = 8/3 = 2.67
    // If width=40, height = 40 / 2.67 = 15 (exceeds max 12)
    // So height-constrained: width = 12 * 2.67 = 32
    assert_eq!(w, 32);
    assert_eq!(h, 12);
}

// ==================== CharSet Tests ====================

#[test]
fn test_charset_default() {
    let charset = CharSet::default();
    assert_eq!(charset, CharSet::Standard);
}

#[test]
fn test_charset_chars() {
    assert_eq!(CharSet::Standard.chars(), STANDARD_CHARSET);
    assert_eq!(CharSet::Blocks.chars(), BLOCKS_CHARSET);
    assert_eq!(CharSet::Minimal.chars(), MINIMAL_CHARSET);
    assert!(CharSet::Braille.chars().is_empty()); // Braille uses different rendering
}

#[test]
fn test_charset_next_cycle() {
    assert_eq!(CharSet::Standard.next(), CharSet::Blocks);
    assert_eq!(CharSet::Blocks.next(), CharSet::Minimal);
    assert_eq!(CharSet::Minimal.next(), CharSet::Braille);
    assert_eq!(CharSet::Braille.next(), CharSet::Standard);
}

#[test]
fn test_charset_full_cycle() {
    let start = CharSet::Standard;
    let after_cycle = start.next().next().next().next();
    assert_eq!(start, after_cycle);
}

#[test]
fn test_charset_names() {
    assert_eq!(CharSet::Standard.name(), "standard");
    assert_eq!(CharSet::Blocks.name(), "blocks");
    assert_eq!(CharSet::Minimal.name(), "minimal");
    assert_eq!(CharSet::Braille.name(), "braille");
}

#[test]
fn test_charset_is_braille() {
    assert!(!CharSet::Standard.is_braille());
    assert!(!CharSet::Blocks.is_braille());
    assert!(!CharSet::Minimal.is_braille());
    assert!(CharSet::Braille.is_braille());
}
