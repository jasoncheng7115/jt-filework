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
    /// Toolbar and list-header background.
    SurfaceHeader,
    /// Every other row, when alternating row colours are on.
    RowAlternate,
    /// Row under the pointer.
    RowHover,
    /// Primary text.
    TextPrimary,
    /// Secondary / dimmed text.
    TextSecondary,
    /// Text drawn on an accent fill, such as a selected row.
    TextOnAccent,
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
        Self::SurfaceHeader,
        Self::RowAlternate,
        Self::RowHover,
        Self::TextPrimary,
        Self::TextSecondary,
        Self::TextOnAccent,
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
            Self::SurfaceHeader => "surface.header",
            Self::RowAlternate => "row.alternate",
            Self::RowHover => "row.hover",
            Self::TextPrimary => "text.primary",
            Self::TextSecondary => "text.secondary",
            Self::TextOnAccent => "text.on_accent",
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
        Self {
            theme: ResolvedTheme::Light,
        }
    }

    /// The dark palette.
    pub const fn dark() -> Self {
        Self {
            theme: ResolvedTheme::Dark,
        }
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

    // Two roles may legitimately resolve to the same value in one appearance
    // and diverge in the other. Merging the arms would erase the semantic
    // mapping that is the whole point of a token, and would make changing one
    // role without the other a refactor instead of an edit.
    #[allow(
        clippy::match_same_arms,
        reason = "distinct roles, coincidentally equal values"
    )]
    const fn light_color(token: ThemeToken) -> Color {
        match token {
            ThemeToken::SurfaceWindow => Color::rgb(0xF2, 0xF3, 0xF5),
            ThemeToken::SurfacePane => Color::rgb(0xFF, 0xFF, 0xFF),
            ThemeToken::SurfacePreview => Color::rgb(0xF7, 0xF8, 0xFA),
            ThemeToken::SurfaceHeader => Color::rgb(0xEC, 0xEE, 0xF1),
            ThemeToken::RowAlternate => Color::rgb(0xF7, 0xF8, 0xFA),
            ThemeToken::RowHover => Color::rgb(0xED, 0xF1, 0xF7),
            ThemeToken::TextPrimary => Color::rgb(0x16, 0x18, 0x1D),
            ThemeToken::TextSecondary => Color::rgb(0x5B, 0x62, 0x70),
            ThemeToken::TextOnAccent => Color::rgb(0xFF, 0xFF, 0xFF),
            ThemeToken::Border => Color::rgb(0xDD, 0xE1, 0xE7),
            ThemeToken::SelectionActive => Color::rgb(0x2C, 0x6B, 0xD8),
            ThemeToken::SelectionInactive => Color::rgb(0xDC, 0xE1, 0xE8),
            ThemeToken::MarkActive => Color::rgb(0x7A, 0x2A, 0x00),
            ThemeToken::FocusRing => Color::rgb(0x2C, 0x6B, 0xD8),
            ThemeToken::PaneActiveIndicator => Color::rgb(0x2C, 0x6B, 0xD8),
            ThemeToken::StatusError => Color::rgb(0xB3, 0x26, 0x1E),
            ThemeToken::StatusWarning => Color::rgb(0x7A, 0x4E, 0x00),
            ThemeToken::StatusSuccess => Color::rgb(0x14, 0x6C, 0x2E),
        }
    }

    // Two roles may legitimately resolve to the same value in one appearance
    // and diverge in the other. Merging the arms would erase the semantic
    // mapping that is the whole point of a token, and would make changing one
    // role without the other a refactor instead of an edit.
    #[allow(
        clippy::match_same_arms,
        reason = "distinct roles, coincidentally equal values"
    )]
    const fn dark_color(token: ThemeToken) -> Color {
        match token {
            ThemeToken::SurfaceWindow => Color::rgb(0x1C, 0x1D, 0x21),
            ThemeToken::SurfacePane => Color::rgb(0x23, 0x24, 0x29),
            ThemeToken::SurfacePreview => Color::rgb(0x19, 0x1A, 0x1E),
            ThemeToken::SurfaceHeader => Color::rgb(0x26, 0x28, 0x2E),
            ThemeToken::RowAlternate => Color::rgb(0x26, 0x28, 0x2E),
            ThemeToken::RowHover => Color::rgb(0x2D, 0x30, 0x38),
            ThemeToken::TextPrimary => Color::rgb(0xE9, 0xEA, 0xEE),
            ThemeToken::TextSecondary => Color::rgb(0xA0, 0xA6, 0xB2),
            ThemeToken::TextOnAccent => Color::rgb(0xFF, 0xFF, 0xFF),
            ThemeToken::Border => Color::rgb(0x34, 0x36, 0x3D),
            ThemeToken::SelectionActive => Color::rgb(0x36, 0x70, 0xD0),
            ThemeToken::SelectionInactive => Color::rgb(0x33, 0x35, 0x3C),
            ThemeToken::MarkActive => Color::rgb(0xF0, 0xA4, 0x5A),
            ThemeToken::FocusRing => Color::rgb(0x7F, 0xB4, 0xFF),
            ThemeToken::PaneActiveIndicator => Color::rgb(0x7F, 0xB4, 0xFF),
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
        assert_eq!(
            ThemeMode::Light.resolve(SystemAppearance::Dark),
            ResolvedTheme::Light
        );
        assert_eq!(
            ThemeMode::Dark.resolve(SystemAppearance::Light),
            ResolvedTheme::Dark
        );
        assert_eq!(
            ThemeMode::System.resolve(SystemAppearance::Dark),
            ResolvedTheme::Dark
        );
        assert_eq!(
            ThemeMode::System.resolve(SystemAppearance::Light),
            ResolvedTheme::Light
        );
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
        // Tokens that are legitimately identical in both appearances, because
        // what they sit on is itself an accent rather than the background.
        const SAME_IN_BOTH: &[ThemeToken] = &[ThemeToken::TextOnAccent];

        for &token in ThemeToken::ALL {
            if SAME_IN_BOTH.contains(&token) {
                continue;
            }
            let light = Palette::light().color(token);
            let dark = Palette::dark().color(token);
            assert_ne!(
                light,
                dark,
                "{} must differ between light and dark, or be listed in SAME_IN_BOTH",
                token.as_str()
            );
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
            assert!(
                ratio >= 4.5,
                "{:?}: contrast {ratio:.2} below AA",
                palette.theme()
            );
        }
    }

    #[test]
    fn secondary_text_meets_wcag_aa_on_pane_background_in_both_themes() {
        for palette in [Palette::light(), Palette::dark()] {
            let text = palette.color(ThemeToken::TextSecondary);
            let bg = palette.color(ThemeToken::SurfacePane);
            let ratio = text.contrast_ratio(bg);
            assert!(
                ratio >= 4.5,
                "{:?}: contrast {ratio:.2} below AA",
                palette.theme()
            );
        }
    }

    #[test]
    fn text_on_accent_is_legible_on_the_selection_fill_in_both_themes() {
        for palette in [Palette::light(), Palette::dark()] {
            let text = palette.color(ThemeToken::TextOnAccent);
            let fill = palette.color(ThemeToken::SelectionActive);
            let ratio = text.contrast_ratio(fill);
            assert!(
                ratio >= 4.5,
                "{:?}: selected-row text contrast {ratio:.2}",
                palette.theme()
            );
        }
    }

    #[test]
    fn hover_and_alternate_rows_stay_subtle_against_the_pane() {
        // Visible enough to read as a band, quiet enough not to compete with
        // the selection.
        for palette in [Palette::light(), Palette::dark()] {
            let pane = palette.color(ThemeToken::SurfacePane);
            for token in [ThemeToken::RowAlternate, ThemeToken::RowHover] {
                let ratio = palette.color(token).contrast_ratio(pane);
                assert!(
                    ratio > 1.01,
                    "{:?}: {} is invisible",
                    palette.theme(),
                    token.as_str()
                );
                assert!(
                    ratio < 1.6,
                    "{:?}: {} is too loud",
                    palette.theme(),
                    token.as_str()
                );
            }
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
            assert!(
                ratio >= 1.5,
                "{:?}: mark vs selection contrast {ratio:.2} too low",
                palette.theme()
            );
        }
    }

    #[test]
    fn active_pane_indicator_is_visible_against_the_window_in_both_themes() {
        for palette in [Palette::light(), Palette::dark()] {
            let indicator = palette.color(ThemeToken::PaneActiveIndicator);
            let window = palette.color(ThemeToken::SurfaceWindow);
            let ratio = indicator.contrast_ratio(window);
            assert!(
                ratio >= 3.0,
                "{:?}: indicator contrast {ratio:.2} too low",
                palette.theme()
            );
        }
    }

    #[test]
    fn contrast_ratio_is_symmetric_and_bounded() {
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        let ratio = black.contrast_ratio(white);
        assert!(
            (ratio - 21.0).abs() < 0.01,
            "black on white is 21:1, got {ratio}"
        );
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
