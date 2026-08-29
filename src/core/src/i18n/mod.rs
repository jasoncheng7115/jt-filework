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

    /// The identifier as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for LocaleId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
