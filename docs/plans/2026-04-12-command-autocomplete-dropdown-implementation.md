# Command Autocomplete Dropdown — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an inline autocomplete dropdown below the TUI input box that shows filtered slash commands with descriptions when the user types `/`.

**Architecture:** New `CommandDropdown` struct owns state (candidates, filtered indices, selection). New `dropdown.rs` widget renders it. `TuiApp` owns the dropdown, conditionally adds a 4th layout zone between input and HUD. `TuiInput::handle_key()` routes arrow/enter/esc to dropdown when open. Event loop calls `dropdown.update_filter()` on every `InputAction::Changed`.

**Tech Stack:** Rust, ratatui (Widget trait, Layout, Span, Style, Block), crossterm (key events). All code in `rust/crates/rusty-claude-cli/src/tui/`.

---

### Task 1: CommandDropdown data model + unit tests

**Files:**
- Create: `rust/crates/rusty-claude-cli/src/tui/dropdown.rs`
- Modify: `rust/crates/rusty-claude-cli/src/tui/mod.rs:1-11` (add `pub(crate) mod dropdown;`)

**Step 1: Write the failing tests**

Create `dropdown.rs` with test module first:

```rust
/// Command autocomplete dropdown state.
pub struct CommandDropdown {
    /// All candidates: (command_name, summary).
    candidates: Vec<(String, String)>,
    /// Indices into `candidates` matching current filter.
    filtered: Vec<usize>,
    /// Selected index within `filtered`.
    selected: usize,
    /// Whether the dropdown is visible.
    open: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dropdown() -> CommandDropdown {
        CommandDropdown::new(vec![
            ("help".into(), "Show help".into()),
            ("model".into(), "Change AI model".into()),
            ("model-info".into(), "Show model details".into()),
            ("memory".into(), "Show memory".into()),
            ("compact".into(), "Compact conversation".into()),
        ])
    }

    #[test]
    fn new_dropdown_starts_closed() {
        let dd = sample_dropdown();
        assert!(!dd.is_open());
        assert_eq!(dd.visible_items().len(), 0);
    }

    #[test]
    fn update_filter_opens_on_slash() {
        let mut dd = sample_dropdown();
        dd.update_filter("/");
        assert!(dd.is_open());
        assert_eq!(dd.visible_items().len(), 5); // all match empty prefix
    }

    #[test]
    fn update_filter_narrows_results() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo");
        assert!(dd.is_open());
        assert_eq!(dd.visible_items().len(), 2); // model, model-info
        assert_eq!(dd.visible_items()[0].0, "model");
        assert_eq!(dd.visible_items()[1].0, "model-info");
    }

    #[test]
    fn update_filter_closes_on_no_match() {
        let mut dd = sample_dropdown();
        dd.update_filter("/zzz");
        assert!(!dd.is_open());
    }

    #[test]
    fn update_filter_closes_without_slash() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo");
        assert!(dd.is_open());
        dd.update_filter("hello");
        assert!(!dd.is_open());
    }

    #[test]
    fn move_down_wraps_around() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo"); // 2 items
        assert_eq!(dd.selected_index(), 0);
        dd.move_down();
        assert_eq!(dd.selected_index(), 1);
        dd.move_down(); // wrap
        assert_eq!(dd.selected_index(), 0);
    }

    #[test]
    fn move_up_wraps_around() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo"); // 2 items
        assert_eq!(dd.selected_index(), 0);
        dd.move_up(); // wrap to last
        assert_eq!(dd.selected_index(), 1);
    }

    #[test]
    fn select_returns_command_name() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo");
        dd.move_down(); // select "model-info"
        let selected = dd.confirm_selection();
        assert_eq!(selected, Some("/model-info".to_string()));
        assert!(!dd.is_open()); // closes after selection
    }

    #[test]
    fn dismiss_closes_without_selection() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo");
        assert!(dd.is_open());
        dd.dismiss();
        assert!(!dd.is_open());
    }

    #[test]
    fn selected_resets_on_filter_change() {
        let mut dd = sample_dropdown();
        dd.update_filter("/mo");
        dd.move_down(); // selected = 1
        dd.update_filter("/mod"); // filter changed, reset to 0
        assert_eq!(dd.selected_index(), 0);
    }

    #[test]
    fn max_visible_caps_at_8() {
        let many: Vec<(String, String)> = (0..20)
            .map(|i| (format!("cmd{i}"), format!("Summary {i}")))
            .collect();
        let mut dd = CommandDropdown::new(many);
        dd.update_filter("/");
        assert_eq!(dd.visible_items().len(), 8); // capped
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd rust && cargo test -p rusty-claude-cli dropdown -- --nocapture 2>&1`
Expected: compile error — `CommandDropdown::new`, `is_open`, etc. not defined yet.

**Step 3: Implement CommandDropdown**

Add implementation above the test module in `dropdown.rs`:

```rust
const MAX_VISIBLE: usize = 8;

impl CommandDropdown {
    pub fn new(candidates: Vec<(String, String)>) -> Self {
        Self {
            candidates,
            filtered: Vec::new(),
            selected: 0,
            open: false,
        }
    }

    /// Recalculate filtered list from the current input text.
    /// Opens the dropdown if input starts with `/` and there are matches.
    pub fn update_filter(&mut self, input: &str) {
        if !input.starts_with('/') {
            self.open = false;
            self.filtered.clear();
            return;
        }
        let prefix = &input[1..]; // text after '/'
        self.filtered = self
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name.starts_with(prefix))
            .map(|(i, _)| i)
            .collect();
        self.selected = 0;
        self.open = !self.filtered.is_empty();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Return up to MAX_VISIBLE items for rendering.
    /// Each item is (name, summary, is_selected).
    pub fn visible_items(&self) -> Vec<(&str, &str, bool)> {
        if !self.open {
            return Vec::new();
        }
        // Window around selected item
        let total = self.filtered.len();
        let visible_count = total.min(MAX_VISIBLE);
        let start = if total <= MAX_VISIBLE {
            0
        } else if self.selected < MAX_VISIBLE / 2 {
            0
        } else if self.selected >= total - MAX_VISIBLE / 2 {
            total - MAX_VISIBLE
        } else {
            self.selected - MAX_VISIBLE / 2
        };

        self.filtered[start..start + visible_count]
            .iter()
            .enumerate()
            .map(|(vi, &ci)| {
                let (name, summary) = &self.candidates[ci];
                (name.as_str(), summary.as_str(), start + vi == self.selected)
            })
            .collect()
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn move_up(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = if self.selected == 0 {
                self.filtered.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Confirm the current selection. Returns `Some("/command_name")`
    /// and closes the dropdown.
    pub fn confirm_selection(&mut self) -> Option<String> {
        if !self.open || self.filtered.is_empty() {
            return None;
        }
        let idx = self.filtered[self.selected];
        let name = self.candidates[idx].0.clone();
        self.open = false;
        Some(format!("/{name}"))
    }

    /// Close the dropdown without selecting.
    pub fn dismiss(&mut self) {
        self.open = false;
    }

    /// Height needed for rendering (items + 2 for border).
    pub fn render_height(&self) -> u16 {
        if !self.open {
            return 0;
        }
        let items = self.filtered.len().min(MAX_VISIBLE);
        items as u16 + 2 // +2 for top/bottom border
    }
}
```

**Step 4: Register the module**

In `mod.rs`, add:
```rust
pub(crate) mod dropdown;
```

**Step 5: Run tests to verify they pass**

Run: `cd rust && cargo test -p rusty-claude-cli dropdown -- --nocapture 2>&1`
Expected: all 10 tests PASS.

**Step 6: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/tui/dropdown.rs rust/crates/rusty-claude-cli/src/tui/mod.rs
git commit -m "feat(tui): add CommandDropdown data model with tests"
```

---

### Task 2: CommandDropdown widget rendering

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/tui/dropdown.rs` (add render function)

**Step 1: Write the rendering function**

Add this function at the bottom of `dropdown.rs` (before `#[cfg(test)]`):

```rust
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Widget},
};
use crate::tui::theme::ClaudeTheme;

/// Render the command dropdown into the given area.
/// Called by TuiApp::render() when dropdown.is_open().
pub fn render_dropdown(dropdown: &CommandDropdown, frame: &mut Frame, area: Rect) {
    let theme = ClaudeTheme::default();
    let items = dropdown.visible_items();
    if items.is_empty() {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(theme.text_muted));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    for (i, (name, summary, is_selected)) in items.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let row_area = Rect {
            x: inner.x,
            y: inner.y + i as u16,
            width: inner.width,
            height: 1,
        };

        let prefix = if *is_selected { " ▸ " } else { "   " };
        let name_style = if *is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan)
        };
        let summary_style = Style::default().fg(theme.text_muted);

        // Pad command name to fixed width for alignment
        let name_col = format!("/{name}");
        let padded_name = format!("{name_col:<16}");

        let line = Line::from(vec![
            Span::styled(prefix, name_style),
            Span::styled(padded_name, name_style),
            Span::styled(*summary, summary_style),
        ]);

        frame.render_widget(ratatui::widgets::Paragraph::new(line), row_area);
    }
}
```

**Step 2: Verify it compiles**

Run: `cd rust && cargo check -p rusty-claude-cli 2>&1`
Expected: no errors.

**Step 3: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/tui/dropdown.rs
git commit -m "feat(tui): add dropdown rendering function"
```

---

### Task 3: Integrate dropdown into TuiApp layout

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/tui/app.rs:30-46` (add dropdown field)
- Modify: `rust/crates/rusty-claude-cli/src/tui/app.rs:50-60` (constructor)
- Modify: `rust/crates/rusty-claude-cli/src/tui/app.rs:92-106` (render method)

**Step 1: Add dropdown field to TuiApp**

In `app.rs`, add import at top:
```rust
use crate::tui::dropdown::{CommandDropdown, render_dropdown};
```

Add field to `TuiApp` struct (after `pub input: TuiInput`):
```rust
    // Command autocomplete dropdown (between input and HUD)
    pub dropdown: CommandDropdown,
```

Update constructor `new()` to accept candidates and initialize dropdown:
```rust
    pub fn new(model_name: String, project_path: String, dropdown_candidates: Vec<(String, String)>) -> Self {
        Self {
            content_lines: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            input: TuiInput::new(),
            dropdown: CommandDropdown::new(dropdown_candidates),
            hud: HudState::new(model_name, project_path),
            should_quit: false,
            mode: TuiMode::Input,
        }
    }
```

**Step 2: Update render() for conditional 4-zone layout**

Replace the `render()` method body:

```rust
    pub fn render(&self, frame: &mut Frame) {
        let input_height = self.input.display_lines() as u16 + 2; // +2 for border
        let hud_height: u16 = 1;
        let dropdown_height = self.dropdown.render_height();

        if dropdown_height > 0 {
            // 4-zone layout: content, input, dropdown, HUD
            let chunks = Layout::vertical([
                Constraint::Min(3),                    // Zone 1: Content
                Constraint::Length(input_height),       // Zone 2: Input
                Constraint::Length(dropdown_height),    // Zone 2.5: Dropdown
                Constraint::Length(hud_height),         // Zone 3: HUD
            ])
            .split(frame.area());

            self.render_content(frame, chunks[0]);
            self.render_input(frame, chunks[1]);
            render_dropdown(&self.dropdown, frame, chunks[2]);
            self.render_hud(frame, chunks[3]);
        } else {
            // Standard 3-zone layout
            let chunks = Layout::vertical([
                Constraint::Min(3),
                Constraint::Length(input_height),
                Constraint::Length(hud_height),
            ])
            .split(frame.area());

            self.render_content(frame, chunks[0]);
            self.render_input(frame, chunks[1]);
            self.render_hud(frame, chunks[2]);
        }
    }
```

**Step 3: Fix all call sites of TuiApp::new()**

In `event.rs`, where `TuiApp::new` is called (~line 107), update to pass dropdown candidates:

```rust
    // Build dropdown candidates from slash command specs
    let dropdown_candidates: Vec<(String, String)> = commands::slash_command_specs()
        .iter()
        .map(|spec| (spec.name.to_string(), spec.summary.to_string()))
        .collect();

    let mut app = TuiApp::new(cli.model.clone(), cwd, dropdown_candidates);
```

Fix test call sites in `app.rs` tests — update `TuiApp::new` calls to pass empty vec:
```rust
TuiApp::new("claude-3".to_string(), "/tmp/project".to_string(), vec![])
```

**Step 4: Verify it compiles and tests pass**

Run: `cd rust && cargo test -p rusty-claude-cli -- --nocapture 2>&1`
Expected: all tests PASS.

**Step 5: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/tui/app.rs rust/crates/rusty-claude-cli/src/tui/event.rs
git commit -m "feat(tui): integrate dropdown into TuiApp layout"
```

---

### Task 4: Wire dropdown into input handling + event loop

**Files:**
- Modify: `rust/crates/rusty-claude-cli/src/tui/event.rs` (event loop)

**Step 1: Add dropdown update on InputAction::Changed**

In the event loop in `event.rs`, find the `InputAction::Changed` arm (~line 322) and add dropdown filter update:

```rust
InputAction::Changed | InputAction::None => {
    // Update dropdown filter when input changes
    if matches!(action, InputAction::Changed) {
        app.dropdown.update_filter(&app.input.buffer);
    }
}
```

**Step 2: Route arrow/enter/esc to dropdown when open**

In the key handling section, BEFORE the existing `app.input.handle_key(key)` call (~line 157), add dropdown intercept:

```rust
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
        KeyCode::Enter => {
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
        KeyCode::Tab | KeyCode::BackTab => {
            // Let tab completion work normally but also update dropdown
            // Fall through to input.handle_key()
        }
        _ => {
            // All other keys (typing, backspace, etc.) fall through
            // to input.handle_key(), then dropdown updates via Changed
        }
    }
}
```

**Step 3: Also update filter after slash command submit**

After `InputAction::Submit` clears the buffer, the dropdown should close. This happens naturally because `update_filter("")` will close it. But we should explicitly call it after submit:

Find the Submit handler and add after `app.input` is cleared:
```rust
// Close dropdown on submit
app.dropdown.dismiss();
```

**Step 4: Verify it compiles**

Run: `cd rust && cargo check -p rusty-claude-cli 2>&1`
Expected: no errors.

**Step 5: Commit**

```bash
git add rust/crates/rusty-claude-cli/src/tui/event.rs
git commit -m "feat(tui): wire dropdown navigation into event loop"
```

---

### Task 5: Manual verification + polish

**Files:**
- May modify: `rust/crates/rusty-claude-cli/src/tui/dropdown.rs` (visual polish)

**Step 1: Build the binary**

Run: `cd rust && cargo build 2>&1`
Expected: builds successfully.

**Step 2: Run all tests**

Run: `cd rust && cargo test --workspace 2>&1`
Expected: all tests pass.

**Step 3: Run clippy**

Run: `cd rust && cargo clippy -p rusty-claude-cli --lib -- -D warnings 2>&1`
Expected: no new warnings from our code.

**Step 4: Run fmt**

Run: `cd rust && cargo fmt 2>&1`

**Step 5: Commit any polish changes**

```bash
git add -A
git commit -m "chore(tui): polish dropdown formatting and clippy fixes"
```

---

## File Change Summary

| File | Action | What |
|------|--------|------|
| `tui/dropdown.rs` | CREATE | CommandDropdown struct + render function + 10 tests |
| `tui/mod.rs` | MODIFY | Add `pub(crate) mod dropdown;` |
| `tui/app.rs` | MODIFY | Add dropdown field, update constructor, conditional 4-zone layout |
| `tui/event.rs` | MODIFY | Dropdown intercept for keys, update filter on Changed |
