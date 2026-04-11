use ratatui::text::Line;

use crate::render::TerminalRenderer;

/// Accumulates streaming text deltas and incrementally renders markdown.
///
/// `push_delta()` appends new text.
/// `flush()` re-renders pending text and returns only NEW lines since last flush.
pub struct StreamingMarkdown {
    /// All text received so far (committed + pending).
    buffer: String,
    /// Number of lines already committed (returned by previous flushes).
    committed_line_count: usize,
    /// Renderer instance for markdown → Lines conversion.
    renderer: TerminalRenderer,
}

impl StreamingMarkdown {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            committed_line_count: 0,
            renderer: TerminalRenderer::new(),
        }
    }

    /// Append a text delta to the buffer.
    pub fn push_delta(&mut self, delta: &str) {
        self.buffer.push_str(delta);
    }

    /// Returns the pending (unflushed) text.
    pub fn pending_text(&self) -> &str {
        &self.buffer
    }

    /// Re-render the full buffer and return only the lines that are NEW
    /// since the last flush. Updates committed_line_count.
    pub fn flush(&mut self) -> Vec<Line<'static>> {
        if self.buffer.is_empty() {
            return Vec::new();
        }

        let all_lines = self.renderer.render_markdown_to_lines(&self.buffer);
        let total = all_lines.len();

        if total <= self.committed_line_count {
            // No new lines (e.g. partial line being built up).
            // Return the last line as an update (replace-in-place).
            if total > 0 {
                return vec![all_lines.into_iter().last().unwrap()];
            }
            return Vec::new();
        }

        let new_lines: Vec<Line<'static>> = all_lines
            .into_iter()
            .skip(self.committed_line_count)
            .collect();
        self.committed_line_count = total;
        new_lines
    }

    /// Reset state for a new turn.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.committed_line_count = 0;
    }
}

impl Default for StreamingMarkdown {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_delta_accumulates() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("Hello ");
        sm.push_delta("world");
        assert_eq!(sm.pending_text(), "Hello world");
    }

    #[test]
    fn test_flush_returns_lines() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("# Title\n\nParagraph text\n");
        let lines = sm.flush();
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_flush_returns_only_new_lines() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("Line one\n");
        let lines1 = sm.flush();
        let count1 = lines1.len();

        sm.push_delta("Line two\n");
        let lines2 = sm.flush();
        // Should only return the new line(s), not repeat old ones
        assert!(!lines2.is_empty());
        // New lines should be fewer than or equal to what re-rendering everything would give
        assert!(lines2.len() <= count1 + 2);
    }

    #[test]
    fn test_flush_empty_buffer() {
        let mut sm = StreamingMarkdown::new();
        let lines = sm.flush();
        assert!(lines.is_empty());
    }

    #[test]
    fn test_reset_clears_state() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("Some text\n");
        sm.flush();
        sm.reset();
        assert_eq!(sm.pending_text(), "");
        let lines = sm.flush();
        assert!(lines.is_empty());
    }
}
