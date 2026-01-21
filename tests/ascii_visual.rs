//! Visual snapshot tests for ASCII rendering quality.
//!
//! This test suite compares rendered ASCII output against golden reference files.
//! Use these tests to evaluate and improve rendering quality for:
//! - Face/feature recognition (eyes, nose, mouth)
//! - Edge sharpness (architecture, clear lines)
//! - Gradient smoothness (sky, shadows)
//! - Detail preservation (patterns, text, silhouettes)
//!
//! # Running Tests
//!
//! Run all visual tests:
//! ```bash
//! cargo test ascii_visual
//! ```
//!
//! Update golden references (regenerate from current output):
//! ```bash
//! UPDATE_GOLDEN=1 cargo test ascii_visual
//! ```
//!
//! # Adding New Test Images
//!
//! 1. Add image to `tests/fixtures/images/{name}.jpg`
//! 2. Add test case using `visual_test!` macro
//! 3. Run with `UPDATE_GOLDEN=1` to generate initial output
//! 4. Hand-edit golden file to create ideal reference

use image::GenericImageView;
use space_recorder::ascii::{
    BLOCKS_CHARSET, CellColor, CharSet, MINIMAL_CHARSET, STANDARD_CHARSET, STRUCTURE_CHARSET_ASCII,
    calculate_dimensions, downsample, downsample_colors_into, downsample_contrast,
    downsample_edge_preserve, map_structure_aware, map_to_chars, map_to_chars_dithered,
    map_to_chars_gamma, map_to_chars_ordered_dither, render_braille, to_grayscale,
};
use space_recorder::camera::{Frame, FrameFormat};
use std::fs;
use std::time::Instant;

/// Test configuration for a visual test case.
struct VisualTestConfig {
    /// Name of the test image (without extension)
    image_name: &'static str,
    /// Character set to use
    charset: CharSet,
    /// Target width in characters
    width: u16,
    /// Target height in characters
    height: u16,
    /// Whether to invert brightness
    invert: bool,
}

/// Directory containing test images.
const IMAGES_DIR: &str = "tests/fixtures/images";

/// Directory containing golden reference outputs.
const GOLDEN_DIR: &str = "tests/fixtures/ascii_golden";

/// Load an image from the fixtures directory and convert to Frame.
fn load_test_image(name: &str) -> Frame {
    let path = format!("{}/{}.jpg", IMAGES_DIR, name);
    let img = image::open(&path).unwrap_or_else(|e| panic!("Failed to load {}: {}", path, e));

    let (width, height) = img.dimensions();
    let rgb = img.to_rgb8();
    let data = rgb.into_raw();

    Frame {
        data,
        width,
        height,
        format: FrameFormat::Rgb,
        timestamp: Instant::now(),
    }
}

/// Render a frame to ASCII art (plain text, no colors).
fn render_ascii(frame: &Frame, width: u16, height: u16, charset: &[char], invert: bool) -> String {
    // Calculate dimensions that fit within requested size
    let (char_width, char_height) = calculate_dimensions(frame.width, frame.height, width, height);

    // Pipeline: grayscale -> downsample -> map to chars
    let grayscale = to_grayscale(frame);
    let brightness = downsample(
        &grayscale,
        frame.width,
        frame.height,
        char_width,
        char_height,
    );
    let chars = map_to_chars(&brightness, charset, invert);

    // Convert to string with newlines
    let mut result = String::with_capacity((char_width as usize + 1) * char_height as usize);
    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && i % (char_width as usize) == 0 {
            result.push('\n');
        }
        result.push(*ch);
    }
    result
}

/// Render a frame to ASCII art with ANSI true-color codes.
#[allow(dead_code)]
fn render_ascii_colored(
    frame: &Frame,
    width: u16,
    height: u16,
    charset: &[char],
    invert: bool,
) -> String {
    let (char_width, char_height) = calculate_dimensions(frame.width, frame.height, width, height);

    // Get colors for each cell
    let mut colors: Vec<CellColor> = Vec::new();
    downsample_colors_into(frame, char_width, char_height, &mut colors);

    // Get characters
    let grayscale = to_grayscale(frame);
    let brightness = downsample(
        &grayscale,
        frame.width,
        frame.height,
        char_width,
        char_height,
    );
    let chars = map_to_chars(&brightness, charset, invert);

    // Build ANSI-colored output
    let mut result = String::new();
    for (i, (ch, color)) in chars.iter().zip(colors.iter()).enumerate() {
        if i > 0 && i % (char_width as usize) == 0 {
            result.push_str("\x1b[0m\n"); // Reset at end of line
        }
        // ANSI 24-bit true color foreground
        result.push_str(&format!(
            "\x1b[38;2;{};{};{}m{}",
            color.r, color.g, color.b, ch
        ));
    }
    result.push_str("\x1b[0m"); // Final reset
    result
}

/// Render a frame to HTML for visual inspection in browser.
fn render_ascii_html(
    frame: &Frame,
    width: u16,
    height: u16,
    charset: &[char],
    invert: bool,
    title: &str,
) -> String {
    let (char_width, char_height) = calculate_dimensions(frame.width, frame.height, width, height);

    let mut colors: Vec<CellColor> = Vec::new();
    downsample_colors_into(frame, char_width, char_height, &mut colors);

    let grayscale = to_grayscale(frame);
    let brightness = downsample(
        &grayscale,
        frame.width,
        frame.height,
        char_width,
        char_height,
    );
    let chars = map_to_chars(&brightness, charset, invert);

    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <style>
        body {{ background: #1a1a1a; margin: 20px; }}
        pre {{
            font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
            font-size: 14px;
            line-height: 1.0;
            letter-spacing: 0;
        }}
        .label {{ color: #888; margin-bottom: 10px; }}
    </style>
</head>
<body>
<div class="label">{} - {}x{}</div>
<pre>"#,
        title, title, char_width, char_height
    );

    for (i, (ch, color)) in chars.iter().zip(colors.iter()).enumerate() {
        if i > 0 && i % (char_width as usize) == 0 {
            html.push_str("</span>\n");
        }
        let c = if *ch == '<' {
            "&lt;".to_string()
        } else if *ch == '>' {
            "&gt;".to_string()
        } else if *ch == '&' {
            "&amp;".to_string()
        } else {
            ch.to_string()
        };
        html.push_str(&format!(
            "<span style=\"color:rgb({},{},{})\">{}</span>",
            color.r, color.g, color.b, c
        ));
    }

    html.push_str("</pre>\n</body>\n</html>");
    html
}

/// Get the golden file path for a test configuration.
fn golden_path(config: &VisualTestConfig) -> String {
    format!(
        "{}/{}_{}_{:03}x{:03}.txt",
        GOLDEN_DIR,
        config.image_name,
        config.charset.name(),
        config.width,
        config.height
    )
}

/// Compare rendered output against golden reference.
/// Returns (matches, diff_report) where diff_report is empty if matches.
fn compare_with_golden(rendered: &str, config: &VisualTestConfig) -> (bool, String) {
    let path = golden_path(config);

    // Check if we should update golden files
    let update_golden = std::env::var("UPDATE_GOLDEN").is_ok();

    if update_golden {
        // Create golden directory if needed
        fs::create_dir_all(GOLDEN_DIR).expect("Failed to create golden directory");
        fs::write(&path, rendered).expect("Failed to write golden file");
        return (true, format!("Updated golden: {}", path));
    }

    // Load golden reference
    let golden = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            return (
                false,
                format!(
                    "Golden file not found: {}\nRun with UPDATE_GOLDEN=1 to create it.",
                    path
                ),
            );
        }
    };

    // Compare
    if rendered == golden {
        return (true, String::new());
    }

    // Generate diff report
    let diff = generate_diff(&golden, rendered);
    (false, diff)
}

/// Generate a character-by-character diff between expected and actual.
fn generate_diff(expected: &str, actual: &str) -> String {
    let mut report = String::new();
    report.push_str("=== DIFF (expected vs actual) ===\n\n");

    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    let max_lines = expected_lines.len().max(actual_lines.len());

    for i in 0..max_lines {
        let exp = expected_lines.get(i).unwrap_or(&"");
        let act = actual_lines.get(i).unwrap_or(&"");

        if exp != act {
            report.push_str(&format!("Line {:3}:\n", i + 1));
            report.push_str(&format!("  exp: {}\n", exp));
            report.push_str(&format!("  act: {}\n", act));

            // Show character-level diff
            let mut diff_line = String::from("       ");
            let max_len = exp.len().max(act.len());
            let exp_chars: Vec<char> = exp.chars().collect();
            let act_chars: Vec<char> = act.chars().collect();

            for j in 0..max_len {
                let e = exp_chars.get(j);
                let a = act_chars.get(j);
                if e != a {
                    diff_line.push('^');
                } else {
                    diff_line.push(' ');
                }
            }
            report.push_str(&diff_line);
            report.push('\n');
        }
    }

    // Add side-by-side view
    report.push_str("\n=== SIDE BY SIDE ===\n");
    report.push_str("EXPECTED:\n");
    report.push_str(expected);
    report.push_str("\n\nACTUAL:\n");
    report.push_str(actual);
    report.push('\n');

    report
}

/// Run a visual test with the given configuration.
fn run_visual_test(config: VisualTestConfig) {
    let frame = load_test_image(config.image_name);

    let charset = match config.charset {
        CharSet::Standard => STANDARD_CHARSET,
        CharSet::Blocks => BLOCKS_CHARSET,
        CharSet::Minimal => MINIMAL_CHARSET,
        CharSet::Braille => {
            // Skip braille for now - it uses different rendering
            return;
        }
    };

    let rendered = render_ascii(&frame, config.width, config.height, charset, config.invert);
    let (matches, diff) = compare_with_golden(&rendered, &config);

    if !matches {
        panic!(
            "Visual test failed for {} ({} {}x{}):\n{}",
            config.image_name,
            config.charset.name(),
            config.width,
            config.height,
            diff
        );
    }
}

/// Macro to define a visual test case.
macro_rules! visual_test {
    ($name:ident, $image:literal, $charset:expr, $width:literal, $height:literal) => {
        #[test]
        fn $name() {
            run_visual_test(VisualTestConfig {
                image_name: $image,
                charset: $charset,
                width: $width,
                height: $height,
                invert: false,
            });
        }
    };
    ($name:ident, $image:literal, $charset:expr, $width:literal, $height:literal, invert) => {
        #[test]
        fn $name() {
            run_visual_test(VisualTestConfig {
                image_name: $image,
                charset: $charset,
                width: $width,
                height: $height,
                invert: true,
            });
        }
    };
}

// =============================================================================
// Face Photo Tests - Test facial feature recognition
// =============================================================================

visual_test!(test_face_standard_40x20, "face", CharSet::Standard, 40, 20);
visual_test!(test_face_standard_80x24, "face", CharSet::Standard, 80, 24);
visual_test!(test_face_blocks_40x20, "face", CharSet::Blocks, 40, 20);
visual_test!(test_face_minimal_40x20, "face", CharSet::Minimal, 40, 20);

// =============================================================================
// Geometric Scene Tests - Test edge sharpness
// =============================================================================

visual_test!(
    test_geometry_standard_40x20,
    "geometry",
    CharSet::Standard,
    40,
    20
);
visual_test!(
    test_geometry_standard_80x24,
    "geometry",
    CharSet::Standard,
    80,
    24
);
visual_test!(
    test_geometry_blocks_40x20,
    "geometry",
    CharSet::Blocks,
    40,
    20
);

// =============================================================================
// Gradient Scene Tests - Test smooth transitions
// =============================================================================

visual_test!(
    test_gradient_standard_40x20,
    "gradient",
    CharSet::Standard,
    40,
    20
);
visual_test!(
    test_gradient_standard_80x24,
    "gradient",
    CharSet::Standard,
    80,
    24
);
visual_test!(
    test_gradient_blocks_40x20,
    "gradient",
    CharSet::Blocks,
    40,
    20
);
visual_test!(
    test_gradient_minimal_40x20,
    "gradient",
    CharSet::Minimal,
    40,
    20
);

// =============================================================================
// High Contrast Tests - Test detail preservation
// =============================================================================

visual_test!(
    test_contrast_standard_40x20,
    "contrast",
    CharSet::Standard,
    40,
    20
);
visual_test!(
    test_contrast_standard_80x24,
    "contrast",
    CharSet::Standard,
    80,
    24
);
visual_test!(
    test_contrast_blocks_40x20,
    "contrast",
    CharSet::Blocks,
    40,
    20
);

// =============================================================================
// Utility: Generate all golden files (run with UPDATE_GOLDEN=1)
// =============================================================================

#[test]
fn generate_baseline_report() {
    // Only run when explicitly requested
    if std::env::var("GENERATE_REPORT").is_err() {
        return;
    }

    let images = ["face", "geometry", "gradient", "contrast"];
    let charsets = [
        (CharSet::Standard, STANDARD_CHARSET),
        (CharSet::Blocks, BLOCKS_CHARSET),
        (CharSet::Minimal, MINIMAL_CHARSET),
    ];
    let sizes = [(40, 20), (80, 24)];

    let mut report = String::new();
    report.push_str("# ASCII Rendering Baseline Report\n\n");
    report.push_str("Generated by `GENERATE_REPORT=1 cargo test generate_baseline_report`\n\n");

    for image in &images {
        report.push_str(&format!("## {}\n\n", image));
        let frame = load_test_image(image);

        for (width, height) in &sizes {
            report.push_str(&format!("### {}x{}\n\n", width, height));

            for (charset_enum, charset) in &charsets {
                report.push_str(&format!("**{}:**\n```\n", charset_enum.name()));
                let rendered = render_ascii(&frame, *width, *height, charset, false);
                report.push_str(&rendered);
                report.push_str("\n```\n\n");
            }
        }
    }

    let report_path = "tests/fixtures/baseline_report.md";
    fs::write(report_path, &report).expect("Failed to write baseline report");
    println!("Baseline report written to {}", report_path);
}

/// Generate HTML files for visual comparison with colors.
/// Run with: GENERATE_HTML=1 cargo test generate_html_preview -- --nocapture
#[test]
fn generate_html_preview() {
    if std::env::var("GENERATE_HTML").is_err() {
        return;
    }

    let images = ["face", "geometry", "gradient", "contrast"];
    let charsets = [
        (CharSet::Standard, STANDARD_CHARSET, "standard"),
        (CharSet::Blocks, BLOCKS_CHARSET, "blocks"),
    ];

    fs::create_dir_all("tests/fixtures/html_preview").expect("Failed to create html_preview dir");

    // Generate individual HTML files
    for image in &images {
        let frame = load_test_image(image);

        for (_charset_enum, charset, name) in &charsets {
            let html = render_ascii_html(
                &frame,
                80,
                30,
                charset,
                false,
                &format!("{} - {}", image, name),
            );
            let path = format!("tests/fixtures/html_preview/{}_{}.html", image, name);
            fs::write(&path, &html).expect("Failed to write HTML");
            println!("Generated: {}", path);
        }
    }

    // Generate combined comparison page
    let mut combined = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>ASCII Rendering Visual Comparison</title>
    <style>
        body { background: #1a1a1a; color: #ccc; font-family: sans-serif; margin: 20px; }
        h1 { color: #fff; }
        h2 { color: #aaa; border-bottom: 1px solid #333; padding-bottom: 10px; }
        .grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 20px; margin-bottom: 40px; }
        .card { background: #222; padding: 15px; border-radius: 8px; }
        .card h3 { margin: 0 0 10px 0; color: #888; font-size: 14px; }
        pre {
            font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
            font-size: 10px;
            line-height: 1.0;
            letter-spacing: 0;
            margin: 0;
            overflow-x: auto;
        }
        .original { max-width: 200px; border-radius: 4px; }
    </style>
</head>
<body>
<h1>ASCII Rendering Visual Comparison</h1>
<p>Generated by <code>GENERATE_HTML=1 cargo test generate_html_preview</code></p>
"#,
    );

    for image in &images {
        combined.push_str(&format!("<h2>{}</h2>\n", image));
        combined.push_str(&format!(
            "<p><img src=\"../images/{}.jpg\" class=\"original\" alt=\"{}\"></p>\n",
            image, image
        ));
        combined.push_str("<div class=\"grid\">\n");

        let frame = load_test_image(image);
        for (_charset_enum, charset, name) in &charsets {
            let (char_width, char_height) = calculate_dimensions(frame.width, frame.height, 60, 25);
            let mut colors: Vec<CellColor> = Vec::new();
            downsample_colors_into(&frame, char_width, char_height, &mut colors);
            let grayscale = to_grayscale(&frame);
            let brightness = downsample(
                &grayscale,
                frame.width,
                frame.height,
                char_width,
                char_height,
            );
            let chars = map_to_chars(&brightness, charset, false);

            combined.push_str(&format!(
                "<div class=\"card\"><h3>{} ({}x{})</h3><pre>",
                name, char_width, char_height
            ));
            for (i, (ch, color)) in chars.iter().zip(colors.iter()).enumerate() {
                if i > 0 && i % (char_width as usize) == 0 {
                    combined.push_str("</span>\n");
                }
                let c = match *ch {
                    '<' => "&lt;",
                    '>' => "&gt;",
                    '&' => "&amp;",
                    _ => "",
                };
                if c.is_empty() {
                    combined.push_str(&format!(
                        "<span style=\"color:rgb({},{},{})\">{}</span>",
                        color.r, color.g, color.b, ch
                    ));
                } else {
                    combined.push_str(&format!(
                        "<span style=\"color:rgb({},{},{})\">{}</span>",
                        color.r, color.g, color.b, c
                    ));
                }
            }
            combined.push_str("</pre></div>\n");
        }
        combined.push_str("</div>\n");
    }

    combined.push_str("</body>\n</html>");
    let path = "tests/fixtures/html_preview/comparison.html";
    fs::write(path, &combined).expect("Failed to write combined HTML");
    println!("Generated: {}", path);
}

/// Compare rendering methods: standard charset and braille, with gamma and contrast.
/// Run with: COMPARE_GAMMA=1 cargo test compare_gamma_correction -- --nocapture
#[test]
fn compare_gamma_correction() {
    if std::env::var("COMPARE_GAMMA").is_err() {
        return;
    }

    fs::create_dir_all("tests/fixtures/html_preview").expect("Failed to create dir");

    let images = ["webcam_01", "webcam_02", "webcam_03", "webcam_04"];
    let charset = STANDARD_CHARSET;

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Webcam ASCII Rendering Comparison</title>
    <style>
        body { background: #1a1a1a; color: #ccc; font-family: sans-serif; margin: 20px; }
        h1 { color: #fff; }
        h2 { color: #aaa; border-bottom: 1px solid #333; padding-bottom: 10px; }
        h3.section { color: #666; margin-top: 20px; }
        .comparison { display: grid; grid-template-columns: repeat(4, 1fr); gap: 15px; margin-bottom: 30px; }
        .card { background: #222; padding: 12px; border-radius: 8px; }
        .card h3 { margin: 0 0 8px 0; color: #888; font-size: 12px; }
        pre {
            font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
            font-size: 9px;
            line-height: 1.0;
            letter-spacing: 0;
            margin: 0;
        }
        pre.braille { font-size: 12px; line-height: 0.6; }
        img.original { max-width: 120px; border-radius: 4px; }
        .note { color: #666; font-size: 10px; margin-top: 5px; }
    </style>
</head>
<body>
<h1>Webcam ASCII Rendering Comparison</h1>
<p>Comparing: Linear vs Gamma (γ=2.2) vs Gamma+Contrast (1.4x) for Standard and Braille charsets.</p>
"#,
    );

    for image in &images {
        let frame = load_test_image(image);
        let (w, h) = calculate_dimensions(frame.width, frame.height, 50, 25);

        let grayscale = to_grayscale(&frame);
        let brightness = downsample(&grayscale, frame.width, frame.height, w, h);
        let brightness_contrast =
            downsample_contrast(&grayscale, frame.width, frame.height, w, h, 1.4);

        let mut colors: Vec<CellColor> = Vec::new();
        downsample_colors_into(&frame, w, h, &mut colors);

        html.push_str(&format!("<h2>{}</h2>\n", image));

        // Standard charset section
        html.push_str("<h3 class=\"section\">Standard Charset</h3>\n<div class=\"comparison\">\n");

        // Original
        html.push_str(&format!(
            "<div class=\"card\"><h3>Original</h3><img src=\"../images/{}.jpg\" class=\"original\"></div>\n",
            image
        ));

        // Linear
        let linear_chars = map_to_chars(&brightness, charset, false);
        html.push_str("<div class=\"card\"><h3>Linear</h3><pre>");
        for (i, (ch, color)) in linear_chars.iter().zip(colors.iter()).enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            let c = match *ch {
                '<' => "&lt;",
                '>' => "&gt;",
                '&' => "&amp;",
                _ => "",
            };
            if c.is_empty() {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, ch
                ));
            } else {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, c
                ));
            }
        }
        html.push_str("</pre></div>\n");

        // Gamma
        let gamma_chars = map_to_chars_gamma(&brightness, charset, false);
        html.push_str("<div class=\"card\"><h3>Gamma (γ=2.2)</h3><pre>");
        for (i, (ch, color)) in gamma_chars.iter().zip(colors.iter()).enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            let c = match *ch {
                '<' => "&lt;",
                '>' => "&gt;",
                '&' => "&amp;",
                _ => "",
            };
            if c.is_empty() {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, ch
                ));
            } else {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, c
                ));
            }
        }
        html.push_str("</pre></div>\n");

        // Gamma + Contrast
        let gamma_contrast_chars = map_to_chars_gamma(&brightness_contrast, charset, false);
        html.push_str("<div class=\"card\"><h3>Gamma + Contrast 1.4x</h3><pre>");
        for (i, (ch, color)) in gamma_contrast_chars.iter().zip(colors.iter()).enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            let c = match *ch {
                '<' => "&lt;",
                '>' => "&gt;",
                '&' => "&amp;",
                _ => "",
            };
            if c.is_empty() {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, ch
                ));
            } else {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, c
                ));
            }
        }
        html.push_str("</pre></div>\n");
        html.push_str("</div>\n");

        // Braille section
        html.push_str("<h3 class=\"section\">Braille</h3>\n<div class=\"comparison\">\n");

        // Original (placeholder)
        html.push_str("<div class=\"card\"><h3>Original</h3><img src=\"../images/{}.jpg\" class=\"original\" style=\"visibility:hidden\"></div>\n");

        // Braille Linear
        let braille_linear =
            render_braille(&grayscale, frame.width, frame.height, w, h, 128, false);
        html.push_str("<div class=\"card\"><h3>Linear (thresh=128)</h3><pre class=\"braille\">");
        for (i, ch) in braille_linear.iter().enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            html.push_str(&format!(
                "<span style=\"color:rgb({},{},{})\">{}</span>",
                colors[i].r, colors[i].g, colors[i].b, ch
            ));
        }
        html.push_str("</pre></div>\n");

        // Braille with lower threshold (more dots = more detail)
        let braille_low = render_braille(&grayscale, frame.width, frame.height, w, h, 80, false);
        html.push_str("<div class=\"card\"><h3>Low Threshold (80)</h3><pre class=\"braille\">");
        for (i, ch) in braille_low.iter().enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            html.push_str(&format!(
                "<span style=\"color:rgb({},{},{})\">{}</span>",
                colors[i].r, colors[i].g, colors[i].b, ch
            ));
        }
        html.push_str("</pre></div>\n");

        // Braille with contrast boost
        let braille_contrast = render_braille(
            &brightness_contrast,
            frame.width,
            frame.height,
            w,
            h,
            100,
            false,
        );
        html.push_str(
            "<div class=\"card\"><h3>Contrast 1.4x (thresh=100)</h3><pre class=\"braille\">",
        );
        for (i, ch) in braille_contrast.iter().enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            html.push_str(&format!(
                "<span style=\"color:rgb({},{},{})\">{}</span>",
                colors[i].r, colors[i].g, colors[i].b, ch
            ));
        }
        html.push_str("</pre></div>\n");

        html.push_str("</div>\n");
    }

    html.push_str("</body>\n</html>");
    let path = "tests/fixtures/html_preview/gamma_comparison.html";
    fs::write(path, &html).expect("Failed to write HTML");
    println!("Generated: {}", path);
}

/// Compare different dithering methods.
/// Run with: COMPARE_DITHER=1 cargo test compare_dithering -- --nocapture
#[test]
fn compare_dithering() {
    if std::env::var("COMPARE_DITHER").is_err() {
        return;
    }

    fs::create_dir_all("tests/fixtures/html_preview").expect("Failed to create dir");

    let images = ["gradient", "face"];
    let charset = STANDARD_CHARSET;

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Dithering Comparison</title>
    <style>
        body { background: #1a1a1a; color: #ccc; font-family: sans-serif; margin: 20px; }
        h1 { color: #fff; }
        h2 { color: #aaa; border-bottom: 1px solid #333; padding-bottom: 10px; }
        .comparison { display: grid; grid-template-columns: repeat(2, 1fr); gap: 20px; margin-bottom: 40px; }
        .card { background: #222; padding: 15px; border-radius: 8px; }
        .card h3 { margin: 0 0 10px 0; color: #888; font-size: 14px; }
        pre {
            font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
            font-size: 10px;
            line-height: 1.0;
            letter-spacing: 0;
            margin: 0;
        }
        img.original { max-width: 150px; border-radius: 4px; }
        .note { color: #666; font-size: 11px; margin-top: 8px; }
    </style>
</head>
<body>
<h1>Dithering Method Comparison</h1>
<p>Dithering creates the illusion of more gray levels by distributing quantization error.</p>
"#,
    );

    for image in &images {
        let frame = load_test_image(image);
        let (w, h) = calculate_dimensions(frame.width, frame.height, 60, 25);

        let grayscale = to_grayscale(&frame);
        let brightness = downsample(&grayscale, frame.width, frame.height, w, h);

        let mut colors: Vec<CellColor> = Vec::new();
        downsample_colors_into(&frame, w, h, &mut colors);

        // Different rendering methods
        let methods: Vec<(&str, Vec<char>, &str)> = vec![
            (
                "No Dithering (gamma)",
                map_to_chars_gamma(&brightness, charset, false),
                "Sharp edges, visible banding",
            ),
            (
                "Floyd-Steinberg",
                map_to_chars_dithered(&brightness, w, h, charset, false, true),
                "Smooth gradients, organic noise",
            ),
            (
                "Ordered (Bayer 4x4)",
                map_to_chars_ordered_dither(&brightness, w, charset, false, true),
                "Regular pattern, fast",
            ),
        ];

        html.push_str(&format!("<h2>{}</h2>\n", image));
        html.push_str(&format!(
            "<p><img src=\"../images/{}.jpg\" class=\"original\" style=\"max-width:200px\"></p>\n",
            image
        ));
        html.push_str("<div class=\"comparison\">\n");

        for (name, chars, desc) in &methods {
            html.push_str(&format!("<div class=\"card\"><h3>{}</h3><pre>", name));
            for (i, (ch, color)) in chars.iter().zip(colors.iter()).enumerate() {
                if i > 0 && i % (w as usize) == 0 {
                    html.push('\n');
                }
                let c = match *ch {
                    '<' => "&lt;",
                    '>' => "&gt;",
                    '&' => "&amp;",
                    _ => "",
                };
                if c.is_empty() {
                    html.push_str(&format!(
                        "<span style=\"color:rgb({},{},{})\">{}</span>",
                        color.r, color.g, color.b, ch
                    ));
                } else {
                    html.push_str(&format!(
                        "<span style=\"color:rgb({},{},{})\">{}</span>",
                        color.r, color.g, color.b, c
                    ));
                }
            }
            html.push_str(&format!("</pre><p class=\"note\">{}</p></div>\n", desc));
        }

        html.push_str("</div>\n");
    }

    html.push_str("</body>\n</html>");
    let path = "tests/fixtures/html_preview/dither_comparison.html";
    fs::write(path, &html).expect("Failed to write HTML");
    println!("Generated: {}", path);
}

/// Compare structure-aware vs standard rendering.
/// Run with: COMPARE_STRUCTURE=1 cargo test compare_structure_aware -- --nocapture
#[test]
fn compare_structure_aware() {
    if std::env::var("COMPARE_STRUCTURE").is_err() {
        return;
    }

    fs::create_dir_all("tests/fixtures/html_preview").expect("Failed to create dir");

    let images = ["geometry", "face"];
    let charset = STANDARD_CHARSET;

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Structure-Aware Character Selection</title>
    <style>
        body { background: #1a1a1a; color: #ccc; font-family: sans-serif; margin: 20px; }
        h1 { color: #fff; }
        h2 { color: #aaa; border-bottom: 1px solid #333; padding-bottom: 10px; }
        .comparison { display: grid; grid-template-columns: repeat(2, 1fr); gap: 20px; margin-bottom: 40px; }
        .card { background: #222; padding: 15px; border-radius: 8px; }
        .card h3 { margin: 0 0 10px 0; color: #888; font-size: 14px; }
        pre {
            font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
            font-size: 10px;
            line-height: 1.0;
            letter-spacing: 0;
            margin: 0;
        }
        img.original { max-width: 150px; border-radius: 4px; }
        .note { color: #666; font-size: 11px; margin-top: 8px; }
    </style>
</head>
<body>
<h1>Structure-Aware Character Selection</h1>
<p>Structure-aware rendering picks characters that match edge direction, not just brightness.</p>
"#,
    );

    for image in &images {
        let frame = load_test_image(image);
        let (w, h) = calculate_dimensions(frame.width, frame.height, 60, 25);

        let grayscale = to_grayscale(&frame);
        let brightness = downsample(&grayscale, frame.width, frame.height, w, h);

        let mut colors: Vec<CellColor> = Vec::new();
        downsample_colors_into(&frame, w, h, &mut colors);

        // Standard gamma-corrected
        let standard_chars = map_to_chars_gamma(&brightness, charset, false);
        // Structure-aware
        let structure_chars = map_structure_aware(
            &grayscale,
            frame.width,
            frame.height,
            w,
            h,
            &STRUCTURE_CHARSET_ASCII,
            true,
        );

        html.push_str(&format!("<h2>{}</h2>\n", image));
        html.push_str(&format!(
            "<p><img src=\"../images/{}.jpg\" class=\"original\" style=\"max-width:200px\"></p>\n",
            image
        ));
        html.push_str("<div class=\"comparison\">\n");

        // Standard
        html.push_str("<div class=\"card\"><h3>Standard (gamma)</h3><pre>");
        for (i, (ch, color)) in standard_chars.iter().zip(colors.iter()).enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            let c = match *ch {
                '<' => "&lt;",
                '>' => "&gt;",
                '&' => "&amp;",
                _ => "",
            };
            if c.is_empty() {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, ch
                ));
            } else {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, c
                ));
            }
        }
        html.push_str("</pre><p class=\"note\">Brightness-only character selection</p></div>\n");

        // Structure-aware
        html.push_str("<div class=\"card\"><h3>Structure-Aware</h3><pre>");
        for (i, (ch, color)) in structure_chars.iter().zip(colors.iter()).enumerate() {
            if i > 0 && i % (w as usize) == 0 {
                html.push('\n');
            }
            let c = match *ch {
                '<' => "&lt;",
                '>' => "&gt;",
                '&' => "&amp;",
                '\\' => "\\",
                _ => "",
            };
            if c.is_empty() {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, ch
                ));
            } else {
                html.push_str(&format!(
                    "<span style=\"color:rgb({},{},{})\">{}</span>",
                    color.r, color.g, color.b, c
                ));
            }
        }
        html.push_str("</pre><p class=\"note\">Uses |-/\\ for edges</p></div>\n");

        html.push_str("</div>\n");
    }

    html.push_str("</body>\n</html>");
    let path = "tests/fixtures/html_preview/structure_comparison.html";
    fs::write(path, &html).expect("Failed to write HTML");
    println!("Generated: {}", path);
}

/// Compare downsampling methods.
/// Run with: COMPARE_DOWNSAMPLE=1 cargo test compare_downsampling -- --nocapture
#[test]
fn compare_downsampling() {
    if std::env::var("COMPARE_DOWNSAMPLE").is_err() {
        return;
    }

    fs::create_dir_all("tests/fixtures/html_preview").expect("Failed to create dir");

    let images = ["face", "geometry"];
    let charset = STANDARD_CHARSET;

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Downsampling Method Comparison</title>
    <style>
        body { background: #1a1a1a; color: #ccc; font-family: sans-serif; margin: 20px; }
        h1 { color: #fff; }
        h2 { color: #aaa; border-bottom: 1px solid #333; padding-bottom: 10px; }
        .comparison { display: grid; grid-template-columns: repeat(2, 1fr); gap: 20px; margin-bottom: 40px; }
        .card { background: #222; padding: 15px; border-radius: 8px; }
        .card h3 { margin: 0 0 10px 0; color: #888; font-size: 14px; }
        pre {
            font-family: 'Monaco', 'Menlo', 'Courier New', monospace;
            font-size: 10px;
            line-height: 1.0;
            letter-spacing: 0;
            margin: 0;
        }
        img.original { max-width: 150px; border-radius: 4px; }
        .note { color: #666; font-size: 11px; margin-top: 8px; }
    </style>
</head>
<body>
<h1>Downsampling Method Comparison</h1>
<p>Different downsampling methods affect detail preservation and contrast.</p>
"#,
    );

    for image in &images {
        let frame = load_test_image(image);
        let (w, h) = calculate_dimensions(frame.width, frame.height, 60, 25);

        let grayscale = to_grayscale(&frame);

        let mut colors: Vec<CellColor> = Vec::new();
        downsample_colors_into(&frame, w, h, &mut colors);

        // Different downsampling methods
        let standard = downsample(&grayscale, frame.width, frame.height, w, h);
        let contrast = downsample_contrast(&grayscale, frame.width, frame.height, w, h, 1.3);
        let edge = downsample_edge_preserve(&grayscale, frame.width, frame.height, w, h, 0.3);

        let methods: Vec<(&str, &[u8], &str)> = vec![
            ("Standard (average)", &standard, "Simple averaging"),
            (
                "Contrast Boosted (1.3x)",
                &contrast,
                "Enhances local contrast",
            ),
            ("Edge Preserve (0.3)", &edge, "Favors edge pixels"),
        ];

        html.push_str(&format!("<h2>{}</h2>\n", image));
        html.push_str(&format!(
            "<p><img src=\"../images/{}.jpg\" class=\"original\" style=\"max-width:200px\"></p>\n",
            image
        ));
        html.push_str("<div class=\"comparison\">\n");

        for (name, brightness, desc) in &methods {
            let chars = map_to_chars_gamma(brightness, charset, false);
            html.push_str(&format!("<div class=\"card\"><h3>{}</h3><pre>", name));
            for (i, (ch, color)) in chars.iter().zip(colors.iter()).enumerate() {
                if i > 0 && i % (w as usize) == 0 {
                    html.push('\n');
                }
                let c = match *ch {
                    '<' => "&lt;",
                    '>' => "&gt;",
                    '&' => "&amp;",
                    _ => "",
                };
                if c.is_empty() {
                    html.push_str(&format!(
                        "<span style=\"color:rgb({},{},{})\">{}</span>",
                        color.r, color.g, color.b, ch
                    ));
                } else {
                    html.push_str(&format!(
                        "<span style=\"color:rgb({},{},{})\">{}</span>",
                        color.r, color.g, color.b, c
                    ));
                }
            }
            html.push_str(&format!("</pre><p class=\"note\">{}</p></div>\n", desc));
        }

        html.push_str("</div>\n");
    }

    html.push_str("</body>\n</html>");
    let path = "tests/fixtures/html_preview/downsample_comparison.html";
    fs::write(path, &html).expect("Failed to write HTML");
    println!("Generated: {}", path);
}

/// Analyze current rendering issues and print diagnostic info.
/// Run with: ANALYZE=1 cargo test analyze_rendering -- --nocapture
#[test]
fn analyze_rendering() {
    if std::env::var("ANALYZE").is_err() {
        return;
    }

    println!("\n=== ASCII Rendering Analysis ===\n");

    println!("## Current Pipeline Issues\n");
    println!("1. **Linear brightness mapping** - No gamma correction");
    println!("   - Human vision is non-linear; mid-gray (128) doesn't look 50% bright");
    println!("   - Fix: Apply gamma correction (~2.2) before character mapping\n");

    println!("2. **Simple averaging for downsampling** - Loses detail");
    println!("   - Each cell averages all pixels, washing out fine details");
    println!("   - Fix: Consider max-pooling edges, or contrast-preserving downsample\n");

    println!("3. **No dithering** - Visible banding in gradients");
    println!("   - With only 10 character levels, gradients show distinct bands");
    println!("   - Fix: Floyd-Steinberg or ordered dithering\n");

    println!("4. **Brightness-only character selection** - Ignores structure");
    println!("   - Character choice based only on average brightness");
    println!("   - '-' and '|' have similar brightness but different visual structure");
    println!("   - Fix: Analyze local gradients/edges to pick structurally appropriate chars\n");

    // Show character brightness distribution
    println!("## Character Set Analysis\n");
    println!("Standard charset density distribution:");
    for (i, ch) in STANDARD_CHARSET.iter().enumerate() {
        let brightness = (i * 255) / (STANDARD_CHARSET.len() - 1);
        println!("  {:3}: '{}' -> brightness {}", i, ch, brightness);
    }

    println!("\n## Recommended Improvements (in order):\n");
    println!("1. Gamma correction - biggest perceptual improvement, easiest to implement");
    println!("2. Dithering - smooths gradients significantly");
    println!("3. Structure-aware chars - improves edges and facial features");
    println!("4. Better downsampling - preserves fine detail");
}
