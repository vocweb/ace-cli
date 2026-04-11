const MAX_VISIBLE: usize = 8;

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
