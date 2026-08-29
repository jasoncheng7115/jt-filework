//! Theme contract.
//!
//! `AGENTS.md` §12 and `docs/I18N_THEME.md` §6–§8. UI code consumes semantic
//! tokens; it never names a colour. A literal colour in UI source is a test
//! failure (`docs/TESTING.md` §3.4), and this module is the only place a
//! colour value is allowed to exist.

use serde::{Deserialize, Serialize};

/// What the user chose.
///
/// `System` is the default (`docs/I18N_THEME.md` §6).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the operating system appearance.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// What the operating system currently reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemAppearance {
    /// The OS is in a light appearance.
    Light,
    /// The OS is in a dark appearance.
    Dark,
}

/// The appearance actually rendered, after resolving [`ThemeMode`] against the
/// system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedTheme {
    /// Render the light palette.
    Light,
    /// Render the dark palette.
    Dark,
}

impl ThemeMode {
    /// Resolve the user's choice against the current system appearance.
    pub const fn resolve(self, system: SystemAppearance) -> ResolvedTheme {
        match self {
            Self::Light => ResolvedTheme::Light,
            Self::Dark => ResolvedTheme::Dark,
            Self::System => match system {
                SystemAppearance::Light => ResolvedTheme::Light,
                SystemAppearance::Dark => ResolvedTheme::Dark,
            },
        }
    }

    /// Whether a system appearance change should repaint the UI.
    pub const fn follows_system(self) -> bool {
        matches!(self, Self::System)
    }

    /// Every mode, for exhaustive tests and settings UI.
    pub const ALL: &'static [Self] = &[Self::System, Self::Light, Self::Dark];

    /// Localization key for the mode's label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::System => "theme.system",
            Self::Light => "theme.light",
            Self::Dark => "theme.dark",
        }
    }
}

/// A semantic role a UI element can paint with.
///
/// Adding a variant is a deliberate act: both palettes must define it, which
/// [`Palette::light`] and [`Palette::dark`] enforce by being exhaustive
/// `match`es, and the completeness test proves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ThemeToken {
    /// Window background.
    SurfaceWindow,
    /// Pane background.
    SurfacePane,
    /// Preview / tool area background.
    SurfacePreview,
    /// Primary text.
    TextPrimary,
    /// Secondary / dimmed text.
    TextSecondary,
    /// Divider and border lines.
    Border,
    /// Selection in the focused pane.
    SelectionActive,
    /// Selection in an unfocused pane.
    SelectionInactive,
    /// CView-style mark. Must be distinguishable from selection
    /// (`AGENTS.md` §10, `docs/UI_UX_SPEC.md` §6).
    MarkActive,
    /// Focus ring.
    FocusRing,
    /// Indicator for the active pane.
    PaneActiveIndicator,
    /// Error state.
    StatusError,
    /// Warning state.
    StatusWarning,
    /// Success state.
    StatusSuccess,
}

impl ThemeToken {
    /// Every token. Used by the completeness test and by theme tooling.
    pub const ALL: &'static [Self] = &[
        Self::SurfaceWindow,
        Self::SurfacePane,
        Self::SurfacePreview,
        Self::TextPrimary,
        Self::TextSecondary,
        Self::Border,
        Self::SelectionActive,
        Self::SelectionInactive,
        Self::MarkActive,
        Self::FocusRing,
        Self::PaneActiveIndicator,
        Self::StatusError,
        Self::StatusWarning,
        Self::StatusSuccess,
    ];

    /// Stable name used in theme files and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SurfaceWindow => "surface.window",
            Self::SurfacePane => "surface.pane",
            Self::SurfacePreview => "surface.preview",
            Self::TextPrimary => "text.primary",
            Self::TextSecondary => "text.secondary",
            Self::Border => "border",
            Self::SelectionActive => "selection.active",
            Self::SelectionInactive => "selection.inactive",
            Self::MarkActive => "mark.active",
            Self::FocusRing => "focus.ring",
            Self::PaneActiveIndicator => "pane.active_indicator",
            Self::StatusError => "status.error",
            Self::StatusWarning => "status.warning",
            Self::StatusSuccess => "status.success",
        }
    }
}

/// Straight (non-premultiplied) 8-bit-per-channel colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel, 255 = opaque.
    pub a: u8,
}

impl Color {
    /// An opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A colour with alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Relative luminance per WCAG 2.x, used by the contrast check.
    pub fn relative_luminance(self) -> f64 {
        fn channel(v: u8) -> f64 {
            let c = f64::from(v) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG contrast ratio against another colour, from 1.0 to 21.0.
    ///
    /// Alpha is ignored; callers must compose against the intended background
    /// first.
    pub fn contrast_ratio(self, other: Self) -> f64 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        let (lighter, darker) = if a >= b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }
}

/// A complete set of token values for one resolved appearance.
///
/// These are Phase 0 placeholder values. Visual design happens after ADR-0001;
/// what matters now is that every token exists in both appearances and meets
/// the contrast rules in `docs/TESTING.md` §10.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    theme: ResolvedTheme,
}

impl Palette {
    /// The light palette.
    pub const fn light() -> Self {
        Self { theme: ResolvedTheme::Light }
    }

    /// The dark palette.
    pub const fn dark() -> Self {
        Self { theme: ResolvedTheme::Dark }
    }

    /// The palette for a resolved appearance.
    pub const fn for_theme(theme: ResolvedTheme) -> Self {
        match theme {
            ResolvedTheme::Light => Self::light(),
            ResolvedTheme::Dark => Self::dark(),
        }
    }

    /// Which appearance this palette paints.
    pub const fn theme(&self) -> ResolvedTheme {
        self.theme
    }

    /// The colour for a token.
    ///
    /// Total by construction: the `match` is exhaustive, so a new token cannot
    /// be added without a value in both appearances.
    pub const fn color(&self, token: ThemeToken) -> Color {
        match self.theme {
            ResolvedTheme::Light => Self::light_color(token),
            ResolvedTheme::Dark => Self::dark_color(token),
        }
    }

    const fn light_color(token: ThemeToken) -> Color {
        match token {
            ThemeToken::SurfaceWindow => Color::rgb(0xF6, 0xF6, 0xF7),
            ThemeToken::SurfacePane => Color::rgb(0xFF, 0xFF, 0xFF),
            ThemeToken::SurfacePreview => Color::rgb(0xF0, 0xF0, 0xF2),
            ThemeToken::TextPrimary => Color::rgb(0x14, 0x14, 0x16),
            ThemeToken::TextSecondary => Color::rgb(0x55, 0x55, 0x5C),
            ThemeToken::Border => Color::rgb(0xD2, 0xD2, 0xD7),
            ThemeToken::SelectionActive => Color::rgb(0x1E, 0x5A, 0xA8),
            ThemeToken::SelectionInactive => Color::rgb(0xD8, 0xDD, 0xE4),
            ThemeToken::MarkActive => Color::rgb(0xC2, 0x62, 0x00),
            ThemeToken::FocusRing => Color::rgb(0x0B, 0x4A, 0x9B),
            ThemeToken::PaneActiveIndicator => Color::rgb(0x17, 0x50, 0xB5),
            ThemeToken::StatusError => Color::rgb(0x9B, 0x1C, 0x1C),
            ThemeToken::StatusWarning => Color::rgb(0x7A, 0x4E, 0x00),
            ThemeToken::StatusSuccess => Color::rgb(0x14, 0x60, 0x2E),
        }
    }

    const fn dark_color(token: ThemeToken) -> Color {
        match token {
            ThemeToken::SurfaceWindow => Color::rgb(0x1A, 0x1A, 0x1D),
            ThemeToken::SurfacePane => Color::rgb(0x22, 0x22, 0x26),
            ThemeToken::SurfacePreview => Color::rgb(0x18, 0x18, 0x1B),
            ThemeToken::TextPrimary => Color::rgb(0xEC, 0xEC, 0xF0),
            ThemeToken::TextSecondary => Color::rgb(0xA8, 0xA8, 0xB2),
            ThemeToken::Border => Color::rgb(0x3A, 0x3A, 0x40),
            ThemeToken::SelectionActive => Color::rgb(0x2F, 0x6F, 0xC4),
            ThemeToken::SelectionInactive => Color::rgb(0x35, 0x35, 0x3C),
            ThemeToken::MarkActive => Color::rgb(0xE8, 0x8A, 0x3C),
            ThemeToken::FocusRing => Color::rgb(0x66, 0xA8, 0xFF),
            ThemeToken::PaneActiveIndicator => Color::rgb(0x8F, 0xC6, 0xFF),
            ThemeToken::StatusError => Color::rgb(0xFF, 0x8A, 0x84),
            ThemeToken::StatusWarning => Color::rgb(0xF0, 0xC0, 0x5A),
            ThemeToken::StatusSuccess => Color::rgb(0x6E, 0xD2, 0x8F),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_resolves_against_system_appearance() {
        assert_eq!(ThemeMode::Light.resolve(SystemAppearance::Dark), ResolvedTheme::Light);
        assert_eq!(ThemeMode::Dark.resolve(SystemAppearance::Light), ResolvedTheme::Dark);
        assert_eq!(ThemeMode::System.resolve(SystemAppearance::Dark), ResolvedTheme::Dark);
        assert_eq!(ThemeMode::System.resolve(SystemAppearance::Light), ResolvedTheme::Light);
    }

    #[test]
    fn only_system_mode_reacts_to_an_appearance_change() {
        assert!(ThemeMode::System.follows_system());
        assert!(!ThemeMode::Light.follows_system());
        assert!(!ThemeMode::Dark.follows_system());
    }

    #[test]
    fn default_mode_is_follow_system() {
        assert_eq!(ThemeMode::default(), ThemeMode::System);
    }

    #[test]
    fn every_token_has_a_value_in_both_palettes() {
        for &token in ThemeToken::ALL {
            let light = Palette::light().color(token);
            let dark = Palette::dark().color(token);
            assert_ne!(light, dark, "{} must differ between light and dark", token.as_str());
        }
    }

    #[test]
    fn token_names_are_unique_and_semantic() {
        let mut names: Vec<_> = ThemeToken::ALL.iter().map(|t| t.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate token name");
    }

    #[test]
    fn primary_text_meets_wcag_aa_on_pane_background_in_both_themes() {
        // docs/TESTING.md 10: contrast of text meets WCAG AA in both themes.
        for palette in [Palette::light(), Palette::dark()] {
            let text = palette.color(ThemeToken::TextPrimary);
            let bg = palette.color(ThemeToken::SurfacePane);
            let ratio = text.contrast_ratio(bg);
            assert!(ratio >= 4.5, "{:?}: contrast {ratio:.2} below AA", palette.theme());
        }
    }

    #[test]
    fn secondary_text_meets_wcag_aa_on_pane_background_in_both_themes() {
        for palette in [Palette::light(), Palette::dark()] {
            let text = palette.color(ThemeToken::TextSecondary);
            let bg = palette.color(ThemeToken::SurfacePane);
            let ratio = text.contrast_ratio(bg);
            assert!(ratio >= 4.5, "{:?}: contrast {ratio:.2} below AA", palette.theme());
        }
    }

    #[test]
    fn mark_is_distinguishable_from_selection_in_both_themes() {
        // AGENTS.md 10: selection and mark are different concepts, and the
        // difference must be visible, not just modelled.
        for palette in [Palette::light(), Palette::dark()] {
            let mark = palette.color(ThemeToken::MarkActive);
            let selection = palette.color(ThemeToken::SelectionActive);
            assert_ne!(mark, selection);
            let ratio = mark.contrast_ratio(selection);
            assert!(ratio >= 1.5, "{:?}: mark vs selection contrast {ratio:.2} too low", palette.theme());
        }
    }

    #[test]
    fn active_pane_indicator_is_visible_against_the_window_in_both_themes() {
        for palette in [Palette::light(), Palette::dark()] {
            let indicator = palette.color(ThemeToken::PaneActiveIndicator);
            let window = palette.color(ThemeToken::SurfaceWindow);
            let ratio = indicator.contrast_ratio(window);
            assert!(ratio >= 3.0, "{:?}: indicator contrast {ratio:.2} too low", palette.theme());
        }
    }

    #[test]
    fn contrast_ratio_is_symmetric_and_bounded() {
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        let ratio = black.contrast_ratio(white);
        assert!((ratio - 21.0).abs() < 0.01, "black on white is 21:1, got {ratio}");
        assert!((white.contrast_ratio(black) - ratio).abs() < f64::EPSILON);
        assert!((white.contrast_ratio(white) - 1.0).abs() < 0.001);
    }

    #[test]
    fn mode_round_trips_through_serde() {
        for &mode in ThemeMode::ALL {
            let json = serde_json::to_string(&mode).unwrap();
            let back: ThemeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }
}
