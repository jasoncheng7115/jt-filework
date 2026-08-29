//! Platform-neutral core of JT FileWork.
//!
//! This crate holds the model every other layer agrees on: file entries,
//! locations, machine-readable error codes, the localization contract and the
//! theme token contract.
//!
//! # Boundaries
//!
//! Per `AGENTS.md` §4 and §5 and ADR-0002, this crate must not depend on:
//!
//! - a GUI toolkit (Qt, Slint, AppKit, WinUI, GTK, WebView)
//! - a platform SDK
//! - the UI layer
//!
//! It must also contain no `cfg(target_os = ...)`. All three properties are
//! enforced by `tests/architecture.rs` (see `docs/TESTING.md` §3.2).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod error;
pub mod file;
pub mod i18n;
pub mod location;
pub mod theme;

pub use error::{Error, ErrorCode, Result};
pub use file::{Attributes, FileEntry, FileKind, PermissionsSummary, RawName, Timestamps};
pub use location::Location;
