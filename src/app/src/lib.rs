//! The application layer: everything between the core crates and a front end.
//!
//! Which pane is active and what it lists, what the marks resolve to and what
//! a command therefore acts on, the operation queue, transfers, the watcher,
//! the viewer and hex editor sessions, settings and the session file. A front
//! end draws what this says and hands it what the person did.
//!
//! It was the Qt bridge's `App` until ADR-0008, which moved it here unchanged
//! so that the terminal front end (`jtf-tui`) and the Qt window share one set
//! of rules. Nothing in it knows which front end it is serving, and nothing in
//! it may depend on a GUI toolkit or a desktop service.

pub mod app;
pub mod hexedit;
pub mod operations;
pub mod transfer;

pub use app::App;
