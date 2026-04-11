# Claude Code CLI -- Visual UI/UX Specification

> Reverse-engineered from Claude Code v2.1.100 binary, official documentation at
> code.claude.com, GitHub issues, community tools (tweakcc), and web research.
> Date: 2026-04-10

---

## 1. Architecture Foundation

Claude Code's TUI is built on a **custom fork of Ink** (React-based terminal
renderer). It maintains a virtual DOM of terminal nodes (`Box`, `Text`) that are
reconciled and transformed into screen buffers. Layout is powered by the **Yoga
layout engine** (CSS Flexbox in the terminal). Rendering uses double-buffering
(`frontFrame` / `backFrame`) with diffing to minimize bytes sent to stdout.

Two rendering modes exist:

| Mode | Activation | Buffer | Scrolling |
|------|-----------|--------|-----------|
| Standard (default) | Default | Normal scrollback | Native terminal scroll |
| Fullscreen | `CLAUDE_CODE_NO_FLICKER=1` | Alternate screen buffer | App-managed (`PgUp`/`PgDn`, mouse wheel) |

---

## 2. Layout Structure

### 2.1 Standard Mode

```
+------------------------------------------------------------------+
|  [Logo/Dashboard]  (shown at idle / startup)                      |
|                                                                    |
|  === Conversation Area (scrolls upward) ==================        |
|  | User message (with background tint)                   |        |
|  | Assistant message (streaming text)                    |        |
|  | Tool call block (collapsible, bordered)               |        |
|  | Tool result block (collapsible)                       |        |
|  | Permission dialog (inline)                            |        |
|  | Assistant continuation...                             |        |
|  =========================================================        |
|                                                                    |
|  [Status Line] (1-2 lines, bottom -- user-configurable)           |
|  +--------------------------------------------------------------+ |
|  | > [Input area -- multiline, bordered]                         | |
|  +--------------------------------------------------------------+ |
+------------------------------------------------------------------+
```

### 2.2 Fullscreen Mode

- Input box stays **fixed at the bottom** of the screen.
- Only visible messages are kept in the render tree (virtual scrolling).
- Memory stays constant regardless of conversation length.
- Mouse capture enabled (click-to-expand, text selection, URL clicking).
- `Ctrl+O` cycles: normal prompt -> transcript mode -> focus view -> back.

### 2.3 Component Hierarchy

```
Ink (Root Renderer)
  +-- REPL Screen (src/screens/REPL.tsx)
  |     +-- VirtualMessageList (virtualized scrolling)
  |     |     +-- UserMessage (with background tint)
  |     |     +-- AssistantMessage (streamed text + markdown)
  |     |     +-- ToolCallMessage (bordered, collapsible)
  |     |     +-- ToolResultMessage
  |     |     |     +-- BashToolResultMessage (ANSI output)
  |     |     |     +-- FileWriteToolUI (diff rendering)
  |     |     |     +-- CollapsedReadSearchContent
  |     |     +-- SystemTextMessage (status + duration)
  |     |     +-- GlimmerMessage (shimmer animation)
  |     |     +-- ThinkingBlock (italic, collapsible)
  |     |     +-- Dialog (permission prompts)
  |     +-- PromptInput (multiline, vim mode support)
  |     +-- StatusLine (user-configurable script output)
  +-- LogoV2 / Dashboard (idle state)
  |     +-- CondensedLogo
  |     +-- Full Dashboard (welcome, release notes)
  +-- NativeAutoUpdater
  +-- TaskList (Ctrl+T toggle)
```

---

## 3. Color Scheme (All Themes -- Extracted from Binary)

### 3.1 Dark Theme (Default)

| Property | RGB | Hex | Usage |
|----------|-----|-----|-------|
| `claude` | `rgb(215,119,87)` | `#D77757` | Brand color, logo, Claude's identity accent |
| `claudeShimmer` | `rgb(235,159,127)` | `#EB9F7F` | Shimmer animation highlight on thinking verb |
| `claudeBlue_FOR_SYSTEM_SPINNER` | `rgb(147,165,255)` | `#93A5FF` | System spinner color |
| `claudeBlueShimmer_FOR_SYSTEM_SPINNER` | `rgb(177,195,255)` | `#B1C3FF` | System spinner shimmer |
| `autoAccept` | `rgb(175,135,255)` | `#AF87FF` | Auto-accept mode indicator |
| `bashBorder` | `rgb(253,93,177)` | `#FD5DB1` | Bash tool call border color |
| `permission` | `rgb(177,185,249)` | `#B1B9F9` | Permission dialog accent |
| `permissionShimmer` | `rgb(207,215,255)` | `#CFD7FF` | Permission dialog shimmer |
| `planMode` | `rgb(72,150,140)` | `#48968C` | Plan mode indicator |
| `ide` | `rgb(71,130,200)` | `#4782C8` | IDE integration color |
| `promptBorder` | `rgb(136,136,136)` | `#888888` | Input box border (idle) |
| `promptBorderShimmer` | `rgb(166,166,166)` | `#A6A6A6` | Input box border (shimmer) |
| `text` | `rgb(255,255,255)` | `#FFFFFF` | Primary text color |
| `inverseText` | `rgb(0,0,0)` | `#000000` | Inverse text (on light bg) |
| `inactive` | `rgb(153,153,153)` | `#999999` | Inactive/disabled elements |
| `subtle` | `rgb(80,80,80)` | `#505050` | Subtle borders, separators |
| `suggestion` | `rgb(177,185,249)` | `#B1B9F9` | Autocomplete suggestions |
| `remember` | `rgb(177,185,249)` | `#B1B9F9` | Memory/remember highlight |
| `background` | `rgb(0,204,204)` | `#00CCCC` | Background accent (e.g., tags) |
| `success` | `rgb(78,186,101)` | `#4EBA65` | Success messages |
| `error` | `rgb(255,107,128)` | `#FF6B80` | Error messages |
| `warning` | `rgb(255,193,7)` | `#FFC107` | Warning messages |
| `warningShimmer` | `rgb(255,223,57)` | `#FFDF39` | Warning shimmer |
| `diffAdded` | `rgb(34,92,43)` | `#225C2B` | Diff: added line background |
| `diffRemoved` | `rgb(122,41,54)` | `#7A2936` | Diff: removed line background |
| `diffAddedDimmed` | `rgb(71,88,74)` | `#47584A` | Diff: added context (dimmed) |
| `diffRemovedDimmed` | `rgb(105,72,77)` | `#69484D` | Diff: removed context (dimmed) |
| `diffAddedWord` | `rgb(56,166,96)` | `#38A660` | Diff: added word highlight |
| `diffRemovedWord` | `rgb(179,89,107)` | `#B3596B` | Diff: removed word highlight |
| `professionalBlue` | `rgb(106,155,204)` | `#6A9BCC` | Professional blue accent |
| `clawd_body` | `rgb(215,119,87)` | `#D77757` | Logo/mascot body color |
| `clawd_background` | `rgb(0,0,0)` | `#000000` | Logo background |
| `userMessageBackground` | `rgb(55,55,55)` | `#373737` | User message background tint |
| `bashMessageBackgroundColor` | `rgb(65,60,65)` | `#413C41` | Bash output background |
| `memoryBackgroundColor` | `rgb(55,65,70)` | `#374146` | Memory block background |
| `rate_limit_fill` | `rgb(177,185,249)` | `#B1B9F9` | Rate limit bar filled |
| `rate_limit_empty` | `rgb(80,83,112)` | `#505370` | Rate limit bar empty |

### 3.2 Light Theme

| Property | RGB | Hex |
|----------|-----|-----|
| `claude` | `rgb(215,119,87)` | `#D77757` |
| `claudeShimmer` | `rgb(245,149,117)` | `#F59575` |
| `text` | `rgb(0,0,0)` | `#000000` |
| `inverseText` | `rgb(255,255,255)` | `#FFFFFF` |
| `inactive` | `rgb(102,102,102)` | `#666666` |
| `subtle` | `rgb(175,175,175)` | `#AFAFAF` |
| `success` | `rgb(44,122,57)` | `#2C7A39` |
| `error` | `rgb(171,43,63)` | `#AB2B3F` |
| `warning` | `rgb(150,108,30)` | `#966C1E` |
| `bashBorder` | `rgb(255,0,135)` | `#FF0087` |
| `autoAccept` | `rgb(135,0,255)` | `#8700FF` |
| `permission` | `rgb(87,105,247)` | `#5769F7` |
| `promptBorder` | `rgb(153,153,153)` | `#999999` |
| `diffAdded` | `rgb(105,219,124)` | `#69DB7C` |
| `diffRemoved` | `rgb(255,168,180)` | `#FFA8B4` |
| `diffAddedWord` | `rgb(47,157,68)` | `#2F9D44` |
| `diffRemovedWord` | `rgb(209,69,75)` | `#D1454B` |
| `userMessageBackground` | `rgb(240,240,240)` | `#F0F0F0` |

### 3.3 Dark-Daltonized Theme (Colorblind-friendly)

| Property | RGB | Hex |
|----------|-----|-----|
| `claude` | `rgb(255,153,51)` | `#FF9933` |
| `bashBorder` | `rgb(51,153,255)` | `#3399FF` |
| `autoAccept` | `rgb(175,135,255)` | `#AF87FF` |
| `success` | `rgb(51,153,255)` | `#3399FF` |
| `error` | `rgb(255,102,102)` | `#FF6666` |
| `warning` | `rgb(255,204,0)` | `#FFCC00` |
| `diffAdded` | `rgb(0,68,102)` | `#004466` |
| `diffRemoved` | `rgb(102,0,0)` | `#660000` |

### 3.4 Rainbow Colors (Shared Across All RGB Themes)

Used for subagent borders, logo gradient, and decorative elements:

| Color | Base RGB | Shimmer RGB |
|-------|----------|-------------|
| `rainbow_red` | `rgb(235,95,87)` | `rgb(250,155,147)` |
| `rainbow_orange` | `rgb(245,139,87)` | `rgb(255,185,137)` |
| `rainbow_yellow` | `rgb(250,195,95)` | `rgb(255,225,155)` |
| `rainbow_green` | `rgb(145,200,130)` | `rgb(185,230,180)` |
| `rainbow_blue` | `rgb(130,170,220)` | `rgb(180,205,240)` |
| `rainbow_indigo` | `rgb(155,130,200)` | `rgb(195,180,230)` |
| `rainbow_violet` | `rgb(200,130,180)` | `rgb(230,180,210)` |

### 3.5 Subagent Colors

Each subagent gets a distinct identity color for its border:

| Name | Purpose |
|------|---------|
| `red_FOR_SUBAGENTS_ONLY` | Subagent 1 |
| `blue_FOR_SUBAGENTS_ONLY` | Subagent 2 |
| `green_FOR_SUBAGENTS_ONLY` | Subagent 3 |
| `yellow_FOR_SUBAGENTS_ONLY` | Subagent 4 |
| `purple_FOR_SUBAGENTS_ONLY` | Subagent 5 |
| `orange_FOR_SUBAGENTS_ONLY` | Subagent 6 |
| `pink_FOR_SUBAGENTS_ONLY` | Subagent 7 |
| `cyan_FOR_SUBAGENTS_ONLY` | Subagent 8 |

### 3.6 Theme Detection

- Claude sends an **OSC 11** escape sequence to query the terminal background RGB.
- Calculates luminance using **ITU-R BT.709** formula.
- Selects dark or light theme based on luminance threshold.
- Checks `COLORTERM=truecolor` for 24-bit support; downgrades to 256-color
  for terminals like Apple Terminal.

### 3.7 Available Themes

1. **auto** -- auto-detect based on terminal background luminance
2. **dark** -- default, 24-bit RGB
3. **light** -- 24-bit RGB
4. **dark-daltonized** -- colorblind-friendly dark (blue/yellow instead of red/green)
5. **light-daltonized** -- colorblind-friendly light
6. **dark-ansi** -- uses only 16 ANSI colors
7. **light-ansi** -- uses only 16 ANSI colors

Live preview via `/theme` command; reverts on Escape. Selection persists globally
in `~/.claude/`.

---

## 4. Streaming Behavior

### 4.1 Text Streaming

- Assistant text streams **token-by-token** (not line-by-line).
- Each token triggers a React re-render via the Ink reconciler.
- The virtual DOM diffs against the previous frame to minimize terminal writes.
- Auto-follow: new output scrolls the viewport to the bottom.
- Scrolling up pauses auto-follow; `Ctrl+End` resumes.

### 4.2 Thinking/Reasoning Phase

- During extended thinking, the CLI shows a **spinner + thinking verb** with
  shimmer animation (no real-time thinking text stream by default).
- Thinking verbs rotate through: `Thinking`, `Pondering`, `Considering`,
  `Contemplating`, `Crafting`, `Composing`, `Connecting`, `Synthesizing`,
  `Architecting`, `Ideating`, `Sketching`, `Processing`, `Mapping`.
- The verb text shimmers between the `claude` color and `claudeShimmer` color.
- After thinking completes, the response text begins streaming.
- Extended thinking blocks can be viewed via `Ctrl+O` (transcript mode).
- Thinking blocks render in **gray italic text** above the response when visible.
- Collapsed by default; expandable with `Ctrl+O`.

### 4.3 Ghost Text / Prompt Suggestions

- After Claude responds, a grayed-out suggestion appears in the input area.
- Based on conversation history and project git history.
- Accept with **Tab** or **Right arrow**; press **Enter** to accept and submit.
- Start typing to dismiss.

---

## 5. Tool Call Rendering

### 5.1 General Structure

Tool calls are rendered as **bordered blocks** within the conversation flow.
Each tool call shows:

```
borderStyle:"round"    borderColor: varies by tool type
+-------------------------------------------------+
| [Tool Name]  [parameters/summary]               |
|                                                   |
| [Tool output / result]                           |
+-------------------------------------------------+
```

### 5.2 Border Styles Used

| Element | Border Style | Border Color Key |
|---------|-------------|-----------------|
| Bash tool call | `round` | `bashBorder` (dark: `#FD5DB1` pink) |
| File edit diff | `round` | `claude` (dark: `#D77757` orange) |
| Permission dialog | `round` | `permission` (dark: `#B1B9F9` blue-lavender) |
| Error message | `single` | `error` (dark: `#FF6B80` red) |
| Warning message | `single` | `warning` (dark: `#FFC107` yellow) |
| System message | `dashed` | `subtle` or `inactive` |
| Blockquote | `quote` | `subtle` |
| Plan mode border | `round` | `planMode` (dark: `#48968C` teal) |
| Subagent border | `round` | subagent color (per agent) |

### 5.3 Collapsible Behavior

- Tool results can be **clicked to expand** (fullscreen mode) or toggled
  via `Ctrl+O`.
- MCP read/search tool calls collapse to a single line like `"Queried {server}"`.
- Only messages with additional hidden content are clickable.
- `Ctrl+Shift+B` toggles **brief mode** (all-or-nothing collapse).
- The tool call and its result expand/collapse together as a unit.

### 5.4 Diff Rendering (File Edit Tool)

File changes display with specialized diff views:

- **Line numbers** shown on the left margin
- **Added lines**: green background (`diffAdded`: `#225C2B` dark)
- **Removed lines**: red background (`diffRemoved`: `#7A2936` dark)
- **Added words** (within changed lines): brighter green (`diffAddedWord`: `#38A660`)
- **Removed words**: brighter red (`diffRemovedWord`: `#B3596B`)
- **Context lines**: dimmed colors (`diffAddedDimmed`, `diffRemovedDimmed`)

### 5.5 Bash Output Rendering

- Raw ANSI escape sequences from bash are parsed by the `Ansi` component.
- Converts terminal attributes (bold, colors, hyperlinks) into Ink `Text` spans.
- Supports full ANSI color palette, 256-color mode, and hyperlinks.
- Background: `bashMessageBackgroundColor` (`#413C41` dark).

---

## 6. Permission Prompts

### 6.1 Structure

Permission dialogs render **inline** within the conversation flow (not as
popups). They are bordered blocks using the `permission` color.

```
╭─────────────────────────────────────────────╮
│  Claude wants to run:                        │
│                                              │
│  > bash_command_here                         │
│                                              │
│  [Allow once]  [Skip]  [Always allow]        │
│                                              │
│  (y/n/a)                                     │
╰─────────────────────────────────────────────╯
```

### 6.2 Permission Dialog Options

For **bash commands** (3 options):
1. **Allow once** -- run this one time
2. **Skip** -- deny this execution
3. **Always allow for this session** -- whitelist for the session

For **file modifications** (similar pattern):
- "Yes, don't ask again" persists until session end

For **read-only operations** (Read, Grep, Glob):
- No prompt; auto-approved

### 6.3 Navigation

- **Left/Right arrows** cycle through dialog tabs/options.
- **y** = allow, **n** = deny/skip, **a** = always allow.
- The selected option is highlighted with a **blue background** using
  the `permission` color (`#B1B9F9`) via true-color escape sequences.
- `Shift+Tab` cycles through permission modes:
  `default` -> `acceptEdits` -> `plan` -> `auto` -> (back to default).

### 6.4 Auto Mode Denials

When auto mode denies a tool call, a notification appears. Denied actions
are recorded in `/permissions` under the **Recently denied** tab. Press `r`
on a denied action to mark it for retry.

---

## 7. Input Area

### 7.1 Prompt Prefix

- The input area is rendered inside a **bordered box** using `round` border style.
- Border color: `promptBorder` (`#888888` dark) with shimmer to `promptBorderShimmer`.
- The prompt prefix is `> ` (greater-than followed by space).
- The prefix may show the current mode (e.g., `plan>` in plan mode).

### 7.2 Multiline Support

- `\` + `Enter` -- works everywhere (escape newline)
- `Shift+Enter` -- works natively in iTerm2, WezTerm, Ghostty, Kitty
- `Ctrl+J` -- line feed character (universal)
- `Option+Enter` -- macOS default
- `/terminal-setup` auto-configures Shift+Enter for VS Code, Alacritty, Zed, Warp

### 7.3 Special Prefixes

- `/` at start: command/skill autocomplete menu
- `!` at start: bash mode (direct shell execution)
- `@` anywhere: file path autocomplete

### 7.4 Vim Mode

Full vi/vim keybinding support when enabled via `/config`:
- Mode switching: `Esc`, `i`/`I`, `a`/`A`, `o`/`O`
- Navigation: `h`/`j`/`k`/`l`, `w`/`e`/`b`, `0`/`$`/`^`, `gg`/`G`
- Editing: `x`, `dd`, `D`, `dw`, `cc`, `C`, `cw`, `yy`, `p`/`P`
- Text objects: `iw`/`aw`, `i"`/`a"`, `i(`/`a(`

### 7.5 Editor Integration

- `Ctrl+G` or `Ctrl+X Ctrl+E` opens the prompt in `$VISUAL` / `$EDITOR`.

---

## 8. Status Bar / HUD

### 8.1 Default Footer Elements

The status area at the bottom shows:

- **Model identifier** and display name
- **Permission mode** indicator (default/acceptEdits/plan/auto)
- **Token consumption** (input/output/cache) -- visible in verbose mode
- **Context window usage** (percentage or progress bar)
- **Time elapsed** for operations
- **PR link** with colored underline (green=approved, yellow=pending,
  red=changes requested, gray=draft, purple=merged)
- **System notifications** (MCP errors, auto-updates, token warnings)
  on the right side

### 8.2 Custom Status Line

Configured via `/statusline` command. Receives JSON on stdin:

```json
{
  "cwd": "/path/to/project",
  "sessionId": "...",
  "sessionName": "...",
  "model": { "id": "...", "displayName": "..." },
  "workspace": "...",
  "version": "2.1.100",
  "outputStyle": "...",
  "cost": { ... },
  "contextWindow": { "used_percentage": 42.5, ... }
}
```

- Runs as a shell script; output is displayed at the bottom.
- Supports multiple lines.
- Temporarily hides during: autocomplete, help menu, permission prompts.
- Does not consume API tokens.
- Common pattern: `Model | In:XXk Out:XXk | [========  ] 42% | git:main`

### 8.3 Task List (`Ctrl+T`)

- Shows up to 10 tasks in the status area.
- Indicators: pending, in-progress, complete.
- Persists across context compactions.

---

## 9. Markdown Rendering

### 9.1 Supported Features

| Feature | Rendering |
|---------|-----------|
| **Bold** (`**text**`) | Bold ANSI attribute |
| *Italic* (`*text*`) | Italic ANSI attribute (dimmed in some terminals) |
| **Bold italic** (`***text***`) | Bold + italic |
| Inline code (`` `code` ``) | Highlighted with background tint |
| Fenced code blocks | Syntax highlighting with language detection |
| Diff blocks (` ```diff `) | Color-coded added/removed lines |
| Tables | Column-aligned with borders |
| Bulleted lists | Standard bullet rendering |
| Numbered lists | Sequential numbering |
| Single-level blockquotes | Left border with `quote` border style |
| Horizontal rules | Rendered as separator line |

### 9.2 Unsupported / Silently Stripped

| Feature | What Happens |
|---------|-------------|
| Headers (`## H2`, `### H3`) | All levels render as identical bold text -- no visual hierarchy |
| Link labels (`[text](url)`) | Label discarded; raw URL shown instead |
| Strikethrough (`~~text~~`) | Rendered as literal `~~text~~` |
| Task lists (`- [x]`) | Both checked/unchecked render as plain bullets |
| Nested blockquotes (`> > >`) | All nesting levels identical |
| HTML entities (`&amp;`) | Raw entity text shown verbatim |

### 9.3 Code Block Syntax Highlighting

- Uses terminal true-color for syntax tokens when `COLORTERM=truecolor`.
- Language auto-detected or specified via fence tag.
- Toggle syntax highlighting with `Ctrl+T` inside `/theme` picker.

---

## 10. Thinking / Reasoning Display

### 10.1 Default Behavior

- Extended thinking tokens are **discarded before rendering** by default.
- A spinner + rotating thinking verb is shown instead.
- No real-time streaming of thinking content (just a spinner).

### 10.2 Verbose Mode (`Ctrl+O`)

- Toggles transcript viewer showing tool call details, execution traces,
  and **extended thinking blocks**.
- Thinking blocks render in **gray italic text** above the response.
- This is the actual chain of thought, not a summary.

### 10.3 Thinking Configuration

- `Alt+T` / `Option+T` toggles extended thinking on/off.
- `--thinking` flag enables from CLI.
- Thinking budget is adaptive (no fixed token count by default).

---

## 11. Animations

### 11.1 Spinner Animation

The spinner cycles through **six Unicode characters**:

```
  \u00B7  (middle dot)           ·
  \u273B  (teardrop-spoked asterisk)  ✻
  \u273D  (heavy teardrop-spoked asterisk)  ✽
  \u2736  (six pointed black star)  ✶
  \u2733  (eight spoked asterisk)  ✳
  \u2722  (four balloon-spoked asterisk)  ✢
```

- The spinner uses ANSI color `38;5;174` (muted pink/salmon, approx `#d78787`).
- Accompanying text (e.g., `"Thinking..."`) uses `38;5;216` (peach, approx `#ffaf87`).
- **Easing**: First and last characters hold slightly longer than middle characters
  (not uniform timing).
- Creates a "pulsing" effect rather than a uniform rotation.

### 11.2 Shimmer / Glimmer Animation

The `GlimmerMessage` component creates a shimmering effect:

- Text color oscillates between base color and shimmer color.
- Example: thinking verb shimmers between `claude` (`#D77757`) and
  `claudeShimmer` (`#EB9F7F`).
- The prompt border shimmers between `promptBorder` (`#888888`) and
  `promptBorderShimmer` (`#A6A6A6`).
- Can be disabled by setting `claudeShimmer` equal to `claude` via tweakcc.

### 11.3 Progress Indicators

- Context window usage: visual progress bar using filled blocks
  (`\u2593` = `▓` for filled, often custom rendering).
- Rate limit bar: `rate_limit_fill` / `rate_limit_empty` colors.
- Terminal title spinner: updates the terminal tab title with the
  spinner character during processing.

---

## 12. Welcome Screen / Logo

### 12.1 Startup Banner

Claude Code displays a filled-in ASCII art banner at startup:

```
   _____ _                 _         _____          _
  / ____| |               | |       / ____|        | |
 | |    | | __ _ _   _  __| | ___  | |     ___   __| | ___
 | |    | |/ _` | | | |/ _` |/ _ \ | |    / _ \ / _` |/ _ \
 | |____| | (_| | |_| | (_| |  __/ | |___| (_) | (_| |  __/
  \_____|_|\__,_|\__,_|\__,_|\___|  \_____\___/ \__,_|\___|
```

- Rendered in the `claude` brand color (`#D77757` orange/terracotta).
- The mascot ("Clawd") uses `clawd_body` (`#D77757`) on `clawd_background` (`#000000`).
- Rainbow gradient effect possible using the `rainbow_*` colors for decorative rendering.

### 12.2 Dashboard (LogoV2)

When idle, the dashboard may show:
- Welcome message
- Recent activity
- Release notes
- Project onboarding status
- Adapts layout based on terminal size

### 12.3 Condensed Logo

In narrow terminals or after first interaction, a condensed version is shown.

---

## 13. Message Visual Differentiation

### 13.1 User Messages

- Background tint: `userMessageBackground` (`#373737` dark, `#F0F0F0` light).
- Prefixed with `>` in the input area.
- Displayed with subtle background color distinction from assistant messages.

### 13.2 Assistant Messages

- No background tint (rendered on default terminal background).
- Text color: `text` (`#FFFFFF` dark, `#000000` light).
- Markdown rendered inline.
- Colored left-margin dots (subtle) for visual grouping of message types.

### 13.3 System Messages

- Rendered via `SystemTextMessage` component.
- Include duration information (e.g., `"Completed in 2.3s"`).
- Use `inactive` color (`#999999`).

### 13.4 Memory Blocks

- Background: `memoryBackgroundColor` (`#374146` dark).
- Used for CLAUDE.md content and auto-memory displays.

---

## 14. Key Visual Constants

### 14.1 Border Characters (Ink `round` style)

```
  \u256D = top-left corner     ╭
  \u256E = top-right corner    ╮
  \u2570 = bottom-left corner  ╰
  \u256F = bottom-right corner ╯
  \u2502 = vertical line       │
  \u2500 = horizontal line     ─
```

### 14.2 Color Emission

All colors emitted as 24-bit true-color ANSI escape sequences:
- Foreground: `\x1B[38;2;R;G;Bm`
- Background: `\x1B[48;2;R;G;Bm`
- These bypass the terminal's 16-color ANSI palette entirely.
- Users cannot adjust through terminal theme settings.

### 14.3 PR Status Underline Colors

| Status | Color |
|--------|-------|
| Approved | Green |
| Pending review | Yellow |
| Changes requested | Red |
| Draft | Gray |
| Merged | Purple |

---

## 15. Responsive Behavior

- Terminal dimensions detected at startup and on resize.
- Yoga layout engine handles automatic reflow.
- Dashboard adapts based on terminal width/height.
- Condensed logo for narrow terminals.
- Virtual scrolling for conversation (only visible messages rendered).

---

## Sources

- [Claude Code Overview](https://code.claude.com/docs/en/overview)
- [Fullscreen Rendering Docs](https://code.claude.com/docs/en/fullscreen)
- [Configure Permissions](https://code.claude.com/docs/en/permissions)
- [Terminal Configuration](https://code.claude.com/docs/en/terminal-config)
- [Status Line Customization](https://code.claude.com/docs/en/statusline)
- [Interactive Mode](https://code.claude.com/docs/en/interactive-mode)
- [Claude Code TUI Architecture (DeepWiki)](https://deepwiki.com/flyboyer/claude-code/8-terminal-ui-(tui)-architecture)
- [Claude Code /theme Explained](https://blog.vincentqiao.com/en/posts/claude-code-theme/)
- [Reverse Engineering Claude's ASCII Spinner](https://medium.com/@kyletmartinez/reverse-engineering-claudes-ascii-spinner-animation-eec2804626e0)
- [GitHub Issue #34702 - Color Customization](https://github.com/anthropics/claude-code/issues/34702)
- [GitHub Issue #26390 - Markdown Renderer](https://github.com/anthropics/claude-code/issues/26390)
- [GitHub Issue #36462 - Collapsible Sections](https://github.com/anthropics/claude-code/issues/36462)
- [GitHub Issue #6038 - Shimmer Effect](https://github.com/anthropics/claude-code/issues/6038)
- [tweakcc Theme System](https://github.com/Piebald-AI/tweakcc)
- [Community Themes](https://github.com/Piebald-AI/claude-code-themes)
- [Fixing Remote Colors](https://ranang.medium.com/fixing-claude-codes-flat-or-washed-out-remote-colors-82f8143351ed)
- Binary extraction: Claude Code v2.1.100 (`/Users/tien/.local/share/claude/versions/2.1.100`)
