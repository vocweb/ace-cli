# HUD Footer Design — ACE CLI

**Date:** 2026-04-10
**Status:** Approved
**Reference:** [claude-hud](https://github.com/jarrodwatts/claude-hud)

## Overview

Add a persistent HUD footer to ACE CLI using ratatui, transforming the terminal into a 3-zone layout with real-time status information. Inspired by claude-hud's features: project path, context health, tool activity, agent tracking, todo progress, and colored text.

## Layout — 3 Zones

```
┌─────────────────────────────────────────────────┐
│              ZONE 1: Main Content               │
│         (scrollable, streaming output)          │
│                                                 │
│  🐙 Here is the answer to your question...     │
│  ```rust                                        │
│  fn main() { println!("hello"); }               │
│  ```                                            │
│  ✓ Read src/main.rs (200 lines)                │
│                                                 │
├─────────────────────────────────────────────────┤
│  ZONE 2: Input Prompt                           │
│  > Write a function that_                       │
├─────────────────────────────────────────────────┤
│ 📁 ~/Projects/app (main*) ║ ▰▰▱▱ 45% 90k/200k ║ ⏱ 3.2s                          │
│ 🧠 claude-opus-4-6 ║ 🔧 ◐ Bash running Read×3 ║ 📋 ▸2/5 "Fix bug" ║ 🤖 Explorer │
└─────────────────────────────────────────────────┘
```

### Zone 1 — Main Content Area
- Chiếm phần lớn terminal height (dynamic)
- Auto-scroll khi có output mới
- User có thể scroll up xem history
- Streaming markdown output, tool results, thinking display

### Zone 2 — Input Prompt
- **Direct typing**: Tự expand theo số dòng user gõ (không giới hạn)
- **Paste detection**: Nếu paste > 5 dòng → collapse thành `[Pasted XX lines]`, giữ full content internally
- Zone 1 co lại tương ứng khi Zone 2 expand

### Zone 3 — HUD Footer (2 dòng, fixed)

**Dòng 1 — Context & Project:**
```
 📁 ~/Projects/my-app (main*)  ║  ▰▱▱▱ 18% 36k/200k  ║  ⏱ 3.2s
```
- Project path: working dir (rút gọn) + git branch + dirty indicator (`*`)
- Context health bar: 4 ô `▰▱`, mỗi ô = 25%
- Turn duration

**Dòng 2 — Model & Activity:**
```
 🧠 claude-opus-4-6  ║  🔧 ◐ Bash running  Read×3 Edit×2  ║  📋 ▸ 2/5 "Fix auth bug"  ║  🤖 Explorer (12s)
```
- Model name (đầu dòng)
- Tool activity: tool đang chạy (◐ spinner) + history (name×count)
- Todo progress: current task + ratio
- Agent tracking: agent đang chạy + type + elapsed time

## Context Health Bar

| Usage | Display | Color |
|-------|---------|-------|
| 0-24% | `▰▱▱▱` | Green |
| 25-49% | `▰▰▱▱` | Green |
| 50-74% | `▰▰▰▱` | Yellow |
| 75-100% | `▰▰▰▰` | Red |

## Color Scheme (Claude Code aesthetic)

| Element | Color | Condition |
|---------|-------|-----------|
| Project path | Yellow | — |
| Git branch | Cyan | — |
| Context bar | Green | < 50% |
| Context bar | Yellow | 50-75% |
| Context bar | Red | > 75% |
| Model name | Cyan | — |
| Tool running | Yellow | đang chạy |
| Tool completed | DarkGrey | history |
| Todo in-progress | Yellow | — |
| Todo complete | Green | all done |
| Agent running | Magenta | — |
| Duration | Blue | — |
| Separators (║) | DarkGrey | — |

## Data Flow & State Management

### HudState struct

```rust
struct HudState {
    // Project
    project_path: String,
    git_branch: Option<String>,
    git_dirty: bool,

    // Context Health
    tokens_used: u64,
    tokens_max: u64,

    // Model
    model_name: String,

    // Tool Activity
    active_tool: Option<(String, Instant)>,
    tool_counts: HashMap<String, u32>,

    // Todo Progress
    todos_total: u32,
    todos_done: u32,
    current_task: Option<String>,

    // Agent Tracking
    agents: Vec<AgentStatus>,

    // Turn
    turn_start: Option<Instant>,
}
```

### Data Sources

| Field | Source | Update trigger |
|-------|--------|----------------|
| project_path, git_* | `current_dir()` + `git` command | App startup + mỗi turn |
| tokens_used/max | API response `usage` field | Mỗi API response |
| model_name | Runtime config | App startup |
| active_tool | Tool executor callbacks | Tool start/end |
| tool_counts | Tool executor callbacks | Tool end |
| todos_* | TaskRegistry | Task create/update |
| agents | TaskRegistry (agent tasks) | Agent spawn/complete |
| turn_start | REPL loop | Mỗi turn start |

### Event-driven update
- HudState nằm trong App struct
- Dirty flag → ratatui redraw footer
- Main content chỉ redraw khi có stream data mới
- Không polling

## Ratatui Integration

### Dependencies
```toml
ratatui = "0.29"
# crossterm đã có — ratatui dùng làm backend
```

### App struct

```rust
struct App {
    // Zone 1
    content_lines: Vec<Line<'static>>,
    scroll_offset: usize,

    // Zone 2
    input_buffer: String,
    input_mode: InputMode,

    // Zone 3
    hud: HudState,

    // Runtime
    runtime: ConversationRuntime,
}

enum InputMode {
    Typing,
    Pasted(usize),
}
```

### Render pipeline changes

| Before | After |
|--------|-------|
| `TerminalRenderer` → ANSI to stdout | `TerminalRenderer` → `Vec<Line<'static>>` (ratatui spans) |
| `Spinner` → ANSI cursor tricks | Spinner state in HudState → footer renders icon |
| `ThinkingDisplay` → cursor movement | Thinking lines → content_lines with Dim style |
| `rustyline` → line editing | Custom key handler in event loop |
| Direct `print!()` / `write!()` | All via `Frame::render_widget()` |

### Event loop

```rust
loop {
    terminal.draw(|frame| {
        let chunks = Layout::vertical([
            Constraint::Min(1),        // Zone 1: content
            Constraint::Length(zone2),  // Zone 2: input (dynamic)
            Constraint::Length(2),      // Zone 3: HUD footer
        ]);
        render_content(frame, chunks[0]);
        render_input(frame, chunks[1]);
        render_hud(frame, chunks[2]);
    })?;

    tokio::select! {
        key = crossterm_events.next() => handle_key(key),
        event = api_stream.next() => handle_stream(event),
    }
}
```

### Paste detection
- Crossterm `Event::Paste(text)` with bracketed paste mode
- Count `\n` → if > 5 → `InputMode::Pasted(line_count)`
- Zone 2 height = `min(input_lines, 5)` for paste, `actual_lines` for typing

## Error Handling & Edge Cases

### Terminal resize
- Ratatui handles `Event::Resize(w, h)` automatically
- Width < 60: HUD collapse to 1 line (path + context bar + model)
- Width < 40: HUD minimal (`▰▰▱▱ 45%` only)

### Graceful exit
- Ctrl+C, `/exit`: restore terminal (disable raw mode, leave alternate screen)
- `Drop` impl or `scopeguard` ensures restore even on panic

### Missing data fallbacks

| Field | Fallback |
|-------|----------|
| git branch | Hide, show path only |
| tokens (no response yet) | `▱▱▱▱ 0%` |
| active tool | Hide tool section |
| todos (none) | Hide todo section |
| agents (none) | Hide agent section |

### Performance
- Ratatui redraw max 30fps (tick_rate = 33ms)
- Content buffer limit: 10,000 lines — trim oldest when exceeded
- HUD only redraws when dirty flag = true

### Rustyline migration
- Biggest risk: losing tab completion, history, vi/emacs keybindings
- Mitigation: implement basics first (char input, backspace, arrows, Shift+Enter, history Up/Down)
- Slash command completion can be added later as popup menu
