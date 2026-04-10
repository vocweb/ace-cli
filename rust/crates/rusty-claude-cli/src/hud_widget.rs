use ratatui::prelude::*;
use ratatui::widgets::Widget;

use crate::hud::HudState;

/// A 2-line footer widget that renders HUD information from `HudState`.
///
/// **Line 1** — Context & Project:
///   `📁 ~/Projects/my-app (main*)  ║  ▰▱▱▱ 18% 36k/200k  ║  ⏱ 3.2s`
///
/// **Line 2** — Model & Activity:
///   `🧠 claude-opus-4-6  ║  🔧 ◐ Bash running  Read×3 Edit×2  ║  📋 ▸ 2/5 "Fix auth bug"  ║  🤖 Explorer (12s)`
pub struct HudFooter<'a> {
    state: &'a HudState,
}

impl<'a> HudFooter<'a> {
    pub fn new(state: &'a HudState) -> Self {
        Self { state }
    }

    /// Build the first line: project path, git branch, context bar, turn duration.
    fn render_line1(&self, _width: u16) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();

        // --- Project path section ---
        spans.push(Span::styled(
            format!(" \u{1F4C1} {}", self.state.short_path()),
            Style::default().fg(Color::Yellow),
        ));

        // Git branch with optional dirty indicator
        if let Some(ref branch) = self.state.git_branch {
            let dirty = if self.state.git_dirty { "*" } else { "" };
            spans.push(Span::raw(" ".to_string()));
            spans.push(Span::styled(
                format!("({branch}{dirty})"),
                Style::default().fg(Color::Cyan),
            ));
        }

        // --- Separator ---
        spans.push(Span::styled(
            "  \u{2551}  ".to_string(),
            Style::default().fg(Color::DarkGray),
        ));

        // --- Context bar section ---
        let pct = self.state.context_percent();
        let bar_color = match pct {
            0..=49 => Color::Green,
            50..=74 => Color::Yellow,
            _ => Color::Red,
        };
        let bar = self.state.context_bar();
        let used_str = HudState::format_tokens(self.state.tokens_used);
        let max_str = HudState::format_tokens(self.state.tokens_max);
        spans.push(Span::styled(
            format!("{bar} {pct}% {used_str}/{max_str}"),
            Style::default().fg(bar_color),
        ));

        // --- Turn duration (only if turn active) ---
        if let Some(dur) = self.state.turn_duration() {
            spans.push(Span::styled(
                "  \u{2551}  ".to_string(),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                format!("\u{23F1} {dur}"),
                Style::default().fg(Color::Blue),
            ));
        }

        Line::from(spans)
    }

    /// Build the second line: model name, tool activity, todo progress, running agents.
    fn render_line2(&self, _width: u16) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();

        // --- Model name ---
        spans.push(Span::styled(
            format!(" \u{1F9E0} {}", self.state.model_name),
            Style::default().fg(Color::Cyan),
        ));

        // --- Tool activity (only if there is an active tool or completed tools) ---
        let has_active_tool = self.state.active_tool.is_some();
        let has_completed_tools = !self.state.tool_counts.is_empty();

        if has_active_tool || has_completed_tools {
            spans.push(Span::styled(
                "  \u{2551}  ".to_string(),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::raw("\u{1F527} ".to_string()));

            // Active tool
            if let Some((ref name, _)) = self.state.active_tool {
                spans.push(Span::styled(
                    format!("\u{25D0} {name} running"),
                    Style::default().fg(Color::Yellow),
                ));
                if has_completed_tools {
                    spans.push(Span::raw("  ".to_string()));
                }
            }

            // Completed tools sorted by count desc, max 4
            if has_completed_tools {
                let mut sorted: Vec<_> = self.state.tool_counts.iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
                sorted.truncate(4);

                let completed_str: Vec<String> = sorted
                    .iter()
                    .map(|(name, count)| format!("{name}\u{00D7}{count}"))
                    .collect();

                spans.push(Span::styled(
                    completed_str.join(" "),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }

        // --- Todo progress (only if there are todos) ---
        if self.state.todos_total > 0 {
            spans.push(Span::styled(
                "  \u{2551}  ".to_string(),
                Style::default().fg(Color::DarkGray),
            ));

            let all_done = self.state.todos_done >= self.state.todos_total;
            let (indicator, todo_color) = if all_done {
                ("\u{2713}", Color::Green)
            } else {
                ("\u{25B8}", Color::Yellow)
            };

            let mut todo_text = format!(
                "\u{1F4CB} {indicator} {}/{}",
                self.state.todos_done, self.state.todos_total
            );

            if let Some(ref task) = self.state.current_task {
                let truncated = if task.len() > 30 {
                    format!(" \"{}...\"", &task[..27])
                } else {
                    format!(" \"{task}\"")
                };
                todo_text.push_str(&truncated);
            }

            spans.push(Span::styled(todo_text, Style::default().fg(todo_color)));
        }

        // --- Running agents (only if there are active agents) ---
        let active_agents: Vec<_> = self.state.agents.iter().filter(|a| !a.completed).collect();

        if !active_agents.is_empty() {
            spans.push(Span::styled(
                "  \u{2551}  ".to_string(),
                Style::default().fg(Color::DarkGray),
            ));

            let agents_str: Vec<String> = active_agents
                .iter()
                .map(|a| {
                    let secs = a.started_at.elapsed().as_secs();
                    format!("{} ({secs}s)", a.agent_type)
                })
                .collect();

            spans.push(Span::styled(
                format!("\u{1F916} {}", agents_str.join(" ")),
                Style::default().fg(Color::Magenta),
            ));
        }

        Line::from(spans)
    }
}

impl Widget for HudFooter<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }

        let width = area.width;

        // Always render line1
        let line1 = self.render_line1(width);
        buf.set_line(area.x, area.y, &line1, width);

        // Render line2 only if we have at least 2 rows
        if area.height >= 2 {
            let line2 = self.render_line2(width);
            buf.set_line(area.x, area.y + 1, &line2, width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hud_footer_renders_line1() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let footer = HudFooter::new(&state);
        let line = footer.render_line1(80);
        // Should contain project path and context bar
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("/tmp/project"));
        assert!(text.contains("0%"));
    }

    #[test]
    fn test_hud_footer_renders_model_name() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let footer = HudFooter::new(&state);
        let line = footer.render_line2(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("claude-opus-4-6"));
    }

    #[test]
    fn test_hud_footer_with_git_branch() {
        let mut state = HudState::new("claude-opus-4-6", "/tmp/project");
        state.git_branch = Some("main".to_string());
        state.git_dirty = true;
        let footer = HudFooter::new(&state);
        let line = footer.render_line1(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("(main*)"));
    }

    #[test]
    fn test_hud_footer_with_tools() {
        let mut state = HudState::new("claude-opus-4-6", "/tmp/project");
        state.tool_start("Read");
        state.tool_end();
        state.tool_start("Read");
        state.tool_end();
        state.tool_start("Edit");
        state.tool_end();

        let footer = HudFooter::new(&state);
        let line = footer.render_line2(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("Read\u{00D7}2"));
        assert!(text.contains("Edit\u{00D7}1"));
    }

    #[test]
    fn test_hud_footer_with_todos() {
        let mut state = HudState::new("claude-opus-4-6", "/tmp/project");
        state.todos_total = 5;
        state.todos_done = 2;
        state.current_task = Some("Fix auth bug".to_string());

        let footer = HudFooter::new(&state);
        let line = footer.render_line2(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("2/5"));
        assert!(text.contains("Fix auth bug"));
    }

    #[test]
    fn test_hud_footer_todo_truncation() {
        let mut state = HudState::new("claude-opus-4-6", "/tmp/project");
        state.todos_total = 5;
        state.todos_done = 2;
        state.current_task = Some(
            "This is a very long task name that should be truncated to thirty chars".to_string(),
        );

        let footer = HudFooter::new(&state);
        let line = footer.render_line2(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("...\""));
    }

    #[test]
    fn test_widget_render_minimal_height() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let footer = HudFooter::new(&state);
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        footer.render(area, &mut buf);
        // Should not panic; line2 is skipped
    }

    #[test]
    fn test_widget_render_full_height() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let footer = HudFooter::new(&state);
        let area = Rect::new(0, 0, 80, 2);
        let mut buf = Buffer::empty(area);
        footer.render(area, &mut buf);
        // Should render both lines without panic
    }
}
