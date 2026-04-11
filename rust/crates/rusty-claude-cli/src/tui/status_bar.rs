use ratatui::prelude::*;
use ratatui::widgets::Widget;

use crate::hud::state::HudState;
use crate::tui::theme::ClaudeTheme;

/// Single-line Claude Code style status bar.
pub struct StatusBar<'a> {
    state: &'a HudState,
    theme: &'a ClaudeTheme,
    permission_label: &'a str,
}

impl<'a> StatusBar<'a> {
    pub fn new(state: &'a HudState, theme: &'a ClaudeTheme, permission_label: &'a str) -> Self {
        Self {
            state,
            theme,
            permission_label,
        }
    }

    fn render_line(&self, _width: u16) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let sep = Span::styled(" · ", Style::default().fg(self.theme.text_muted));

        // Model name
        spans.push(Span::styled(
            format!(" {}", self.state.model_name),
            Style::default().fg(self.theme.brand),
        ));
        spans.push(sep.clone());

        // Permission mode
        spans.push(Span::styled(
            self.permission_label.to_string(),
            Style::default().fg(self.theme.permission),
        ));
        spans.push(sep.clone());

        // Token count
        let tokens_str = HudState::format_tokens(self.state.tokens_used);
        spans.push(Span::styled(
            format!("{tokens_str} tokens"),
            Style::default().fg(self.theme.text_secondary),
        ));
        spans.push(sep.clone());

        // Context % and progress bar
        let pct = self.state.context_percent();
        let bar_color = context_bar_color(pct, self.theme);
        spans.push(Span::styled(
            format!("{pct}% "),
            Style::default().fg(bar_color),
        ));

        let (filled_bar, empty_bar) = render_context_bar(pct);
        spans.push(Span::styled(filled_bar, Style::default().fg(bar_color)));
        spans.push(Span::styled(
            empty_bar,
            Style::default().fg(self.theme.text_muted),
        ));

        // Turn duration (only during active turn)
        if let Some(dur) = self.state.turn_duration() {
            spans.push(sep);
            spans.push(Span::styled(
                dur,
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        Line::from(spans)
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        // Fill background
        for x in area.x..area.x + area.width {
            buf[(x, area.y)].set_char(' ');
        }
        let line = self.render_line(area.width);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Returns (filled_string, empty_string) for a 10-char progress bar.
pub fn render_context_bar(percent: u8) -> (String, String) {
    let filled = ((percent as usize) * 10) / 100;
    let empty = 10 - filled;
    ("█".repeat(filled), "░".repeat(empty))
}

fn context_bar_color(percent: u8, theme: &ClaudeTheme) -> Color {
    match percent {
        0..=49 => theme.success,
        50..=74 => theme.warning,
        _ => theme.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::state::HudState;
    use crate::tui::theme::ClaudeTheme;

    #[test]
    fn test_status_bar_renders_model() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let theme = ClaudeTheme::default();
        let bar = StatusBar::new(&state, &theme, "default");
        let line = bar.render_line(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("claude-opus-4-6"));
    }

    #[test]
    fn test_status_bar_renders_permission_mode() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let theme = ClaudeTheme::default();
        let bar = StatusBar::new(&state, &theme, "plan");
        let line = bar.render_line(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("plan"));
    }

    #[test]
    fn test_status_bar_renders_tokens() {
        let mut state = HudState::new("claude-opus-4-6", "/tmp/project");
        state.tokens_used = 45_200;
        let theme = ClaudeTheme::default();
        let bar = StatusBar::new(&state, &theme, "default");
        let line = bar.render_line(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("45k tokens"));
    }

    #[test]
    fn test_context_bar_50_percent() {
        let (filled, empty) = render_context_bar(50);
        assert_eq!(filled.chars().count(), 5);
        assert_eq!(empty.chars().count(), 5);
    }

    #[test]
    fn test_context_bar_0_percent() {
        let (filled, empty) = render_context_bar(0);
        assert_eq!(filled.chars().count(), 0);
        assert_eq!(empty.chars().count(), 10);
    }

    #[test]
    fn test_context_bar_100_percent() {
        let (filled, empty) = render_context_bar(100);
        assert_eq!(filled.chars().count(), 10);
        assert_eq!(empty.chars().count(), 0);
    }

    #[test]
    fn test_status_bar_with_turn_duration() {
        let mut state = HudState::new("claude-opus-4-6", "/tmp/project");
        state.start_turn();
        let theme = ClaudeTheme::default();
        let bar = StatusBar::new(&state, &theme, "default");
        let line = bar.render_line(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        // Should contain something like "0.0s"
        assert!(text.contains('s'));
    }
}
