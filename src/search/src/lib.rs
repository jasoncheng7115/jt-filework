//! Deterministic search.
//!
//! `AGENTS.md` §15 is the rule this crate exists to keep: deterministic search
//! is first-class, and AI never silently replaces exact filename, wildcard,
//! regex, metadata, date, size or content matching. Everything here is exact
//! and repeatable; nothing here guesses.
//!
//! A query parses into a [`Query`] of typed terms, and a walk answers it.
//! Parsing is separate from walking so that a bad query fails instantly with a
//! precise message instead of after scanning a disk
//! (`docs/SEARCH_AI.md` §2.2).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod parse;
mod query;
mod walk;

pub use parse::{parse, ParseError};
pub use query::{Comparison, Query, SizeUnit, Term};
pub use walk::{search, SearchHandle, SearchUpdate};
