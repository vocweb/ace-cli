# Command Autocomplete Dropdown — Design

**Date:** 2026-04-12
**Status:** Approved

## Goal

Add an autocomplete dropdown to the TUI that shows slash command hints with descriptions when the user types `/`. Similar to Claude Code CLI's command autocomplete UX.

## Decisions

| Decision | Choice |
|----------|--------|
| Trigger | Typing `/` in input |
| Position | Below input box, above HUD bar |
| Navigation | Arrow keys + Enter |
| Max visible items | 8 |
| Approach | Inline dropdown widget (Approach A) |

## Layout

```
┌─────────────────────────────────────────┐
│  Zone 1: Content (scrollable)           │  ← shrinks when dropdown open
│  ...                                    │
╭─────────────────────────────────────────╮
│ > /mo                                   │  ← Zone 2: Input
╰─────────────────────────────────────────╯
┌─────────────────────────────────────────┐
│ ▸ /model         Change AI model        │  ← Dropdown (max 8 items)
│   /model-info    Show model details     │
│   /models        List available models  │
└─────────────────────────────────────────┘
── opus · default · 1.2k tokens ──────────  ← Zone 3: HUD
```

When dropdown is closed, layout returns to the standard 3-zone split.

## Data Model

```rust
pub struct CommandDropdown {
    /// All command candidates (name, summary).
    candidates: Vec<(String, String)>,
    /// Filtered indices into candidates based on current input.
    filtered: Vec<usize>,
    /// Currently selected index within filtered list.
    selected: usize,
    /// Whether dropdown is visible.
    open: bool,
}
```

- `candidates` populated from `SlashCommandSpec::SLASH_COMMAND_SPECS` (name + summary)
- `filtered` recalculated on every input change
- `selected` wraps around (top ↔ bottom)

## Trigger & Filtering

- **Open**: Input starts with `/`
- **Filter**: Prefix match on text after `/` (e.g., `/mo` filters for commands starting with "mo")
- **Close**: Esc, Enter (select), delete past `/`, or 0 filtered results
- **Disabled**: During `TuiMode::Streaming` or `TuiMode::Permission`

## Navigation

- `↑/↓`: Move selection in dropdown
- `Enter`: Fill input with selected command, close dropdown
- `Esc`: Close dropdown, keep current text
- Continue typing: Re-filter list, reset selection to index 0

## Rendering

- New widget: `CommandDropdownWidget` implements ratatui `Widget`
- Each item: `Span::styled(name, cyan+bold)` + `Span::styled(summary, muted)`
- Selected item: `▸` prefix with bold highlight
- Border: `BorderType::Plain` (thin) — distinct from input box `BorderType::Rounded`
- Height: `min(filtered.len(), 8) + 2` (includes border)

## Integration Points

1. **`TuiApp`**: Add `dropdown: CommandDropdown` field
2. **`TuiApp::render()`**: Add conditional 4th zone when dropdown is open
3. **`TuiInput::handle_key()`**: Route arrow/enter/esc to dropdown when open
4. **Event loop**: Update dropdown filter on every input change
