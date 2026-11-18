//! The colour theme the window shows. The saved theme comes from `settings_get`. The sidebar button changes
//! the theme at once and saves it with `settings_set`; a save that the shell refuses is shown beside the button.
//!
//! The design (docs/New_Design/design.md) says light is the default, and that dark follows the system only
//! when the person has not chosen a theme. A saved setting that was never set means "follow the system".

use noct_core::ipc::settings::AppTheme;

/// A theme the window can show. It is written to `data-theme` on the `<html>` element, which selects the
/// colour tokens in `shell.css`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    /// The value of `data-theme`.
    pub fn attribute(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    /// The other theme, which the sidebar button switches to.
    pub fn toggled(self) -> Theme {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }

    /// The theme as the settings store it, for `settings_set`.
    pub fn setting(self) -> AppTheme {
        match self {
            Theme::Light => AppTheme::Light,
            Theme::Dark => AppTheme::Dark,
        }
    }

    /// The theme for a saved setting. A setting that was never chosen follows the system: dark only when the
    /// system prefers dark, light otherwise.
    pub fn from_setting(saved: Option<AppTheme>, system_prefers_dark: bool) -> Theme {
        match saved {
            Some(AppTheme::Light) => Theme::Light,
            Some(AppTheme::Dark) => Theme::Dark,
            None if system_prefers_dark => Theme::Dark,
            None => Theme::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_theme_is_shown_whatever_the_system_prefers() {
        assert_eq!(
            Theme::from_setting(Some(AppTheme::Light), true),
            Theme::Light
        );
        assert_eq!(
            Theme::from_setting(Some(AppTheme::Dark), false),
            Theme::Dark
        );
    }

    #[test]
    fn a_theme_never_chosen_follows_the_system() {
        assert_eq!(Theme::from_setting(None, true), Theme::Dark);
        assert_eq!(Theme::from_setting(None, false), Theme::Light);
    }

    #[test]
    fn toggling_twice_returns_to_the_same_theme() {
        assert_eq!(Theme::Light.toggled(), Theme::Dark);
        assert_eq!(Theme::Light.toggled().toggled(), Theme::Light);
        assert_eq!(Theme::Dark.toggled().toggled(), Theme::Dark);
    }

    #[test]
    fn a_theme_is_saved_as_the_setting_it_shows() {
        for theme in [Theme::Light, Theme::Dark] {
            assert_eq!(Theme::from_setting(Some(theme.setting()), true), theme);
        }
    }

    #[test]
    fn the_attribute_names_match_the_stylesheet() {
        assert_eq!(Theme::Light.attribute(), "light");
        assert_eq!(Theme::Dark.attribute(), "dark");
    }
}
