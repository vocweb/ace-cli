use crossterm::event::{KeyEvent, MouseEvent};
use serde_json::Value;
use std::sync::mpsc;

/// Permission options presented to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOption {
    AllowOnce,
    Skip,
    AlwaysAllow,
}

/// User's response to a permission prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResponse {
    Allowed,
    Denied,
    AlwaysAllow,
}

/// Token usage info sent at end of turn.
#[derive(Debug, Clone)]
pub struct TurnUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Events flowing through the TUI event channel.
///
/// API thread sends streaming/tool/permission/turn events.
/// Crossterm reader thread sends terminal events.
pub enum TuiEvent {
    // --- Streaming ---
    /// A text token arrived from the assistant.
    StreamDelta(String),
    /// A thinking/reasoning token arrived.
    StreamThinkingDelta(String),
    /// Thinking block completed.
    StreamThinkingEnd,
    /// Flush the markdown buffer to render now.
    StreamMarkdownFlush,

    // --- Tool lifecycle ---
    /// A tool call is starting.
    ToolStart {
        name: String,
        input: Value,
    },
    /// A tool call completed with output.
    ToolResult {
        name: String,
        output: String,
        is_error: bool,
    },

    // --- Permission ---
    /// The runtime needs permission to execute a tool.
    /// The TUI should display a prompt and send the response back via `response_tx`.
    PermissionRequest {
        tool_name: String,
        description: String,
        options: Vec<PermissionOption>,
        response_tx: mpsc::Sender<PermissionResponse>,
    },

    // --- Turn lifecycle ---
    /// A new turn has started.
    TurnStart,
    /// The turn completed (possibly with usage info).
    TurnEnd {
        usage: Option<TurnUsage>,
    },
    /// The turn failed with an error message.
    TurnError(String),

    // --- Terminal events (from crossterm) ---
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TuiEvent>();
    }

    #[test]
    fn test_permission_option_equality() {
        assert_eq!(PermissionOption::AllowOnce, PermissionOption::AllowOnce);
        assert_ne!(PermissionOption::AllowOnce, PermissionOption::Skip);
    }

    #[test]
    fn test_permission_response_equality() {
        assert_eq!(PermissionResponse::Allowed, PermissionResponse::Allowed);
        assert_ne!(PermissionResponse::Allowed, PermissionResponse::Denied);
    }

    #[test]
    fn test_turn_usage_clone() {
        let usage = TurnUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        let cloned = usage.clone();
        assert_eq!(cloned.input_tokens, 100);
        assert_eq!(cloned.output_tokens, 50);
    }
}
