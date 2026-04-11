use ratatui::style::Color;

/// Claude Code CLI dark theme — 24-bit true color palette.
#[derive(Debug, Clone, Copy)]
pub struct ClaudeTheme {
    pub brand: Color,              // #D77757 terracotta orange
    pub brand_shimmer: Color,      // #EB9F7F peach shimmer
    pub bash_border: Color,        // #FD5DB1 pink
    pub permission: Color,         // #B1B9F9 blue-lavender
    pub success: Color,            // #4EBA65 green
    pub error: Color,              // #FF6B80 coral red
    pub warning: Color,            // #FFD700 gold
    pub text_primary: Color,       // #FFFFFF white
    pub text_secondary: Color,     // #888888 grey
    pub text_muted: Color,         // #666666 dark grey
    pub input_border: Color,       // #888888 grey
    pub input_border_focus: Color, // #A6A6A6 light grey
    pub spinner: Color,            // #D78787 salmon
    pub spinner_text: Color,       // #FFAF87 peach
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
    /// Returns the appropriate border color for a given tool name.
    pub fn tool_border_color(&self, tool_name: &str) -> Color {
        match tool_name {
            "Bash" | "BashOutput" => self.bash_border,
            "Edit" | "Write" => self.brand,
            _ => self.text_secondary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn test_default_theme_brand_color() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.brand, Color::Rgb(215, 119, 87));
    }

    #[test]
    fn test_default_theme_all_colors_are_rgb() {
        let theme = ClaudeTheme::default();
        // All colors should be Rgb variants (24-bit true color)
        assert!(matches!(theme.brand, Color::Rgb(_, _, _)));
        assert!(matches!(theme.brand_shimmer, Color::Rgb(_, _, _)));
        assert!(matches!(theme.bash_border, Color::Rgb(_, _, _)));
        assert!(matches!(theme.permission, Color::Rgb(_, _, _)));
        assert!(matches!(theme.success, Color::Rgb(_, _, _)));
        assert!(matches!(theme.error, Color::Rgb(_, _, _)));
        assert!(matches!(theme.text_primary, Color::Rgb(_, _, _)));
        assert!(matches!(theme.text_secondary, Color::Rgb(_, _, _)));
        assert!(matches!(theme.text_muted, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_tool_border_color_bash() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.tool_border_color("Bash"), Color::Rgb(253, 93, 177));
        assert_eq!(
            theme.tool_border_color("BashOutput"),
            Color::Rgb(253, 93, 177)
        );
    }

    #[test]
    fn test_tool_border_color_edit() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.tool_border_color("Edit"), Color::Rgb(215, 119, 87));
        assert_eq!(theme.tool_border_color("Write"), Color::Rgb(215, 119, 87));
    }

    #[test]
    fn test_tool_border_color_read() {
        let theme = ClaudeTheme::default();
        assert_eq!(theme.tool_border_color("Read"), Color::Rgb(136, 136, 136));
        assert_eq!(theme.tool_border_color("Glob"), Color::Rgb(136, 136, 136));
        assert_eq!(theme.tool_border_color("Grep"), Color::Rgb(136, 136, 136));
    }

    #[test]
    fn test_tool_border_color_unknown() {
        let theme = ClaudeTheme::default();
        assert_eq!(
            theme.tool_border_color("SomeUnknownTool"),
            Color::Rgb(136, 136, 136)
        );
    }
}
