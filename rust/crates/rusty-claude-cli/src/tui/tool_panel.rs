use crate::tui::theme::ClaudeTheme;
use ratatui::prelude::*;

/// Render a bordered tool call header as ratatui Lines.
///
/// ```text
/// ╭─ Bash ──────────────────────────────╮
/// │ npm install express                  │
/// ╰─────────────────────────────────────╯
/// ```
pub fn render_tool_header(
    name: &str,
    input_preview: &str,
    width: u16,
    theme: &ClaudeTheme,
) -> Vec<Line<'static>> {
    let border_color = theme.tool_border_color(name);
    let w = width as usize;

    // Top border: ╭─ Name ─...─╮
    let label = format!("─ {} ", name);
    let remaining = w.saturating_sub(label.len() + 2); // 2 for ╭ and ╮
    let top = format!("╭{}{}╮", label, "─".repeat(remaining));

    let mut lines = vec![Line::from(Span::styled(
        top,
        Style::default().fg(border_color),
    ))];

    // Content lines: │ text │
    for content_line in input_preview.lines().take(3) {
        let text = format!("│ {}", content_line);
        let padded = format!("{:<width$}│", text, width = w.saturating_sub(1));
        lines.push(Line::from(Span::styled(
            padded,
            Style::default().fg(border_color),
        )));
    }

    // Bottom border: ╰─...─╯
    let bottom = format!("╰{}╯", "─".repeat(w.saturating_sub(2)));
    lines.push(Line::from(Span::styled(
        bottom,
        Style::default().fg(border_color),
    )));

    lines
}

/// Render a tool result line.
///
/// Success: `  ⎿ ✓ output text`
/// Error:   `  ⎿ ✗ error text`
pub fn render_tool_result(
    _name: &str,
    output: &str,
    is_error: bool,
    theme: &ClaudeTheme,
) -> Vec<Line<'static>> {
    let (icon, color) = if is_error {
        ("✗", theme.error)
    } else {
        ("✓", theme.success)
    };

    let output_lines: Vec<&str> = output.lines().collect();
    let line_count = output_lines.len();

    if line_count == 0 {
        return vec![Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(theme.text_muted)),
            Span::styled(format!("{icon} done"), Style::default().fg(color)),
        ])];
    }

    // Collapsed if > 5 lines
    if line_count > 5 {
        let summary = output_lines[0].chars().take(60).collect::<String>();
        return vec![Line::from(vec![
            Span::styled("  ⎿ ", Style::default().fg(theme.text_muted)),
            Span::styled(format!("{icon} "), Style::default().fg(color)),
            Span::styled(summary, Style::default().fg(theme.text_secondary)),
            Span::styled(
                format!(" ({line_count} lines, Enter to expand)"),
                Style::default().fg(theme.text_muted),
            ),
        ])];
    }

    // Show all lines
    let mut lines = Vec::new();
    for (i, line) in output_lines.iter().enumerate() {
        if i == 0 {
            lines.push(Line::from(vec![
                Span::styled("  ⎿ ", Style::default().fg(theme.text_muted)),
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::styled(line.to_string(), Style::default().fg(theme.text_primary)),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("    ", Style::default().fg(theme.text_muted)),
                Span::styled(line.to_string(), Style::default().fg(theme.text_primary)),
            ]));
        }
    }
    lines
}

/// Render a "tool running" indicator line.
///
/// `  ⎿ ◐ Running... (3.2s)`
pub fn render_tool_running(
    spinner_char: char,
    elapsed_secs: f64,
    theme: &ClaudeTheme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ⎿ ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format!("{spinner_char} Running... ({elapsed_secs:.1}s)"),
            Style::default().fg(theme.warning),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::ClaudeTheme;
    use ratatui::style::Color;

    #[test]
    fn test_tool_header_contains_name() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_header("Bash", "npm install", 60, &theme);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("Bash"));
        assert!(text.contains("npm install"));
    }

    #[test]
    fn test_tool_header_has_borders() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_header("Edit", "src/main.rs", 60, &theme);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("╭"));
        assert!(text.contains("╰"));
    }

    #[test]
    fn test_tool_header_bash_is_pink() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_header("Bash", "echo hello", 60, &theme);
        let first_span = &lines[0].spans[0];
        assert_eq!(first_span.style.fg, Some(Color::Rgb(253, 93, 177)));
    }

    #[test]
    fn test_tool_result_success() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_result("Bash", "ok", false, &theme);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("✓"));
    }

    #[test]
    fn test_tool_result_error() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_result("Bash", "failed", true, &theme);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("✗"));
    }

    #[test]
    fn test_tool_result_collapsed_long_output() {
        let theme = ClaudeTheme::default();
        let long_output = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let lines = render_tool_result("Bash", &long_output, false, &theme);
        // Should be collapsed to 1 line
        assert_eq!(lines.len(), 1);
        let text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("20 lines"));
    }

    #[test]
    fn test_tool_result_short_output_expanded() {
        let theme = ClaudeTheme::default();
        let output = "line 1\nline 2\nline 3";
        let lines = render_tool_result("Bash", output, false, &theme);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_tool_running_indicator() {
        let theme = ClaudeTheme::default();
        let line = render_tool_running('◐', 3.2, &theme);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("◐"));
        assert!(text.contains("3.2s"));
    }
}
