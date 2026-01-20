//! Buffer for storing PTY output.

/// Buffer for storing PTY output.
///
/// This buffer accumulates raw PTY output and provides a view for rendering.
/// For MVP, this stores the raw output string that gets rendered as a Paragraph.
/// Future versions may implement VT100 parsing for proper terminal emulation.
#[derive(Debug)]
pub struct PtyBuffer {
    /// Raw output content (accumulated from PTY)
    content: String,
    /// Scroll offset (lines from the end)
    scroll: u16,
    /// Maximum number of lines to keep (prevents unbounded growth)
    max_lines: usize,
}

impl Default for PtyBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PtyBuffer {
    /// Create a new empty PTY buffer.
    pub fn new() -> Self {
        Self {
            content: String::new(),
            scroll: 0,
            max_lines: 10_000, // Keep last 10k lines by default
        }
    }

    /// Create a new buffer with a custom max lines limit.
    pub fn with_max_lines(max_lines: usize) -> Self {
        Self {
            content: String::new(),
            scroll: 0,
            max_lines,
        }
    }

    /// Append raw bytes from PTY output.
    ///
    /// Converts bytes to string (lossy for non-UTF8) and appends to buffer.
    /// Trims buffer to max_lines if exceeded.
    pub fn append(&mut self, data: &[u8]) {
        // Convert bytes to string, replacing invalid UTF-8 sequences
        let text = String::from_utf8_lossy(data);
        self.content.push_str(&text);

        // Trim to max_lines if exceeded
        self.trim_to_max_lines();
    }

    /// Append a string directly.
    pub fn append_str(&mut self, text: &str) {
        self.content.push_str(text);
        self.trim_to_max_lines();
    }

    /// Clear the buffer contents.
    pub fn clear(&mut self) {
        self.content.clear();
        self.scroll = 0;
    }

    /// Get the raw content as a string slice.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the current scroll offset.
    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Set the scroll offset.
    pub fn set_scroll(&mut self, scroll: u16) {
        self.scroll = scroll;
    }

    /// Scroll up by the given number of lines.
    pub fn scroll_up(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_add(lines);
    }

    /// Scroll down by the given number of lines.
    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll = self.scroll.saturating_sub(lines);
    }

    /// Get the number of lines in the buffer.
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Trim the buffer to keep only the last max_lines lines.
    fn trim_to_max_lines(&mut self) {
        let line_count = self.content.lines().count();
        if line_count > self.max_lines {
            // Find the byte index where we should start keeping content
            let lines_to_remove = line_count - self.max_lines;
            let mut lines_seen = 0;
            let mut byte_index = 0;

            for (i, c) in self.content.char_indices() {
                if c == '\n' {
                    lines_seen += 1;
                    if lines_seen >= lines_to_remove {
                        byte_index = i + 1; // Start after the newline
                        break;
                    }
                }
            }

            if byte_index > 0 && byte_index < self.content.len() {
                self.content = self.content[byte_index..].to_string();
            }
        }
    }

    /// Get visible content for rendering (accounting for scroll offset).
    ///
    /// Returns lines from the end of the buffer, offset by scroll position.
    /// This is suitable for rendering in a fixed-height viewport.
    pub fn visible_content(&self, viewport_height: usize) -> String {
        if self.content.is_empty() || viewport_height == 0 {
            return String::new();
        }

        let lines: Vec<&str> = self.content.lines().collect();
        let total_lines = lines.len();

        // Calculate the range of lines to show
        let scroll = self.scroll as usize;
        let end = total_lines.saturating_sub(scroll);
        let start = end.saturating_sub(viewport_height);

        lines[start..end].join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_buffer_new() {
        let buf = PtyBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.content(), "");
        assert_eq!(buf.scroll(), 0);
        assert_eq!(buf.line_count(), 0);
    }

    #[test]
    fn test_pty_buffer_append_bytes() {
        let mut buf = PtyBuffer::new();
        buf.append(b"Hello, world!\n");
        assert_eq!(buf.content(), "Hello, world!\n");
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_pty_buffer_append_str() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Line 1\nLine 2\n");
        assert_eq!(buf.content(), "Line 1\nLine 2\n");
        assert_eq!(buf.line_count(), 2);
    }

    #[test]
    fn test_pty_buffer_multiple_appends() {
        let mut buf = PtyBuffer::new();
        buf.append(b"First ");
        buf.append(b"Second ");
        buf.append_str("Third");
        assert_eq!(buf.content(), "First Second Third");
    }

    #[test]
    fn test_pty_buffer_clear() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Some content\n");
        buf.set_scroll(5);
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.scroll(), 0);
    }

    #[test]
    fn test_pty_buffer_scroll() {
        let mut buf = PtyBuffer::new();
        assert_eq!(buf.scroll(), 0);

        buf.set_scroll(10);
        assert_eq!(buf.scroll(), 10);

        buf.scroll_up(5);
        assert_eq!(buf.scroll(), 15);

        buf.scroll_down(3);
        assert_eq!(buf.scroll(), 12);

        // Scroll down shouldn't go below 0
        buf.scroll_down(100);
        assert_eq!(buf.scroll(), 0);
    }

    #[test]
    fn test_pty_buffer_max_lines() {
        let mut buf = PtyBuffer::with_max_lines(3);

        // Add 5 lines
        buf.append_str("Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n");

        // Should only keep the last 3 lines
        assert_eq!(buf.line_count(), 3);
        assert!(buf.content().contains("Line 3"));
        assert!(buf.content().contains("Line 4"));
        assert!(buf.content().contains("Line 5"));
        assert!(!buf.content().contains("Line 1"));
        assert!(!buf.content().contains("Line 2"));
    }

    #[test]
    fn test_pty_buffer_visible_content() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Line 1\nLine 2\nLine 3\nLine 4\nLine 5");

        // View last 3 lines (no scroll)
        let visible = buf.visible_content(3);
        assert!(visible.contains("Line 3"));
        assert!(visible.contains("Line 4"));
        assert!(visible.contains("Line 5"));
        assert!(!visible.contains("Line 1"));

        // Scroll up by 1 line
        buf.set_scroll(1);
        let visible = buf.visible_content(3);
        assert!(visible.contains("Line 2"));
        assert!(visible.contains("Line 3"));
        assert!(visible.contains("Line 4"));
        assert!(!visible.contains("Line 5"));
    }

    #[test]
    fn test_pty_buffer_visible_content_empty() {
        let buf = PtyBuffer::new();
        assert_eq!(buf.visible_content(10), "");
    }

    #[test]
    fn test_pty_buffer_visible_content_zero_height() {
        let mut buf = PtyBuffer::new();
        buf.append_str("Some content\n");
        assert_eq!(buf.visible_content(0), "");
    }

    #[test]
    fn test_pty_buffer_lossy_utf8() {
        let mut buf = PtyBuffer::new();
        // Invalid UTF-8 sequence
        buf.append(&[0xff, 0xfe, b'H', b'i']);
        // Should contain the valid part plus replacement characters
        assert!(buf.content().contains("Hi"));
    }

    #[test]
    fn test_pty_buffer_default() {
        let buf = PtyBuffer::default();
        assert!(buf.is_empty());
        assert_eq!(buf.max_lines, 10_000);
    }
}
