# TUI Claude Code Overhaul — Design Document

**Date:** 2026-04-10
**Goal:** Full overhaul of ACE CLI TUI to match Claude Code CLI UI/UX as closely as possible.
**Approach:** Phase-by-Phase (refactor first, features second)
**Constraints:** No deadline, do it right. Inline streaming. Full permission UI. Clone visual identity.

---

## Phase 0: Refactor Monolith

### Problem
`main.rs` is 12,577 lines containing argument parsing, `LiveCli` struct, REPL loop, TUI loop, session management, OAuth, tool execution, slash command handling, and formatting.

### Target Module Structure

```
crates/rusty-claude-cli/src/
├── main.rs              (~200 lines)  Entry point, parse_args, dispatch
├── cli.rs               (~800 lines)  LiveCli struct + core methods
├── session_mgr.rs       (~400 lines)  Session create/load/persist/list
├── repl.rs              (~300 lines)  run_repl() - non-TUI REPL loop
├── oauth.rs             (~300 lines)  OAuth flow
├── args.rs              (~300 lines)  parse_args + CliAction enum
├── tui/
│   ├── mod.rs           (~50 lines)   re-exports
│   ├── app.rs           (existing tui_app.rs, evolved)
│   ├── input.rs         (existing tui_input.rs)
│   ├── event.rs         (~200 lines)  Event loop + event dispatch
│   ├── streaming.rs     (~400 lines)  Inline streaming renderer
│   ├── permission.rs    (~200 lines)  Permission prompt widget
│   ├── tool_panel.rs    (~300 lines)  Tool call bordered blocks
│   ├── theme.rs         (~150 lines)  Claude Code color theme
│   └── spinner.rs       (~100 lines)  Claude Code spinner + shimmer
├── hud/
│   ├── mod.rs           (existing hud.rs)
│   └── widget.rs        (existing hud_widget.rs)
├── render.rs            (existing, for non-TUI markdown)
└── init.rs              (existing)
```

### Rules
- No behavior changes — only move code, preserve public API
- `LiveCli` extracts to `cli.rs`
- `run_tui_repl` moves to `tui/event.rs`
- `run_repl` moves to `repl.rs`
- Tests move with code
- `cargo test` must pass after each extraction step

---

## Phase 1: Claude Code Visual Theme

### Color Palette (24-bit true color)

| Name | Hex | Usage |
|------|-----|-------|
| Brand | `#D77757` | Branding, file edit borders |
| Brand Shimmer | `#EB9F7F` | Shimmer animation target |
| Bash Border | `#FD5DB1` | Bash tool call borders |
| Permission | `#B1B9F9` | Permission prompts |
| Success | `#4EBA65` | Success indicators |
| Error | `#FF6B80` | Errors |
| Warning | `#FFD700` | Warnings |
| Text Primary | `#FFFFFF` | Main text |
| Text Secondary | `#888888` | Secondary info, separators |
| Text Muted | `#666666` | Timestamps, metadata |
| Input Border | `#888888` | Input box border (idle) |
| Input Border Focus | `#A6A6A6` | Input box border (shimmer) |
| Spinner Char | `#D78787` | Spinner character (ANSI 174) |
| Spinner Text | `#FFAF87` | Spinner label (ANSI 216) |

### Theme Struct

```rust
pub struct ClaudeTheme {
    pub brand: Color,           // #D77757
    pub brand_shimmer: Color,   // #EB9F7F
    pub bash_border: Color,     // #FD5DB1
    pub permission: Color,      // #B1B9F9
    pub success: Color,         // #4EBA65
    pub error: Color,           // #FF6B80
    pub text_primary: Color,    // #FFFFFF
    pub text_secondary: Color,  // #888888
    pub text_muted: Color,      // #666666
    pub input_border: Color,    // #888888
    pub spinner: Color,         // #D78787
    pub spinner_text: Color,    // #FFAF87
}
```

### Border Characters
Rounded: `╭` `╮` `╰` `╯` `─` `│`

### Spinner Characters
6 Unicode chars: `·` `✻` `✽` `✶` `✳` `✢` (non-uniform timing)

### Thinking Verbs (rotating ~2s each)
Thinking, Pondering, Considering, Contemplating, Crafting, Composing, Connecting, Synthesizing, Architecting, Ideating, Sketching, Processing, Mapping

---

## Phase 2: Inline Streaming Architecture

### Problem
Current TUI exits alternate screen during turn execution (screen-swap), causing visual flash and losing TUI context.

### Solution: Channel-Based Streaming

```
┌─────────────┐     mpsc channel      ┌──────────────┐
│  API Thread  │ ──── TuiEvent ──────▶ │  TUI Event   │
│  (tokio)     │                       │  Loop         │
│              │                       │  (crossterm)  │
└─────────────┘                       └──────────────┘
```

### TuiEvent Enum

```rust
pub enum TuiEvent {
    // Streaming
    StreamDelta(String),
    StreamThinkingDelta(String),
    StreamThinkingEnd,
    StreamMarkdownFlush,

    // Tool lifecycle
    ToolStart { name: String, input: Value },
    ToolResult { name: String, output: String, is_error: bool },

    // Permission
    PermissionRequest {
        tool_name: String,
        description: String,
        options: Vec<PermissionOption>,
        response_tx: oneshot::Sender<PermissionResponse>,
    },

    // Turn lifecycle
    TurnStart,
    TurnEnd { usage: Option<TokenUsage> },
    TurnError(String),

    // Terminal events
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
}
```

### Incremental Markdown Rendering

```rust
pub struct StreamingMarkdown {
    buffer: String,
    rendered_lines: Vec<Line<'static>>,
    pending_text: String,
}
```

- `push_delta()` accumulates text
- `flush()` runs every ~100ms or on newline, parses pending markdown, returns new lines only

---

## Phase 3: Tool Call Rendering

### Visual Style

```
╭─ Bash ──────────────────────────────────────────────╮
│ npm install express                                  │
╰──────────────────────────────────────────────────────╯
  ⎿ stdout: added 64 packages in 2.3s
```

### Border Colors

| Tool | Color | Hex |
|------|-------|-----|
| Bash, BashOutput | Pink | `#FD5DB1` |
| Edit, Write | Orange | `#D77757` |
| Read, Glob, Grep | Grey | `#888888` |
| Others | Grey | `#888888` |

### Collapsible Output
- Default collapsed if > 5 lines
- Summary: `✓ 42 tests passed (collapsed — 87 lines, press Enter to expand)`
- Enter to expand/collapse

### Result Prefix
- Success: `⎿ ✓` (green)
- Error: `⎿ ✗` (red)
- Running: `⎿ ◐ running...` (yellow, spinner `◐ ◓ ◑ ◒`)

---

## Phase 4: Permission UI

### Visual

```
╭─ Bash ──────────────────────────────────────────────╮
│ rm -rf node_modules && npm install                   │
╰──────────────────────────────────────────────────────╯

  Allow Bash: rm -rf node_modules && npm install?

  ▸ Allow once    Skip    Always allow for session

  (y) allow · (n) skip · (a) always · (Shift+Tab) cycle mode
```

### Key Bindings

| Key | Action |
|-----|--------|
| `y` / `Enter` | Allow once |
| `n` / `Esc` | Skip (deny) |
| `a` | Always allow |
| `←` / `→` | Navigate options |
| `Shift+Tab` | Cycle modes |

### Blocking Mechanism
- API thread sends `PermissionRequest` with `oneshot::Sender`
- TUI switches to permission mode (only Y/N/A keys accepted)
- User responds → `oneshot::send()` → API thread continues

---

## Phase 5: Input Area (Bordered Box)

### Visual

```
╭──────────────────────────────────────────────────────╮
│ > your prompt here█                                   │
╰──────────────────────────────────────────────────────╯
```

### Border Behavior
- Idle: `#888888` grey
- Typing: shimmer `#888888` ↔ `#A6A6A6` (~500ms)
- Streaming: dim `#666666`, input disabled
- Permission mode: `#B1B9F9`

### Special Prefixes
- `/` → Cyan (slash command)
- `!` → Pink `#FD5DB1` (shell escape)
- Default → White

### Autocomplete Popup
Below input box, max 5 candidates, selected highlighted.

---

## Phase 6: Status Bar

### Single-line format

```
 claude-opus-4-6 · default · 45.2k tokens · 23% ████░░░░░░ · 3.2s
```

### Elements

| Element | Color |
|---------|-------|
| Model | `#D77757` brand |
| Separator `·` | `#666666` muted |
| Permission mode | `#B1B9F9` |
| Tokens | `#888888` secondary |
| Context % | green/yellow/red by threshold |
| Progress bar (10 chars) | `█` filled, `░` empty |
| Duration | `#888888` (only during turn) |

### Replaces
Current 2-line HudFooter → 1-line StatusBar. Tool activity and todos move inline into Zone 1.

---

## Phase 7: Thinking Display & Animations

### Thinking Spinner
Rotating verb with shimmer between `#D77757` ↔ `#EB9F7F`:
```
 ✻ Thinking...  →  ✽ Pondering...  →  ✶ Considering...
```

### ShimmerState
- `tick_spinner()` every ~150ms (spinner char)
- `tick_verb()` every ~2s (verb rotation)

### Thinking Content (if --show-thinking)
```
 ✻ Thinking...
 │ Let me analyze this code structure...
```
- Italic, `#666666` muted, `│` prefix per line
- On completion: collapse to `▸ Thought for 4.2s (click to expand)`

### Stream Cursor
Blinking `█` at end of streaming text, `#D77757` brand, 500ms blink rate.

### Tool Running Spinner
`◐ ◓ ◑ ◒` rotation every ~250ms inside tool result area.

---

## Zone Layout (Final)

```
┌─────────────────────────────────────────────────────┐
│ Zone 1: CONTENT (scrollable)                         │
│ - Conversation, tool blocks, permissions, streaming  │
├─────────────────────────────────────────────────────┤
│ Zone 2: INPUT (bordered box)                         │
│ ╭────────────────────────────────────────────────╮   │
│ │ > prompt here                                  │   │
│ ╰────────────────────────────────────────────────╯   │
├─────────────────────────────────────────────────────┤
│ Zone 3: STATUS BAR (1 line)                          │
│ claude-opus-4-6 · default · 45k tokens · 23% ████░░ │
└─────────────────────────────────────────────────────┘
```
