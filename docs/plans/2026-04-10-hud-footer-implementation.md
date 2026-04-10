# HUD Footer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a persistent 3-zone TUI layout (content, input, HUD footer) to ACE CLI using ratatui, providing real-time project path, context health, tool activity, agent tracking, todo progress, and model info.

**Architecture:** Replace the current inline stdout rendering with a ratatui alternate-screen app. The `App` struct holds all state for 3 zones. An async event loop handles keyboard input and API stream events via `tokio::select!`. The existing `TerminalRenderer` is adapted to produce `ratatui::text::Line` instead of ANSI strings. `rustyline` is replaced with a custom input handler operating in raw mode.

**Tech Stack:** Rust, ratatui 0.29 (crossterm backend), crossterm 0.28 (already present), tokio (already present), pulldown-cmark (already present), syntect (already present)

---

## Phase 1: Foundation — Ratatui Scaffold & App Struct

### Task 1: Add ratatui dependency

**Files:**
- Modify: `rust/Cargo.toml` (workspace deps)
- Modify: `rust/crates/rusty-claude-cli/Cargo.toml` (add dep)

**Step 1: Add ratatui to workspace dependencies**

In `rust/Cargo.toml`, add to `[workspace.dependencies]`:
```toml
[workspace.dependencies]
serde_json = "1"
ratatui = "0.29"
```

**Step 2: Add ratatui to cli crate**

In `rust/crates/rusty-claude-cli/Cargo.toml`, add under `[dependencies]`:
```toml
ratatui = { workspace = true }
```

**Step 3: Verify it compiles**

Run: `cd rust && cargo check -p rusty-claude-cli`
Expected: compiles with no errors (ratatui re-exports crossterm types)

**Step 4: Commit**

```bash
git add rust/Cargo.toml rust/crates/rusty-claude-cli/Cargo.toml
git commit -m "deps: add ratatui 0.29 for TUI layout"
```

---

### Task 2: Create HudState struct and module

**Files:**
- Create: `rust/crates/rusty-claude-cli/src/hud.rs`
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` (add `mod hud;`)

**Step 1: Write tests for HudState**

In `rust/crates/rusty-claude-cli/src/hud.rs`:
```rust
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct AgentStatus {
    pub agent_type: String,
    pub description: String,
    pub started_at: Instant,
    pub completed: bool,
}

#[derive(Debug)]
pub struct HudState {
    // Project
    pub project_path: String,
    pub git_branch: Option<String>,
    pub git_dirty: bool,

    // Context Health
    pub tokens_used: u64,
    pub tokens_max: u64,

    // Model
    pub model_name: String,

    // Tool Activity
    pub active_tool: Option<(String, Instant)>,
    pub tool_counts: HashMap<String, u32>,

    // Todo Progress
    pub todos_total: u32,
    pub todos_done: u32,
    pub current_task: Option<String>,

    // Agent Tracking
    pub agents: Vec<AgentStatus>,

    // Turn
    pub turn_start: Option<Instant>,

    // Dirty flag
    pub dirty: bool,
}

impl HudState {
    pub fn new(model_name: String, project_path: String) -> Self {
        Self {
            project_path,
            git_branch: None,
            git_dirty: false,
            tokens_used: 0,
            tokens_max: 200_000,
            model_name,
            active_tool: None,
            tool_counts: HashMap::new(),
            todos_total: 0,
            todos_done: 0,
            current_task: None,
            agents: Vec::new(),
            turn_start: None,
            dirty: true,
        }
    }

    /// Returns context usage as percentage 0-100
    pub fn context_percent(&self) -> u8 {
        if self.tokens_max == 0 { return 0; }
        ((self.tokens_used as f64 / self.tokens_max as f64) * 100.0).min(100.0) as u8
    }

    /// Returns the 4-block bar string (▰/▱)
    pub fn context_bar(&self) -> String {
        let pct = self.context_percent();
        let filled = match pct {
            0..=24 => 1,
            25..=49 => 2,
            50..=74 => 3,
            _ => 4,
        };
        let empty = 4 - filled;
        format!("{}{}", "▰".repeat(filled), "▱".repeat(empty))
    }

    /// Returns shortened project path (last 2 segments + home dir abbreviation)
    pub fn short_path(&self) -> String {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = if self.project_path.starts_with(&home) {
            format!("~{}", &self.project_path[home.len()..])
        } else {
            self.project_path.clone()
        };
        path
    }

    /// Format token count as human-readable (e.g., 4.2k, 150k)
    pub fn format_tokens(tokens: u64) -> String {
        if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{:.0}k", tokens as f64 / 1_000.0)
        } else {
            format!("{}", tokens)
        }
    }

    /// Record a tool starting
    pub fn tool_start(&mut self, name: String) {
        self.active_tool = Some((name, Instant::now()));
        self.dirty = true;
    }

    /// Record a tool completing
    pub fn tool_end(&mut self) {
        if let Some((name, _)) = self.active_tool.take() {
            *self.tool_counts.entry(name).or_insert(0) += 1;
        }
        self.dirty = true;
    }

    /// Update token usage
    pub fn update_tokens(&mut self, used: u64, max: u64) {
        self.tokens_used = used;
        self.tokens_max = max;
        self.dirty = true;
    }

    /// Start a new turn
    pub fn start_turn(&mut self) {
        self.turn_start = Some(Instant::now());
        self.dirty = true;
    }

    /// Get turn elapsed duration as formatted string
    pub fn turn_duration(&self) -> Option<String> {
        self.turn_start.map(|start| {
            let elapsed = start.elapsed();
            format!("{:.1}s", elapsed.as_secs_f64())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_percent_zero() {
        let hud = HudState::new("test".into(), "/tmp".into());
        assert_eq!(hud.context_percent(), 0);
    }

    #[test]
    fn test_context_percent_half() {
        let mut hud = HudState::new("test".into(), "/tmp".into());
        hud.tokens_used = 100_000;
        hud.tokens_max = 200_000;
        assert_eq!(hud.context_percent(), 50);
    }

    #[test]
    fn test_context_percent_max_zero() {
        let mut hud = HudState::new("test".into(), "/tmp".into());
        hud.tokens_max = 0;
        assert_eq!(hud.context_percent(), 0);
    }

    #[test]
    fn test_context_bar_low() {
        let mut hud = HudState::new("test".into(), "/tmp".into());
        hud.tokens_used = 10_000;
        hud.tokens_max = 200_000; // 5%
        assert_eq!(hud.context_bar(), "▰▱▱▱");
    }

    #[test]
    fn test_context_bar_half() {
        let mut hud = HudState::new("test".into(), "/tmp".into());
        hud.tokens_used = 100_000;
        hud.tokens_max = 200_000; // 50%
        assert_eq!(hud.context_bar(), "▰▰▰▱");
    }

    #[test]
    fn test_context_bar_full() {
        let mut hud = HudState::new("test".into(), "/tmp".into());
        hud.tokens_used = 180_000;
        hud.tokens_max = 200_000; // 90%
        assert_eq!(hud.context_bar(), "▰▰▰▰");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(HudState::format_tokens(500), "500");
        assert_eq!(HudState::format_tokens(4_200), "4k");
        assert_eq!(HudState::format_tokens(150_000), "150k");
        assert_eq!(HudState::format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_tool_tracking() {
        let mut hud = HudState::new("test".into(), "/tmp".into());
        hud.tool_start("Read".into());
        assert!(hud.active_tool.is_some());
        hud.tool_end();
        assert!(hud.active_tool.is_none());
        assert_eq!(hud.tool_counts.get("Read"), Some(&1));
        hud.tool_start("Read".into());
        hud.tool_end();
        assert_eq!(hud.tool_counts.get("Read"), Some(&2));
    }
}
```

**Step 2: Add mod declaration in main.rs**

At the top of `rust/crates/rusty-claude-cli/src/main.rs`, near the other `mod` declarations, add:
```rust
mod hud;
```

**Step 3: Run tests**

Run: `cd rust && cargo test -p rusty-claude-cli -- hud`
Expected: All 7 tests pass

**Step 4: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/hud.rs rust/crates/rusty-claude-cli/src/main.rs
git commit -m "feat(hud): add HudState struct with context bar and tool tracking"
```

---

### Task 3: Create HUD footer widget (Zone 3 renderer)

**Files:**
- Create: `rust/crates/rusty-claude-cli/src/hud_widget.rs`
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` (add `mod hud_widget;`)

**Step 1: Write the HUD footer widget**

Create `rust/crates/rusty-claude-cli/src/hud_widget.rs`:
```rust
use ratatui::prelude::*;
use ratatui::widgets::Widget;

use crate::hud::HudState;

/// Renders the 2-line HUD footer (Zone 3)
pub struct HudFooter<'a> {
    state: &'a HudState,
}

impl<'a> HudFooter<'a> {
    pub fn new(state: &'a HudState) -> Self {
        Self { state }
    }

    fn render_line1(&self, width: u16) -> Line<'static> {
        let sep = Span::styled(" ║ ", Style::default().fg(Color::DarkGray));

        // Project path + git
        let mut path_spans = vec![
            Span::raw(" 📁 "),
            Span::styled(self.state.short_path(), Style::default().fg(Color::Yellow)),
        ];
        if let Some(branch) = &self.state.git_branch {
            let dirty = if self.state.git_dirty { "*" } else { "" };
            path_spans.push(Span::raw(" ("));
            path_spans.push(Span::styled(
                format!("{branch}{dirty}"),
                Style::default().fg(Color::Cyan),
            ));
            path_spans.push(Span::raw(")"));
        }

        // Context health bar
        let pct = self.state.context_percent();
        let bar_color = match pct {
            0..=49 => Color::Green,
            50..=74 => Color::Yellow,
            _ => Color::Red,
        };
        let bar = self.state.context_bar();
        let tokens_str = format!(
            " {}% {}/{}",
            pct,
            HudState::format_tokens(self.state.tokens_used),
            HudState::format_tokens(self.state.tokens_max),
        );
        let context_spans = vec![
            Span::styled(bar, Style::default().fg(bar_color)),
            Span::styled(tokens_str, Style::default().fg(bar_color)),
        ];

        // Turn duration
        let duration_spans = if let Some(dur) = self.state.turn_duration() {
            vec![
                Span::raw("⏱ "),
                Span::styled(dur, Style::default().fg(Color::Blue)),
            ]
        } else {
            vec![]
        };

        let mut spans = path_spans;
        spans.push(sep.clone());
        spans.extend(context_spans);
        if !duration_spans.is_empty() {
            spans.push(sep);
            spans.extend(duration_spans);
        }

        Line::from(spans)
    }

    fn render_line2(&self, width: u16) -> Line<'static> {
        let sep = Span::styled(" ║ ", Style::default().fg(Color::DarkGray));

        // Model name
        let model_spans = vec![
            Span::raw(" 🧠 "),
            Span::styled(
                self.state.model_name.clone(),
                Style::default().fg(Color::Cyan),
            ),
        ];

        // Tool activity
        let mut tool_spans = vec![Span::raw("🔧 ")];
        if let Some((name, started)) = &self.state.active_tool {
            let elapsed = started.elapsed().as_secs();
            tool_spans.push(Span::styled(
                format!("◐ {name} "),
                Style::default().fg(Color::Yellow),
            ));
        }
        // Top completed tools (sorted by count, max 4)
        let mut sorted_tools: Vec<_> = self.state.tool_counts.iter().collect();
        sorted_tools.sort_by(|a, b| b.1.cmp(a.1));
        for (name, count) in sorted_tools.iter().take(4) {
            tool_spans.push(Span::styled(
                format!("{name}×{count} "),
                Style::default().fg(Color::DarkGray),
            ));
        }

        // Todo progress
        let mut todo_spans = vec![];
        if self.state.todos_total > 0 {
            let all_done = self.state.todos_done == self.state.todos_total;
            let icon_color = if all_done { Color::Green } else { Color::Yellow };
            let icon = if all_done { "✓" } else { "▸" };
            todo_spans.push(Span::raw("📋 "));
            todo_spans.push(Span::styled(
                format!("{icon} {}/{}", self.state.todos_done, self.state.todos_total),
                Style::default().fg(icon_color),
            ));
            if let Some(task) = &self.state.current_task {
                let truncated: String = task.chars().take(30).collect();
                todo_spans.push(Span::styled(
                    format!(" \"{truncated}\""),
                    Style::default().fg(icon_color),
                ));
            }
        }

        // Agent tracking
        let mut agent_spans = vec![];
        let running_agents: Vec<_> = self.state.agents.iter().filter(|a| !a.completed).collect();
        for agent in running_agents.iter().take(3) {
            let elapsed = agent.started_at.elapsed().as_secs();
            agent_spans.push(Span::raw("🤖 "));
            agent_spans.push(Span::styled(
                format!("{} ({elapsed}s)", agent.agent_type),
                Style::default().fg(Color::Magenta),
            ));
        }

        // Assemble line 2
        let mut spans = model_spans;
        if !tool_spans.is_empty() {
            spans.push(sep.clone());
            spans.extend(tool_spans);
        }
        if !todo_spans.is_empty() {
            spans.push(sep.clone());
            spans.extend(todo_spans);
        }
        if !agent_spans.is_empty() {
            spans.push(sep);
            spans.extend(agent_spans);
        }

        Line::from(spans)
    }
}

impl<'a> Widget for HudFooter<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            // Minimal: just context bar on 1 line
            let line = self.render_line1(area.width);
            buf.set_line(area.x, area.y, &line, area.width);
            return;
        }

        let line1 = self.render_line1(area.width);
        let line2 = self.render_line2(area.width);

        buf.set_line(area.x, area.y, &line1, area.width);
        buf.set_line(area.x, area.y + 1, &line2, area.width);
    }
}
```

**Step 2: Add mod declaration**

In `main.rs`, add near other mod declarations:
```rust
mod hud_widget;
```

**Step 3: Verify compilation**

Run: `cd rust && cargo check -p rusty-claude-cli`
Expected: compiles successfully

**Step 4: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/hud_widget.rs rust/crates/rusty-claude-cli/src/main.rs
git commit -m "feat(hud): add HudFooter ratatui widget for Zone 3"
```

---

## Phase 2: Input Handler — Replace Rustyline (Zone 2)

### Task 4: Create custom input handler

**Files:**
- Create: `rust/crates/rusty-claude-cli/src/tui_input.rs`
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` (add `mod tui_input;`)

**Step 1: Write the input handler**

Create `rust/crates/rusty-claude-cli/src/tui_input.rs`:
```rust
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    /// User typing directly — show all lines, auto-expand
    Typing,
    /// User pasted text with N total lines — collapse if > 5
    Pasted(usize),
}

#[derive(Debug)]
pub struct TuiInput {
    /// Full input buffer (always holds complete text)
    pub buffer: String,
    /// Cursor position (byte offset)
    pub cursor: usize,
    /// Current input mode
    pub mode: InputMode,
    /// Command history
    pub history: Vec<String>,
    /// Current history index (None = editing new input)
    pub history_index: Option<usize>,
    /// Saved buffer when browsing history
    pub saved_buffer: Option<String>,
}

pub enum InputAction {
    /// No action needed, just redraw
    None,
    /// User submitted input (Enter)
    Submit(String),
    /// User wants to exit (Ctrl+C or Ctrl+D on empty)
    Exit,
    /// Content changed, recalculate zone height
    Changed,
}

impl TuiInput {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            mode: InputMode::Typing,
            history: Vec::new(),
            history_index: None,
            saved_buffer: None,
        }
    }

    /// Handle a crossterm key event, return what action to take
    pub fn handle_key(&mut self, event: KeyEvent) -> InputAction {
        match (event.code, event.modifiers) {
            // Submit: Enter (without Shift)
            (KeyCode::Enter, m) if !m.contains(KeyModifiers::SHIFT) => {
                let text = self.buffer.clone();
                if !text.trim().is_empty() {
                    self.history.push(text.clone());
                }
                self.buffer.clear();
                self.cursor = 0;
                self.mode = InputMode::Typing;
                self.history_index = None;
                self.saved_buffer = None;
                InputAction::Submit(text)
            }

            // Newline: Shift+Enter or Ctrl+J
            (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
                self.insert_char('\n');
                InputAction::Changed
            }
            (KeyCode::Char('j'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.insert_char('\n');
                InputAction::Changed
            }

            // Exit: Ctrl+C or Ctrl+D on empty buffer
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => InputAction::Exit,
            (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) && self.buffer.is_empty() => {
                InputAction::Exit
            }

            // Character input
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) || m.contains(KeyModifiers::SHIFT) => {
                self.insert_char(c);
                InputAction::Changed
            }

            // Backspace
            (KeyCode::Backspace, _) => {
                if self.cursor > 0 {
                    let prev = self.prev_char_boundary();
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                    InputAction::Changed
                } else {
                    InputAction::None
                }
            }

            // Delete
            (KeyCode::Delete, _) => {
                if self.cursor < self.buffer.len() {
                    let next = self.next_char_boundary();
                    self.buffer.drain(self.cursor..next);
                    InputAction::Changed
                } else {
                    InputAction::None
                }
            }

            // Arrow keys
            (KeyCode::Left, _) => {
                if self.cursor > 0 {
                    self.cursor = self.prev_char_boundary();
                }
                InputAction::None
            }
            (KeyCode::Right, _) => {
                if self.cursor < self.buffer.len() {
                    self.cursor = self.next_char_boundary();
                }
                InputAction::None
            }

            // History: Up/Down
            (KeyCode::Up, _) => {
                self.history_up();
                InputAction::Changed
            }
            (KeyCode::Down, _) => {
                self.history_down();
                InputAction::Changed
            }

            // Home/End
            (KeyCode::Home, _) => {
                self.cursor = 0;
                InputAction::None
            }
            (KeyCode::End, _) => {
                self.cursor = self.buffer.len();
                InputAction::None
            }

            _ => InputAction::None,
        }
    }

    /// Handle paste event from bracketed paste
    pub fn handle_paste(&mut self, text: String) {
        let line_count = text.lines().count();
        self.buffer.insert_str(self.cursor, &text);
        self.cursor += text.len();
        if line_count > 5 {
            self.mode = InputMode::Pasted(line_count);
        }
    }

    /// Number of display lines for Zone 2 height calculation
    pub fn display_lines(&self) -> usize {
        match &self.mode {
            InputMode::Typing => {
                let count = self.buffer.lines().count().max(1);
                // Account for trailing newline
                if self.buffer.ends_with('\n') { count + 1 } else { count }
            }
            InputMode::Pasted(total) => {
                if *total > 5 { 1 } else { *total }
            }
        }
    }

    /// Display text for rendering (may be collapsed for large pastes)
    pub fn display_text(&self) -> String {
        match &self.mode {
            InputMode::Typing => self.buffer.clone(),
            InputMode::Pasted(total) if *total > 5 => {
                format!("[Pasted {} lines]", total)
            }
            _ => self.buffer.clone(),
        }
    }

    fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.mode = InputMode::Typing;
        self.history_index = None;
    }

    fn prev_char_boundary(&self) -> usize {
        let mut pos = self.cursor - 1;
        while !self.buffer.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    fn next_char_boundary(&self) -> usize {
        let mut pos = self.cursor + 1;
        while pos < self.buffer.len() && !self.buffer.is_char_boundary(pos) {
            pos += 1;
        }
        pos
    }

    fn history_up(&mut self) {
        if self.history.is_empty() { return; }
        match self.history_index {
            None => {
                self.saved_buffer = Some(self.buffer.clone());
                self.history_index = Some(self.history.len() - 1);
            }
            Some(0) => return,
            Some(i) => {
                self.history_index = Some(i - 1);
            }
        }
        if let Some(i) = self.history_index {
            self.buffer = self.history[i].clone();
            self.cursor = self.buffer.len();
        }
    }

    fn history_down(&mut self) {
        match self.history_index {
            None => return,
            Some(i) if i >= self.history.len() - 1 => {
                self.history_index = None;
                if let Some(saved) = self.saved_buffer.take() {
                    self.buffer = saved;
                } else {
                    self.buffer.clear();
                }
                self.cursor = self.buffer.len();
            }
            Some(i) => {
                self.history_index = Some(i + 1);
                self.buffer = self.history[i + 1].clone();
                self.cursor = self.buffer.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, KeyEventKind, KeyEventState};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_basic_typing() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('h')));
        input.handle_key(key(KeyCode::Char('i')));
        assert_eq!(input.buffer, "hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_submit() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('h')));
        input.handle_key(key(KeyCode::Char('i')));
        let action = input.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, InputAction::Submit(s) if s == "hi"));
        assert!(input.buffer.is_empty());
    }

    #[test]
    fn test_backspace() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Char('b')));
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.buffer, "a");
    }

    #[test]
    fn test_shift_enter_newline() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key_mod(KeyCode::Enter, KeyModifiers::SHIFT));
        input.handle_key(key(KeyCode::Char('b')));
        assert_eq!(input.buffer, "a\nb");
    }

    #[test]
    fn test_paste_collapse() {
        let mut input = TuiInput::new();
        let long_text = (0..10).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        input.handle_paste(long_text);
        assert_eq!(input.mode, InputMode::Pasted(10));
        assert_eq!(input.display_lines(), 1);
        assert_eq!(input.display_text(), "[Pasted 10 lines]");
    }

    #[test]
    fn test_paste_short() {
        let mut input = TuiInput::new();
        input.handle_paste("line1\nline2\nline3".into());
        assert_eq!(input.mode, InputMode::Typing); // 3 lines, no collapse
        assert_eq!(input.display_lines(), 3);
    }

    #[test]
    fn test_history_navigation() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Enter));
        input.handle_key(key(KeyCode::Char('b')));
        input.handle_key(key(KeyCode::Enter));
        // Now navigate up
        input.handle_key(key(KeyCode::Up));
        assert_eq!(input.buffer, "b");
        input.handle_key(key(KeyCode::Up));
        assert_eq!(input.buffer, "a");
        // Navigate back down
        input.handle_key(key(KeyCode::Down));
        assert_eq!(input.buffer, "b");
        input.handle_key(key(KeyCode::Down));
        assert!(input.buffer.is_empty()); // back to empty
    }

    #[test]
    fn test_exit_ctrl_c() {
        let mut input = TuiInput::new();
        let action = input.handle_key(key_mod(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(matches!(action, InputAction::Exit));
    }
}
```

**Step 2: Add mod declaration in main.rs**

```rust
mod tui_input;
```

**Step 3: Run tests**

Run: `cd rust && cargo test -p rusty-claude-cli -- tui_input`
Expected: All 7 tests pass

**Step 4: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/tui_input.rs rust/crates/rusty-claude-cli/src/main.rs
git commit -m "feat(hud): add TuiInput handler replacing rustyline for raw mode"
```

---

## Phase 3: TUI App Shell — 3-Zone Layout

### Task 5: Create TuiApp struct and main event loop

**Files:**
- Create: `rust/crates/rusty-claude-cli/src/tui_app.rs`
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` (add `mod tui_app;`)

**Step 1: Write the TuiApp with 3-zone layout**

Create `rust/crates/rusty-claude-cli/src/tui_app.rs`:
```rust
use std::io;

use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Terminal,
};

use crate::hud::HudState;
use crate::hud_widget::HudFooter;
use crate::tui_input::{InputAction, TuiInput};

const MAX_CONTENT_LINES: usize = 10_000;
const TICK_RATE_MS: u64 = 33; // ~30fps

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
}

impl TuiApp {
    pub fn new(model_name: String, project_path: String) -> Self {
        Self {
            content_lines: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            input: TuiInput::new(),
            hud: HudState::new(model_name, project_path),
            should_quit: false,
        }
    }

    /// Append content lines to Zone 1 (from streaming output)
    pub fn push_content(&mut self, lines: Vec<Line<'static>>) {
        self.content_lines.extend(lines);
        // Trim if exceeding max
        if self.content_lines.len() > MAX_CONTENT_LINES {
            let drain_count = self.content_lines.len() - MAX_CONTENT_LINES;
            self.content_lines.drain(..drain_count);
            self.scroll_offset = self.scroll_offset.saturating_sub(drain_count);
        }
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    /// Push a single text line to content
    pub fn push_text(&mut self, text: String, style: Style) {
        self.content_lines.push(Line::styled(text, style));
        if self.auto_scroll {
            self.scroll_to_bottom();
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.content_lines.len().saturating_sub(1);
    }

    /// Render the 3-zone layout
    pub fn render(&self, frame: &mut Frame) {
        let input_height = self.input.display_lines() as u16 + 1; // +1 for prompt prefix line
        let hud_height: u16 = if frame.area().width < 60 { 1 } else { 2 };

        let chunks = Layout::vertical([
            Constraint::Min(3),                    // Zone 1: content (fill remaining)
            Constraint::Length(input_height),       // Zone 2: input (dynamic)
            Constraint::Length(hud_height),         // Zone 3: HUD footer (fixed)
        ])
        .split(frame.area());

        self.render_content(frame, chunks[0]);
        self.render_input(frame, chunks[1]);
        self.render_hud(frame, chunks[2]);
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let visible_height = area.height as usize;
        let total = self.content_lines.len();

        // Calculate visible window
        let start = if total > visible_height {
            self.scroll_offset.min(total - visible_height)
        } else {
            0
        };
        let end = (start + visible_height).min(total);

        let visible: Vec<Line> = self.content_lines[start..end].to_vec();
        let content = Paragraph::new(visible).wrap(Wrap { trim: false });
        frame.render_widget(content, area);

        // Scrollbar
        if total > visible_height {
            let mut scrollbar_state =
                ScrollbarState::new(total).position(start);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut scrollbar_state,
            );
        }
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let display = self.input.display_text();
        let prompt_text = format!("> {display}");
        let input_widget = Paragraph::new(prompt_text)
            .style(Style::default().fg(Color::White));
        frame.render_widget(input_widget, area);

        // Show cursor position
        if area.height > 0 {
            let cursor_x = area.x + 2 + self.visible_cursor_x();
            let cursor_y = area.y + self.visible_cursor_y();
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    fn render_hud(&self, frame: &mut Frame, area: Rect) {
        let footer = HudFooter::new(&self.hud);
        frame.render_widget(footer, area);
    }

    /// Calculate cursor X position in the visible input area
    fn visible_cursor_x(&self) -> u16 {
        let text_before_cursor = &self.input.buffer[..self.input.cursor];
        let last_line = text_before_cursor.rsplit('\n').next().unwrap_or(text_before_cursor);
        last_line.len() as u16
    }

    /// Calculate cursor Y position (which line the cursor is on)
    fn visible_cursor_y(&self) -> u16 {
        let text_before_cursor = &self.input.buffer[..self.input.cursor];
        text_before_cursor.matches('\n').count() as u16
    }

    /// Handle scroll events (mouse wheel or Page Up/Down)
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset += lines;
        let max = self.content_lines.len().saturating_sub(1);
        if self.scroll_offset >= max {
            self.scroll_offset = max;
            self.auto_scroll = true;
        }
    }
}

/// Initialize terminal for ratatui (alternate screen + raw mode + bracketed paste)
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore terminal to normal state
pub fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableBracketedPaste)?;
    Ok(())
}
```

**Step 2: Add mod declaration**

```rust
mod tui_app;
```

**Step 3: Verify compilation**

Run: `cd rust && cargo check -p rusty-claude-cli`
Expected: compiles successfully

**Step 4: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/tui_app.rs rust/crates/rusty-claude-cli/src/main.rs
git commit -m "feat(hud): add TuiApp with 3-zone ratatui layout"
```

---

## Phase 4: Adapt Rendering Pipeline

### Task 6: Adapt TerminalRenderer to produce ratatui Lines

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/render.rs`

This is the most complex task. The current `TerminalRenderer` writes ANSI escape codes as strings. We need a new method that produces `Vec<ratatui::text::Line<'static>>` instead.

**Step 1: Add ratatui conversion method to TerminalRenderer**

Add to `render.rs` after existing methods:
```rust
use ratatui::prelude::*;

impl TerminalRenderer {
    /// Convert markdown text to ratatui Lines (instead of ANSI strings)
    pub fn render_markdown_to_lines(&self, text: &str) -> Vec<Line<'static>> {
        let parser = pulldown_cmark::Parser::new(text);
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut style_stack: Vec<Style> = vec![Style::default()];

        for event in parser {
            match event {
                pulldown_cmark::Event::Start(tag) => {
                    let style = match &tag {
                        pulldown_cmark::Tag::Heading { .. } => {
                            Style::default().fg(self.ratatui_color(self.color_theme.heading)).bold()
                        }
                        pulldown_cmark::Tag::Emphasis => {
                            Style::default().fg(self.ratatui_color(self.color_theme.emphasis)).italic()
                        }
                        pulldown_cmark::Tag::Strong => {
                            Style::default().fg(self.ratatui_color(self.color_theme.strong)).bold()
                        }
                        pulldown_cmark::Tag::Link { .. } => {
                            Style::default().fg(self.ratatui_color(self.color_theme.link)).underlined()
                        }
                        pulldown_cmark::Tag::BlockQuote(_) => {
                            Style::default().fg(self.ratatui_color(self.color_theme.quote))
                        }
                        pulldown_cmark::Tag::CodeBlock(_) => {
                            Style::default().fg(self.ratatui_color(self.color_theme.inline_code))
                        }
                        _ => *style_stack.last().unwrap_or(&Style::default()),
                    };
                    style_stack.push(style);
                }
                pulldown_cmark::Event::End(_) => {
                    style_stack.pop();
                    // End of block-level element → flush line
                    if !current_spans.is_empty() {
                        lines.push(Line::from(current_spans.drain(..).collect::<Vec<_>>()));
                    }
                }
                pulldown_cmark::Event::Text(text) => {
                    let style = *style_stack.last().unwrap_or(&Style::default());
                    for (i, line_text) in text.split('\n').enumerate() {
                        if i > 0 {
                            lines.push(Line::from(current_spans.drain(..).collect::<Vec<_>>()));
                        }
                        if !line_text.is_empty() {
                            current_spans.push(Span::styled(line_text.to_string(), style));
                        }
                    }
                }
                pulldown_cmark::Event::Code(code) => {
                    let style = Style::default().fg(self.ratatui_color(self.color_theme.inline_code));
                    current_spans.push(Span::styled(format!("`{code}`"), style));
                }
                pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                    lines.push(Line::from(current_spans.drain(..).collect::<Vec<_>>()));
                }
                _ => {}
            }
        }
        // Flush remaining spans
        if !current_spans.is_empty() {
            lines.push(Line::from(current_spans));
        }

        if lines.is_empty() {
            lines.push(Line::raw(text.to_string()));
        }

        lines
    }

    /// Convert crossterm Color to ratatui Color (they use same enum)
    fn ratatui_color(&self, color: crossterm::style::Color) -> Color {
        match color {
            crossterm::style::Color::Black => Color::Black,
            crossterm::style::Color::Red => Color::Red,
            crossterm::style::Color::Green => Color::Green,
            crossterm::style::Color::Yellow => Color::Yellow,
            crossterm::style::Color::Blue => Color::Blue,
            crossterm::style::Color::Magenta => Color::Magenta,
            crossterm::style::Color::Cyan => Color::Cyan,
            crossterm::style::Color::White => Color::White,
            crossterm::style::Color::DarkGrey => Color::DarkGray,
            crossterm::style::Color::DarkCyan => Color::Rgb(0, 139, 139),
            crossterm::style::Color::Rgb { r, g, b } => Color::Rgb(r, g, b),
            crossterm::style::Color::AnsiValue(v) => Color::Indexed(v),
            _ => Color::White,
        }
    }
}
```

**Note:** ratatui re-exports crossterm's `Color` type, so the conversion may simplify to a direct cast. Verify at compile time — if `ratatui::style::Color` and `crossterm::style::Color` are the same type, remove the conversion function.

**Step 2: Add method to format tool calls as ratatui Lines**

Add to render.rs or a new section:
```rust
impl TerminalRenderer {
    /// Format a tool call start as ratatui Lines
    pub fn format_tool_start_lines(&self, tool_name: &str, input: &str) -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
                Span::styled("  ╭─ ", Style::default().fg(Color::DarkGray)),
                Span::styled(tool_name.to_string(), Style::default().fg(Color::Cyan).bold()),
                Span::styled(" ─╮", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::raw(truncate_str(input, 120)),
            ]),
        ]
    }

    /// Format a tool result as ratatui Lines
    pub fn format_tool_result_lines(&self, tool_name: &str, output: &str, is_error: bool) -> Vec<Line<'static>> {
        let (icon, color) = if is_error { ("✗", Color::Red) } else { ("✓", Color::Green) };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!("  {icon} "), Style::default().fg(color)),
                Span::styled(tool_name.to_string(), Style::default().fg(Color::DarkGray)),
            ]),
        ];
        // Add truncated output
        for line in output.lines().take(20) {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::raw(truncate_str(line, 160)),
            ]));
        }
        lines.push(Line::from(Span::styled("  ╰───╯", Style::default().fg(Color::DarkGray))));
        lines
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}
```

**Step 3: Run existing tests + check**

Run: `cd rust && cargo test -p rusty-claude-cli && cargo check -p rusty-claude-cli`
Expected: All existing tests pass, no compile errors

**Step 4: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/render.rs
git commit -m "feat(hud): adapt TerminalRenderer to produce ratatui Lines"
```

---

## Phase 5: Wire Data Sources

### Task 7: Add token tracking to runtime → HudState

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs` — update `consume_stream()` to push token usage to HudState

**Step 1: In consume_stream(), capture token usage events**

Find the `MessageDelta` handler in `consume_stream()` (around line 7613). Currently it records usage into events. Add a callback/channel to also update HudState:

The approach: add an `Arc<Mutex<HudState>>` parameter to `consume_stream()`, or use a `tokio::sync::mpsc` channel to send HUD updates from the stream consumer to the TUI event loop.

**Channel-based approach (recommended):**

Define event enum in `tui_app.rs`:
```rust
pub enum HudEvent {
    TokenUpdate { used: u64, max: u64 },
    ToolStart(String),
    ToolEnd(String),
    AgentStart { agent_type: String, description: String },
    AgentEnd(String),
    TodoUpdate { done: u32, total: u32, current: Option<String> },
    TurnStart,
    TurnEnd,
}
```

In `consume_stream()`, send events through an `mpsc::UnboundedSender<HudEvent>`.

In the TUI event loop, `tokio::select!` on the HudEvent receiver alongside keyboard events and API stream.

**Step 2: Wire tool start/end callbacks**

In main.rs, `format_tool_call_start()` and `format_tool_result()` are called when tools execute. At these call sites, also send `HudEvent::ToolStart` / `HudEvent::ToolEnd`.

**Step 3: Wire agent tracking**

The `execute_agent()` function (around line 3286) spawns agents. Send `HudEvent::AgentStart` when spawning and `HudEvent::AgentEnd` when the agent thread completes.

**Step 4: Wire todo tracking**

The `TaskRegistry` in runtime crate tracks tasks. Add a callback or channel when tasks are created/updated to send `HudEvent::TodoUpdate`.

**Step 5: Commit**

```bash
git commit -am "feat(hud): wire token, tool, agent, todo tracking to HudState"
```

---

### Task 8: Wire git info at startup

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/hud.rs`

**Step 1: Add git info fetcher**

```rust
impl HudState {
    /// Fetch git branch and dirty status
    pub fn refresh_git(&mut self) {
        // Get branch
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
        {
            if output.status.success() {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                self.git_branch = Some(branch);
            }
        }
        // Get dirty status
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
        {
            self.git_dirty = output.status.success() && !output.stdout.is_empty();
        }
        self.dirty = true;
    }
}
```

**Step 2: Call at app startup and each turn**

In the TUI event loop initialization:
```rust
app.hud.refresh_git();
```

**Step 3: Test and commit**

Run: `cd rust && cargo test -p rusty-claude-cli`

```bash
git commit -am "feat(hud): add git branch and dirty status to HudState"
```

---

## Phase 6: Integration — Replace REPL Loop

### Task 9: Create TUI REPL mode alongside existing REPL

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

This is the highest-risk task. Strategy: **keep the old `run_repl()` intact**, create a new `run_tui_repl()` that uses the ratatui TUI. Gate behind a `--tui` flag initially so both modes work.

**Step 1: Add --tui flag to argument parser**

In `parse_args()` (around line 399), add:
```rust
"--tui" => { tui_mode = true; }
```

**Step 2: Create run_tui_repl()**

New function that:
1. Calls `tui_app::init_terminal()`
2. Creates `TuiApp::new(model, path)`
3. Initializes HudState with git info, model name
4. Prints startup banner into Zone 1 content
5. Enters the main event loop:

```rust
async fn run_tui_repl(cli: LiveCli) -> Result<()> {
    let mut terminal = tui_app::init_terminal()?;
    let mut app = TuiApp::new(cli.model.clone(), current_dir_string());

    // Setup
    app.hud.refresh_git();
    app.push_text(format!("🐙 ACE CLI — {}", cli.model), Style::default().fg(Color::Cyan));

    // Create HUD event channel
    let (hud_tx, mut hud_rx) = tokio::sync::mpsc::unbounded_channel();

    // Event loop
    let tick_rate = std::time::Duration::from_millis(TICK_RATE_MS);

    loop {
        // Draw
        terminal.draw(|frame| app.render(frame))?;

        // Wait for events
        tokio::select! {
            // Keyboard / terminal events
            _ = async {
                if crossterm::event::poll(tick_rate).unwrap_or(false) {
                    if let Ok(event) = crossterm::event::read() {
                        match event {
                            Event::Key(key) if key.kind == KeyEventKind::Press => {
                                match app.input.handle_key(key) {
                                    InputAction::Submit(text) => {
                                        // Process the submitted text
                                        app.push_text(format!("> {text}"), Style::default().fg(Color::White).bold());
                                        app.hud.start_turn();

                                        // Handle slash commands or run turn
                                        if text.starts_with('/') {
                                            handle_slash_command(&text, &mut app, &cli);
                                        } else {
                                            // Run API turn (spawn task, stream results)
                                            run_turn_tui(&text, &mut app, &cli, hud_tx.clone()).await;
                                        }
                                    }
                                    InputAction::Exit => {
                                        app.should_quit = true;
                                    }
                                    _ => {}
                                }
                            }
                            Event::Paste(text) => {
                                app.input.handle_paste(text);
                            }
                            Event::Mouse(mouse) => {
                                // Handle scroll
                                match mouse.kind {
                                    crossterm::event::MouseEventKind::ScrollUp => app.scroll_up(3),
                                    crossterm::event::MouseEventKind::ScrollDown => app.scroll_down(3),
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
            } => {}

            // HUD state updates
            Some(event) = hud_rx.recv() => {
                match event {
                    HudEvent::TokenUpdate { used, max } => app.hud.update_tokens(used, max),
                    HudEvent::ToolStart(name) => app.hud.tool_start(name),
                    HudEvent::ToolEnd(_) => app.hud.tool_end(),
                    HudEvent::TurnStart => app.hud.start_turn(),
                    HudEvent::TurnEnd => { app.hud.turn_start = None; }
                    HudEvent::TodoUpdate { done, total, current } => {
                        app.hud.todos_done = done;
                        app.hud.todos_total = total;
                        app.hud.current_task = current;
                        app.hud.dirty = true;
                    }
                    HudEvent::AgentStart { agent_type, description } => {
                        app.hud.agents.push(AgentStatus {
                            agent_type, description,
                            started_at: Instant::now(),
                            completed: false,
                        });
                        app.hud.dirty = true;
                    }
                    HudEvent::AgentEnd(agent_type) => {
                        if let Some(a) = app.hud.agents.iter_mut().find(|a| a.agent_type == agent_type && !a.completed) {
                            a.completed = true;
                        }
                        app.hud.dirty = true;
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    tui_app::restore_terminal()?;
    Ok(())
}
```

**Step 3: Route --tui flag**

In `run()` dispatch, when `CliAction::Repl` and `tui_mode == true`:
```rust
CliAction::Repl if tui_mode => run_tui_repl(cli).await?,
CliAction::Repl => run_repl(cli).await?, // existing
```

**Step 4: Test manually**

Run: `cd rust && cargo run --bin ace -- --tui`
Expected: Terminal switches to alternate screen, shows 3-zone layout with empty content, input prompt, and HUD footer with project path and model name

**Step 5: Commit**

```bash
git commit -am "feat(hud): add --tui flag with ratatui 3-zone REPL mode"
```

---

### Task 10: Wire streaming output to TUI content area

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: Create TUI-aware stream consumer**

Adapt `consume_stream()` to optionally write to `TuiApp` instead of stdout. Two approaches:

**Option A (recommended):** Create `consume_stream_tui()` that sends content lines through a channel:
```rust
async fn consume_stream_tui(
    stream: ApiStream,
    content_tx: mpsc::UnboundedSender<Vec<Line<'static>>>,
    hud_tx: mpsc::UnboundedSender<HudEvent>,
    renderer: &TerminalRenderer,
    show_thinking: bool,
) -> Result<()> {
    let mut markdown_stream = MarkdownStreamState::default();

    while let Some(event) = stream.next().await {
        match event {
            ApiStreamEvent::ContentBlockDelta(delta) => match delta {
                TextDelta(text) => {
                    if let Some(rendered) = markdown_stream.push(&text, renderer) {
                        let lines = renderer.render_markdown_to_lines(&rendered);
                        content_tx.send(lines)?;
                    }
                }
                ThinkingDelta(text) if show_thinking => {
                    let line = Line::styled(text, Style::default().fg(Color::DarkGray));
                    content_tx.send(vec![line])?;
                }
                _ => {}
            },
            ApiStreamEvent::MessageDelta(delta) => {
                if let Some(usage) = delta.usage {
                    let tu = usage.token_usage();
                    hud_tx.send(HudEvent::TokenUpdate {
                        used: tu.input_tokens as u64 + tu.output_tokens as u64,
                        max: 200_000, // from model config
                    })?;
                }
            }
            ApiStreamEvent::ContentBlockStop => {
                if let Some(remaining) = markdown_stream.flush(renderer) {
                    let lines = renderer.render_markdown_to_lines(&remaining);
                    content_tx.send(lines)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
```

**Step 2: In the TUI event loop, receive content lines**

Add a `content_rx` channel to `tokio::select!`:
```rust
Some(lines) = content_rx.recv() => {
    app.push_content(lines);
}
```

**Step 3: Test with a real API call**

Run: `cd rust && cargo run --bin ace -- --tui`
Type a question, verify streaming output appears in Zone 1

**Step 4: Commit**

```bash
git commit -am "feat(hud): wire streaming API output to TUI content area"
```

---

## Phase 7: Polish & Edge Cases

### Task 11: Handle terminal resize and graceful exit

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/tui_app.rs`
- Modify: `rust/crates/rusty-claude-cli/src/main.rs`

**Step 1: Add panic hook for terminal restore**

In `run_tui_repl()`, before the event loop:
```rust
let original_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |panic_info| {
    let _ = restore_terminal();
    original_hook(panic_info);
}));
```

**Step 2: Handle Ctrl+C signal**

Add tokio signal handler:
```rust
tokio::select! {
    // ... existing branches ...
    _ = tokio::signal::ctrl_c() => {
        app.should_quit = true;
    }
}
```

**Step 3: Add responsive HUD collapse logic**

Already handled in `HudFooter::render()` — when `area.height < 2`, renders single line. Test by resizing terminal.

**Step 4: Commit**

```bash
git commit -am "fix(hud): add panic recovery and signal handling for graceful exit"
```

---

### Task 12: Add scroll keybindings to content area

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/tui_app.rs`

**Step 1: Add Page Up/Down handling**

In the key event handler, before passing to `app.input.handle_key()`:
```rust
(KeyCode::PageUp, _) => { app.scroll_up(area_height / 2); }
(KeyCode::PageDown, _) => { app.scroll_down(area_height / 2); }
```

**Step 2: Commit**

```bash
git commit -am "feat(hud): add PageUp/PageDown scrolling for content area"
```

---

### Task 13: Final integration test

**Step 1: Run full test suite**

Run: `cd rust && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: All pass

**Step 2: Manual smoke test**

Run: `cd rust && cargo run --bin ace -- --tui`
Verify:
- [ ] 3-zone layout renders correctly
- [ ] Input accepts typing with Shift+Enter for newlines
- [ ] Paste > 5 lines shows "[Pasted XX lines]"
- [ ] HUD shows project path + git branch
- [ ] HUD shows model name
- [ ] Context health bar updates during conversation
- [ ] Tool activity shows during tool execution
- [ ] Scrolling works (PageUp/PageDown, mouse wheel)
- [ ] Ctrl+C exits cleanly (terminal restored)
- [ ] Terminal resize doesn't break layout
- [ ] `/exit` works

**Step 3: Commit any fixes**

```bash
git commit -am "test: smoke test fixes for TUI mode"
```

---

## Summary

| Phase | Tasks | Key Risk |
|-------|-------|----------|
| 1. Foundation | Tasks 1-3 | Low — new files only |
| 2. Input | Task 4 | Medium — rustyline replacement |
| 3. App Shell | Task 5 | Medium — layout integration |
| 4. Rendering | Task 6 | High — markdown pipeline adaptation |
| 5. Data Wiring | Tasks 7-8 | Medium — channel plumbing |
| 6. Integration | Tasks 9-10 | High — REPL replacement |
| 7. Polish | Tasks 11-13 | Low — edge cases |

**Total: 13 tasks across 7 phases**

**Critical path:** Task 6 (render adaptation) and Task 9 (REPL integration) are highest risk. The `--tui` flag approach mitigates risk by keeping the old REPL working.
