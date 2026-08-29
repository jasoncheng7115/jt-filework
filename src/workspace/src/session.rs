//! Session memory: reopening the application returns you to where you were.
//!
//! `AGENTS.md` §7 and `docs/UI_UX_SPEC.md` §16 require the workspace — split
//! tree, panes, tabs, locations, history, sort, filter, columns, view mode,
//! scroll, selection, marks, locale and theme — to survive a restart.
//!
//! It is also a **preference**, not a law. Some people want a clean window
//! every launch, and on a shared or audited machine remembering the last paths
//! is a privacy question, not a convenience one. So:
//!
//! - [`RestoreOnLaunch`] chooses what a launch does.
//! - The *setting itself* is always persisted; turning memory off must still
//!   be remembered.
//! - Turning memory off **discards** any state already stored. A switch that
//!   leaves yesterday's paths on disk is not an off switch.
//! - A missing, corrupt or future-versioned session starts fresh and says so,
//!   rather than silently losing the user's layout
//!   (`docs/UI_TEST_PLAN.md` SESS-003).
//!
//! This module owns the *policy and the format*. Writing bytes to disk —
//! atomically, per `docs/UI_TEST_PLAN.md` SESS-005 — belongs to the platform
//! layer.

use jtf_core::{Error, ErrorCode, Location};
use serde::{Deserialize, Serialize};

use crate::workspace::Workspace;

/// Format version of the stored session.
///
/// A stored session from a newer version is not guessed at; it starts fresh
/// and says why.
pub const SESSION_FORMAT_VERSION: u32 = 1;

/// What happens when the application launches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum RestoreOnLaunch {
    /// Restore the last session exactly: layout, tabs, locations, marks.
    /// The default.
    #[default]
    LastSession,
    /// Start with a single pane at the user's home location.
    HomeLocation,
    /// Start with a single pane at a fixed location the user chose.
    FixedLocation {
        /// Where every launch begins.
        location: Location,
    },
}

impl RestoreOnLaunch {
    /// Whether the workspace should be written to disk at all.
    pub const fn remembers_workspace(&self) -> bool {
        matches!(self, Self::LastSession)
    }

    /// Localization key for the setting's label.
    pub const fn label_key(&self) -> &'static str {
        match self {
            Self::LastSession => "settings.startup.last_session",
            Self::HomeLocation => "settings.startup.home",
            Self::FixedLocation { .. } => "settings.startup.fixed_location",
        }
    }
}

/// How the file list is drawn.
///
/// Monospace by default, and not only for the date column: with proportional
/// text every column's contents jitter, sizes do not line up digit for digit,
/// and a long list becomes harder to scan than it needs to be. An empty
/// family means "the platform's own fixed-width font", which is the right
/// default on each OS and carries no licensing question
/// (`AGENTS.md` §18.2: use the platform's own text stack).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontSettings {
    /// Font family, or empty for the platform's default fixed-width font.
    #[serde(default)]
    pub family: String,
    /// Point size, or 0 for the platform default.
    #[serde(default)]
    pub point_size: u16,
    /// Whether the list uses a fixed-width font at all.
    #[serde(default = "default_true")]
    pub monospace: bool,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            family: String::new(),
            point_size: 0,
            monospace: true,
        }
    }
}

/// Session-related preferences.
///
/// Always persisted, independently of whether the workspace is.
///
/// The booleans are independent user choices, not a state machine: any
/// combination is meaningful, which is what makes them settings.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSettings {
    /// What a launch does.
    pub restore_on_launch: RestoreOnLaunch,
    /// Whether the reopen-closed-tab history is part of the saved session.
    #[serde(default = "default_true")]
    pub remember_closed_tabs: bool,
    /// Whether the marked set is part of the saved session.
    #[serde(default = "default_true")]
    pub remember_marks: bool,
    /// How the file list is drawn.
    #[serde(default)]
    pub font: FontSettings,
    /// Which keymap preset is active. Empty means the platform default.
    #[serde(default)]
    pub keymap: String,
    /// Whether folders sort ahead of files, or everything sorts together.
    ///
    /// Defaults to folders first, which is what every desktop file manager
    /// does — but people who sort by date to see what changed want one list,
    /// not two, so it is a preference rather than a rule.
    #[serde(default = "default_true")]
    pub folders_first: bool,
    /// Whether the folder tree sidebar is shown.
    #[serde(default)]
    pub tree_visible: bool,
    /// Its width in logical pixels. 0 means the default.
    #[serde(default)]
    pub tree_width: u16,
    /// Whether the inspector panel is shown.
    #[serde(default)]
    pub inspector_visible: bool,
    /// Its width in logical pixels. 0 means the default.
    #[serde(default)]
    pub inspector_width: u16,
}

const fn default_true() -> bool {
    true
}

impl Default for SessionSettings {
    /// Remember everything.
    ///
    /// Written by hand rather than derived: `#[derive(Default)]` would give
    /// `false` for the two booleans, and serde's `default = "..."` only
    /// applies when deserializing. Deriving here would silently make the
    /// product's default "forget the user's session", which is the opposite of
    /// what `docs/UI_UX_SPEC.md` §16 specifies.
    fn default() -> Self {
        Self {
            restore_on_launch: RestoreOnLaunch::default(),
            remember_closed_tabs: true,
            remember_marks: true,
            font: FontSettings::default(),
            keymap: String::new(),
            folders_first: true,
            // Off by default: a sidebar that appears uninvited on first launch
            // is a decision made for the user rather than by them.
            tree_visible: false,
            tree_width: 0,
            inspector_visible: false,
            inspector_width: 0,
        }
    }
}

impl SessionSettings {
    /// Remember everything. The default.
    pub fn remembering() -> Self {
        Self::default()
    }

    /// Remember nothing about where the user was.
    pub fn forgetting() -> Self {
        Self {
            restore_on_launch: RestoreOnLaunch::HomeLocation,
            remember_closed_tabs: false,
            remember_marks: false,
            font: FontSettings::default(),
            keymap: String::new(),
            folders_first: true,
            // Off by default: a sidebar that appears uninvited on first launch
            // is a decision made for the user rather than by them.
            tree_visible: false,
            tree_width: 0,
            inspector_visible: false,
            inspector_width: 0,
        }
    }
}

/// What is written to disk between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    version: u32,
    settings: SessionSettings,
    /// Absent when the user has turned session memory off.
    workspace: Option<Workspace>,
}

/// Why a launch did or did not restore.
///
/// Carried to the UI so a fresh window is always explained
/// (`docs/UI_UX_SPEC.md` §13: never fail silently).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RestoreOutcome {
    /// The previous session was restored.
    Restored,
    /// The user asked for a fresh start. Not a problem, and not reported as one.
    DisabledByPreference,
    /// There was no stored session, e.g. first launch.
    NothingStored,
    /// The stored session could not be read.
    Unreadable(ErrorCode),
    /// The stored session came from a newer format version.
    UnsupportedVersion(u32),
}

impl RestoreOutcome {
    /// Whether the previous session came back.
    pub const fn restored(self) -> bool {
        matches!(self, Self::Restored)
    }

    /// Whether the user should be told something went wrong.
    ///
    /// A deliberate fresh start and a first launch are not problems.
    pub const fn needs_notice(self) -> bool {
        matches!(self, Self::Unreadable(_) | Self::UnsupportedVersion(_))
    }

    /// Localization key for the notice, where one is warranted.
    pub const fn notice_key(self) -> Option<&'static str> {
        match self {
            Self::Unreadable(_) => Some("session.restore.unreadable"),
            Self::UnsupportedVersion(_) => Some("session.restore.unsupported_version"),
            Self::Restored | Self::DisabledByPreference | Self::NothingStored => None,
        }
    }
}

/// A restored workspace together with the reason it looks the way it does.
///
/// The settings are returned alongside rather than stored inside
/// [`Workspace`]: the workspace is what the user is looking at, the settings
/// are how the application behaves, and keeping two copies of a preference in
/// sync is how they drift apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Restored {
    /// The workspace to show.
    pub workspace: Workspace,
    /// The preferences that were stored, or the defaults.
    pub settings: SessionSettings,
    /// Why (`docs/UI_TEST_PLAN.md` SESS-003).
    pub outcome: RestoreOutcome,
}

impl Session {
    /// Capture the current workspace, honouring the settings.
    ///
    /// When memory is off, no workspace is stored — and because this is the
    /// only way a session is produced, turning memory off *discards* whatever
    /// was stored before.
    pub fn capture(workspace: &Workspace, settings: SessionSettings) -> Self {
        let stored = if settings.restore_on_launch.remembers_workspace() {
            let mut copy = workspace.clone();
            if !settings.remember_marks {
                copy.clear_all_marks();
            }
            if !settings.remember_closed_tabs {
                copy.clear_closed_tab_history();
            }
            Some(copy)
        } else {
            None
        };
        Self {
            version: SESSION_FORMAT_VERSION,
            settings,
            workspace: stored,
        }
    }

    /// A session that remembers only the settings.
    pub fn settings_only(settings: SessionSettings) -> Self {
        Self {
            version: SESSION_FORMAT_VERSION,
            settings,
            workspace: None,
        }
    }

    /// The stored preferences.
    pub const fn settings(&self) -> &SessionSettings {
        &self.settings
    }

    /// The stored format version.
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Whether a workspace was stored.
    pub const fn has_workspace(&self) -> bool {
        self.workspace.is_some()
    }

    /// Serialize for storage.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::Internal`] if the session cannot be encoded.
    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self)
            .map_err(|e| Error::new(ErrorCode::Internal, format!("encode session: {e}")))
    }

    /// Parse stored session bytes.
    ///
    /// Stored state is untrusted input (`docs/SECURITY.md` §2): it may have
    /// been truncated by a crash, edited by hand, or written by a different
    /// version.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::ParseFailed`] for malformed input.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        serde_json::from_str(text)
            .map_err(|e| Error::new(ErrorCode::ParseFailed, format!("decode session: {e}")))
    }

    /// Produce the workspace to show at launch.
    ///
    /// `home` is used whenever a fresh workspace is needed.
    pub fn restore(stored: Option<&str>, home: &Location) -> Restored {
        let Some(text) = stored else {
            return Restored {
                workspace: Workspace::new(home.clone()),
                settings: SessionSettings::default(),
                outcome: RestoreOutcome::NothingStored,
            };
        };

        let session = match Self::from_json(text) {
            Ok(session) => session,
            Err(error) => {
                return Restored {
                    workspace: Workspace::new(home.clone()),
                    settings: SessionSettings::default(),
                    outcome: RestoreOutcome::Unreadable(error.code()),
                };
            }
        };

        if session.version > SESSION_FORMAT_VERSION {
            return Restored {
                workspace: Self::fresh_workspace(&session.settings, home),
                settings: session.settings,
                outcome: RestoreOutcome::UnsupportedVersion(session.version),
            };
        }

        match session.workspace {
            Some(workspace) if session.settings.restore_on_launch.remembers_workspace() => {
                if workspace.invariants_hold() {
                    Restored {
                        workspace,
                        settings: session.settings,
                        outcome: RestoreOutcome::Restored,
                    }
                } else {
                    // Structurally decodable but internally inconsistent, e.g.
                    // a tree naming a pane that is not in the map.
                    Restored {
                        workspace: Workspace::new(home.clone()),
                        settings: session.settings,
                        outcome: RestoreOutcome::Unreadable(ErrorCode::ParseFailed),
                    }
                }
            }
            Some(_) | None => {
                let outcome = if session.settings.restore_on_launch.remembers_workspace() {
                    RestoreOutcome::NothingStored
                } else {
                    RestoreOutcome::DisabledByPreference
                };
                Restored {
                    workspace: Self::fresh_workspace(&session.settings, home),
                    settings: session.settings,
                    outcome,
                }
            }
        }
    }

    fn fresh_workspace(settings: &SessionSettings, home: &Location) -> Workspace {
        let at = match &settings.restore_on_launch {
            RestoreOnLaunch::FixedLocation { location } => location.clone(),
            RestoreOnLaunch::LastSession | RestoreOnLaunch::HomeLocation => home.clone(),
        };
        Workspace::new(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Orientation;
    use crate::LayoutPreset;

    fn home() -> Location {
        Location::local("/Users/test")
    }

    fn busy_workspace() -> Workspace {
        let mut w = Workspace::new(home());
        w.apply_preset(LayoutPreset::Quad);
        w.new_tab(Location::local("/Users/test/Downloads"));
        w.active_tab_mut()
            .unwrap()
            .marks_mut()
            .mark(Location::local("/Users/test/Downloads/a.zip"));
        w.active_tab_mut()
            .unwrap()
            .set_scroll(crate::view::ScrollPosition {
                first_visible_row: 120,
                row_offset: 0.25,
            });
        w.split_active(Orientation::Vertical);
        w
    }

    #[test]
    fn a_captured_session_restores_the_workspace_exactly() {
        let before = busy_workspace();
        let json = Session::capture(&before, SessionSettings::remembering())
            .to_json()
            .unwrap();

        let restored = Session::restore(Some(&json), &home());

        assert_eq!(restored.outcome, RestoreOutcome::Restored);
        assert_eq!(
            restored.workspace, before,
            "layout, tabs, locations and marks all return"
        );
        assert!(restored.workspace.invariants_hold());
    }

    #[test]
    fn the_list_is_monospace_by_default() {
        let f = FontSettings::default();
        assert!(
            f.monospace,
            "columns only line up if the font is fixed-width"
        );
        assert!(
            f.family.is_empty(),
            "empty means the platform's own fixed font"
        );
        assert_eq!(f.point_size, 0, "0 means the platform default size");
    }

    #[test]
    fn font_settings_survive_a_session_round_trip() {
        let settings = SessionSettings {
            font: FontSettings {
                family: "JetBrains Mono".to_string(),
                point_size: 13,
                monospace: true,
            },
            ..SessionSettings::default()
        };
        let json = Session::capture(&busy_workspace(), settings.clone())
            .to_json()
            .unwrap();
        let back = Session::from_json(&json).unwrap();
        assert_eq!(back.settings().font, settings.font);
    }

    #[test]
    fn remembering_everything_is_the_default() {
        // docs/UI_UX_SPEC.md 16. A derived Default would quietly flip these
        // to false; this test is what stops that regression.
        let d = SessionSettings::default();
        assert_eq!(d.restore_on_launch, RestoreOnLaunch::LastSession);
        assert!(d.remember_closed_tabs);
        assert!(d.remember_marks);
        assert!(d.restore_on_launch.remembers_workspace());
    }

    #[test]
    fn first_launch_starts_at_home_without_complaining() {
        let restored = Session::restore(None, &home());
        assert_eq!(restored.outcome, RestoreOutcome::NothingStored);
        assert!(!restored.outcome.needs_notice());
        assert_eq!(restored.workspace.active_tab().unwrap().location(), &home());
    }

    #[test]
    fn turning_memory_off_stores_no_workspace_at_all() {
        // The privacy half of the switch: off must mean nothing is kept, not
        // "kept but ignored".
        let session = Session::capture(&busy_workspace(), SessionSettings::forgetting());
        assert!(!session.has_workspace());

        let json = session.to_json().unwrap();
        assert!(
            !json.contains("Downloads"),
            "no path from the last session may survive on disk"
        );
    }

    #[test]
    fn the_setting_itself_is_remembered_even_when_the_workspace_is_not() {
        let json = Session::capture(&busy_workspace(), SessionSettings::forgetting())
            .to_json()
            .unwrap();
        let back = Session::from_json(&json).unwrap();
        assert_eq!(
            back.settings().restore_on_launch,
            RestoreOnLaunch::HomeLocation
        );
    }

    #[test]
    fn a_fresh_start_by_preference_is_not_reported_as_a_problem() {
        let json = Session::capture(&busy_workspace(), SessionSettings::forgetting())
            .to_json()
            .unwrap();
        let restored = Session::restore(Some(&json), &home());

        assert_eq!(restored.outcome, RestoreOutcome::DisabledByPreference);
        assert!(!restored.outcome.needs_notice());
        assert_eq!(restored.workspace.pane_count(), 1);
        assert_eq!(restored.workspace.active_tab().unwrap().location(), &home());
        assert_eq!(
            restored.settings.restore_on_launch,
            RestoreOnLaunch::HomeLocation,
            "the preference survives into the new session"
        );
    }

    #[test]
    fn a_fixed_start_location_is_honoured() {
        let settings = SessionSettings {
            restore_on_launch: RestoreOnLaunch::FixedLocation {
                location: Location::local("/Projects"),
            },
            ..SessionSettings::default()
        };
        let json = Session::capture(&busy_workspace(), settings)
            .to_json()
            .unwrap();
        let restored = Session::restore(Some(&json), &home());
        assert_eq!(
            restored.workspace.active_tab().unwrap().location(),
            &Location::local("/Projects")
        );
    }

    #[test]
    fn a_corrupt_session_starts_fresh_and_says_so() {
        // SESS-003: never silently lose the layout without a word.
        for broken in ["", "{", "null", "{\"version\":1}", "not json at all"] {
            let restored = Session::restore(Some(broken), &home());
            assert!(!restored.outcome.restored(), "{broken:?} must not restore");
            assert!(
                restored.outcome.needs_notice(),
                "{broken:?} must be reported"
            );
            assert_eq!(
                restored.outcome.notice_key(),
                Some("session.restore.unreadable")
            );
            assert!(restored.workspace.invariants_hold());
        }
    }

    #[test]
    fn a_session_from_a_newer_version_is_not_guessed_at() {
        let mut session = Session::capture(&busy_workspace(), SessionSettings::remembering());
        session.version = SESSION_FORMAT_VERSION + 7;
        let json = session.to_json().unwrap();

        let restored = Session::restore(Some(&json), &home());
        assert_eq!(
            restored.outcome,
            RestoreOutcome::UnsupportedVersion(SESSION_FORMAT_VERSION + 7)
        );
        assert!(restored.outcome.needs_notice());
    }

    #[test]
    fn a_structurally_valid_but_inconsistent_workspace_is_rejected() {
        let json = Session::capture(&busy_workspace(), SessionSettings::remembering())
            .to_json()
            .unwrap();
        // Point the active pane at something that does not exist.
        let broken = json.replace("\"active_pane\":", "\"active_pane\":9999,\"_old\":");

        let restored = Session::restore(Some(&broken), &home());
        assert!(!restored.outcome.restored());
        assert!(
            restored.workspace.invariants_hold(),
            "the fallback is always sound"
        );
    }

    #[test]
    fn marks_can_be_excluded_from_the_saved_session() {
        let settings = SessionSettings {
            remember_marks: false,
            ..SessionSettings::default()
        };
        let json = Session::capture(&busy_workspace(), settings)
            .to_json()
            .unwrap();

        let restored = Session::restore(Some(&json), &home());
        assert!(restored.outcome.restored(), "the layout still comes back");
        assert_eq!(
            restored.workspace.total_marked(),
            0,
            "but the marked set was not written"
        );
        assert!(restored.workspace.pane_count() > 1, "layout is unaffected");
    }

    #[test]
    fn closed_tab_history_can_be_excluded_from_the_saved_session() {
        let mut w = busy_workspace();
        let extra = w.new_tab(Location::local("/tmp/scratch"));
        w.active_pane_mut().close_tab(extra, false);
        assert_eq!(w.active_pane().closed_tab_count(), 1);

        let settings = SessionSettings {
            remember_closed_tabs: false,
            ..SessionSettings::default()
        };
        let json = Session::capture(&w, settings).to_json().unwrap();
        assert!(
            !json.contains("scratch"),
            "a closed tab's path must not be written"
        );

        let restored = Session::restore(Some(&json), &home());
        assert_eq!(restored.workspace.active_pane().closed_tab_count(), 0);
    }

    #[test]
    fn capture_does_not_mutate_the_live_workspace() {
        let mut w = busy_workspace();
        let before = w.clone();
        let _ = Session::capture(&w, SessionSettings::forgetting());
        assert_eq!(
            w, before,
            "saving must never change what the user is looking at"
        );
        // And the live marks are still there.
        w.active_tab_mut().unwrap().marks_mut().clear();
    }

    #[test]
    fn an_older_session_without_the_newer_fields_still_loads() {
        // Forward compatibility for settings added after 1.0: serde defaults.
        let minimal = r#"{"version":1,"settings":{"restore_on_launch":{"mode":"home_location"}},"workspace":null}"#;
        let session = Session::from_json(minimal).unwrap();
        assert!(session.settings().remember_closed_tabs);
        assert!(session.settings().remember_marks);
    }
}
