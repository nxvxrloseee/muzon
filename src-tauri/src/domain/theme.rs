use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub background: String,
    pub sidebar_background: String,
    pub player_bar_background: String,
    pub card_background: String,
    pub card_hover: String,
    pub accent_primary: String,
    pub accent_secondary: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub divider: String,
    pub progress_track: String,
    pub progress_fill: String,
    pub karaoke_inactive_line: String,
    pub karaoke_active_line: String,
    pub karaoke_active_word_highlight: String,
}

impl Theme {
    pub fn default_dark() -> Self {
        Self {
            background: "#0b0b0f".into(),
            sidebar_background: "#111116".into(),
            player_bar_background: "#15151b".into(),
            card_background: "#1a1a21".into(),
            card_hover: "#232330".into(),
            accent_primary: "#7c5cff".into(),
            accent_secondary: "#ff5c8a".into(),
            text_primary: "#f2f2f5".into(),
            text_secondary: "#9a9aa5".into(),
            divider: "#26262f".into(),
            progress_track: "#2a2a33".into(),
            progress_fill: "#7c5cff".into(),
            karaoke_inactive_line: "#6b6b76".into(),
            karaoke_active_line: "#f2f2f5".into(),
            karaoke_active_word_highlight: "#7c5cff".into(),
        }
    }

    pub fn default_light() -> Self {
        Self {
            background: "#f5f5f7".into(),
            sidebar_background: "#ffffff".into(),
            player_bar_background: "#ffffff".into(),
            card_background: "#ffffff".into(),
            card_hover: "#ececf0".into(),
            accent_primary: "#6a4cff".into(),
            accent_secondary: "#e0447a".into(),
            text_primary: "#17171c".into(),
            text_secondary: "#5c5c66".into(),
            divider: "#e2e2e8".into(),
            progress_track: "#e2e2e8".into(),
            progress_fill: "#6a4cff".into(),
            karaoke_inactive_line: "#9a9aa5".into(),
            karaoke_active_line: "#17171c".into(),
            karaoke_active_word_highlight: "#6a4cff".into(),
        }
    }

    /// Build a theme from a Material You scheme as desktop shells write it
    /// (caelestia, rice, matugen): role name -> hex, with or without '#'.
    /// Missing roles fall back to the built-in dark theme, so a partial or
    /// unfamiliar scheme still gives a usable window.
    pub fn from_material(colours: &std::collections::HashMap<String, String>) -> Self {
        let base = Theme::default_dark();
        let pick = |names: &[&str], fallback: &str| -> String {
            for name in names {
                if let Some(hex) = colours.get(*name) {
                    let hex = hex.trim();
                    let with_hash = if hex.starts_with('#') {
                        hex.to_string()
                    } else {
                        format!("#{hex}")
                    };
                    if is_valid_hex_color(&with_hash) {
                        return with_hash;
                    }
                }
            }
            fallback.to_string()
        };

        Self {
            background: pick(&["surface", "background"], &base.background),
            sidebar_background: pick(&["surfaceContainerLow", "surface"], &base.sidebar_background),
            player_bar_background: pick(&["surfaceContainer", "surface"], &base.player_bar_background),
            card_background: pick(&["surfaceContainerHigh", "surfaceContainer"], &base.card_background),
            card_hover: pick(&["surfaceContainerHighest", "surfaceBright"], &base.card_hover),
            accent_primary: pick(&["primary"], &base.accent_primary),
            accent_secondary: pick(&["tertiary", "secondary"], &base.accent_secondary),
            text_primary: pick(&["onSurface", "onBackground"], &base.text_primary),
            text_secondary: pick(&["onSurfaceVariant", "outline"], &base.text_secondary),
            divider: pick(&["outlineVariant", "outline"], &base.divider),
            progress_track: pick(&["surfaceContainerHighest", "surfaceVariant"], &base.progress_track),
            progress_fill: pick(&["primary"], &base.progress_fill),
            karaoke_inactive_line: pick(&["outline", "onSurfaceVariant"], &base.karaoke_inactive_line),
            karaoke_active_line: pick(&["onSurface"], &base.karaoke_active_line),
            karaoke_active_word_highlight: pick(&["primary"], &base.karaoke_active_word_highlight),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in self.as_pairs() {
            if !is_valid_hex_color(value) {
                return Err(format!("Некорректный цвет для {name}: {value}"));
            }
        }
        Ok(())
    }

    fn as_pairs(&self) -> [(&'static str, &str); 15] {
        [
            ("background", &self.background),
            ("sidebarBackground", &self.sidebar_background),
            ("playerBarBackground", &self.player_bar_background),
            ("cardBackground", &self.card_background),
            ("cardHover", &self.card_hover),
            ("accentPrimary", &self.accent_primary),
            ("accentSecondary", &self.accent_secondary),
            ("textPrimary", &self.text_primary),
            ("textSecondary", &self.text_secondary),
            ("divider", &self.divider),
            ("progressTrack", &self.progress_track),
            ("progressFill", &self.progress_fill),
            ("karaokeInactiveLine", &self.karaoke_inactive_line),
            ("karaokeActiveLine", &self.karaoke_active_line),
            ("karaokeActiveWordHighlight", &self.karaoke_active_word_highlight),
        ]
    }
}

fn is_valid_hex_color(s: &str) -> bool {
    let s = s.strip_prefix('#').unwrap_or(s);
    (s.len() == 6 || s.len() == 8) && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dark_and_light_validate() {
        assert!(Theme::default_dark().validate().is_ok());
        assert!(Theme::default_light().validate().is_ok());
    }

    #[test]
    fn rejects_bad_hex() {
        let mut theme = Theme::default_dark();
        theme.accent_primary = "not-a-color".into();
        assert!(theme.validate().is_err());
    }

    #[test]
    fn material_scheme_maps_to_roles() {
        let colours: std::collections::HashMap<String, String> = [
            ("surface", "0a0f0f"),
            ("surfaceContainer", "141a1a"),
            ("primary", "#9bd0cc"),
            ("onSurface", "dce8e6"),
            ("outlineVariant", "414847"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        let theme = Theme::from_material(&colours);
        assert_eq!(theme.background, "#0a0f0f");
        assert_eq!(theme.player_bar_background, "#141a1a");
        assert_eq!(theme.accent_primary, "#9bd0cc");
        assert_eq!(theme.progress_fill, "#9bd0cc");
        assert_eq!(theme.divider, "#414847");
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn material_falls_back_for_missing_and_broken_roles() {
        let dark = Theme::default_dark();
        let colours: std::collections::HashMap<String, String> = [("primary", "nonsense")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let theme = Theme::from_material(&colours);
        assert_eq!(theme.accent_primary, dark.accent_primary);
        assert_eq!(theme.background, dark.background);
        assert!(theme.validate().is_ok());
    }

    #[test]
    fn accepts_hex_without_hash_and_with_alpha() {
        let mut theme = Theme::default_dark();
        theme.accent_primary = "aabbccdd".into();
        assert!(theme.validate().is_ok());
    }
}
