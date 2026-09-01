//! A session file this program did not write.
//!
//! The session lives in a file the user can edit, can copy between machines,
//! and can be handed one of by anyone who says "try my layout". Everything
//! that reads it is reading untrusted input, and the first thing untrusted
//! input does to a recursive reader is go deep.
//!
//! `docs/SECURITY.md` §13.3: recursion over untrusted data is bounded, and the
//! bound has a test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use jtf_core::Location;
use jtf_workspace::{Orientation, Session, SessionSettings, Workspace};

fn home() -> Location {
    Location::local("/Users/someone")
}

/// A real session with `splits` nested splits, built through the API that
/// builds them for a person, so everything about it is valid.
fn real_nested_session(splits: usize) -> String {
    let mut workspace = Workspace::new(home());
    for _ in 0..splits {
        workspace.split_active(Orientation::Horizontal);
    }
    Session::capture(&workspace, SessionSettings::default())
        .to_json()
        .expect("a real session serializes")
}

/// A window tree nested `depth` levels, done on the *text*.
///
/// Beyond a certain depth there is no such thing as a valid session - the
/// panes cannot all be distinct without a pane map to match - and that is
/// fine, because these fixtures exist to prove the reader does not *crash*.
/// A file that is refused for two reasons is still refused.
///
/// The nesting is textual because building a deep `serde_json::Value` and
/// asking for its string overflows the stack inside the test: `serde_json`
/// bounds its deserializer at 128 levels and does not bound its serializer at
/// all. Ours is never asked to write a deep tree, because `MAX_SPLIT_DEPTH`
/// stops one existing - but a test that crashes while building its own input
/// has proved nothing about the code it meant to test.
fn text_nested_session(depth: usize) -> String {
    let text = real_nested_session(1);
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let node = value["workspace"]["windows"]["1"].to_string();
    let leaf = value["workspace"]["windows"]["1"]["first"].to_string();

    let mut deep = node;
    for level in 0..depth {
        deep = format!(
            r#"{{"node":"split","id":{level},"orientation":"horizontal","ratio":0.5,"first":{deep},"second":{leaf}}}"#
        );
    }

    // Spliced in while the value is still shallow enough to serialize.
    value["workspace"]["windows"]["1"] = serde_json::Value::String("@NODE@".into());
    value.to_string().replace(r#""@NODE@""#, &deep)
}

/// The control. Without this passing, every test below could be refusing its
/// fixture for some reason that has nothing to do with depth.
#[test]
fn an_ordinary_nested_layout_restores() {
    let restored = Session::restore(Some(&real_nested_session(3)), &home());
    assert!(
        restored.outcome.restored(),
        "a four-pane layout is something a person really builds: {:?}",
        restored.outcome
    );
    assert_eq!(restored.workspace.pane_order().len(), 4);
}

/// Depth the parser accepts but our own invariants must not: well inside
/// `serde_json`'s 128 levels, well outside `MAX_SPLIT_DEPTH`. This is the case
/// our code has to catch itself - and it matters because the Qt layer builds
/// the layout by recursing over exactly this tree.
#[test]
fn a_tree_past_our_own_depth_limit_is_refused_by_the_invariants() {
    let restored = Session::restore(Some(&text_nested_session(64)), &home());
    assert!(!restored.outcome.restored());
    assert_eq!(
        restored.workspace.pane_order().len(),
        1,
        "what comes back is a usable window, not a half-built tree"
    );
}

/// And past the parser's own limit. The point is not which layer refuses it -
/// it is that the answer is an error rather than a process that is no longer
/// running. An overflow is not something a caller can handle.
#[test]
fn a_tree_ten_thousand_deep_is_an_error_rather_than_an_overflow() {
    let restored = Session::restore(Some(&text_nested_session(10_000)), &home());
    assert!(!restored.outcome.restored());
    assert_eq!(restored.workspace.pane_order().len(), 1);
}

/// Deeply nested JSON that is not a workspace at all. It goes through the
/// migration path, which walks a raw `serde_json::Value` before any of our
/// types exist - a second recursive reader over the same untrusted file.
#[test]
fn deeply_nested_json_in_the_migration_path_is_refused() {
    let mut json = String::from("null");
    for _ in 0..10_000 {
        json = format!(r#"{{"a":{json}}}"#);
    }
    // Version 1 forces the migration chain to run over the raw value.
    let text = format!(r#"{{"version":1,"junk":{json}}}"#);
    let restored = Session::restore(Some(&text), &home());
    assert!(!restored.outcome.restored());
    assert_eq!(restored.workspace.pane_order().len(), 1);
}

/// A very long flat file is not the same problem and must still work: bounding
/// depth must not turn into bounding size.
#[test]
fn a_wide_session_is_not_mistaken_for_a_deep_one() {
    let text = real_nested_session(8);
    let restored = Session::restore(Some(&text), &home());
    assert!(restored.outcome.restored(), "{:?}", restored.outcome);
    assert_eq!(restored.workspace.pane_order().len(), 9);
}
