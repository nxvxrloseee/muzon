use serde::{Deserialize, Serialize};
use specta::Type;

/// Below this the text over the desktop behind stops being readable.
pub const MIN_BACKGROUND_OPACITY: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    /// Whether the window is created with an alpha channel. WebKitGTK can only
    /// be given one when the window is created, so a change takes effect on the
    /// next launch. Off by default: an opaque window is cheaper to composite.
    pub transparent_window: bool,
    /// How opaque the window's backgrounds are while it is transparent, from
    /// `MIN_BACKGROUND_OPACITY` to 1.
    pub background_opacity: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            transparent_window: false,
            background_opacity: 0.85,
        }
    }
}

/// The file as stored. Separate from `Appearance` because `#[serde(default)]`
/// there would make every field optional in the generated TypeScript too.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    transparent_window: Option<bool>,
    background_opacity: Option<f64>,
}

impl Appearance {
    /// Reads a saved file, filling whatever it lacks (a file from an older
    /// version, a hand edit) from the defaults. `None` only for unreadable JSON.
    pub fn from_json(json: &str) -> Option<Self> {
        let stored: Stored = serde_json::from_str(json).ok()?;
        let default = Self::default();
        Some(
            Self {
                transparent_window: stored
                    .transparent_window
                    .unwrap_or(default.transparent_window),
                background_opacity: stored
                    .background_opacity
                    .unwrap_or(default.background_opacity),
            }
            .normalized(),
        )
    }

    /// Clamps the opacity into range rather than rejecting it: a hand-edited
    /// file or a slider overshoot should still give a usable window.
    pub fn normalized(self) -> Self {
        let opacity = if self.background_opacity.is_finite() {
            self.background_opacity.clamp(MIN_BACKGROUND_OPACITY, 1.0)
        } else {
            Self::default().background_opacity
        };
        Self {
            background_opacity: opacity,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_is_clamped_into_range() {
        let low = Appearance {
            transparent_window: true,
            background_opacity: 0.0,
        };
        assert_eq!(low.normalized().background_opacity, MIN_BACKGROUND_OPACITY);
        let high = Appearance {
            transparent_window: true,
            background_opacity: 7.0,
        };
        assert_eq!(high.normalized().background_opacity, 1.0);
        let nan = Appearance {
            transparent_window: true,
            background_opacity: f64::NAN,
        };
        assert_eq!(
            nan.normalized().background_opacity,
            Appearance::default().background_opacity
        );
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed = Appearance::from_json(r#"{"transparentWindow":true}"#).unwrap();
        assert!(parsed.transparent_window);
        assert_eq!(
            parsed.background_opacity,
            Appearance::default().background_opacity
        );
        assert!(Appearance::from_json("not json").is_none());
    }
}
