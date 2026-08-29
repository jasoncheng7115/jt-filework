//! Filesystem providers.
//!
//! A provider turns a [`Location`](jtf_core::Location) into
//! [`FileEntry`](jtf_core::FileEntry) rows. The local filesystem is one
//! provider; archives and search results will be others, which is why this is
//! a trait rather than a function.
//!
//! # Never on the UI thread
//!
//! `AGENTS.md` §3 forbids directory enumeration on the UI thread, and means
//! it: a directory on a stalled network mount can block for minutes. So the
//! interesting API here is [`enumerate_async`], which delivers rows in
//! batches, honours a [`CancellationToken`](jtf_jobs::CancellationToken) at
//! every entry, and can be abandoned without waiting.
//!
//! [`Provider::list`] exists for tests and for callers that genuinely have a
//! small, local, known-good directory. It is not the normal path.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod folder_size;
mod local;
mod provider;

pub use folder_size::{measure, FolderSize, SizeCache, FRESH_FOR, MAX_CACHED, MAX_DEPTH};
pub use local::LocalProvider;
pub use provider::{Batch, EnumerationHandle, Provider};
