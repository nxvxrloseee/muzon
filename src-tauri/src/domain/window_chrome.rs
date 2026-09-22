/// Session names that manage windows themselves - tiling compositors and window
/// managers, where the keyboard and the compositor's own bar do the work a
/// titlebar would, and a row of buttons is just a wasted line of pixels.
const TILING_SESSIONS: &[&str] = &[
    "hyprland", "sway", "river", "niri", "wayfire", "i3", "bspwm", "dwm", "awesome", "qtile",
    "xmonad", "spectrwm", "herbstluftwm",
];

/// Whether the app should draw its own minimise/maximise/close buttons.
///
/// The window is `decorations: false`, so nothing else draws them. On a desktop
/// that expects client-side decorations (GNOME, KDE, XFCE, Cinnamon, ...) that
/// leaves no way to minimise, maximise or even move the window, so we draw the
/// controls ourselves. Under a tiling compositor the same buttons are noise.
///
/// Unknown sessions get the controls: being unable to close a window is a much
/// worse failure than one redundant row.
pub fn draws_own_controls(
    xdg_current_desktop: Option<&str>,
    desktop_session: Option<&str>,
    hyprland_signature: Option<&str>,
) -> bool {
    if hyprland_signature.is_some() {
        return false;
    }
    // XDG_CURRENT_DESKTOP is colon-separated and often prefixed by the distro
    // ("ubuntu:GNOME"), so match per component rather than on the whole value.
    !([xdg_current_desktop, desktop_session])
        .into_iter()
        .flatten()
        .flat_map(|value| value.split(':'))
        .any(|name| {
            TILING_SESSIONS
                .iter()
                .any(|tiling| name.eq_ignore_ascii_case(tiling))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktops_with_client_side_decorations_get_controls() {
        assert!(draws_own_controls(Some("GNOME"), Some("gnome"), None));
        assert!(draws_own_controls(Some("KDE"), Some("plasmawayland"), None));
        assert!(draws_own_controls(Some("ubuntu:GNOME"), None, None));
        assert!(draws_own_controls(Some("XFCE"), None, None));
    }

    #[test]
    fn tiling_compositors_do_not() {
        assert!(!draws_own_controls(Some("Hyprland"), None, None));
        assert!(!draws_own_controls(Some("sway"), None, None));
        assert!(!draws_own_controls(None, Some("i3"), None));
    }

    #[test]
    fn hyprlands_own_marker_is_enough_on_its_own() {
        // Hyprland is often launched without XDG_CURRENT_DESKTOP set at all.
        assert!(!draws_own_controls(None, None, Some("abc123_1700000000")));
    }

    #[test]
    fn a_tiling_name_inside_a_composite_value_still_counts() {
        assert!(!draws_own_controls(Some("wlroots:sway"), None, None));
    }

    #[test]
    fn an_unknown_or_empty_session_gets_controls() {
        assert!(draws_own_controls(None, None, None));
        assert!(draws_own_controls(Some("SomethingNew"), None, None));
    }

    #[test]
    fn matching_is_case_insensitive_but_not_substring_based() {
        assert!(!draws_own_controls(Some("HYPRLAND"), None, None));
        // "i3" must not match a desktop that merely contains it.
        assert!(draws_own_controls(Some("Deepin3"), None, None));
    }
}
