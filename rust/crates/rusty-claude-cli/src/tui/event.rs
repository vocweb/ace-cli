use std::time::Duration;

use crate::args::{resolve_repl_model, AllowedToolSet, CliOutputFormat};
use crate::cli::LiveCli;
use crate::render::TerminalRenderer;
use crate::repl::{enforce_broad_cwd_policy, run_stale_base_preflight};
use crate::tui::app::{init_terminal, restore_terminal, TuiApp, TuiMode};
use crate::tui::input::InputAction;
use crate::tui::spinner::{ShimmerState, SpinnerState};
use crate::tui::theme::ClaudeTheme;
use commands::{slash_command_specs, SlashCommand};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use runtime::{ContentBlock, PermissionMode};

/// Render a Claude Code style startup banner.
///
/// ```text
/// ╭──────────────────────────────────────────────╮
/// │  ACE CLI v0.1.0                              │
/// │  Model: claude-opus-4-6                      │
/// │  Session: abc123                             │
/// ╰──────────────────────────────────────────────╯
/// ```
fn render_startup_banner(
    model: &str,
    session_id: &str,
    width: u16,
    theme: &ClaudeTheme,
) -> Vec<Line<'static>> {
    let w = width as usize;
    let border_color = theme.brand;

    let top = format!("╭{}╮", "─".repeat(w.saturating_sub(2)));
    let bot = format!("╰{}╯", "─".repeat(w.saturating_sub(2)));

    let version = env!("CARGO_PKG_VERSION");
    let line1 = format!("  ACE CLI v{version}");
    let line2 = format!("  Model: {model}");
    let line3 = format!("  Session: {session_id}");

    let fmt_line = |text: String| -> Line<'static> {
        let padded = format!("│{:<width$}│", text, width = w.saturating_sub(2));
        Line::from(Span::styled(padded, Style::default().fg(border_color)))
    };

    vec![
        Line::from(Span::styled(top, Style::default().fg(border_color))),
        fmt_line(line1),
        fmt_line(line2),
        fmt_line(line3),
        Line::from(Span::styled(bot, Style::default().fg(border_color))),
        Line::from(""),
    ]
}

/// TUI REPL mode: ratatui alternate screen with 3-zone layout (content, input, HUD footer).
///
/// Stays in the alternate screen throughout the session. During turn execution
/// a spinner is rendered, and once the turn completes the TUI is repainted
/// with the new session messages. The turn runs on the main thread (keeping the
/// `LiveCli` ownership simple), while the spinner frame is drawn immediately
/// before blocking.
///
/// Note: `cli.run_turn()` writes ANSI output directly to stdout (the alternate
/// screen buffer) while it executes. The subsequent `terminal.draw()` call
/// repaints the full screen, overwriting any stray output.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_tui_repl(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    base_commit: Option<String>,
    reasoning_effort: Option<String>,
    allow_broad_cwd: bool,
    show_thinking: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use ratatui::style::{Color as RColor, Style as RStyle};

    enforce_broad_cwd_policy(allow_broad_cwd, CliOutputFormat::Text)?;
    run_stale_base_preflight(base_commit.as_deref());
    let resolved_model = resolve_repl_model(model);
    let mut cli = LiveCli::new(resolved_model, true, allowed_tools, permission_mode)?;
    cli.set_reasoning_effort(reasoning_effort);
    let effective_show_thinking =
        show_thinking || crate::args::config_show_thinking_for_current_dir().unwrap_or(false);
    if effective_show_thinking {
        cli.set_show_thinking(true);
    }

    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .display()
        .to_string();

    // Initialise TUI
    let mut terminal = init_terminal()?;

    // Install panic hook to always restore terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    // Build dropdown candidates from slash command specs
    let dropdown_candidates: Vec<(String, String)> = slash_command_specs()
        .iter()
        .map(|spec| (spec.name.to_string(), spec.summary.to_string()))
        .collect();

    let mut app = TuiApp::new(cli.model.clone(), cwd, dropdown_candidates);
    app.hud.refresh_git();

    // Provide slash-command completions for Tab
    if let Ok(candidates) = cli.repl_completion_candidates() {
        app.input.set_completions(candidates);
    }

    // Push startup banner into Zone 1
    let theme = ClaudeTheme::default();
    let banner = render_startup_banner(&cli.model, &cli.session.id, 50, &theme);
    app.push_content(banner);

    loop {
        // Draw TUI
        terminal.draw(|frame| app.render(frame))?;

        // Poll for events with 33ms timeout (~30fps)
        if event::poll(Duration::from_millis(33))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    // Ctrl+C always works — quit cleanly
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        cli.persist_session()?;
                        app.should_quit = true;
                        continue;
                    }

                    // Ignore input while streaming (turn is running on main thread,
                    // so this branch is actually unreachable during streaming — kept
                    // for correctness when mode is checked outside the turn loop).
                    if app.mode == TuiMode::Streaming {
                        continue;
                    }

                    // PageUp/PageDown for scrolling Zone 1
                    match (key.code, key.modifiers) {
                        (KeyCode::PageUp, _) => {
                            app.scroll_up(10);
                            continue;
                        }
                        (KeyCode::PageDown, _) => {
                            app.scroll_down(10);
                            continue;
                        }
                        _ => {}
                    }

                    // When dropdown is open, intercept navigation keys
                    if app.dropdown.is_open() {
                        match key.code {
                            KeyCode::Down => {
                                app.dropdown.move_down();
                                continue;
                            }
                            KeyCode::Up => {
                                app.dropdown.move_up();
                                continue;
                            }
                            KeyCode::Tab | KeyCode::Enter => {
                                if let Some(cmd) = app.dropdown.confirm_selection() {
                                    app.input.buffer = cmd;
                                    app.input.cursor = app.input.buffer.len();
                                    // Don't submit — just fill the input
                                }
                                continue;
                            }
                            KeyCode::Esc => {
                                app.dropdown.dismiss();
                                continue;
                            }
                            _ => {
                                // All other keys (typing, backspace, etc.) fall through
                                // to input.handle_key(), then dropdown updates via Changed
                            }
                        }
                    }

                    match app.input.handle_key(key) {
                        InputAction::Submit(text) => {
                            // Close dropdown on submit
                            app.dropdown.dismiss();

                            let trimmed = text.trim().to_string();
                            if trimmed.is_empty() {
                                continue;
                            }
                            if matches!(trimmed.as_str(), "/exit" | "/quit") {
                                cli.persist_session()?;
                                app.should_quit = true;
                                continue;
                            }

                            // Record prompt in Zone 1
                            app.push_text(
                                format!("> {trimmed}"),
                                RStyle::default()
                                    .fg(RColor::White)
                                    .add_modifier(ratatui::style::Modifier::BOLD),
                            );

                            // Handle slash commands synchronously.
                            // Capture stdout/stderr output via the cli module's
                            // shadowed println!/eprintln! macros so the text
                            // lands in Zone 1 instead of corrupting the ratatui
                            // alternate screen.
                            let handled = match SlashCommand::parse(&trimmed) {
                                Ok(Some(command)) => {
                                    crate::cli::begin_output_capture();
                                    let _ = cli.handle_repl_command(command);
                                    let captured = crate::cli::end_output_capture();

                                    if !captured.is_empty() {
                                        for line in captured.lines() {
                                            app.push_text(
                                                line.to_string(),
                                                RStyle::default().fg(RColor::White),
                                            );
                                        }
                                        app.push_text(String::new(), RStyle::default());
                                    }

                                    // Sync HUD state after command (model may have changed)
                                    app.hud.model_name.clone_from(&cli.model);

                                    true
                                }
                                Ok(None) => false,
                                Err(error) => {
                                    app.push_text(
                                        format!("Error: {error}"),
                                        RStyle::default().fg(RColor::Red),
                                    );
                                    true
                                }
                            };

                            if !handled {
                                // Switch to Streaming mode and draw the spinner frame
                                // before blocking on the turn.
                                app.mode = TuiMode::Streaming;
                                app.hud.start_turn();

                                // Inject a spinner line into Zone 1 that will be visible
                                // during the turn (and overwritten afterwards).
                                let spinner_line_idx = app.content_lines.len();
                                let mut shimmer = ShimmerState::new();
                                let spinner = SpinnerState::new();
                                app.content_lines.push(ratatui::text::Line::from(
                                    ratatui::text::Span::styled(
                                        format!(
                                            "  {} {}…",
                                            shimmer.current_verb(),
                                            spinner.current_char()
                                        ),
                                        RStyle::default().fg(shimmer.current_color()),
                                    ),
                                ));
                                if app.auto_scroll {
                                    app.scroll_to_bottom();
                                }

                                // Draw the spinner frame so the user sees it immediately.
                                terminal.draw(|frame| app.render(frame))?;

                                // Update shimmer state for the stored line (cosmetic only —
                                // the turn runs synchronously so we can't animate further).
                                shimmer.tick();
                                if spinner_line_idx < app.content_lines.len() {
                                    app.content_lines[spinner_line_idx] =
                                        ratatui::text::Line::from(ratatui::text::Span::styled(
                                            format!(
                                                "  {} {}…",
                                                shimmer.current_verb(),
                                                spinner.current_char()
                                            ),
                                            RStyle::default().fg(shimmer.current_color()),
                                        ));
                                }

                                let msg_count_before = cli.runtime.session().messages.len();
                                cli.record_prompt_history(&trimmed);

                                // Run the turn on the main thread (blocking).
                                // Use run_turn_quiet() which sets emit_output=false
                                // so no raw ANSI is written to the alternate screen.
                                // All content is rendered into Zone 1 afterwards by
                                // render_turn_messages().
                                let turn_result = cli.run_turn_quiet(&trimmed);

                                // Remove the spinner line.
                                if spinner_line_idx < app.content_lines.len() {
                                    app.content_lines.remove(spinner_line_idx);
                                }

                                // Switch back to Input mode.
                                app.mode = TuiMode::Input;

                                match turn_result {
                                    Ok(()) => {
                                        // Update HUD after turn
                                        app.hud.turn_start = None;
                                        app.hud.refresh_git();
                                        app.hud.model_name.clone_from(&cli.model);
                                        if let Some(usage) = cli.cumulative_token_usage() {
                                            let total = u64::from(usage.input_tokens)
                                                + u64::from(usage.output_tokens);
                                            app.hud.update_tokens(total, app.hud.tokens_max);
                                        }

                                        // Extract new messages from session and
                                        // render to Zone 1 with clear visual
                                        // hierarchy.
                                        let renderer = TerminalRenderer::new();
                                        let messages = &cli.runtime.session().messages;
                                        render_turn_messages(
                                            &mut app,
                                            &renderer,
                                            messages,
                                            msg_count_before,
                                        );
                                    }
                                    Err(error) => {
                                        app.push_text(
                                            format!("Error: {error}"),
                                            RStyle::default().fg(RColor::Red),
                                        );
                                    }
                                }

                                // Refresh completions after turn
                                if let Ok(candidates) = cli.repl_completion_candidates() {
                                    app.input.set_completions(candidates);
                                }
                            }
                        }
                        InputAction::Exit => {
                            cli.persist_session()?;
                            app.should_quit = true;
                        }
                        InputAction::Changed => {
                            app.dropdown.update_filter(&app.input.buffer);
                        }
                        InputAction::None => {}
                    }
                }
                Event::Paste(text) => {
                    if app.mode != TuiMode::Streaming {
                        app.input.handle_paste(text);
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    crossterm::event::MouseEventKind::ScrollUp => app.scroll_up(3),
                    crossterm::event::MouseEventKind::ScrollDown => app.scroll_down(3),
                    _ => {}
                },
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    restore_terminal()?;
    Ok(())
}

// ── Post-turn message rendering ──────────────────────────────────────────

use runtime::MessageRole;

/// Returns `true` if `text` is a thinking-block summary marker emitted by
/// the API client (e.g. "▶ Thinking (42 chars hidden)").
fn is_thinking_marker(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("▶ Thinking") || t.starts_with("▸ Thinking")
}

/// Render session messages added during a turn into Zone 1 with clear
/// visual hierarchy:
///
/// - **Assistant text** → rendered as markdown (the main response).
/// - **Thinking markers** → rendered dimmed and collapsed.
/// - **Tool calls** → shown as a single compact line (no raw JSON).
/// - **Tool results** → errors are shown; successes are collapsed.
fn render_turn_messages(
    app: &mut TuiApp,
    renderer: &TerminalRenderer,
    messages: &[runtime::ConversationMessage],
    skip: usize,
) {
    use ratatui::prelude::{Color as RColor, Line as RLine, Modifier, Span, Style as RStyle};

    let theme = ClaudeTheme::default();

    for msg in messages.iter().skip(skip) {
        // Add a subtle role separator for assistant messages.
        if msg.role == MessageRole::Assistant {
            app.push_content(vec![RLine::from(Span::styled(
                "  ── assistant ──────────────────────────────",
                RStyle::default().fg(theme.text_muted),
            ))]);
        }

        for block in &msg.blocks {
            match block {
                ContentBlock::Text { text } => {
                    if is_thinking_marker(text) {
                        // Render thinking markers in a collapsed, dimmed style.
                        app.push_content(vec![RLine::from(Span::styled(
                            format!("  {}", text.trim()),
                            RStyle::default()
                                .fg(theme.text_muted)
                                .add_modifier(Modifier::DIM),
                        ))]);
                    } else {
                        // Normal assistant text — render as markdown.
                        let lines = renderer.render_markdown_to_lines(text);
                        app.push_content(lines);
                    }
                }
                ContentBlock::ToolUse { name, .. } => {
                    // Compact tool-call indicator: just the name, no raw JSON.
                    app.push_content(vec![RLine::from(vec![
                        Span::styled("  ▸ ", RStyle::default().fg(theme.brand)),
                        Span::styled(
                            name.clone(),
                            RStyle::default()
                                .fg(RColor::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])]);
                }
                ContentBlock::ToolResult {
                    tool_name,
                    output,
                    is_error,
                    ..
                } => {
                    if *is_error {
                        // Show errors — they matter to the user.
                        let lines =
                            TerminalRenderer::format_tool_result_lines(tool_name, output, true);
                        app.push_content(lines);
                    } else {
                        // Collapse successful results to a single check mark.
                        app.push_content(vec![RLine::from(vec![
                            Span::styled("  ✓ ", RStyle::default().fg(theme.success)),
                            Span::styled(
                                tool_name.clone(),
                                RStyle::default().fg(theme.text_secondary),
                            ),
                        ])]);
                    }
                }
            }
        }
    }

    // Blank line after the response for spacing.
    app.push_text(String::new(), ratatui::style::Style::default());
}
