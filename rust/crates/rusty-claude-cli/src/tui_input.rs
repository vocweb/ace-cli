use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    /// Direct typing -- show all lines, auto-expand
    Typing,
    /// Pasted N lines -- collapse if > 5 lines
    Pasted(usize),
}

pub enum InputAction {
    /// Just redraw
    None,
    /// Enter pressed
    Submit(String),
    /// Ctrl+C or Ctrl+D on empty
    Exit,
    /// Content changed, recalculate zone height
    Changed,
}

pub struct TuiInput {
    pub buffer: String,
    /// Byte offset into `buffer`
    pub cursor: usize,
    pub mode: InputMode,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    /// Saved buffer when browsing history
    pub saved_buffer: Option<String>,
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

    /// Process a key event and return the resulting action.
    pub fn handle_key(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            // Enter without Shift -> Submit
            KeyCode::Enter
                if !key.modifiers.contains(KeyModifiers::SHIFT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
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

            // Shift+Enter -> insert newline
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.buffer.insert(self.cursor, '\n');
                self.cursor += 1;
                self.mode = InputMode::Typing;
                InputAction::Changed
            }

            // Ctrl+J -> insert newline
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.insert(self.cursor, '\n');
                self.cursor += 1;
                self.mode = InputMode::Typing;
                InputAction::Changed
            }

            // Ctrl+C -> Exit
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => InputAction::Exit,

            // Ctrl+D on empty -> Exit
            KeyCode::Char('d')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.buffer.is_empty() =>
            {
                InputAction::Exit
            }

            // Regular character input
            KeyCode::Char(c) => {
                self.buffer.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                self.mode = InputMode::Typing;
                InputAction::Changed
            }

            // Backspace -> delete before cursor
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let prev = self.prev_char_boundary(self.cursor);
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                    self.mode = InputMode::Typing;
                    InputAction::Changed
                } else {
                    InputAction::None
                }
            }

            // Delete -> delete after cursor
            KeyCode::Delete => {
                if self.cursor < self.buffer.len() {
                    let next = self.next_char_boundary(self.cursor);
                    self.buffer.drain(self.cursor..next);
                    self.mode = InputMode::Typing;
                    InputAction::Changed
                } else {
                    InputAction::None
                }
            }

            // Left arrow -> move cursor left
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.prev_char_boundary(self.cursor);
                }
                InputAction::None
            }

            // Right arrow -> move cursor right
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    self.cursor = self.next_char_boundary(self.cursor);
                }
                InputAction::None
            }

            // Up -> history navigation (previous)
            KeyCode::Up => {
                self.history_up();
                InputAction::Changed
            }

            // Down -> history navigation (next)
            KeyCode::Down => {
                self.history_down();
                InputAction::Changed
            }

            // Home -> cursor to start
            KeyCode::Home => {
                self.cursor = 0;
                InputAction::None
            }

            // End -> cursor to end
            KeyCode::End => {
                self.cursor = self.buffer.len();
                InputAction::None
            }

            _ => InputAction::None,
        }
    }

    /// Insert pasted text at the cursor position.
    /// If the resulting buffer has more than 5 lines, switch to Pasted mode.
    pub fn handle_paste(&mut self, text: String) {
        self.buffer.insert_str(self.cursor, &text);
        self.cursor += text.len();

        let line_count = self.buffer.lines().count().max(1);
        // Account for trailing newline
        let line_count = if self.buffer.ends_with('\n') {
            line_count + 1
        } else {
            line_count
        };

        if line_count > 5 {
            self.mode = InputMode::Pasted(line_count);
        } else {
            self.mode = InputMode::Typing;
        }
    }

    /// Return the number of display lines for the current buffer and mode.
    pub fn display_lines(&self) -> usize {
        match &self.mode {
            InputMode::Typing => {
                let count = self.buffer.lines().count().max(1);
                if self.buffer.ends_with('\n') {
                    count + 1
                } else {
                    count
                }
            }
            InputMode::Pasted(n) if *n > 5 => 1,
            InputMode::Pasted(n) => *n,
        }
    }

    /// Return the text to display for the current buffer and mode.
    pub fn display_text(&self) -> String {
        match &self.mode {
            InputMode::Pasted(n) if *n > 5 => format!("[Pasted {} lines]", n),
            _ => self.buffer.clone(),
        }
    }

    // -- History navigation --

    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Save current buffer and jump to most recent history entry
                self.saved_buffer = Some(self.buffer.clone());
                let idx = self.history.len() - 1;
                self.history_index = Some(idx);
                self.buffer = self.history[idx].clone();
                self.cursor = self.buffer.len();
            }
            Some(idx) if idx > 0 => {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.buffer = self.history[new_idx].clone();
                self.cursor = self.buffer.len();
            }
            _ => {} // Already at oldest entry
        }
    }

    fn history_down(&mut self) {
        match self.history_index {
            Some(idx) => {
                if idx + 1 < self.history.len() {
                    let new_idx = idx + 1;
                    self.history_index = Some(new_idx);
                    self.buffer = self.history[new_idx].clone();
                    self.cursor = self.buffer.len();
                } else {
                    // Restore saved buffer
                    self.history_index = None;
                    self.buffer = self.saved_buffer.take().unwrap_or_default();
                    self.cursor = self.buffer.len();
                }
            }
            None => {} // Not browsing history
        }
    }

    // -- Char boundary helpers (UTF-8 safety) --

    fn prev_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos.saturating_sub(1);
        while p > 0 && !self.buffer.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos + 1;
        while p < self.buffer.len() && !self.buffer.is_char_boundary(p) {
            p += 1;
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
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
        match action {
            InputAction::Submit(text) => assert_eq!(text, "hi"),
            _ => panic!("Expected Submit"),
        }
        assert_eq!(input.buffer, "");
        assert_eq!(input.history.len(), 1);
        assert_eq!(input.history[0], "hi");
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
        input.handle_key(key_with(KeyCode::Enter, KeyModifiers::SHIFT));
        input.handle_key(key(KeyCode::Char('b')));
        assert_eq!(input.buffer, "a\nb");
    }

    #[test]
    fn test_paste_collapse() {
        let mut input = TuiInput::new();
        let pasted = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10";
        input.handle_paste(pasted.to_string());
        assert_eq!(input.mode, InputMode::Pasted(10));
        assert_eq!(input.display_lines(), 1);
        assert_eq!(input.display_text(), "[Pasted 10 lines]");
    }

    #[test]
    fn test_paste_short() {
        let mut input = TuiInput::new();
        let pasted = "line1\nline2\nline3";
        input.handle_paste(pasted.to_string());
        assert_eq!(input.mode, InputMode::Typing);
        assert_eq!(input.display_lines(), 3);
    }

    #[test]
    fn test_history_navigation() {
        let mut input = TuiInput::new();

        // Submit "a"
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Enter));

        // Submit "b"
        input.handle_key(key(KeyCode::Char('b')));
        input.handle_key(key(KeyCode::Enter));

        // Up -> "b"
        input.handle_key(key(KeyCode::Up));
        assert_eq!(input.buffer, "b");

        // Up -> "a"
        input.handle_key(key(KeyCode::Up));
        assert_eq!(input.buffer, "a");

        // Down -> "b"
        input.handle_key(key(KeyCode::Down));
        assert_eq!(input.buffer, "b");

        // Down -> back to empty (restored saved buffer)
        input.handle_key(key(KeyCode::Down));
        assert_eq!(input.buffer, "");
    }

    #[test]
    fn test_exit_ctrl_c() {
        let mut input = TuiInput::new();
        let action = input.handle_key(key_with(KeyCode::Char('c'), KeyModifiers::CONTROL));
        match action {
            InputAction::Exit => {}
            _ => panic!("Expected Exit"),
        }
    }

    #[test]
    fn test_ctrl_d_exit_on_empty() {
        let mut input = TuiInput::new();
        let action = input.handle_key(key_with(KeyCode::Char('d'), KeyModifiers::CONTROL));
        match action {
            InputAction::Exit => {}
            _ => panic!("Expected Exit on empty buffer"),
        }
    }

    #[test]
    fn test_ctrl_d_no_exit_with_content() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('x')));
        let action = input.handle_key(key_with(KeyCode::Char('d'), KeyModifiers::CONTROL));
        // Ctrl+D with content should not exit
        match action {
            InputAction::Exit => panic!("Should not exit when buffer has content"),
            _ => {}
        }
    }

    #[test]
    fn test_left_right_cursor() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Char('b')));
        input.handle_key(key(KeyCode::Char('c')));
        assert_eq!(input.cursor, 3);

        input.handle_key(key(KeyCode::Left));
        assert_eq!(input.cursor, 2);

        input.handle_key(key(KeyCode::Left));
        assert_eq!(input.cursor, 1);

        input.handle_key(key(KeyCode::Right));
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_home_end() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Char('b')));
        input.handle_key(key(KeyCode::Char('c')));

        input.handle_key(key(KeyCode::Home));
        assert_eq!(input.cursor, 0);

        input.handle_key(key(KeyCode::End));
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn test_delete_key() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Char('b')));
        input.handle_key(key(KeyCode::Char('c')));
        // Move cursor to start
        input.handle_key(key(KeyCode::Home));
        // Delete first char
        input.handle_key(key(KeyCode::Delete));
        assert_eq!(input.buffer, "bc");
    }

    #[test]
    fn test_utf8_handling() {
        let mut input = TuiInput::new();
        // Insert a multi-byte character
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key(KeyCode::Char('\u{00e9}'))); // e-acute (2 bytes)
        input.handle_key(key(KeyCode::Char('b')));
        assert_eq!(input.buffer, "a\u{00e9}b");
        assert_eq!(input.cursor, 4); // 1 + 2 + 1

        // Move left past the multi-byte char
        input.handle_key(key(KeyCode::Left)); // at 'b' -> before 'b'
        assert_eq!(input.cursor, 3);
        input.handle_key(key(KeyCode::Left)); // before e-acute
        assert_eq!(input.cursor, 1);

        // Backspace should delete the e-acute
        input.handle_key(key(KeyCode::Right)); // back to after e-acute
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.buffer, "ab");
    }

    #[test]
    fn test_ctrl_j_newline() {
        let mut input = TuiInput::new();
        input.handle_key(key(KeyCode::Char('a')));
        input.handle_key(key_with(KeyCode::Char('j'), KeyModifiers::CONTROL));
        input.handle_key(key(KeyCode::Char('b')));
        assert_eq!(input.buffer, "a\nb");
    }
}
