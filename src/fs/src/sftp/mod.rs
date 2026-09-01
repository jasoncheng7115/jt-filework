//! Browsing a host over SFTP (`docs/adr/0004-sftp.md`).
//!
//! Stage one: listing, navigating and reading. Nothing here writes to the
//! server; the write path is stage two, and until it exists the provider
//! refuses those operations rather than half-doing them.
//!
//! # Why this is a provider and not a special case
//!
//! `Provider` already promises exactly what a remote filesystem needs:
//! enumeration that runs off the UI thread, delivers rows in batches, and
//! stops when the token is cancelled. A network directory is the case that
//! promise was written for — the local one merely also benefits.
//!
//! # What is not stored
//!
//! No password reaches this module's memory for longer than the connection
//! attempt, and none is ever written anywhere. Authentication is by agent or
//! by key file, which is what a user who can `sftp host` already has.

pub mod connect;
pub mod host_keys;
pub mod provider;

pub use connect::{Connection, Endpoint, RemoteEntry, UnknownHostPolicy};
pub use host_keys::{known_hosts_path, verify, HostKeyVerdict};
pub use provider::SftpProvider;
