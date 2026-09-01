//! Upgrade behaviour — `docs/UPGRADE.md` §10.
//!
//! These are about the moment a user updates and finds their bookmarks,
//! layout and keys either intact or gone. Every case here is one somebody
//! will actually meet: an old file, a newer file, a file half-written when
//! the machine lost power.
//!
//! The fixtures matter more than the assertions. Each released format version
//! leaves a real file in `tests/fixtures/session/vN.json` and it stays there
//! forever; a fixture edited to make a test pass has stopped being evidence.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use jtf_core::Location;
use jtf_workspace::{RestoreOutcome, Session, SESSION_FORMAT_VERSION};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture(version: u32) -> String {
    let path = repo_root()
        .join("tests/fixtures/session")
        .join(format!("v{version}.json"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn home() -> Location {
    Location::local("/Users/someone")
}

#[test]
fn every_released_format_version_has_a_fixture() {
    // The chain can only be trusted as far back as there is a real file to
    // read. A missing fixture means a version nobody can prove still loads.
    for version in 1..=SESSION_FORMAT_VERSION {
        let path = repo_root()
            .join("tests/fixtures/session")
            .join(format!("v{version}.json"));
        assert!(
            path.is_file(),
            "no fixture for session format v{version}: {}",
            path.display()
        );
    }
}

#[test]
fn a_v1_session_still_loads() {
    let restored = Session::restore(Some(&fixture(1)), &home());
    assert!(
        !restored.outcome.needs_notice(),
        "a file from a released version must load without complaint, got {:?}",
        restored.outcome
    );
    assert!(restored.settings.folders_first);
    assert_eq!(
        restored.places.bookmarks().len(),
        1,
        "the bookmark someone made in v1 survives the upgrade; losing it is \
         exactly what this whole chain exists to prevent"
    );
    assert_eq!(restored.places.bookmarks()[0].display_name(), "Projects");
}

#[test]
fn settings_added_since_v1_take_their_defaults() {
    // The v1 fixture predates thumbnails, the key hint strip, the inspector
    // and the locale preference. Every one of them must arrive at its
    // documented default rather than at whatever zero happens to mean.
    let restored = Session::restore(Some(&fixture(1)), &home());
    assert!(
        restored.settings.thumbnails,
        "thumbnails are on by default, and an upgrade must not turn a \
         feature off just because the old file never mentioned it"
    );
    assert!(
        restored.settings.key_hints_visible,
        "the v1->v2 step brings the hint strip up to its new default; a v1 \
         file records the field explicitly, so without the step the new \
         default would reach nobody who had ever run the program"
    );
    assert!(!restored.settings.inspector_visible);
    assert!(
        restored.settings.locale.is_empty(),
        "no stored choice means follow the system"
    );
}

/// The current format, read by the build that writes it. Trivial today and
/// not trivial next time: this is the test that fails when a field is renamed
/// without a migration step, which is the mistake the chain exists to catch.
#[test]
fn the_current_format_version_loads_from_its_own_fixture() {
    let restored = Session::restore(Some(&fixture(SESSION_FORMAT_VERSION)), &home());
    assert!(
        !restored.outcome.needs_notice(),
        "the current format must load without complaint, got {:?}",
        restored.outcome
    );
}

/// Every fixture, not only the two named above. A version added later gets
/// this for free, which is the point: the guarantee is about the chain, not
/// about whichever versions someone remembered to write a test for.
#[test]
fn every_fixture_walks_the_chain_to_something_usable() {
    for version in 1..=SESSION_FORMAT_VERSION {
        let restored = Session::restore(Some(&fixture(version)), &home());
        assert!(
            !restored.outcome.needs_notice(),
            "v{version} did not survive the chain: {:?}",
            restored.outcome
        );
        assert!(
            restored.workspace.invariants_hold(),
            "v{version} migrated into a workspace that does not hold together"
        );
    }
}

#[test]
fn an_unknown_field_is_ignored_rather_than_fatal() {
    // A file written by a *newer* build of the same format version - two
    // machines on a synced folder, one updated first.
    let mut value: serde_json::Value = serde_json::from_str(&fixture(1)).unwrap();
    value["settings"]["a_setting_from_the_future"] = serde_json::json!("hello");
    let restored = Session::restore(Some(&value.to_string()), &home());
    assert!(
        !restored.outcome.needs_notice(),
        "an unrecognised setting is not a reason to throw away the file it \
         arrived in"
    );
    assert!(restored.settings.folders_first);
}

#[test]
fn a_newer_format_version_starts_fresh_and_says_so() {
    let mut value: serde_json::Value = serde_json::from_str(&fixture(1)).unwrap();
    value["version"] = serde_json::json!(SESSION_FORMAT_VERSION + 1);
    let restored = Session::restore(Some(&value.to_string()), &home());

    assert!(
        matches!(restored.outcome, RestoreOutcome::UnsupportedVersion(_)),
        "an older build must not guess at a newer format, got {:?}",
        restored.outcome
    );
    assert!(
        restored.outcome.needs_notice(),
        "and it must say so: a window that silently forgot everything looks \
         like data loss, because it is indistinguishable from it"
    );
    assert_eq!(
        restored.places.bookmarks().len(),
        1,
        "the newer file's bookmarks are still read; a layout we cannot use is \
         no reason to drop them"
    );
}

#[test]
fn a_truncated_file_is_reported_rather_than_crashing() {
    // What a crash mid-write used to leave behind, before the write became
    // atomic. Old copies of it exist in the wild.
    let text = fixture(1);
    let half = &text[..text.len() / 2];
    let restored = Session::restore(Some(half), &home());
    assert!(matches!(restored.outcome, RestoreOutcome::Unreadable(_)));
    assert!(restored.outcome.needs_notice());
}

#[test]
fn garbage_is_reported_rather_than_crashing() {
    for text in ["", "{", "null", "[]", "\u{0}\u{1}\u{2}"] {
        let restored = Session::restore(Some(text), &home());
        assert!(
            restored.outcome.needs_notice() || !restored.outcome.restored(),
            "{text:?} must not be treated as a restored session"
        );
    }
}

#[test]
fn the_file_records_which_application_version_wrote_it() {
    let session = Session::settings_only(jtf_workspace::SessionSettings::default());
    let json = session.to_json().unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value["app_version"],
        serde_json::json!(env!("CARGO_PKG_VERSION")),
        "a bug report that says \"it broke after I updated\" needs something \
         to check"
    );
}
