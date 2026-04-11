use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use crate::tui::events::PermissionResponse;
use crate::tui::theme::ClaudeTheme;

/// Interactive permission prompt state.
pub struct PermissionPrompt {
    pub tool_name: String,
    pub command_preview: String,
    pub selected: usize, // 0=AllowOnce, 1=Skip, 2=AlwaysAllow
}

const OPTIONS: &[(&str, PermissionResponse)] = &[
    ("Allow once", PermissionResponse::Allowed),
    ("Skip", PermissionResponse::Denied),
    ("Always allow for session", PermissionResponse::AlwaysAllow),
];

impl PermissionPrompt {
    pub fn new(tool_name: &str, command_preview: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            command_preview: command_preview.to_string(),
            selected: 0,
        }
    }

    pub fn navigate_left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = OPTIONS.len() - 1;
        }
    }

    pub fn navigate_right(&mut self) {
        self.selected = (self.selected + 1) % OPTIONS.len();
    }

    /// Returns the PermissionResponse for the currently selected option.
    pub fn confirm(&self) -> PermissionResponse {
        OPTIONS[self.selected].1.clone()
    }

    /// Handle a key event. Returns Some(response) if the user made a choice.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PermissionResponse> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => Some(PermissionResponse::Allowed),
            KeyCode::Char('n') | KeyCode::Esc => Some(PermissionResponse::Denied),
            KeyCode::Char('a') => Some(PermissionResponse::AlwaysAllow),
            KeyCode::Left => {
                self.navigate_left();
                None
            }
            KeyCode::Right => {
                self.navigate_right();
                None
            }
            KeyCode::Tab => {
                self.navigate_right();
                None
            }
            KeyCode::BackTab => {
                self.navigate_left();
                None
            }
            _ => None,
        }
    }

    /// Render the permission prompt as ratatui Lines.
    pub fn render(&self, theme: &ClaudeTheme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Blank line
        lines.push(Line::from(""));

        // "Allow Bash: command?"
        lines.push(Line::from(vec![
            Span::styled("  Allow ", Style::default().fg(theme.permission)),
            Span::styled(
                format!("{}: ", self.tool_name),
                Style::default()
                    .fg(theme.permission)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}?", truncate(&self.command_preview, 60)),
                Style::default().fg(theme.text_primary),
            ),
        ]));

        // Blank line
        lines.push(Line::from(""));

        // Options line: ▸ Allow once    Skip    Always allow for session
        let mut option_spans: Vec<Span<'static>> = vec![Span::raw("  ".to_string())];
        for (i, (label, _)) in OPTIONS.iter().enumerate() {
            if i == self.selected {
                option_spans.push(Span::styled(
                    format!("▸ {label}"),
                    Style::default()
                        .fg(theme.permission)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                option_spans.push(Span::styled(
                    label.to_string(),
                    Style::default().fg(theme.text_muted),
                ));
            }
            if i < OPTIONS.len() - 1 {
                option_spans.push(Span::raw("    ".to_string()));
            }
        }
        lines.push(Line::from(option_spans));

        // Blank line
        lines.push(Line::from(""));

        // Hint line
        lines.push(Line::from(Span::styled(
            "  (y) allow · (n) skip · (a) always".to_string(),
            Style::default().fg(theme.text_muted),
        )));

        lines
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::events::PermissionResponse;
    use crate::tui::theme::ClaudeTheme;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_default_selection_is_allow() {
        let prompt = PermissionPrompt::new("Bash", "echo hello");
        assert_eq!(prompt.selected, 0);
        assert_eq!(prompt.confirm(), PermissionResponse::Allowed);
    }

    #[test]
    fn test_navigate_right() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        prompt.navigate_right();
        assert_eq!(prompt.selected, 1);
        assert_eq!(prompt.confirm(), PermissionResponse::Denied);
    }

    #[test]
    fn test_navigate_wraps_right() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        prompt.navigate_right(); // 1
        prompt.navigate_right(); // 2
        prompt.navigate_right(); // 0
        assert_eq!(prompt.selected, 0);
    }

    #[test]
    fn test_navigate_wraps_left() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        prompt.navigate_left(); // wraps to 2
        assert_eq!(prompt.selected, 2);
        assert_eq!(prompt.confirm(), PermissionResponse::AlwaysAllow);
    }

    #[test]
    fn test_handle_key_y_allows() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        let result = prompt.handle_key(key(KeyCode::Char('y')));
        assert_eq!(result, Some(PermissionResponse::Allowed));
    }

    #[test]
    fn test_handle_key_n_denies() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        let result = prompt.handle_key(key(KeyCode::Char('n')));
        assert_eq!(result, Some(PermissionResponse::Denied));
    }

    #[test]
    fn test_handle_key_a_always() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        let result = prompt.handle_key(key(KeyCode::Char('a')));
        assert_eq!(result, Some(PermissionResponse::AlwaysAllow));
    }

    #[test]
    fn test_handle_key_arrow_navigates() {
        let mut prompt = PermissionPrompt::new("Bash", "test");
        let result = prompt.handle_key(key(KeyCode::Right));
        assert_eq!(result, None);
        assert_eq!(prompt.selected, 1);
    }

    #[test]
    fn test_render_produces_lines() {
        let theme = ClaudeTheme::default();
        let prompt = PermissionPrompt::new("Bash", "echo hello");
        let lines = prompt.render(&theme);
        assert!(lines.len() >= 4);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("Allow"));
        assert!(text.contains("Bash"));
        assert!(text.contains("echo hello"));
    }

    #[test]
    fn test_render_shows_selected_option() {
        let theme = ClaudeTheme::default();
        let prompt = PermissionPrompt::new("Bash", "test");
        let lines = prompt.render(&theme);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("▸ Allow once"));
    }
}
