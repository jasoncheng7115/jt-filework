//! Machine-readable error codes with a separate localized display text.
//!
//! `docs/I18N_THEME.md` §4: "Error code and localized error message are
//! separate." Code is stable and never translated; the display text is looked
//! up from the localization catalogue by key.

use core::fmt;

/// Stable, machine-readable error identity.
///
/// Codes are part of the public contract: they appear in logs, in job records
/// and in tests. Never renumber or repurpose one; add a new variant instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The requested path or item does not exist.
    NotFound,
    /// The operation was refused by the platform for permission reasons.
    PermissionDenied,
    /// A destination entry already exists and the caller must decide.
    AlreadyExists,
    /// The item is not of the kind the operation requires.
    WrongKind,
    /// A path was syntactically invalid or escaped its allowed root.
    InvalidPath,
    /// A limit declared in `docs/SECURITY.md` §10 was exceeded.
    LimitExceeded,
    /// The operation was cancelled by the user or by a superseding request.
    Cancelled,
    /// The operation timed out.
    TimedOut,
    /// The underlying device or mount is unavailable or stalled.
    DeviceUnavailable,
    /// Input from an untrusted source could not be parsed.
    ParseFailed,
    /// A localization key was missing from every catalogue including fallback.
    MissingLocalization,
    /// The requested capability is not supported on this platform.
    Unsupported,
    /// An external provider or helper process failed.
    ProviderFailed,
    /// An I/O failure that does not map to a more specific code.
    Io,
    /// An invariant of this program was violated. Always a bug.
    Internal,
}

impl ErrorCode {
    /// The stable string form used in logs, job records and tests.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "E_NOT_FOUND",
            Self::PermissionDenied => "E_PERMISSION_DENIED",
            Self::AlreadyExists => "E_ALREADY_EXISTS",
            Self::WrongKind => "E_WRONG_KIND",
            Self::InvalidPath => "E_INVALID_PATH",
            Self::LimitExceeded => "E_LIMIT_EXCEEDED",
            Self::Cancelled => "E_CANCELLED",
            Self::TimedOut => "E_TIMED_OUT",
            Self::DeviceUnavailable => "E_DEVICE_UNAVAILABLE",
            Self::ParseFailed => "E_PARSE_FAILED",
            Self::MissingLocalization => "E_MISSING_LOCALIZATION",
            Self::Unsupported => "E_UNSUPPORTED",
            Self::ProviderFailed => "E_PROVIDER_FAILED",
            Self::Io => "E_IO",
            Self::Internal => "E_INTERNAL",
        }
    }

    /// Localization key for the user-visible message.
    ///
    /// The key is derived from the code so a new code cannot be added without
    /// a corresponding catalogue entry; the parity test in
    /// `src/core/src/i18n` will fail until both locales define it.
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::NotFound => "error.not_found",
            Self::PermissionDenied => "error.permission_denied",
            Self::AlreadyExists => "error.already_exists",
            Self::WrongKind => "error.wrong_kind",
            Self::InvalidPath => "error.invalid_path",
            Self::LimitExceeded => "error.limit_exceeded",
            Self::Cancelled => "error.cancelled",
            Self::TimedOut => "error.timed_out",
            Self::DeviceUnavailable => "error.device_unavailable",
            Self::ParseFailed => "error.parse_failed",
            Self::MissingLocalization => "error.missing_localization",
            Self::Unsupported => "error.unsupported",
            Self::ProviderFailed => "error.provider_failed",
            Self::Io => "error.io",
            Self::Internal => "error.internal",
        }
    }

    /// Every code, for exhaustive tests and catalogue parity checks.
    pub const ALL: &'static [Self] = &[
        Self::NotFound,
        Self::PermissionDenied,
        Self::AlreadyExists,
        Self::WrongKind,
        Self::InvalidPath,
        Self::LimitExceeded,
        Self::Cancelled,
        Self::TimedOut,
        Self::DeviceUnavailable,
        Self::ParseFailed,
        Self::MissingLocalization,
        Self::Unsupported,
        Self::ProviderFailed,
        Self::Io,
        Self::Internal,
    ];
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An error carrying a stable code and developer-facing context.
///
/// The `context` field is **never** shown to users. User-visible text comes
/// from `code.message_key()` resolved through the localizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    code: ErrorCode,
    context: String,
}

impl Error {
    /// Create an error with developer-facing context.
    pub fn new(code: ErrorCode, context: impl Into<String>) -> Self {
        Self {
            code,
            context: context.into(),
        }
    }

    /// Create an error with no additional context.
    pub fn bare(code: ErrorCode) -> Self {
        Self {
            code,
            context: String::new(),
        }
    }

    /// The stable machine-readable code.
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Developer-facing context. Not for display to users.
    pub fn context(&self) -> &str {
        &self.context
    }

    /// Localization key for the user-visible message.
    pub const fn message_key(&self) -> &'static str {
        self.code.message_key()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.context.is_empty() {
            write!(f, "{}", self.code)
        } else {
            write!(f, "{}: {}", self.code, self.context)
        }
    }
}

impl std::error::Error for Error {}

/// Convenience alias used throughout the workspace.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_are_unique_and_stable() {
        let strings: HashSet<_> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            strings.len(),
            ErrorCode::ALL.len(),
            "duplicate error code string"
        );

        let keys: HashSet<_> = ErrorCode::ALL.iter().map(|c| c.message_key()).collect();
        assert_eq!(keys.len(), ErrorCode::ALL.len(), "duplicate message key");
    }

    #[test]
    fn every_code_string_has_the_expected_shape() {
        for code in ErrorCode::ALL {
            assert!(code.as_str().starts_with("E_"), "{code} must start with E_");
            assert!(
                code.message_key().starts_with("error."),
                "{code} key must be error.*"
            );
        }
    }

    #[test]
    fn display_separates_code_from_context() {
        let e = Error::new(ErrorCode::NotFound, "/tmp/missing");
        assert_eq!(e.code(), ErrorCode::NotFound);
        assert_eq!(e.to_string(), "E_NOT_FOUND: /tmp/missing");
        assert_eq!(Error::bare(ErrorCode::Io).to_string(), "E_IO");
    }

    #[test]
    fn user_visible_text_is_a_key_not_english() {
        // AGENTS.md 11: no user-visible English literal escapes from core.
        let e = Error::bare(ErrorCode::PermissionDenied);
        assert_eq!(e.message_key(), "error.permission_denied");
    }
}
