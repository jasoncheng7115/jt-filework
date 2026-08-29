//! Localization contract.
//!
//! `AGENTS.md` §11 makes i18n mandatory from the first UI string, and
//! `docs/I18N_THEME.md` fixes the rules: stable semantic keys, English
//! fallback, no sentence assembled from fragments, and detectable missing
//! translations.
//!
//! # Why a hand-written catalogue format
//!
//! The localization *framework* cannot be chosen yet: `TODO.md` P0 requires it
//! to be compatible with the GUI stack, and that is still open (ADR-0001).
//! Rather than block, this module defines a minimal dependency-free catalogue
//! and keeps every consumer behind [`Localizer`]. Swapping in Fluent or a
//! toolkit-native system later is a change to this module, not to the callers.

mod catalog;
mod localizer;

pub use catalog::{Catalog, CatalogError, Message};
pub use localizer::{Args, Localizer};

use serde::{Deserialize, Serialize};

/// A BCP-47-style locale identifier, e.g. `en` or `zh-TW`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LocaleId(String);

impl LocaleId {
    /// English. The fallback locale (`docs/I18N_THEME.md` §4).
    pub const EN: &'static str = "en";
    /// Taiwan Traditional Chinese.
    pub const ZH_TW: &'static str = "zh-TW";

    /// Wrap an identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The English fallback locale.
    pub fn english() -> Self {
        Self::new(Self::EN)
    }

    /// Taiwan Traditional Chinese.
    pub fn traditional_chinese() -> Self {
        Self::new(Self::ZH_TW)
    }

    /// Every locale the application ships, best match first.
    pub const SHIPPED: &'static [&'static str] = &[Self::EN, Self::ZH_TW];

    /// The identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The shipped locale that best matches a system locale tag.
    ///
    /// The tag is whatever the platform reports — `zh_TW`, `zh-Hant-TW`,
    /// `zh_Hant_HK`, `en_GB` — in whatever separator and case it favours, so
    /// this normalizes rather than compares.
    ///
    /// Traditional Chinese is matched by script or by region, not by the
    /// language alone: `zh` covers both scripts, and showing Traditional
    /// Chinese to a Simplified reader is not a smaller mistake than showing
    /// them English. A `zh` with neither a script nor a region we recognise
    /// therefore falls back rather than guessing.
    ///
    /// Anything unrecognised becomes English, which is the fallback locale
    /// (`docs/I18N_THEME.md` §4) rather than an error: a file manager that
    /// refuses to start because it does not speak your language is worse than
    /// one that starts in English.
    pub fn best_match(system_tag: &str) -> Self {
        Self::match_shipped(system_tag).unwrap_or_else(Self::english)
    }

    /// The best shipped locale for an ordered list of preferred tags.
    ///
    /// Platforms report a *list*, in the user's own order of preference, and
    /// only the list is trustworthy. macOS in particular reports a single
    /// "locale" that combines the region with the language the application
    /// was actually launched in: on a machine set to Traditional Chinese with
    /// a Taiwan region, an application with no Chinese bundle is told
    /// `en_TW`. Reading that one value would leave every such user in
    /// English, which is the bug this function exists to avoid.
    ///
    /// Each tag is tried in turn and the first one we ship wins, so a user who
    /// prefers a language we do not have still gets their second choice
    /// rather than the fallback.
    pub fn best_match_of<'a>(tags: impl IntoIterator<Item = &'a str>) -> Self {
        tags.into_iter()
            .find_map(Self::match_shipped)
            .unwrap_or_else(Self::english)
    }

    /// The shipped locale a tag names, or `None` when we ship nothing for it.
    fn match_shipped(system_tag: &str) -> Option<Self> {
        let normalized = system_tag.replace('_', "-").to_ascii_lowercase();
        let mut parts = normalized.split('-').filter(|p| !p.is_empty());
        let language = parts.next()?;

        if language == "en" {
            return Some(Self::english());
        }
        if language != "zh" {
            return None;
        }

        // Traditional regions and the explicit Hant script. Hong Kong and
        // Macau write Traditional; mainland China and Singapore write
        // Simplified, which we do not ship.
        parts
            .any(|part| matches!(part, "hant" | "tw" | "hk" | "mo"))
            .then(Self::traditional_chinese)
    }
}

impl core::fmt::Display for LocaleId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod locale_match_tests {
    use super::LocaleId;

    #[test]
    fn traditional_chinese_is_matched_however_the_platform_spells_it() {
        for tag in [
            "zh-TW",
            "zh_TW",
            "zh-Hant",
            "zh-Hant-TW",
            "zh_Hant_HK",
            "zh-HK",
            "zh-MO",
            "ZH-HANT-TW",
        ] {
            assert_eq!(
                LocaleId::best_match(tag),
                LocaleId::traditional_chinese(),
                "{tag} is a Traditional Chinese system"
            );
        }
    }

    #[test]
    fn simplified_chinese_falls_back_rather_than_being_shown_the_wrong_script() {
        for tag in ["zh-CN", "zh_Hans", "zh-Hans-CN", "zh-SG"] {
            assert_eq!(
                LocaleId::best_match(tag),
                LocaleId::english(),
                "{tag} writes Simplified, which is not shipped; showing it \
                 Traditional would be as wrong as showing it English, and \
                 English is at least honest about being a fallback"
            );
        }
    }

    #[test]
    fn a_bare_zh_is_not_assumed_to_be_traditional() {
        assert_eq!(
            LocaleId::best_match("zh"),
            LocaleId::english(),
            "`zh` names the language and neither script; guessing has a 50% \
             chance of being the wrong one"
        );
    }

    #[test]
    fn everything_else_is_english() {
        for tag in ["en", "en-GB", "en_US", "ja", "de-DE", "", "nonsense"] {
            assert_eq!(LocaleId::best_match(tag), LocaleId::english(), "{tag}");
        }
    }

    #[test]
    fn the_first_shipped_language_in_the_list_wins() {
        // What macOS actually reports on a Traditional Chinese machine with a
        // Taiwan region, for an application it does not know is localized.
        let macos = [
            "zh-Hant-TW",
            "zh-TW",
            "zh-Hant",
            "en-Latn-TW",
            "en-TW",
            "en",
        ];
        assert_eq!(
            LocaleId::best_match_of(macos),
            LocaleId::traditional_chinese()
        );
    }

    #[test]
    fn a_language_we_do_not_ship_is_skipped_rather_than_ending_the_search() {
        assert_eq!(
            LocaleId::best_match_of(["ja-JP", "zh-Hant-TW", "en"]),
            LocaleId::traditional_chinese(),
            "a user whose first choice we do not have should get their \
             second, not the fallback"
        );
    }

    #[test]
    fn an_empty_list_is_english() {
        assert_eq!(LocaleId::best_match_of([]), LocaleId::english());
    }

    #[test]
    fn every_shipped_locale_matches_itself() {
        for tag in LocaleId::SHIPPED {
            assert_eq!(
                LocaleId::best_match(tag).as_str(),
                *tag,
                "a shipped locale must match itself, or the picker and the \
                 detector disagree about what is available"
            );
        }
    }
}
