use ratatui::style::Color;

/// Claude Code spinner characters (Unicode).
/// Non-uniform timing: first and last hold longer.
pub const SPINNER_CHARS: &[char] = &[
    '\u{00B7}', '\u{273B}', '\u{273D}', '\u{2736}', '\u{2733}', '\u{2722}',
];

/// Tool running spinner: quarter-circle rotation.
pub const TOOL_SPINNER_CHARS: &[char] = &['\u{25D0}', '\u{25D3}', '\u{25D1}', '\u{25D2}'];

/// Rotating verbs shown during thinking (~2s each).
pub const THINKING_VERBS: &[&str] = &[
    "Thinking",
    "Pondering",
    "Considering",
    "Contemplating",
    "Crafting",
    "Composing",
    "Connecting",
    "Synthesizing",
    "Architecting",
    "Ideating",
    "Sketching",
    "Processing",
    "Mapping",
];

/// Brand colors for shimmer oscillation.
const SHIMMER_COLOR_A: Color = Color::Rgb(215, 119, 87); // #D77757
const SHIMMER_COLOR_B: Color = Color::Rgb(235, 159, 127); // #EB9F7F

/// Spinner state for the main thinking indicator.
pub struct SpinnerState {
    tick: usize,
}

impl SpinnerState {
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    pub fn current_char(&self) -> char {
        SPINNER_CHARS[self.tick % SPINNER_CHARS.len()]
    }

    pub fn tick(&mut self) {
        self.tick += 1;
    }
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Shimmer state: oscillates color + rotates thinking verbs.
pub struct ShimmerState {
    tick: usize,
    verb_index: usize,
}

impl ShimmerState {
    pub fn new() -> Self {
        Self {
            tick: 0,
            verb_index: 0,
        }
    }

    pub fn current_color(&self) -> Color {
        if self.tick % 2 == 0 {
            SHIMMER_COLOR_A
        } else {
            SHIMMER_COLOR_B
        }
    }

    pub fn current_verb(&self) -> &'static str {
        THINKING_VERBS[self.verb_index % THINKING_VERBS.len()]
    }

    pub fn tick(&mut self) {
        self.tick += 1;
    }

    pub fn tick_verb(&mut self) {
        self.verb_index += 1;
    }
}

impl Default for ShimmerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool execution spinner (◐ ◓ ◑ ◒).
pub struct ToolSpinnerState {
    tick: usize,
}

impl ToolSpinnerState {
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    pub fn current_char(&self) -> char {
        TOOL_SPINNER_CHARS[self.tick % TOOL_SPINNER_CHARS.len()]
    }

    pub fn tick(&mut self) {
        self.tick += 1;
    }
}

impl Default for ToolSpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Blinking cursor at end of streaming text.
pub struct StreamCursor {
    pub visible: bool,
}

impl StreamCursor {
    pub fn new() -> Self {
        Self { visible: true }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

impl Default for StreamCursor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_spinner_starts_at_first_char() {
        let s = SpinnerState::new();
        assert_eq!(s.current_char(), SPINNER_CHARS[0]);
    }

    #[test]
    fn test_spinner_cycles_chars() {
        let mut s = SpinnerState::new();
        let c1 = s.current_char();
        s.tick();
        let c2 = s.current_char();
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_spinner_wraps_around() {
        let mut s = SpinnerState::new();
        for _ in 0..SPINNER_CHARS.len() {
            s.tick();
        }
        // Should wrap back to first
        assert_eq!(s.current_char(), SPINNER_CHARS[0]);
    }

    #[test]
    fn test_shimmer_starts_at_first_verb() {
        let s = ShimmerState::new();
        assert_eq!(s.current_verb(), THINKING_VERBS[0]);
    }

    #[test]
    fn test_shimmer_alternates_colors() {
        let mut s = ShimmerState::new();
        let c1 = s.current_color();
        s.tick();
        let c2 = s.current_color();
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_shimmer_verb_rotates() {
        let mut s = ShimmerState::new();
        let v1 = s.current_verb();
        s.tick_verb();
        let v2 = s.current_verb();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_shimmer_verb_wraps() {
        let mut s = ShimmerState::new();
        for _ in 0..THINKING_VERBS.len() {
            s.tick_verb();
        }
        assert_eq!(s.current_verb(), THINKING_VERBS[0]);
    }

    #[test]
    fn test_tool_spinner_cycles() {
        let mut s = ToolSpinnerState::new();
        let c1 = s.current_char();
        s.tick();
        let c2 = s.current_char();
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_tool_spinner_wraps() {
        let mut s = ToolSpinnerState::new();
        for _ in 0..TOOL_SPINNER_CHARS.len() {
            s.tick();
        }
        assert_eq!(s.current_char(), TOOL_SPINNER_CHARS[0]);
    }

    #[test]
    fn test_stream_cursor_toggle() {
        let mut c = StreamCursor::new();
        assert!(c.visible);
        c.toggle();
        assert!(!c.visible);
        c.toggle();
        assert!(c.visible);
    }

    // Suppress unused import warning for Color in tests
    #[allow(dead_code)]
    fn _use_color(_: Color) {}
}
