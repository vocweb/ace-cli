use std::io;

use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Terminal,
};

use crate::hud::state::HudState;
use crate::hud::widget::HudFooter;
use crate::tui::input::TuiInput;

const MAX_CONTENT_LINES: usize = 10_000;

/// Mode the TUI is currently operating in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiMode {
    /// Normal input mode — user can type.
    Input,
    /// Receiving AI response; input is disabled, spinner is shown.
    Streaming,
    /// Waiting for a permission response from the user.
    Permission,
}

pub struct TuiApp {
    // Zone 1: Content
    pub content_lines: Vec<Line<'static>>,
    pub scroll_offset: usize,
    pub auto_scroll: bool,

    // Zone 2: Input
    pub input: TuiInput,

    // Zone 3: HUD
    pub hud: HudState,

    // App state
    pub should_quit: bool,
    /// Current interaction mode.
    pub mode: TuiMode,
}

impl TuiApp {
    /// Create a new TuiApp with empty content and default state.
    pub fn new(model_name: String, project_path: String) -> Self {
        Self {
            content_lines: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            input: TuiInput::new(),
            hud: HudState::new(model_name, project_path),
            should_quit: false,
            mode: TuiMode::Input,
        }
    }

    /// Append lines to the content area (Zone 1).
    /// Trims to MAX_CONTENT_LINES and auto-scrolls if enabled.
    pub fn push_content(&mut self, lines: Vec<Line<'static>>) {
        self.content_lines.extend(lines);

        // Trim old content if we exceed the maximum
        if self.content_lines.len() > MAX_CONTENT_LINES {
            let excess = self.content_lines.len() - MAX_CONTENT_LINES;
            self.content_lines.drain(..excess);
            // Adjust scroll offset so the view doesn't jump
            self.scroll_offset = self.scroll_offset.saturating_sub(excess);
        }

        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    /// Convenience: push a single styled line of text.
    pub fn push_text(&mut self, text: String, style: Style) {
        let line = Line::from(Span::styled(text, style));
        self.push_content(vec![line]);
    }

    /// Set scroll_offset so the last content line is visible.
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.content_lines.len().saturating_sub(1);
    }

    /// Render the 3-zone layout into the given frame.
    pub fn render(&self, frame: &mut Frame) {
        let input_height = self.input.display_lines() as u16 + 2; // +2 for border
        let hud_height: u16 = if frame.area().width < 60 { 1 } else { 2 };

        let chunks = Layout::vertical([
            Constraint::Min(3),               // Zone 1: Content
            Constraint::Length(input_height), // Zone 2: Input
            Constraint::Length(hud_height),   // Zone 3: HUD
        ])
        .split(frame.area());

        self.render_content(frame, chunks[0]);
        self.render_input(frame, chunks[1]);
        self.render_hud(frame, chunks[2]);
    }

    /// Render the scrollable content area (Zone 1).
    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height as usize;
        let total_lines = self.content_lines.len();

        // Calculate the visible window
        let start = if total_lines <= visible_height {
            0
        } else {
            self.scroll_offset
                .min(total_lines.saturating_sub(visible_height))
        };
        let end = (start + visible_height).min(total_lines);

        let visible_lines: Vec<Line<'static>> = self.content_lines[start..end].to_vec();

        let paragraph = Paragraph::new(visible_lines).wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);

        // Render scrollbar if content exceeds visible area
        if total_lines > visible_height {
            let mut scrollbar_state =
                ScrollbarState::new(total_lines.saturating_sub(visible_height)).position(start);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }

    /// Render the input prompt (Zone 2).
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        use ratatui::widgets::{Block, BorderType, Borders};

        let theme = crate::tui::theme::ClaudeTheme::default();

        // Border color based on mode
        let border_color = match self.mode {
            TuiMode::Input => {
                // Shimmer will be added later; use idle border color for now
                theme.input_border
            }
            TuiMode::Streaming => theme.text_muted,
            TuiMode::Permission => theme.permission,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        match self.mode {
            TuiMode::Streaming => {
                // Show a "processing" indicator inside the bordered box.
                let paragraph = Paragraph::new(Span::styled(
                    "  Processing…  (Ctrl+C to cancel)",
                    Style::default().fg(theme.text_muted),
                ));
                frame.render_widget(paragraph, inner);
            }
            _ => {
                // Prefix color based on input content
                let prefix_color = if self.input.buffer.starts_with('/') {
                    Color::Cyan
                } else if self.input.buffer.starts_with('!') {
                    theme.bash_border
                } else {
                    theme.text_primary
                };

                let display_text = self.input.display_text();

                let paragraph = Paragraph::new(Line::from(vec![
                    Span::styled(
                        "> ",
                        Style::default()
                            .fg(prefix_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(display_text, Style::default().fg(theme.text_primary)),
                ]));
                frame.render_widget(paragraph, inner);

                // Set cursor position (inside the border)
                if self.mode == TuiMode::Input {
                    let cursor_x = self.input.visible_cursor_x() + 2; // +2 for "> "
                    let cursor_y = self.input.visible_cursor_y();
                    frame.set_cursor_position(Position::new(
                        inner.x + cursor_x as u16,
                        inner.y + cursor_y as u16,
                    ));
                }
            }
        }
    }

    /// Render the HUD footer (Zone 3).
    fn render_hud(&self, frame: &mut Frame, area: Rect) {
        let footer = HudFooter::new(&self.hud);
        frame.render_widget(footer, area);
    }

    /// Scroll up by a number of lines, disabling auto-scroll.
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false;
    }

    /// Scroll down by a number of lines, re-enabling auto-scroll if at bottom.
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);

        // Clamp to maximum valid offset
        let max_offset = self.content_lines.len().saturating_sub(1);
        if self.scroll_offset >= max_offset {
            self.scroll_offset = max_offset;
            self.auto_scroll = true;
        }
    }
}

/// Initialize the terminal for TUI mode: raw mode, alternate screen, bracketed paste.
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(io::stdout());
    Terminal::new(backend)
}

/// Restore the terminal to its original state.
pub fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_app() {
        let app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        assert!(app.content_lines.is_empty());
        assert_eq!(app.scroll_offset, 0);
        assert!(app.auto_scroll);
        assert!(!app.should_quit);
        assert_eq!(app.mode, TuiMode::Input);
    }

    #[test]
    fn test_tui_mode_default_is_input() {
        let app = TuiApp::new("claude-3".to_string(), "/tmp".to_string());
        assert_eq!(app.mode, TuiMode::Input);
    }

    #[test]
    fn test_tui_mode_transitions() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp".to_string());
        app.mode = TuiMode::Streaming;
        assert_eq!(app.mode, TuiMode::Streaming);
        app.mode = TuiMode::Permission;
        assert_eq!(app.mode, TuiMode::Permission);
        app.mode = TuiMode::Input;
        assert_eq!(app.mode, TuiMode::Input);
    }

    #[test]
    fn test_push_text() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        app.push_text("Hello, world!".to_string(), Style::default());
        assert_eq!(app.content_lines.len(), 1);
    }

    #[test]
    fn test_push_content_auto_scroll() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        for i in 0..20 {
            app.push_text(format!("Line {i}"), Style::default());
        }
        assert_eq!(app.content_lines.len(), 20);
        // auto_scroll is on, so scroll_offset should be at the end
        assert_eq!(app.scroll_offset, 19);
    }

    #[test]
    fn test_push_content_trimming() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        // Push more than MAX_CONTENT_LINES
        let lines: Vec<Line<'static>> = (0..MAX_CONTENT_LINES + 100)
            .map(|i| Line::from(format!("Line {i}")))
            .collect();
        app.push_content(lines);
        assert_eq!(app.content_lines.len(), MAX_CONTENT_LINES);
    }

    #[test]
    fn test_scroll_up_disables_auto_scroll() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        for i in 0..50 {
            app.push_text(format!("Line {i}"), Style::default());
        }
        assert!(app.auto_scroll);
        app.scroll_up(5);
        assert!(!app.auto_scroll);
        assert_eq!(app.scroll_offset, 44); // 49 - 5
    }

    #[test]
    fn test_scroll_down_re_enables_auto_scroll() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        for i in 0..50 {
            app.push_text(format!("Line {i}"), Style::default());
        }
        app.scroll_up(10);
        assert!(!app.auto_scroll);

        // Scroll down past the bottom
        app.scroll_down(20);
        assert!(app.auto_scroll);
        assert_eq!(app.scroll_offset, 49);
    }

    #[test]
    fn test_scroll_up_clamp_at_zero() {
        let mut app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        app.push_text("Hello".to_string(), Style::default());
        app.scroll_up(100);
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_input_height_with_border() {
        let app = TuiApp::new("claude-3".to_string(), "/tmp/project".to_string());
        // 1 line input + 2 border lines = 3
        let input_height = app.input.display_lines() as u16 + 2;
        assert_eq!(input_height, 3);
    }

    #[test]
    fn test_visible_cursor_helpers() {
        let mut input = TuiInput::new();
        // Type "abc\ndef" and check cursor position helpers
        input.buffer = "abc\ndef".to_string();
        input.cursor = 7; // at end
        assert_eq!(input.visible_cursor_x(), 3); // "def" = 3 chars
        assert_eq!(input.visible_cursor_y(), 1); // 1 newline before cursor

        input.cursor = 3; // right before '\n'
        assert_eq!(input.visible_cursor_x(), 3); // "abc" = 3 chars
        assert_eq!(input.visible_cursor_y(), 0); // no newline before cursor
    }
}
