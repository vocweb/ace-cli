use std::collections::HashMap;
use std::process::Command;
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
    pub project_path: String,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub tokens_used: u64,
    pub tokens_max: u64,
    pub model_name: String,
    pub active_tool: Option<(String, Instant)>,
    pub tool_counts: HashMap<String, u32>,
    pub todos_total: u32,
    pub todos_done: u32,
    pub current_task: Option<String>,
    pub agents: Vec<AgentStatus>,
    pub turn_start: Option<Instant>,
    pub dirty: bool,
}

impl HudState {
    pub fn new(model_name: impl Into<String>, project_path: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            project_path: project_path.into(),
            git_branch: None,
            git_dirty: false,
            tokens_used: 0,
            tokens_max: 200_000,
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

    /// Returns context window usage as a percentage (0-100).
    pub fn context_percent(&self) -> u8 {
        if self.tokens_max == 0 {
            return 0;
        }
        let pct = (self.tokens_used * 100) / self.tokens_max;
        u8::try_from(pct.min(100)).unwrap_or(100)
    }

    /// Returns a 4-block bar using ▰ (filled) and ▱ (empty).
    /// 0-24% → 1 filled, 25-49% → 2, 50-74% → 3, 75-100% → 4.
    pub fn context_bar(&self) -> String {
        let pct = self.context_percent();
        let filled = match pct {
            0..=24 => 1,
            25..=49 => 2,
            50..=74 => 3,
            _ => 4,
        };
        let mut bar = String::new();
        for i in 0..4 {
            if i < filled {
                bar.push('▰');
            } else {
                bar.push('▱');
            }
        }
        bar
    }

    /// Replaces $HOME prefix with ~ in the project path.
    pub fn short_path(&self) -> String {
        if let Ok(home) = std::env::var("HOME") {
            if self.project_path.starts_with(&home) {
                return format!("~{}", &self.project_path[home.len()..]);
            }
        }
        self.project_path.clone()
    }

    /// Formats a token count for display.
    /// <1000 → as-is, 1k-999k → "Xk", 1M+ → "X.YM".
    pub fn format_tokens(n: u64) -> String {
        if n < 1_000 {
            n.to_string()
        } else if n < 1_000_000 {
            format!("{}k", n / 1_000)
        } else {
            let whole = n / 1_000_000;
            let frac = (n % 1_000_000) / 100_000;
            format!("{whole}.{frac}M")
        }
    }

    /// Marks the start of a tool invocation.
    pub fn tool_start(&mut self, name: impl Into<String>) {
        let name = name.into();
        self.active_tool = Some((name, Instant::now()));
        self.dirty = true;
    }

    /// Marks the end of the current tool invocation and increments its count.
    pub fn tool_end(&mut self) {
        if let Some((name, _)) = self.active_tool.take() {
            let count = self.tool_counts.entry(name).or_insert(0);
            *count += 1;
        }
        self.dirty = true;
    }

    /// Updates token usage and sets dirty flag.
    pub fn update_tokens(&mut self, used: u64, max: u64) {
        self.tokens_used = used;
        self.tokens_max = max;
        self.dirty = true;
    }

    /// Records the start of a new turn.
    pub fn start_turn(&mut self) {
        self.turn_start = Some(Instant::now());
        self.dirty = true;
    }

    /// Returns the elapsed turn duration formatted as "X.Ys".
    pub fn turn_duration(&self) -> Option<String> {
        self.turn_start.map(|start| {
            let elapsed = start.elapsed();
            let secs = elapsed.as_secs_f64();
            format!("{secs:.1}s")
        })
    }

    /// Refreshes git branch and dirty status by running child processes.
    pub fn refresh_git(&mut self) {
        // Get current branch
        self.git_branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&self.project_path)
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                } else {
                    None
                }
            });

        // Check dirty status
        self.git_dirty = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.project_path)
            .output()
            .ok()
            .is_some_and(|out| out.status.success() && !out.stdout.is_empty());

        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_percent_zero() {
        let state = HudState::new("claude-3", "/tmp/project");
        assert_eq!(state.context_percent(), 0);
    }

    #[test]
    fn test_context_percent_half() {
        let mut state = HudState::new("claude-3", "/tmp/project");
        state.tokens_used = 100_000;
        assert_eq!(state.context_percent(), 50);
    }

    #[test]
    fn test_context_percent_max_zero() {
        let mut state = HudState::new("claude-3", "/tmp/project");
        state.tokens_max = 0;
        assert_eq!(state.context_percent(), 0);
    }

    #[test]
    fn test_context_bar_low() {
        let mut state = HudState::new("claude-3", "/tmp/project");
        state.tokens_used = 10_000; // 5%
        assert_eq!(state.context_bar(), "▰▱▱▱");
    }

    #[test]
    fn test_context_bar_half() {
        let mut state = HudState::new("claude-3", "/tmp/project");
        state.tokens_used = 100_000; // 50%
        assert_eq!(state.context_bar(), "▰▰▰▱");
    }

    #[test]
    fn test_context_bar_full() {
        let mut state = HudState::new("claude-3", "/tmp/project");
        state.tokens_used = 180_000; // 90%
        assert_eq!(state.context_bar(), "▰▰▰▰");
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
        let mut state = HudState::new("claude-3", "/tmp/project");
        assert!(state.active_tool.is_none());

        state.tool_start("Read");
        assert!(state.active_tool.is_some());
        assert_eq!(state.active_tool.as_ref().unwrap().0, "Read");

        state.tool_end();
        assert!(state.active_tool.is_none());
        assert_eq!(state.tool_counts.get("Read"), Some(&1));

        // Second invocation increments count
        state.tool_start("Read");
        state.tool_end();
        assert_eq!(state.tool_counts.get("Read"), Some(&2));
    }
}
