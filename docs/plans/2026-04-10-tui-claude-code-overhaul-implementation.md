# TUI Claude Code Overhaul — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Full overhaul of ACE CLI TUI to visually and functionally match Claude Code CLI.

**Architecture:** Phase-by-phase refactor of monolithic main.rs (12.5K lines) into modules, then build inline streaming, tool blocks, permission UI, bordered input, status bar, and animations — all within ratatui alternate screen.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, tokio (async streaming), mpsc channels, pulldown-cmark, syntect.

---

## Phase 0: Refactor Monolith

> Break `main.rs` (12,577 lines) into focused modules. No behavior changes.
> After each task: `cargo test --workspace` must pass.

### Task 0.1: Extract `args.rs` — Argument Parsing

**Files:**
- Create: `crates/rusty-claude-cli/src/args.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Create `args.rs` with the following items moved from `main.rs`:
- `enum CliAction` (lines 300-391)
- `enum LocalHelpTopic` (lines 394-398)
- `enum CliOutputFormat` + impl (lines 401-418)
- `fn parse_args()` (lines 419-750)
- Argument helpers (lines 751-891): `parse_local_help_action`, `is_help_flag`, `parse_single_word_command_alias`, `bare_slash_command_guidance`, `join_optional_args`, `parse_direct_slash_cli_action`
- Error/suggestion functions (lines 892-1015): `format_unknown_option`, `format_unknown_direct_slash_command`, `format_unknown_slash_command`, `omc_compatibility_note_for_unknown_slash_command`, `render_suggestion_line`, `suggest_slash_commands`, `suggest_closest_term`, `ranked_suggestions`, `levenshtein_distance`
- Model/config resolution (lines 1016-1196): `resolve_model_alias`, `resolve_model_alias_with_config`, `config_alias_for_current_dir`, `normalize_allowed_tools`, `current_tool_registry`, `parse_permission_mode_arg`, `permission_mode_from_label`, `permission_mode_from_resolved`, `default_permission_mode`, `config_permission_mode_for_current_dir`, `config_model_for_current_dir`, `resolve_repl_model`, `provider_label`, `format_connected_line`, `filter_tool_specs`, `parse_system_prompt_args`
- Export/resume parsing (lines 1197-1290): `parse_export_args`, `parse_resume_args`
- Move related constants: `CLI_OPTION_SUGGESTIONS`, `type AllowedToolSet`
- Move related tests from `#[cfg(test)] mod tests` that test these functions

**Step 2:** In `main.rs`, add `mod args;` and `use args::*;`

**Step 3:** Run `cargo test --workspace` — all tests pass

**Step 4:** Run `cargo clippy --workspace --all-targets -- -D warnings` — no warnings

**Step 5:** Commit
```bash
git add -A && git commit -m "refactor: extract argument parsing to args.rs"
```

---

### Task 0.2: Extract `diagnostics.rs` — Doctor & Health Checks

**Files:**
- Create: `crates/rusty-claude-cli/src/diagnostics.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Create `diagnostics.rs` with items from `main.rs`:
- `enum DiagnosticLevel` + impl (lines 1291-1311)
- `struct DiagnosticCheck` + impl (lines 1312-1368)
- `struct DoctorReport` + impl (lines 1369-1429)
- All diagnostic/health functions (lines 1430-2032): `render_diagnostic_check`, `render_doctor_report`, `run_doctor`, `run_worker_state`, `run_mcp_serve`, `check_auth_health`, `check_config_health`, `check_workspace_health`, `check_sandbox_health`, `check_system_health`, `resume_command_can_absorb_token`, `looks_like_slash_command_token`, `dump_manifests`, `print_bootstrap_plan`
- Move related tests

**Step 2:** In `main.rs`, add `mod diagnostics;` and `use diagnostics::*;`

**Step 3:** Run `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract diagnostics to diagnostics.rs"
```

---

### Task 0.3: Extract `oauth.rs` — Authentication

**Files:**
- Create: `crates/rusty-claude-cli/src/oauth.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Move from `main.rs` to `oauth.rs`:
- `fn default_oauth_config()` (lines 2033-2047)
- `fn run_login()` (lines 2048-2122)
- `fn emit_login_browser_open_failure()` (lines 2123-2139)
- `fn run_logout()` (lines 2140-2154)
- `fn open_browser()` (lines 2155-2175)
- `fn wait_for_oauth_callback()` (lines 2176-2208)
- Related constants: `DEFAULT_OAUTH_CALLBACK_PORT`
- Move related tests

**Step 2:** `mod oauth;` + `use oauth::*;` in main.rs

**Step 3:** Run tests + clippy

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract OAuth flow to oauth.rs"
```

---

### Task 0.4: Extract `session_mgr.rs` — Session Management

**Files:**
- Create: `crates/rusty-claude-cli/src/session_mgr.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Move to `session_mgr.rs`:
- `struct SessionHandle` (lines 3484-3487)
- `struct ManagedSessionSummary` (lines 3490-3497)
- `struct PromptHistoryEntry` (lines 3511-3514)
- All session functions (lines 5440-5721): `sessions_dir`, `create_managed_session_handle`, `resolve_session_reference`, `resolve_managed_session_path`, `is_managed_session_file`, `collect_sessions_from_dir`, `list_managed_sessions`, `latest_managed_session`, `delete_managed_session`, `confirm_session_deletion`, `format_missing_session_reference`, `format_no_managed_sessions`, `render_session_list`, `format_session_modified_age`, `write_session_clear_backup`, `session_clear_backup_path`
- Session-related constants: `PRIMARY_SESSION_EXTENSION`, `LEGACY_SESSION_EXTENSION`, `LATEST_SESSION_REFERENCE`, `SESSION_REFERENCE_ALIASES`
- Move related tests

**Step 2:** `mod session_mgr;` + `use session_mgr::*;`

**Step 3:** Run tests + clippy

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract session management to session_mgr.rs"
```

---

### Task 0.5: Extract `formatting.rs` — Report & Display Formatting

**Files:**
- Create: `crates/rusty-claude-cli/src/formatting.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Move to `formatting.rs`:
- Format report functions (lines 2505-2833): `format_model_report`, `format_model_switch_report`, `format_permissions_report`, `format_permissions_switch_report`, `format_cost_report`, `format_resume_report`, `render_resume_usage`, `format_compact_report`, `format_auto_compaction_notice`
- Git utility functions: `parse_git_status_metadata`, `parse_git_status_branch`, `parse_git_workspace_summary`, `resolve_git_branch_for`, `run_git_capture_in`, `find_git_root_in`, `parse_git_status_metadata_for`
- `struct StatusContext`, `struct StatusUsage`, `struct GitWorkspaceSummary` + impl
- Status/help/report rendering (lines 5722-6815): all `format_*`, `render_*`, `print_*` functions
- Tool formatting (lines 8287-8833): `format_tool_call_start`, `format_tool_result`, `format_bash_call`, `format_bash_result`, `format_read_result`, etc.
- Move related tests

**Step 2:** `mod formatting;` + `use formatting::*;`

**Step 3:** Run tests + clippy

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract formatting functions to formatting.rs"
```

---

### Task 0.6: Extract `runtime_build.rs` — Runtime Construction

**Files:**
- Create: `crates/rusty-claude-cli/src/runtime_build.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Move to `runtime_build.rs`:
- `struct RuntimePluginState` (lines 3516-3521)
- `struct RuntimeMcpState` + impl (lines 3523-3528, 3634-3822)
- `struct BuiltRuntime` + impl + Deref/DerefMut/Drop (lines 3530-3607)
- MCP structs: `ToolSearchRequest`, `McpToolRequest`, `ListMcpResourcesRequest`, `ReadMcpResourceRequest`
- MCP helper functions (lines 3823-3931)
- `struct HookAbortMonitor` + impl (lines 3932-3986)
- Runtime build functions (lines 6978-7471): `build_system_prompt`, `build_runtime_plugin_state`, `build_runtime_plugin_state_with_loader`, `build_plugin_manager`, `resolve_plugin_path`, `runtime_hook_config_from_plugin_hooks`
- Internal progress structs and impls (lines 7063-7391)
- `build_runtime`, `build_runtime_with_plugin_state` (lines 7392-7471)
- Move related tests

**Step 2:** `mod runtime_build;` + `use runtime_build::*;`

**Step 3:** Run tests + clippy

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract runtime construction to runtime_build.rs"
```

---

### Task 0.7: Extract `api_client.rs` — API Client & Tool Executor

**Files:**
- Create: `crates/rusty-claude-cli/src/api_client.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Move to `api_client.rs`:
- `struct CliHookProgressReporter` + impl (lines 7472-7504)
- `struct CliPermissionPrompter` + impl (lines 7505-7559)
- `struct AnthropicRuntimeClient` + impl (lines 7560-7917)
- Auth resolution: `resolve_cli_auth_source`, `resolve_cli_auth_source_for_cwd`, `load_runtime_oauth_config_for`
- Error handling (lines 7918-8091): `request_ends_with_tool_result`, `format_user_visible_api_error`, `format_context_window_blocked_error`, `final_assistant_text`, `collect_tool_uses`, `collect_tool_results`, `collect_prompt_cache_events`
- `struct CliToolExecutor` + impl (lines 8834-8976)
- `fn convert_messages()` (lines 8977-9018)
- Move related tests

**Step 2:** `mod api_client;` + `use api_client::*;`

**Step 3:** Run tests + clippy

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract API client and tool executor to api_client.rs"
```

---

### Task 0.8: Extract `cli.rs` — LiveCli Core

**Files:**
- Create: `crates/rusty-claude-cli/src/cli.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Move to `cli.rs`:
- `struct LiveCli` (lines 3499-3508)
- Full `impl LiveCli` block (lines 3987-5439) — all 50+ methods
- Slash command completion (lines 8091-8203): `slash_command_completion_candidates_with_sessions`
- Move related tests

**Step 2:** `mod cli;` + `use cli::*;`

**Step 3:** Run tests + clippy

**Step 4:** Commit
```bash
git add -A && git commit -m "refactor: extract LiveCli to cli.rs"
```

---

### Task 0.9: Extract `repl.rs` and Reorganize TUI modules

**Files:**
- Create: `crates/rusty-claude-cli/src/repl.rs`
- Create: `crates/rusty-claude-cli/src/tui/mod.rs`
- Move: `tui_app.rs` → `tui/app.rs`
- Move: `tui_input.rs` → `tui/input.rs`
- Create: `crates/rusty-claude-cli/src/tui/event.rs`
- Modify: `crates/rusty-claude-cli/src/main.rs`

**Step 1:** Create `repl.rs` with `fn run_repl()` (lines 3185-3265) moved from main.rs

**Step 2:** Create `tui/mod.rs`:
```rust
pub mod app;
pub mod event;
pub mod input;
```

**Step 3:** Move `tui_app.rs` → `tui/app.rs`, `tui_input.rs` → `tui/input.rs`. Update imports.

**Step 4:** Create `tui/event.rs` with `fn run_tui_repl()` (lines 3273-3481) moved from main.rs

**Step 5:** Create `hud/mod.rs` re-exporting from `hud.rs` and `hud_widget.rs` (or rename to `hud/state.rs` and `hud/widget.rs`)

**Step 6:** Update `main.rs` to use `mod tui;`, `mod repl;`, `mod hud;`

**Step 7:** Run tests + clippy

**Step 8:** Commit
```bash
git add -A && git commit -m "refactor: reorganize TUI into tui/ module and extract repl.rs"
```

---

### Task 0.10: Clean up main.rs — Verify Final State

**Step 1:** Verify `main.rs` is now ~200-400 lines containing only:
- `mod` declarations
- `fn main()`
- `fn run()` (dispatch)
- Remaining small helpers
- `fn read_piped_stdin()`, `fn merge_prompt_with_stdin()`

**Step 2:** Run full verification:
```bash
cd rust && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

**Step 3:** Commit
```bash
git add -A && git commit -m "refactor: complete Phase 0 — main.rs monolith extraction"
```

---

## Phase 1: Claude Code Theme

### Task 1.1: Create `tui/theme.rs` — Color Theme

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/theme.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_theme_brand_color() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.brand, Color::Rgb(215, 119, 87));
    }

    #[test]
    fn test_tool_border_color_bash() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.tool_border_color("Bash"), Color::Rgb(253, 93, 177));
    }

    #[test]
    fn test_tool_border_color_edit() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.tool_border_color("Edit"), Color::Rgb(215, 119, 87));
    }

    #[test]
    fn test_tool_border_color_default() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.tool_border_color("Unknown"), Color::Rgb(136, 136, 136));
    }
}
```

**Step 2:** Run tests, verify they fail

**Step 3: Implement**
```rust
use ratatui::style::Color;

#[derive(Debug, Clone, Copy)]
pub struct ClaudeTheme {
    pub brand: Color,
    pub brand_shimmer: Color,
    pub bash_border: Color,
    pub permission: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub input_border: Color,
    pub input_border_focus: Color,
    pub spinner: Color,
    pub spinner_text: Color,
}

impl Default for ClaudeTheme {
    fn default() -> Self {
        Self {
            brand: Color::Rgb(215, 119, 87),
            brand_shimmer: Color::Rgb(235, 159, 127),
            bash_border: Color::Rgb(253, 93, 177),
            permission: Color::Rgb(177, 185, 249),
            success: Color::Rgb(78, 186, 101),
            error: Color::Rgb(255, 107, 128),
            warning: Color::Rgb(255, 215, 0),
            text_primary: Color::Rgb(255, 255, 255),
            text_secondary: Color::Rgb(136, 136, 136),
            text_muted: Color::Rgb(102, 102, 102),
            input_border: Color::Rgb(136, 136, 136),
            input_border_focus: Color::Rgb(166, 166, 166),
            spinner: Color::Rgb(215, 135, 135),
            spinner_text: Color::Rgb(255, 175, 135),
        }
    }
}

impl ClaudeTheme {
    pub fn tool_border_color(&self, tool_name: &str) -> Color {
        match tool_name {
            "Bash" | "BashOutput" => self.bash_border,
            "Edit" | "Write" => self.brand,
            _ => self.text_secondary,
        }
    }
}
```

**Step 4:** Run tests, verify pass

**Step 5:** Add `pub mod theme;` to `tui/mod.rs`

**Step 6:** Commit
```bash
git add -A && git commit -m "feat(tui): add Claude Code color theme"
```

---

### Task 1.2: Create `tui/spinner.rs` — Spinner & Shimmer

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/spinner.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_cycles_chars() {
        let mut s = SpinnerState::new();
        let c1 = s.current_char();
        s.tick();
        let c2 = s.current_char();
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_shimmer_alternates_colors() {
        let theme = crate::tui::theme::ClaudeTheme::default();
        let mut s = ShimmerState::new();
        let c1 = s.current_color(&theme);
        s.tick();
        let c2 = s.current_color(&theme);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_thinking_verb_rotates() {
        let mut s = ShimmerState::new();
        let v1 = s.current_verb();
        s.tick_verb();
        let v2 = s.current_verb();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_tool_spinner_cycles() {
        let mut s = ToolSpinnerState::new();
        let c1 = s.current_char();
        s.tick();
        let c2 = s.current_char();
        assert_ne!(c1, c2);
    }
}
```

**Step 2:** Run tests, verify fail

**Step 3: Implement**
```rust
use ratatui::style::Color;
use crate::tui::theme::ClaudeTheme;

const SPINNER_CHARS: &[char] = &['\u{00B7}', '\u{273B}', '\u{273D}', '\u{2736}', '\u{2733}', '\u{2722}'];
const TOOL_SPINNER_CHARS: &[char] = &['\u{25D0}', '\u{25D3}', '\u{25D1}', '\u{25D2}'];
const THINKING_VERBS: &[&str] = &[
    "Thinking", "Pondering", "Considering", "Contemplating",
    "Crafting", "Composing", "Connecting", "Synthesizing",
    "Architecting", "Ideating", "Sketching", "Processing", "Mapping",
];

pub struct SpinnerState { tick: usize }
pub struct ShimmerState { tick: usize, verb_index: usize }
pub struct ToolSpinnerState { tick: usize }
// impl new(), tick(), current_char(), current_color(), current_verb()
```

**Step 4:** Run tests, verify pass

**Step 5:** Commit
```bash
git add -A && git commit -m "feat(tui): add spinner and shimmer animation state"
```

---

## Phase 2: Inline Streaming

### Task 2.1: Define `TuiEvent` enum

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/events.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_event_send_sync() {
        fn assert_send<T: Send>() {}
        assert_send::<TuiEvent>();
    }
}
```

**Step 2:** Implement `TuiEvent` enum as specified in design doc:
- `StreamDelta(String)`, `StreamThinkingDelta(String)`, `StreamThinkingEnd`, `StreamMarkdownFlush`
- `ToolStart { name, input }`, `ToolResult { name, output, is_error }`
- `PermissionRequest { tool_name, description, options, response_tx }`
- `TurnStart`, `TurnEnd { usage }`, `TurnError(String)`
- `Key(KeyEvent)`, `Mouse(MouseEvent)`, `Paste(String)`, `Resize(u16, u16)`
- `PermissionOption` and `PermissionResponse` enums

**Step 3:** Run tests, verify pass

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): define TuiEvent enum for channel-based streaming"
```

---

### Task 2.2: Create `tui/streaming.rs` — Incremental Markdown Renderer

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/streaming.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_delta_accumulates() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("Hello ");
        sm.push_delta("world");
        assert_eq!(sm.pending_text(), "Hello world");
    }

    #[test]
    fn test_flush_returns_new_lines() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("# Title\n\nParagraph text\n");
        let lines = sm.flush();
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_flush_idempotent() {
        let mut sm = StreamingMarkdown::new();
        sm.push_delta("Hello\n");
        let lines1 = sm.flush();
        let lines2 = sm.flush();
        assert!(!lines1.is_empty());
        assert!(lines2.is_empty()); // no new content
    }

    #[test]
    fn test_stream_cursor_blinks() {
        let mut cursor = StreamCursor::new();
        assert!(cursor.visible);
        cursor.toggle();
        assert!(!cursor.visible);
    }
}
```

**Step 2:** Implement `StreamingMarkdown` struct:
- `push_delta(&mut self, delta: &str)` — append to pending
- `flush(&mut self) -> Vec<Line<'static>>` — parse pending markdown, return new lines
- Uses existing `TerminalRenderer::render_markdown_to_lines()` internally
- `StreamCursor` with blink toggle

**Step 3:** Run tests, verify pass

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): add incremental streaming markdown renderer"
```

---

### Task 2.3: Rewrite `tui/event.rs` — Channel-Based Event Loop

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/event.rs`
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1:** Add `TuiMode` enum to `app.rs`:
```rust
pub enum TuiMode {
    Input,          // Normal input mode
    Streaming,      // Receiving stream, input disabled
    Permission,     // Waiting for permission response
}
```

**Step 2:** Rewrite `run_tui_repl()` in `tui/event.rs`:
- Create `mpsc::channel::<TuiEvent>()`
- Spawn crossterm event reader thread → sends Key/Mouse/Paste/Resize events
- On user submit: spawn tokio task that runs turn, sends StreamDelta/ToolStart/PermissionRequest/TurnEnd via channel
- Main loop: `rx.try_recv()` for TuiEvents + redraw at ~30fps
- Replace screen-swap with inline rendering

**Step 3:** Update `TuiApp`:
- Add `mode: TuiMode` field
- Add `streaming: StreamingMarkdown` field
- Add `theme: ClaudeTheme` field
- Handle TuiEvent variants in render cycle

**Step 4:** Run `cargo test --workspace`

**Step 5:** Manual test: `cargo run -- --tui` — verify streaming works inline

**Step 6:** Commit
```bash
git add -A && git commit -m "feat(tui): implement channel-based inline streaming event loop"
```

---

### Task 2.4: Bridge API Streaming to TuiEvent Channel

**Files:**
- Modify: `crates/rusty-claude-cli/src/cli.rs` (or create `tui/bridge.rs`)
- Modify: `crates/rusty-claude-cli/src/tui/event.rs`

**Step 1:** Create method on `LiveCli` (or standalone fn) that:
- Accepts `tx: mpsc::Sender<TuiEvent>`
- Runs `runtime.run_turn()` on tokio runtime
- Intercepts SSE stream events and converts to TuiEvent:
  - `ContentBlockDelta::TextDelta` → `TuiEvent::StreamDelta`
  - `ContentBlockDelta::ThinkingDelta` → `TuiEvent::StreamThinkingDelta`
  - Tool use start → `TuiEvent::ToolStart`
  - Tool result → `TuiEvent::ToolResult`
- Sends `TuiEvent::TurnEnd` when done

**Step 2:** Update event loop to use this bridge instead of direct `cli.run_turn()`

**Step 3:** Run tests + manual test

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): bridge API streaming events to TUI channel"
```

---

## Phase 3: Tool Call Rendering

### Task 3.1: Create `tui/tool_panel.rs` — Bordered Tool Blocks

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/tool_panel.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::ClaudeTheme;

    #[test]
    fn test_render_tool_header() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_header("Bash", "npm install", &theme);
        let text: String = lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("Bash"));
        assert!(text.contains("npm install"));
        assert!(text.contains("\u{256D}")); // ╭
    }

    #[test]
    fn test_render_tool_result_success() {
        let theme = ClaudeTheme::default();
        let lines = render_tool_result("Bash", "ok", false, &theme);
        let text: String = lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
            .collect();
        assert!(text.contains("\u{2713}")); // ✓
    }

    #[test]
    fn test_collapsible_long_output() {
        let theme = ClaudeTheme::default();
        let long_output = (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let panel = ToolPanel::new("Bash", "cargo test", &theme);
        let panel = panel.with_result("ok", &long_output, false);
        assert!(panel.collapsed);
        assert_eq!(panel.total_output_lines, 20);
    }
}
```

**Step 2:** Implement:
- `fn render_tool_header(name, input_preview, theme) -> Vec<Line>` — renders `╭─ Name ─╮ │ input │ ╰───╯`
- `fn render_tool_result(name, output, is_error, theme) -> Vec<Line>` — renders `⎿ ✓/✗ output`
- `struct ToolPanel` with collapsed state, expand/collapse toggle
- Collapsible: default collapsed if > 5 output lines

**Step 3:** Run tests, verify pass

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): add bordered tool panel rendering"
```

---

### Task 3.2: Integrate Tool Panels into Event Loop

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/event.rs`
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1:** Handle `TuiEvent::ToolStart` → call `render_tool_header()`, push to Zone 1
**Step 2:** Handle `TuiEvent::ToolResult` → call `render_tool_result()`, push to Zone 1
**Step 3:** Add tool running spinner (◐ ◓ ◑ ◒) during active tool
**Step 4:** Run tests + manual test

**Step 5:** Commit
```bash
git add -A && git commit -m "feat(tui): integrate tool panels into streaming event loop"
```

---

## Phase 4: Permission UI

### Task 4.1: Create `tui/permission.rs` — Permission Prompt Widget

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/permission.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_prompt_default_selection() {
        let prompt = PermissionPrompt::new("Bash", "rm -rf /tmp/test");
        assert_eq!(prompt.selected, 0); // AllowOnce
    }

    #[test]
    fn test_navigate_right() {
        let mut prompt = PermissionPrompt::new("Bash", "rm -rf /tmp/test");
        prompt.navigate_right();
        assert_eq!(prompt.selected, 1); // Skip
    }

    #[test]
    fn test_navigate_wraps() {
        let mut prompt = PermissionPrompt::new("Bash", "rm -rf /tmp/test");
        prompt.navigate_right();
        prompt.navigate_right();
        prompt.navigate_right(); // wraps to 0
        assert_eq!(prompt.selected, 0);
    }

    #[test]
    fn test_confirm_returns_response() {
        let prompt = PermissionPrompt::new("Bash", "test");
        assert_eq!(prompt.confirm(), PermissionResponse::Allowed);
    }

    #[test]
    fn test_render_produces_lines() {
        let theme = crate::tui::theme::ClaudeTheme::default();
        let prompt = PermissionPrompt::new("Bash", "echo hello");
        let lines = prompt.render(&theme);
        assert!(!lines.is_empty());
    }
}
```

**Step 2:** Implement:
```rust
pub struct PermissionPrompt {
    tool_name: String,
    command_preview: String,
    options: Vec<PermissionOption>,
    selected: usize,
}

impl PermissionPrompt {
    pub fn new(tool_name: &str, command_preview: &str) -> Self;
    pub fn navigate_left(&mut self);
    pub fn navigate_right(&mut self);
    pub fn confirm(&self) -> PermissionResponse;
    pub fn render(&self, theme: &ClaudeTheme) -> Vec<Line<'static>>;
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<PermissionResponse>;
}
```

**Step 3:** Run tests, verify pass

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): add permission prompt widget"
```

---

### Task 4.2: Integrate Permission Prompt into Event Loop

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/event.rs`
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1:** Handle `TuiEvent::PermissionRequest`:
- Switch `TuiMode::Permission`
- Render permission prompt in Zone 1
- Route Y/N/A keys to PermissionPrompt
- On confirm: send response via `oneshot::Sender`, switch back to `TuiMode::Streaming`

**Step 2:** Update `render_input()` to show permission-mode border color when in Permission mode

**Step 3:** Manual test with a tool that requires permission

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): integrate permission prompts into event loop"
```

---

## Phase 5: Input Box

### Task 5.1: Create Bordered InputBox Widget

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1: Write tests**
```rust
#[test]
fn test_input_height_includes_border() {
    let app = TuiApp::new("claude-3".to_string(), "/tmp".to_string());
    // 1 line input + 2 border lines = 3
    assert_eq!(app.input_widget_height(), 3);
}

#[test]
fn test_input_prefix_color_slash() {
    let theme = ClaudeTheme::default();
    let color = input_prefix_color("/help", &theme);
    assert_eq!(color, Color::Cyan);
}

#[test]
fn test_input_prefix_color_bang() {
    let theme = ClaudeTheme::default();
    let color = input_prefix_color("!ls", &theme);
    assert_eq!(color, theme.bash_border);
}
```

**Step 2:** Update `render_input()` in `TuiApp`:
- Wrap input text in `Block::bordered()` with `BorderType::Rounded`
- Border color based on `TuiMode` (idle/typing/streaming/permission)
- `>` prefix colored by input type (/ = cyan, ! = pink, default = white)
- Cursor position adjusted for border padding

**Step 3:** Run tests + manual test

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): add bordered input box with Claude Code styling"
```

---

### Task 5.2: Add Border Shimmer Effect

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1:** Add shimmer tick counter to `TuiApp`
**Step 2:** In render loop, alternate input border color between `#888888` and `#A6A6A6` every ~500ms (every 15 frames at 30fps)
**Step 3:** Manual test — verify shimmer visible when typing

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): add input border shimmer effect"
```

---

### Task 5.3: Add Autocomplete Popup

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`
- Modify: `crates/rusty-claude-cli/src/tui/input.rs`

**Step 1: Write tests**
```rust
#[test]
fn test_filtered_completions() {
    let mut input = TuiInput::new();
    input.set_completions(vec!["/help".into(), "/history".into(), "/hooks".into(), "/exit".into()]);
    input.buffer = "/he".to_string();
    let filtered = input.filtered_completions();
    assert_eq!(filtered, vec!["/help"]);
}
```

**Step 2:** Add `filtered_completions()` method to `TuiInput`
**Step 3:** Render popup below input box: max 5 items, selected highlighted
**Step 4:** Run tests + manual test

**Step 5:** Commit
```bash
git add -A && git commit -m "feat(tui): add autocomplete popup for slash commands"
```

---

## Phase 6: Status Bar

### Task 6.1: Replace HudFooter with StatusBar Widget

**Files:**
- Create: `crates/rusty-claude-cli/src/tui/status_bar.rs`
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`
- Modify: `crates/rusty-claude-cli/src/tui/mod.rs`

**Step 1: Write tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::HudState;
    use crate::tui::theme::ClaudeTheme;

    #[test]
    fn test_status_bar_renders_model() {
        let state = HudState::new("claude-opus-4-6", "/tmp/project");
        let theme = ClaudeTheme::default();
        let bar = StatusBar::new(&state, &theme);
        let line = bar.render_line(80);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("claude-opus-4-6"));
    }

    #[test]
    fn test_context_progress_bar() {
        let spans = render_context_bar(50);
        // 5 filled, 5 empty at 50%
        let filled: String = spans[0].content.to_string();
        let empty: String = spans[1].content.to_string();
        assert_eq!(filled.chars().count(), 5);
        assert_eq!(empty.chars().count(), 5);
    }

    #[test]
    fn test_context_bar_color_green() {
        let spans = render_context_bar(30);
        // Should be green at 30%
        assert!(matches!(spans[0].style.fg, Some(Color::Rgb(78, 186, 101))));
    }
}
```

**Step 2:** Implement `StatusBar` widget:
- Single line: `model · permission · tokens · context% ████░░░░░░ · duration`
- `fn render_context_bar(percent: u8) -> Vec<Span>` — 10-char progress bar
- Colors from `ClaudeTheme`

**Step 3:** Update `TuiApp::render()`:
- Replace `HudFooter` with `StatusBar`
- Zone 3 height = 1 line

**Step 4:** Run tests + manual test

**Step 5:** Commit
```bash
git add -A && git commit -m "feat(tui): replace HudFooter with single-line Claude Code status bar"
```

---

## Phase 7: Thinking Display & Animations

### Task 7.1: Add Thinking Indicator to Streaming

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/event.rs`
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1:** Handle `TuiEvent::StreamThinkingDelta`:
- If `--show-thinking`: render thinking text italic, muted, with `│` prefix
- If not: show spinner with rotating verb + shimmer

**Step 2:** Handle `TuiEvent::StreamThinkingEnd`:
- Collapse thinking into `▸ Thought for X.Xs`

**Step 3:** Add shimmer timer: every ~150ms tick spinner, every ~2s tick verb

**Step 4:** Manual test with `--show-thinking` and without

**Step 5:** Commit
```bash
git add -A && git commit -m "feat(tui): add thinking indicator with shimmer animation"
```

---

### Task 7.2: Add Stream Cursor

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/app.rs`

**Step 1:** During streaming mode, append blinking `█` cursor at end of last content line
**Step 2:** Toggle visibility every ~500ms (15 frames)
**Step 3:** Remove cursor when streaming ends

**Step 4:** Commit
```bash
git add -A && git commit -m "feat(tui): add blinking stream cursor during response"
```

---

### Task 7.3: Add Startup Banner

**Files:**
- Modify: `crates/rusty-claude-cli/src/tui/event.rs`

**Step 1:** Replace current banner with Claude Code style:
```
╭──────────────────────────────────────────────╮
│  ACE CLI v0.1.0                              │
│  Model: claude-opus-4-6                      │
│  Session: abc123                             │
╰──────────────────────────────────────────────╯
```
- Border color: `#D77757` brand
- Text: white

**Step 2:** Commit
```bash
git add -A && git commit -m "feat(tui): add Claude Code style startup banner"
```

---

## Final Verification

### Task 8.1: Full Test Suite & Cleanup

**Step 1:** Run full verification:
```bash
cd rust && cargo fmt && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

**Step 2:** Manual test all features:
- `cargo run -- --tui` — verify startup banner
- Submit a prompt — verify inline streaming
- Check tool calls — verify bordered blocks, collapsible output
- Check permission prompts — verify Y/N/A keys work
- Check input box — verify border, shimmer, autocomplete
- Check status bar — verify model, tokens, context bar
- Check thinking — verify spinner animation
- Check scrolling — PageUp/PageDown, mouse wheel

**Step 3:** Commit any final fixes

**Step 4:** Final commit
```bash
git add -A && git commit -m "feat(tui): complete Claude Code UI/UX overhaul"
```
