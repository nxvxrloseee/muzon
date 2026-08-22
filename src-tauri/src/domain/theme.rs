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
    fn accepts_hex_without_hash_and_with_alpha() {
        let mut theme = Theme::default_dark();
        theme.accent_primary = "aabbccdd".into();
        assert!(theme.validate().is_ok());
    }
}
