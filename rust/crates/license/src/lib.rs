//! ACE license tier management.
//!
//! This crate provides the scaffolding for commercial license tiers.
//! Currently all features are enabled for all tiers (Community).
//! Pro and Enterprise tiers will be gated in future releases.

/// License tier for the ACE CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseTier {
    /// Free and open-source tier (MIT license).
    Community,
    /// Paid individual tier with additional features.
    Pro,
    /// Organization tier with team management, SSO, and SLA.
    Enterprise,
}

impl Default for LicenseTier {
    fn default() -> Self {
        Self::Community
    }
}

impl std::fmt::Display for LicenseTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Community => write!(f, "Community"),
            Self::Pro => write!(f, "Pro"),
            Self::Enterprise => write!(f, "Enterprise"),
        }
    }
}

/// Returns the current license tier.
///
/// In the future, this will validate a license key from the environment
/// or configuration. For now, all installations are Community tier.
#[must_use]
pub fn current_tier() -> LicenseTier {
    // Future: check ACE_LICENSE_KEY env var or config file
    LicenseTier::Community
}

/// Check if a feature is available for the given tier.
///
/// Currently returns `true` for all features (no gating).
#[must_use]
pub fn check_feature(_tier: LicenseTier, _feature: &str) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tier_is_community() {
        assert_eq!(LicenseTier::default(), LicenseTier::Community);
    }

    #[test]
    fn current_tier_returns_community() {
        assert_eq!(current_tier(), LicenseTier::Community);
    }

    #[test]
    fn all_features_enabled_for_now() {
        assert!(check_feature(LicenseTier::Community, "gemini"));
        assert!(check_feature(LicenseTier::Pro, "gemini"));
        assert!(check_feature(LicenseTier::Enterprise, "sso"));
    }

    #[test]
    fn display_format() {
        assert_eq!(format!("{}", LicenseTier::Community), "Community");
        assert_eq!(format!("{}", LicenseTier::Pro), "Pro");
        assert_eq!(format!("{}", LicenseTier::Enterprise), "Enterprise");
    }
}
