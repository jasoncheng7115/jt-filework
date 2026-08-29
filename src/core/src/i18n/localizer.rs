//! Resolving a key to display text.
//!
//! `docs/I18N_THEME.md` §4: English is the fallback, missing translations are
//! detectable in development, and a key that exists nowhere is an error rather
//! than an English string leaking into the UI.

use std::collections::BTreeMap;

use crate::error::{Error, ErrorCode, Result};

use super::{Catalog, LocaleId};

/// Named arguments for a message.
///
/// Sentences are never assembled from translated fragments
/// (`AGENTS.md` §11); values here are data such as counts, names and paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args(BTreeMap<String, String>);

impl Args {
    /// No arguments.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace an argument.
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(name.into(), value.into());
        self
    }

    /// The argument names supplied.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub(crate) const fn map(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

/// Resolves localization keys against a primary catalogue with an English
/// fallback.
#[derive(Debug, Clone)]
pub struct Localizer {
    primary: Catalog,
    fallback: Catalog,
}

impl Localizer {
    /// Build a localizer.
    ///
    /// `fallback` must be the English catalogue
    /// (`docs/I18N_THEME.md` §4). Passing the same catalogue for both is
    /// valid and is what the English locale does.
    pub fn new(primary: Catalog, fallback: Catalog) -> Self {
        Self { primary, fallback }
    }

    /// The active locale.
    pub const fn locale(&self) -> &LocaleId {
        self.primary.locale()
    }

    /// The fallback locale.
    pub const fn fallback_locale(&self) -> &LocaleId {
        self.fallback.locale()
    }

    /// Replace the primary catalogue, keeping the fallback.
    ///
    /// Switching locale must not disturb workspace state
    /// (`AGENTS.md` §11); this method deliberately touches nothing but the
    /// catalogue.
    pub fn set_primary(&mut self, primary: Catalog) {
        self.primary = primary;
    }

    /// Whether a key resolves in the primary catalogue.
    ///
    /// Used by development tooling to surface untranslated strings.
    pub fn is_translated(&self, key: &str) -> bool {
        self.primary.contains(key)
    }

    /// Resolve a key with no arguments.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::MissingLocalization`] when the key is absent from both the
    /// primary and the fallback catalogue.
    pub fn text(&self, key: &str) -> Result<String> {
        self.format(key, &Args::new())
    }

    /// Resolve a key and substitute arguments.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::MissingLocalization`] when the key is absent from both
    /// catalogues.
    pub fn format(&self, key: &str, args: &Args) -> Result<String> {
        let message = self.primary.get(key).or_else(|| self.fallback.get(key));
        let Some(message) = message else {
            // Detectability comes from the CI parity check
            // (docs/TESTING.md 3.3) and from `is_translated`, not from a
            // panic: a file manager must not die because a string is missing.
            return Err(Error::new(
                ErrorCode::MissingLocalization,
                format!("key `{key}` missing in `{}` and fallback", self.primary.locale()),
            ));
        };
        Ok(message.render(args.map()))
    }

    /// Resolve a key, falling back to the key itself if it is missing.
    ///
    /// For code paths that must render something rather than fail, such as a
    /// crash reporter. Never use this to paper over a missing translation.
    pub fn text_or_key(&self, key: &str) -> String {
        self.text(key).unwrap_or_else(|_| key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localizer(primary_text: &str) -> Localizer {
        let english = Catalog::parse(
            LocaleId::english(),
            "menu.file.open = Open\nmenu.file.rename = Rename\njobs.copying = Copying {count} items",
        )
        .unwrap();
        let primary = Catalog::parse(LocaleId::new(LocaleId::ZH_TW), primary_text).unwrap();
        Localizer::new(primary, english)
    }

    #[test]
    fn resolves_from_the_primary_catalogue() {
        let l = localizer("menu.file.open = \u{958b}\u{555f}");
        assert_eq!(l.text("menu.file.open").unwrap(), "\u{958b}\u{555f}");
    }

    #[test]
    fn falls_back_to_english_when_untranslated() {
        let l = localizer("menu.file.open = \u{958b}\u{555f}");
        assert_eq!(l.text("menu.file.rename").unwrap(), "Rename");
        assert!(!l.is_translated("menu.file.rename"), "gap must be detectable");
        assert!(l.is_translated("menu.file.open"));
    }

    #[test]
    fn a_key_missing_everywhere_is_a_typed_error_not_english_text() {
        let l = localizer("");
        let err = l.text("nope.not.here").unwrap_err();
        assert_eq!(err.code(), ErrorCode::MissingLocalization);
        // The last-resort renderer shows the key, never an invented sentence.
        assert_eq!(l.text_or_key("nope.not.here"), "nope.not.here");
    }

    #[test]
    fn arguments_are_substituted_through_the_fallback_too() {
        let l = localizer("");
        let out = l.format("jobs.copying", &Args::new().with("count", "7")).unwrap();
        assert_eq!(out, "Copying 7 items");
    }

    #[test]
    fn switching_locale_only_swaps_the_catalogue() {
        let mut l = localizer("menu.file.open = \u{958b}\u{555f}");
        assert_eq!(l.locale().as_str(), LocaleId::ZH_TW);

        let english = Catalog::parse(LocaleId::english(), "menu.file.open = Open").unwrap();
        l.set_primary(english);

        assert_eq!(l.locale().as_str(), LocaleId::EN);
        assert_eq!(l.text("menu.file.open").unwrap(), "Open");
        assert_eq!(l.fallback_locale().as_str(), LocaleId::EN);
    }
}
