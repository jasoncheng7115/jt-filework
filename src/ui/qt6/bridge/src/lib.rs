//! C ABI over the jt-filework core.
//!
//! The Qt 6 front end is C++ (`AGENTS.md` §18: native compiled code, and Qt
//! Widgets is a C++ API). This crate is the only place the two languages
//! meet.
//!
//! # Why this shape
//!
//! `AGENTS.md` §4 requires that core logic never depend on a GUI toolkit. So
//! the dependency runs one way only: C++ calls Rust, Rust knows nothing about
//! Qt. Every decision — what the split tree looks like, what rows exist, what
//! a key resolves to, what colour a token is — is made in Rust. The C++ side
//! draws and forwards input.
//!
//! # Unsafe
//!
//! The workspace denies `unsafe_code`. This crate is the documented
//! exception: a C ABI cannot exist without raw pointers, exactly as a platform
//! adapter cannot exist without the platform's own API
//! (`docs/SECURITY.md` §11). Every `unsafe` block here is small, and the
//! invariant it relies on is stated above it.
//!
//! # String convention
//!
//! Text is copied into a caller-provided buffer and the byte length is
//! returned; a return larger than the buffer means the text was truncated.
//! No allocation crosses the boundary, so there is no free function to forget
//! to call, and no per-row allocation during scrolling (`AGENTS.md` §18.2).

#![allow(
    unsafe_code,
    reason = "a C ABI requires raw pointers; see the module documentation"
)]
#![allow(
    clippy::missing_safety_doc,
    reason = "safety stated per function below"
)]

mod app;
mod ffi;
mod operations;

pub use app::App;
