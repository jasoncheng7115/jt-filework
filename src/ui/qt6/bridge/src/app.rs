//! Application state.
//!
//! Everything the UI shows lives here, in Rust. The C++ side holds one
//! pointer to an [`App`] and asks it questions.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jtf_commands::{Command, CommandId, CommandRegistry, KeyChord, Keymap, KeymapError};
use jtf_core::i18n::{Catalog, LocaleId, Localizer};
use jtf_core::theme::{Palette, ResolvedTheme, SystemAppearance, ThemeMode, ThemeToken};
use jtf_core::{Error, FileEntry, FileKind, Location};
use jtf_fs::{Batch, EnumerationHandle, LocalProvider, Provider, SftpProvider, SizeCache};
use jtf_jobs::{CancellationToken, Canceller, Progress};
use jtf_ops::{ConflictPolicy, Plan, PlanError, RenamePattern, RenamePreview, UndoRecord};
use jtf_search::{SearchHandle, SearchUpdate};
use jtf_viewer::{detect, ContentKind, Encoding, HexView, TextView};
use jtf_workspace::{
    sort_entries_with, Bookmark, FontSettings, LayoutPreset, Orientation, PaneId, Places, Session,
    SessionSettings, SortKey, SortSpec, ViewMode, Workspace,
};

/// The columns the list can show, in order.
///
/// Derived from the model's own `Column::ALL` rather than repeated here, so a
/// column added to the model reaches the UI without a second edit that
/// someone has to remember. The first four are visible by default; the rest
/// are offered in the header's menu.
/// The column at display position `index`, from the model's default layout.
pub(crate) fn column_at(index: i32) -> Option<jtf_workspace::Column> {
    usize::try_from(index).ok().and_then(|i| {
        jtf_workspace::default_columns()
            .get(i)
            .map(|spec| spec.column)
    })
}

pub(crate) const COLUMN_NAME: i32 = 0;
pub(crate) const COLUMN_SIZE: i32 = 1;
pub(crate) const COLUMN_MODIFIED: i32 = 2;
pub(crate) const COLUMN_KIND: i32 = 3;

/// How many columns exist.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
pub(crate) const COLUMN_COUNT: i32 = jtf_workspace::Column::ALL.len() as i32;

/// How many operations may wait behind the running one.
///
/// Bounded because each holds a whole plan, and a plan holds every step it
/// will take (`docs/SECURITY.md` §13).
const MAX_QUEUED: usize = 64;

/// How many recent folders the sidebar shows when the user has not chosen.
///
/// Fewer than are kept: the list exists to hold what you were just doing, and
/// once it is long enough to scroll it has stopped being that.
const DEFAULT_RECENT_SHOWN: usize = 10;

/// An archive being extracted, or written, on a worker thread.
///
/// Same shape as `Measuring` and for the same reason: unpacking a large
/// archive on the UI thread would freeze the window, and there is no useful
/// percentage to report - the member count is known but their sizes are not
/// until they arrive - so what is shown is how much has come out so far.
struct Archiving {
    updates: std::sync::mpsc::Receiver<ArchiveUpdate>,
    canceller: jtf_jobs::Canceller,
    files: u64,
    bytes: u64,
    refused: u64,
    /// Where the job has got to. Three states, named, because
    /// `Option<Option<String>>` spelled them without saying them.
    outcome: ArchiveOutcome,
    /// Whether this was a compress rather than an extract, for the wording.
    compressing: bool,
}

/// A folder comparison running on a worker thread.
///
/// Same shape and same reason as `Archiving`: comparing two trees is a
/// disk-bound walk, doubled, and on a server it is a network-bound one. Doing
/// it on the interface thread would freeze the window for as long as it takes,
/// which is the thing `AGENTS.md` §3 forbids.
struct Comparing {
    updates: std::sync::mpsc::Receiver<CompareUpdate>,
    canceller: jtf_jobs::Canceller,
    /// What was compared, kept so the window can name it after the fact.
    left: String,
    right: String,
    recursive: bool,
    outcome: CompareOutcome,
    /// The running totals, so the window can say what is happening rather
    /// than sit silent for the length of the walk.
    progress: jtf_fs::CompareProgress,
}

/// What the comparison thread sends back.
enum CompareUpdate {
    Progress(jtf_fs::CompareProgress),
    Done(std::result::Result<jtf_fs::Comparison, String>),
}

/// How far a `Comparing` has got.
pub(crate) enum CompareOutcome {
    /// Still walking.
    Running,
    /// Finished, with the answer.
    Done(jtf_fs::Comparison),
    /// Finished, with the reason it stopped.
    Failed(String),
}

/// A disc-usage analysis running on a worker thread.
///
/// Same shape and same reason as `Comparing`: walking a disc is bound by how
/// long the disc takes, and doing it on the interface thread would freeze the
/// window for that whole time.
struct Analysing {
    updates: std::sync::mpsc::Receiver<UsageUpdate>,
    canceller: jtf_jobs::Canceller,
    /// What is being analysed, kept so the window can name it.
    root: String,
    outcome: UsageOutcome,
    progress: jtf_fs::UsageProgress,
}

/// What the analysis thread sends back.
enum UsageUpdate {
    Progress(jtf_fs::UsageProgress),
    Done(Box<jtf_fs::Usage>),
}

/// How far an `Analysing` has got.
pub(crate) enum UsageOutcome {
    /// Still walking.
    Running,
    /// Finished, with the breakdown.
    Done(Box<jtf_fs::Usage>),
}

/// How far an `Archiving` has got.
pub(crate) enum ArchiveOutcome {
    /// Still working.
    Running,
    /// Finished, with everything it was asked to do.
    Succeeded,
    /// Finished, with the reason it stopped.
    Failed(String),
}

enum ArchiveUpdate {
    Progress { files: u64, bytes: u64 },
    Done { refused: u64, error: Option<String> },
}

/// A folder measurement running on a worker thread.
///
/// Measuring used to run on the calling thread, which is the UI thread: asking
/// for the size of a large tree froze the window until the walk finished, with
/// nothing on screen to say why (`AGENTS.md` §3 forbids exactly this). It is a
/// job now, and like every other job it reports as it goes and can be
/// cancelled.
///
/// There is no percentage. The total is not known until the walk ends - that
/// is what the walk is for - so what is reported is how much has been counted
/// so far, and the bar says "working" rather than inventing a fraction.
struct Measuring {
    updates: std::sync::mpsc::Receiver<(PathBuf, jtf_fs::FolderSize, bool)>,
    canceller: jtf_jobs::Canceller,
    /// Running totals across every folder in this request.
    files: u64,
    bytes: u64,
    /// How many of the requested folders have finished.
    done: usize,
    total: usize,
}

/// An open viewer.
///
/// One at a time: the UI shows one viewer window, and holding several open
/// file handles for windows nobody is looking at is a leak with extra steps.
struct ViewerSession {
    path: PathBuf,
    kind: ContentKind,
    text: Option<TextView>,
    hex: Option<HexView>,
    /// Whether the user asked for hex on something that is textual.
    forced_hex: bool,
}

/// Which way [`App::mark_listed`] moves.
#[derive(Clone, Copy)]
pub(crate) enum MarkAction {
    All,
    None,
    Invert,
}

/// What one pane is currently showing.
struct PaneView {
    /// Everything the enumeration produced.
    entries: Vec<FileEntry>,
    /// Indices into `entries` that pass the filter, in display order.
    ///
    /// A filter narrows what is shown without discarding what was read, so
    /// clearing it is instant and does not re-scan the directory.
    visible: Vec<usize>,
    /// A running search, when this pane is showing results rather than a
    /// directory.
    search: Option<SearchHandle>,
    /// The query that produced these results, empty when browsing.
    query: String,
    /// The folder the running search is in, for the status line. Cleared when
    /// the walk ends, so a finished search does not go on naming a folder.
    search_in: String,
    /// Bumped whenever the row set changes identity rather than merely
    /// growing: a new location, a re-sort, a filter change. The UI uses it to
    /// decide between inserting rows and rebuilding the whole model, which is
    /// the difference between one model reset and four hundred of them while
    /// a large directory loads.
    generation: u64,
    handle: Option<EnumerationHandle>,
    sort: SortSpec,
    error: Option<Error>,
    /// Rows delivered so far; lets the UI say "still loading" honestly.
    loading: bool,
    /// How many visible rows are directories, and how many bytes the visible
    /// files add up to.
    ///
    /// Counted once when `visible` is rebuilt rather than on every status
    /// repaint: the status line is refreshed on a frame boundary, and walking
    /// 100K rows sixty times a second to print one number is exactly the kind
    /// of display cost `AGENTS.md` 18 rules out.
    folder_count: usize,
    visible_bytes: u64,
    /// The entry the cursor should land on once this listing arrives.
    ///
    /// Set when leaving a folder upwards, so stepping out puts the cursor on
    /// the folder you just left rather than back at the top. Consumed once.
    focus_name: Option<String>,
}

/// Apply what a running search has reported. Returns whether it ended, and
/// whether anything the list draws from changed.
///
/// Its own function rather than four arms inside the pump: the pump is long
/// enough already, and none of this is about the pump.
fn drain_search(view: &mut PaneView, updates: Vec<SearchUpdate>) -> (bool, bool) {
    let mut finished = false;
    let mut changed = false;
    for update in updates {
        match update {
            SearchUpdate::Matches(rows) => {
                changed = true;
                view.entries.extend(rows);
            }
            SearchUpdate::Done { .. } => {
                finished = true;
                changed = true;
            }
            SearchUpdate::Failed(error) => {
                view.error = Some(error);
                finished = true;
                changed = true;
            }
            // Deliberately not a change: re-sorting and re-filtering the whole
            // list once per directory visited is the cost this reports on.
            SearchUpdate::Progress { directory, .. } => {
                view.search_in = directory.to_string_lossy().into_owned();
            }
        }
    }
    (finished, changed)
}

impl PaneView {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            visible: Vec::new(),
            search: None,
            query: String::new(),
            search_in: String::new(),
            generation: 0,
            handle: None,
            sort: SortSpec::default(),
            error: None,
            loading: false,
            folder_count: 0,
            visible_bytes: 0,
            focus_name: None,
        }
    }
}

/// The whole application.
///
/// Public only so the C ABI can name it in a pointer type; its methods are
/// crate-internal, because `ffi.rs` is this crate's actual interface.
pub struct App {
    workspace: Workspace,
    views: BTreeMap<PaneId, PaneView>,
    /// One provider per kind of place. Chosen by asking each whether it
    /// handles the location, rather than by a match here: adding a kind of
    /// storage should not mean editing every caller.
    provider: LocalProvider,
    sftp: SftpProvider,
    localizer: Localizer,
    locale: LocaleId,
    theme_mode: ThemeMode,
    show_hidden: bool,
    settings: SessionSettings,
    pending_plan: Option<Plan>,
    planning: Option<crate::operations::Planning>,
    plan_error: Option<PlanError>,
    running: Option<crate::operations::Running>,
    /// Operations waiting for the running one to finish.
    queue: std::collections::VecDeque<(Plan, ConflictPolicy)>,
    viewer: Option<ViewerSession>,
    /// A second, independent read for the inspector's preview.
    ///
    /// Separate from `viewer` rather than shared with it: the inspector
    /// follows the cursor and the viewer window is where the user parked
    /// themselves, and one stealing the other's file handle is a bug we have
    /// already had once. Preview reads only the first screenful.
    preview: Option<ViewerSession>,
    last_summary: Option<crate::operations::Summary>,
    undo_stack: Vec<UndoRecord>,
    batch_preview: Option<RenamePreview>,
    registry: CommandRegistry,
    keymap: Keymap,
    dropped_bindings: usize,
    repo_root: PathBuf,
    session_path: PathBuf,
    /// How the stored session was read at launch. Kept so that a session from
    /// a newer build is never overwritten, and so the user can be told once.
    session_outcome: jtf_workspace::RestoreOutcome,
    /// Whether the notice about that has been handed to the UI already.
    notice_taken: bool,
    places: Places,
    /// The platform's ordered language preferences, comma-separated, so
    /// "follow the system" can be re-resolved without asking Qt again.
    system_locale: String,
    folder_sizes: SizeCache,
    /// A folder measurement in flight, if there is one.
    measuring: Option<Measuring>,
    /// An extraction or compression in flight, if there is one.
    archiving: Option<Archiving>,
    /// The folder comparison, if the window is open.
    comparing: Option<Comparing>,
    /// The disc-usage analysis, if that window is open.
    analysing: Option<Analysing>,
    /// Seconds east of UTC, as the platform reports it.
    ///
    /// Set by the UI at startup. Zero until then, which is UTC - wrong, but
    /// wrong in a way that is the same everywhere rather than a guess.
    utc_offset: i32,
    /// The removable disks the last refresh found.
    devices: Vec<jtf_platform_devices::Device>,
    /// The image being written to a disk, if one is.
    burning: Option<Burning>,
    /// The listing the archive window is showing, if one is open.
    archive_entries: Vec<jtf_viewer::ArchiveEntry>,
}

impl App {
    /// Start the application, restoring the previous session if the user's
    /// preference allows it (`docs/PRODUCT_SPEC.md` §5.1).
    pub(crate) fn new(system_locale: &str) -> Self {
        let repo_root = locate_repo_root();
        let session_path = session_path();
        let home = home_location();

        let stored = fs::read_to_string(&session_path).ok();

        // Keep a copy of anything written by an older format before this run
        // can overwrite it. An upgrade that goes wrong is recoverable if the
        // previous file still exists and is not if it does not, and the cost
        // is one small file copied once per format change.
        if let Some(text) = stored.as_deref() {
            back_up_before_migrating(&session_path, text);
        }

        let restored = Session::restore(stored.as_deref(), &home);
        let settings = restored.settings.clone();
        // A file this build cannot read is a file this build must not write
        // over. Recorded here because `save_session` runs on a timer and on
        // quit, and by then the outcome of the read is long gone.
        let session_outcome = restored.outcome;

        // What the user asked for, if they ever asked; otherwise whatever
        // the system is set to. The workspace's own stored locale is not
        // consulted: it records what was displayed last time, which for
        // anyone who never opened the settings is just the previous default.
        let locale = if restored.settings.locale.is_empty() {
            LocaleId::best_match_of(system_locale.split(','))
        } else {
            LocaleId::new(restored.settings.locale.clone())
        };
        let localizer = Localizer::new(
            load_catalog(&repo_root, &locale),
            load_catalog(&repo_root, &LocaleId::english()),
        );
        let theme_mode = restored.workspace.theme_mode();

        let mut app = Self {
            workspace: restored.workspace,
            views: BTreeMap::new(),
            provider: LocalProvider::new(),
            sftp: SftpProvider::new(),
            localizer,
            locale,
            theme_mode,
            show_hidden: false,
            pending_plan: None,
            planning: None,
            plan_error: None,
            running: None,
            queue: std::collections::VecDeque::new(),
            viewer: None,
            preview: None,
            last_summary: None,
            undo_stack: Vec::new(),
            batch_preview: None,
            registry: CommandRegistry::baseline(),
            dropped_bindings: 0,
            keymap: load_keymap(&repo_root, &settings.keymap),
            settings,
            places: restored.places,
            system_locale: system_locale.to_string(),
            folder_sizes: SizeCache::new(),
            measuring: None,
            archiving: None,
            comparing: None,
            analysing: None,
            utc_offset: 0,
            devices: Vec::new(),
            burning: None,
            archive_entries: Vec::new(),
            repo_root,
            session_path,
            session_outcome,
            notice_taken: false,
        };
        // The user's own bindings are layered on after construction, because
        // dropping one needs the registry the app now owns.
        let overrides = apply_user_overrides(&mut app.keymap, &app.registry);
        app.dropped_bindings = overrides;
        app.refresh_all_panes();
        app
    }

    // ---------------------------------------------------------------- layout

    /// The layout tree, as JSON, for the C++ side to build splitters from.
    ///
    /// Parsed only when the layout changes, never per frame, so the cost of
    /// going through text is irrelevant and the alternative — a second
    /// hand-written tree ABI — would be worse.
    /// Every window id, in creation order.
    pub(crate) fn window_ids(&self) -> Vec<u64> {
        self.workspace
            .window_ids()
            .into_iter()
            .map(jtf_workspace::WindowId::get)
            .collect()
    }

    /// Move a tab into a window of its own. Returns the new window id, or 0.
    pub(crate) fn tear_off_tab(&mut self, pane: PaneId, tab_index: usize) -> u64 {
        let Some(tab) = self
            .workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(tab_index).map(jtf_workspace::Tab::id))
        else {
            return 0;
        };
        self.workspace
            .tear_off_tab(pane, tab)
            .map_or(0, |(window, _)| window.get())
    }

    /// Move a tab from one pane into another, which may be another window.
    pub(crate) fn merge_tab_into(&mut self, from: PaneId, tab_index: usize, into: PaneId) -> bool {
        let Some(tab) = self
            .workspace
            .pane(from)
            .and_then(|p| p.tabs().get(tab_index).map(jtf_workspace::Tab::id))
        else {
            return false;
        };
        self.workspace.merge_tab_into(from, tab, into).is_ok()
    }

    pub(crate) fn layout_json_for(&self, window: u64) -> String {
        self.workspace
            .root_of(jtf_workspace::WindowId::new(window))
            .map_or_else(String::new, Self::node_json)
    }

    fn node_json(n: &jtf_workspace::WorkspaceNode) -> String {
        Self::layout_node(n)
    }

    pub(crate) fn layout_json(&self) -> String {
        fn node(n: &jtf_workspace::WorkspaceNode) -> String {
            match n {
                jtf_workspace::WorkspaceNode::Pane { id } => {
                    format!(r#"{{"pane":{}}}"#, id.get())
                }
                jtf_workspace::WorkspaceNode::Split {
                    orientation,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    let vertical = matches!(orientation, Orientation::Vertical);
                    format!(
                        r#"{{"vertical":{},"ratio":{:.4},"first":{},"second":{}}}"#,
                        vertical,
                        ratio,
                        node(first),
                        node(second)
                    )
                }
            }
        }
        node(self.workspace.root())
    }

    /// One node as JSON. Shared by every window's layout.
    fn layout_node(n: &jtf_workspace::WorkspaceNode) -> String {
        match n {
            jtf_workspace::WorkspaceNode::Pane { id } => format!(r#"{{"pane":{}}}"#, id.get()),
            jtf_workspace::WorkspaceNode::Split {
                orientation,
                ratio,
                first,
                second,
                ..
            } => {
                let vertical = matches!(orientation, Orientation::Vertical);
                format!(
                    r#"{{"vertical":{},"ratio":{:.4},"first":{},"second":{}}}"#,
                    vertical,
                    ratio,
                    Self::layout_node(first),
                    Self::layout_node(second)
                )
            }
        }
    }

    pub(crate) fn pane_ids(&self) -> Vec<PaneId> {
        self.workspace.pane_order()
    }

    pub(crate) const fn active_pane(&self) -> PaneId {
        self.workspace.active_pane_id()
    }

    pub(crate) fn focus_pane(&mut self, pane: PaneId) {
        self.workspace.focus_pane(pane);
    }

    pub(crate) fn focus_next_pane(&mut self) {
        self.workspace.focus_next_pane();
    }

    pub(crate) fn split_active(&mut self, vertical: bool) {
        let orientation = if vertical {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        };
        let new_pane = self.workspace.split_active(orientation);
        self.views.insert(new_pane, PaneView::new());
        self.start_enumeration(new_pane);
    }

    /// The pane a copy or a move would go to, by id.
    ///
    /// By id, not by position in `pane_ids`: every other function across this
    /// boundary takes and returns a `PaneId` value, and returning an index
    /// from this one meant the UI compared an index against an id and matched
    /// the wrong pane - or none.
    pub(crate) fn target_pane(&self) -> Option<PaneId> {
        self.workspace.target_pane_id()
    }

    pub(crate) fn can_close_pane(&self, pane: PaneId) -> bool {
        self.workspace.can_close_pane(pane)
    }

    pub(crate) fn close_active_pane(&mut self) -> bool {
        let pane = self.workspace.active_pane_id();
        if self.workspace.close_pane(pane).is_ok() {
            self.views.remove(&pane);
            true
        } else {
            false
        }
    }

    /// Close one named pane, whichever window it is in.
    ///
    /// Closing a torn-off window has to remove its panes, or the window is
    /// only gone from the screen: the workspace still holds it, the session
    /// records it, and the next launch opens it again. `close_pane` already
    /// drops a torn-off window when its last pane goes; nothing was calling it
    /// when the window itself was dismissed.
    pub(crate) fn close_pane(&mut self, pane: PaneId) -> bool {
        if self.workspace.close_pane(pane).is_ok() {
            self.views.remove(&pane);
            true
        } else {
            false
        }
    }

    pub(crate) fn apply_preset(&mut self, preset: LayoutPreset) {
        self.workspace.apply_preset(preset);
        self.refresh_all_panes();
    }

    // ------------------------------------------------------------------ tabs

    pub(crate) fn tab_count(&self, pane: PaneId) -> usize {
        self.workspace
            .pane(pane)
            .map_or(0, jtf_workspace::Pane::tab_count)
    }

    pub(crate) fn active_tab_index(&self, pane: PaneId) -> usize {
        self.workspace
            .pane(pane)
            .map_or(0, jtf_workspace::Pane::active_index)
    }

    pub(crate) fn tab_title(&self, pane: PaneId, index: usize) -> String {
        self.workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .map_or_else(String::new, |t| display_name_of(t.location()))
    }

    pub(crate) fn new_tab(&mut self) {
        let at = self
            .workspace
            .active_tab()
            .map_or_else(home_location, |t| t.location().clone());
        self.workspace.new_tab(at);
        let pane = self.workspace.active_pane_id();
        self.start_enumeration(pane);
    }

    /// Open `path` in a window of its own, and return that window's id.
    ///
    /// Built out of the two operations that already exist rather than a new
    /// one in the workspace: a tab on that folder, torn off. Tearing off is
    /// what makes a window here - there is no other way to make one - so
    /// reusing it means a window opened this way and a window made by dragging
    /// a tab out are the same kind of thing, with the same session entry.
    pub(crate) fn open_in_new_window(&mut self, pane: PaneId, path: &str) -> u64 {
        if path.is_empty() {
            return 0;
        }
        self.workspace.focus_pane(pane);
        self.new_tab();
        let pane = self.workspace.active_pane_id();
        self.navigate(pane, path);
        let index = self.active_tab_index(pane);
        self.tear_off_tab(pane, index)
    }

    /// Duplicate the tab at `index` in `pane`, placing the copy beside it.
    ///
    /// Takes the tab by index rather than acting on whichever tab happens to
    /// be active: this answers a right-click on one particular tab, which is
    /// not always the active one. It copies the tab's *location*, so a tab on
    /// a server duplicates to the same server - reading a path out and
    /// navigating to it cannot, because a remote location has no local path
    /// to read.
    pub(crate) fn duplicate_tab(&mut self, pane: PaneId, index: usize) {
        let Some(id) = self
            .workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .map(jtf_workspace::Tab::id)
        else {
            return;
        };
        if self.workspace.duplicate_tab(pane, id).is_ok() {
            self.start_enumeration(pane);
        }
    }

    /// Pin or unpin the tab at `index`, and report what it became.
    ///
    /// Pinning is fully modelled - a pinned tab keeps a leading place in the
    /// strip, cannot be reordered out of it, and refuses to close without
    /// force - and the interface had no way to reach any of it.
    pub(crate) fn toggle_tab_pinned(&mut self, pane: PaneId, index: usize) -> bool {
        let Some(tab) = self
            .workspace
            .pane_mut(pane)
            .and_then(|p| p.tabs_mut().get_mut(index))
        else {
            return false;
        };
        let pinned = !tab.is_pinned();
        tab.set_pinned(pinned);
        pinned
    }

    /// Whether the tab at `index` is pinned.
    pub(crate) fn tab_is_pinned(&self, pane: PaneId, index: usize) -> bool {
        self.workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .is_some_and(jtf_workspace::Tab::is_pinned)
    }

    pub(crate) fn close_tab(&mut self, pane: PaneId, index: usize) {
        let Some(id) = self
            .workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .map(jtf_workspace::Tab::id)
        else {
            return;
        };
        let last_tab = self
            .workspace
            .pane(pane)
            .is_some_and(|p| p.tab_count() <= 1);
        if !last_tab {
            if let Some(p) = self.workspace.pane_mut(pane) {
                p.close_tab(id, true);
            }
            self.start_enumeration(pane);
            return;
        }

        // Closing the only tab of a pane closes the pane. Refusing was the
        // easy reading of "a pane must have a tab", but it left the close
        // control on the last tab doing nothing at all, which reads as a
        // broken button rather than as a rule. The one case that must still
        // refuse is the last tab of the last pane: there would be nothing
        // left to look at.
        if self.workspace.close_pane(pane).is_ok() {
            self.views.remove(&pane);
            return;
        }
        self.start_enumeration(pane);
    }

    pub(crate) fn activate_tab(&mut self, pane: PaneId, index: usize) {
        let Some(id) = self
            .workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .map(jtf_workspace::Tab::id)
        else {
            return;
        };
        if let Some(p) = self.workspace.pane_mut(pane) {
            p.activate(id);
        }
        self.start_enumeration(pane);
    }

    // ------------------------------------------------------------ navigation

    /// The pane's folder as a *local* path, empty when it has none.
    ///
    /// The emptiness is load-bearing: bookmarks and typed relative paths are
    /// both about the local filesystem, and an empty answer is how they
    /// decline a server. Anything meant for a person to read wants
    /// [`Self::display_path`] instead.
    pub(crate) fn current_path(&self, pane: PaneId) -> String {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|t| t.location().as_path())
            .map_or_else(String::new, |p| p.display().to_string())
    }

    /// The pane's folder as something to show: the path bar, the folder tree,
    /// the preview panel's heading.
    pub(crate) fn display_path(&self, pane: PaneId) -> String {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(String::new, |t| t.location().display_text())
    }

    /// Point a pane at a location that is already resolved.
    ///
    /// `navigate` takes text a person typed and has to turn it into a local
    /// path first. A remote location has no such text form here - it is
    /// assembled from a host, a port, a user and a path that the connection
    /// dialog has already separated - so it goes straight in.
    pub(crate) fn navigate_to_location(&mut self, pane: PaneId, location: Location) {
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.navigate_to(location);
            }
        }
        self.start_enumeration(pane);
    }

    pub(crate) fn navigate(&mut self, pane: PaneId, path: &str) {
        // What a person types is not yet a path: `~`, `$HOME`, `..` and a
        // bare relative name all have to become one first, or typing `~`
        // navigates to a folder actually named `~`.
        let home = home_location()
            .as_path()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        let current = PathBuf::from(self.current_path(pane));
        let Some(target) =
            jtf_core::pathinput::expand(path, &home, &current, &|name| std::env::var(name).ok())
        else {
            return;
        };
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.navigate_to(Location::local(target));
            }
        }
        self.start_enumeration(pane);
    }

    /// Measure the folders among the pane's targets, and remember the totals.
    ///
    /// Every marked folder is measured separately, because "how big is each of
    /// these" is the question people actually have when they select several.
    /// Returns how many were measured.
    ///
    /// Runs synchronously: this is a deliberate action on a chosen set, not
    /// something that happens on arrival, and the alternative - a job with
    /// progress - is worth building only once someone points it at a folder
    /// large enough to notice.
    /// Measure the folders the user means.
    ///
    /// Marks and selection are how you ask for *several* folders at once. When
    /// they name no folder at all - nothing marked, or marks that are all
    /// files - the folder under the cursor is what was meant, and asking the
    /// user to select it first is asking them to repeat something they have
    /// already said by putting the cursor there.
    pub(crate) fn measure_folder_sizes(&mut self, pane: PaneId) -> usize {
        let mut folders: Vec<PathBuf> = self
            .operation_sources(pane)
            .into_iter()
            .filter(|path| path.is_dir())
            .collect();
        if folders.is_empty() {
            folders = self
                .workspace
                .pane(pane)
                .and_then(jtf_workspace::Pane::active_tab)
                .and_then(jtf_workspace::Tab::active_entry)
                .and_then(|location| location.as_path())
                .filter(|path| path.is_dir())
                .map(|path| vec![path.to_path_buf()])
                .unwrap_or_default();
        }
        if folders.is_empty() {
            return 0;
        }
        let total = folders.len();
        let (sender, updates) = std::sync::mpsc::channel();
        let (token, canceller) = CancellationToken::new();
        std::thread::spawn(move || {
            for path in folders {
                if token.is_cancelled() {
                    break;
                }
                let progress_sender = sender.clone();
                let progress_path = path.clone();
                let size = jtf_fs::measure_with(&path, &token, |running| {
                    // Ignored on purpose: a closed receiver means the window
                    // is gone, and the walk stops on the next cancellation
                    // check rather than panicking here.
                    let _ = progress_sender.send((progress_path.clone(), *running, false));
                });
                if sender.send((path, size, true)).is_err() {
                    break;
                }
            }
        });
        self.measuring = Some(Measuring {
            updates,
            canceller,
            files: 0,
            bytes: 0,
            done: 0,
            total,
        });
        total
    }

    /// Start extracting the archive under the cursor into `destination`.
    ///
    /// Returns whether there was an archive to extract. `Z` on an archive is
    /// CView's key for this; the destination is asked for first, exactly as
    /// `CV.HLP` §二 describes.
    pub(crate) fn start_extract(&mut self, pane: PaneId, destination: &str) -> bool {
        let Some(archive) = self.archive_under_cursor(pane) else {
            return false;
        };
        let target = PathBuf::from(destination);
        let (sender, updates) = std::sync::mpsc::channel();
        let (token, canceller) = CancellationToken::new();
        std::thread::spawn(move || {
            let progress = sender.clone();
            let outcome =
                jtf_fs::extract_archive(&archive, &target, &token, |done: &jtf_fs::Extracted| {
                    let _ = progress.send(ArchiveUpdate::Progress {
                        files: done.files,
                        bytes: done.bytes,
                    });
                });
            let _ = sender.send(match outcome {
                Ok(done) => ArchiveUpdate::Done {
                    refused: done.refused,
                    error: None,
                },
                Err(error) => ArchiveUpdate::Done {
                    refused: 0,
                    error: Some(error.context().to_string()),
                },
            });
        });
        self.archiving = Some(Archiving {
            updates,
            canceller,
            files: 0,
            bytes: 0,
            refused: 0,
            outcome: ArchiveOutcome::Running,
            compressing: false,
        });
        true
    }

    /// Extract `members` from `archive` into `destination`, or all of it when
    /// the list is empty.
    ///
    /// Takes the archive by path rather than from the cursor, because the
    /// archive window acts on the archive it is showing and the cursor has
    /// long since moved on.
    pub(crate) fn start_extract_from(
        &mut self,
        archive: &str,
        destination: &str,
        members: Vec<String>,
    ) -> bool {
        let archive = PathBuf::from(archive);
        if !archive.is_file() {
            return false;
        }
        let target = PathBuf::from(destination);
        let (sender, updates) = std::sync::mpsc::channel();
        let (token, canceller) = CancellationToken::new();
        std::thread::spawn(move || {
            let progress = sender.clone();
            let outcome = jtf_fs::extract_container_members(
                &archive,
                &target,
                &members,
                &token,
                |done: &jtf_fs::Extracted| {
                    let _ = progress.send(ArchiveUpdate::Progress {
                        files: done.files,
                        bytes: done.bytes,
                    });
                },
            );
            let _ = sender.send(match outcome {
                Ok(done) => ArchiveUpdate::Done {
                    refused: done.refused,
                    error: None,
                },
                Err(error) => ArchiveUpdate::Done {
                    refused: 0,
                    error: Some(error.context().to_string()),
                },
            });
        });
        self.archiving = Some(Archiving {
            updates,
            canceller,
            files: 0,
            bytes: 0,
            refused: 0,
            outcome: ArchiveOutcome::Running,
            compressing: false,
        });
        true
    }

    /// Start compressing the marked entries, or the one under the cursor,
    /// into `archive`.
    pub(crate) fn start_compress(&mut self, pane: PaneId, archive: &str) -> bool {
        let mut sources = self.operation_sources(pane);
        if sources.is_empty() {
            return false;
        }
        sources.sort();
        let target = PathBuf::from(archive);
        let (sender, updates) = std::sync::mpsc::channel();
        let (token, canceller) = CancellationToken::new();
        std::thread::spawn(move || {
            let progress = sender.clone();
            let outcome = jtf_fs::create_archive(&target, &sources, &token, |files| {
                let _ = progress.send(ArchiveUpdate::Progress { files, bytes: 0 });
            });
            let _ = sender.send(match outcome {
                Ok(_) => ArchiveUpdate::Done {
                    refused: 0,
                    error: None,
                },
                Err(error) => ArchiveUpdate::Done {
                    refused: 0,
                    error: Some(error.context().to_string()),
                },
            });
        });
        self.archiving = Some(Archiving {
            updates,
            canceller,
            files: 0,
            bytes: 0,
            refused: 0,
            outcome: ArchiveOutcome::Running,
            compressing: true,
        });
        true
    }

    // ------------------------------------------------- comparing two folders

    /// Start comparing the folders two panes are showing.
    ///
    /// Returns whether it started: it will not compare a pane against itself,
    /// and it will not start a second comparison over a running one.
    pub(crate) fn start_compare(&mut self, left: PaneId, right: PaneId, recursive: bool) -> bool {
        if left == right {
            return false;
        }
        if matches!(
            self.comparing.as_ref().map(|job| &job.outcome),
            Some(CompareOutcome::Running)
        ) {
            return false;
        }
        let Some(left_location) = self.pane_location(left) else {
            return false;
        };
        let Some(right_location) = self.pane_location(right) else {
            return false;
        };

        // Handles for the worker. The SFTP one shares the pool, so a server
        // the pane has already signed in to is not signed in to again.
        let local = self.provider;
        let sftp = self.sftp.clone();
        let (token, canceller) = CancellationToken::new();
        let (sender, updates) = std::sync::mpsc::channel();
        let left_display = left_location.display_text();
        let right_display = right_location.display_text();
        std::thread::spawn(move || {
            let list = |location: &Location| {
                if sftp.handles(location) {
                    sftp.list(location, &token)
                } else {
                    local.list(location, &token)
                }
            };
            let reporter = sender.clone();
            let outcome = jtf_fs::compare_with(
                &left_location,
                &right_location,
                &list,
                recursive,
                &token,
                &mut |progress| {
                    let _ = reporter.send(CompareUpdate::Progress(progress));
                },
            );
            let _ = sender.send(CompareUpdate::Done(
                outcome.map_err(|error| error.context().to_string()),
            ));
        });

        self.comparing = Some(Comparing {
            updates,
            canceller,
            left: left_display,
            right: right_display,
            recursive,
            outcome: CompareOutcome::Running,
            progress: jtf_fs::CompareProgress::default(),
        });
        true
    }

    /// Take whatever the comparison thread has said. Returns whether anything
    /// changed.
    pub(crate) fn pump_compare(&mut self) -> bool {
        let Some(job) = self.comparing.as_mut() else {
            return false;
        };
        if !matches!(job.outcome, CompareOutcome::Running) {
            return false;
        }
        let mut changed = false;
        // Drained rather than read one at a time: the walk reports per folder
        // and only the latest total is worth showing, so a slow poll must not
        // leave a backlog to catch up on one tick at a time.
        loop {
            match job.updates.try_recv() {
                Ok(CompareUpdate::Progress(progress)) => {
                    job.progress = progress;
                    changed = true;
                }
                Ok(CompareUpdate::Done(Ok(comparison))) => {
                    job.outcome = CompareOutcome::Done(comparison);
                    return true;
                }
                Ok(CompareUpdate::Done(Err(error))) => {
                    job.outcome = CompareOutcome::Failed(error);
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // The worker is gone without an answer - it panicked, or
                    // the channel closed early. Said plainly rather than left
                    // spinning.
                    job.outcome = CompareOutcome::Failed(String::new());
                    return true;
                }
            }
        }
        changed
    }

    /// The running totals of the comparison: folders read, rows, differences.
    pub(crate) fn compare_progress(&self) -> (u64, u64, u64) {
        self.comparing.as_ref().map_or((0, 0, 0), |job| {
            (
                job.progress.folders,
                job.progress.rows,
                job.progress.differences,
            )
        })
    }

    /// Stop a running comparison, leaving whatever it had.
    pub(crate) fn cancel_compare(&mut self) {
        if let Some(job) = self.comparing.as_ref() {
            job.canceller.cancel();
        }
    }

    /// Where the comparison has got to.
    pub(crate) fn compare_outcome(&self) -> Option<&CompareOutcome> {
        self.comparing.as_ref().map(|job| &job.outcome)
    }

    /// The finished comparison, if there is one.
    pub(crate) fn comparison(&self) -> Option<&jtf_fs::Comparison> {
        match self.comparing.as_ref().map(|job| &job.outcome) {
            Some(CompareOutcome::Done(comparison)) => Some(comparison),
            _ => None,
        }
    }

    /// One row of the finished comparison.
    pub(crate) fn comparison_row(&self, index: usize) -> Option<&jtf_fs::ComparisonRow> {
        self.comparison().and_then(|c| c.rows.get(index))
    }

    /// What was compared, for the window's heading.
    pub(crate) fn compare_sides(&self) -> (&str, &str) {
        self.comparing
            .as_ref()
            .map_or(("", ""), |job| (job.left.as_str(), job.right.as_str()))
    }

    /// Whether the running comparison was asked to walk subfolders.
    pub(crate) fn compare_is_recursive(&self) -> bool {
        self.comparing.as_ref().is_some_and(|job| job.recursive)
    }

    /// Stop the comparison and forget it.
    ///
    /// Cancelled rather than merely dropped: the worker checks the token
    /// between folders, so a walk over a large tree stops rather than running
    /// on into a channel nobody is reading.
    pub(crate) fn close_compare(&mut self) {
        if let Some(job) = self.comparing.take() {
            job.canceller.cancel();
        }
    }

    // ------------------------------------------------- where the space went

    /// Start analysing what fills `path`.
    ///
    /// Returns whether it started: it will not start a second analysis over a
    /// running one, and it will not analyse a location with no path on this
    /// machine - a server's disc is not ours to walk.
    pub(crate) fn start_usage(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        if matches!(
            self.analysing.as_ref().map(|job| &job.outcome),
            Some(UsageOutcome::Running)
        ) {
            return false;
        }
        let root = PathBuf::from(path);
        if !root.is_dir() {
            return false;
        }
        let (token, canceller) = CancellationToken::new();
        let (sender, updates) = std::sync::mpsc::channel();
        let display = path.to_string();
        std::thread::spawn(move || {
            let reporter = sender.clone();
            let usage = jtf_fs::analyse_usage_with(&root, &token, |progress| {
                let _ = reporter.send(UsageUpdate::Progress(progress));
            });
            let _ = sender.send(UsageUpdate::Done(Box::new(usage)));
        });
        self.analysing = Some(Analysing {
            updates,
            canceller,
            root: display,
            outcome: UsageOutcome::Running,
            progress: jtf_fs::UsageProgress::default(),
        });
        true
    }

    /// Take whatever the analysis thread has said. Returns whether anything
    /// changed.
    pub(crate) fn pump_usage(&mut self) -> bool {
        let Some(job) = self.analysing.as_mut() else {
            return false;
        };
        if !matches!(job.outcome, UsageOutcome::Running) {
            return false;
        }
        let mut changed = false;
        // Drained rather than read one at a time: only the latest total is
        // worth showing, so a slow poll must not catch up one tick at a time.
        loop {
            match job.updates.try_recv() {
                Ok(UsageUpdate::Progress(progress)) => {
                    job.progress = progress;
                    changed = true;
                }
                Ok(UsageUpdate::Done(usage)) => {
                    job.outcome = UsageOutcome::Done(usage);
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // The worker is gone without an answer. Treated as
                    // finished with nothing rather than left spinning.
                    job.outcome = UsageOutcome::Done(Box::default());
                    return true;
                }
            }
        }
        changed
    }

    /// Whether the analysis has finished.
    pub(crate) fn usage_is_done(&self) -> bool {
        matches!(
            self.analysing.as_ref().map(|job| &job.outcome),
            Some(UsageOutcome::Done(_))
        )
    }

    /// The finished breakdown, if there is one.
    pub(crate) fn usage(&self) -> Option<&jtf_fs::Usage> {
        match self.analysing.as_ref().map(|job| &job.outcome) {
            Some(UsageOutcome::Done(usage)) => Some(usage),
            _ => None,
        }
    }

    /// The running totals: bytes, files, folders.
    pub(crate) fn usage_progress(&self) -> (u64, u64, u64) {
        self.analysing.as_ref().map_or((0, 0, 0), |job| {
            (job.progress.bytes, job.progress.files, job.progress.folders)
        })
    }

    /// The folder the walk is in, or empty when it is not running.
    pub(crate) fn usage_in(&self) -> String {
        self.analysing.as_ref().map_or_else(String::new, |job| {
            if matches!(job.outcome, UsageOutcome::Running) {
                job.progress.directory.to_string_lossy().into_owned()
            } else {
                String::new()
            }
        })
    }

    /// What is being analysed.
    pub(crate) fn usage_root(&self) -> &str {
        self.analysing.as_ref().map_or("", |job| job.root.as_str())
    }

    /// Stop the walk, keeping nothing: a half-counted breakdown would read as
    /// a complete one.
    pub(crate) fn cancel_usage(&mut self) {
        if let Some(job) = self.analysing.as_ref() {
            job.canceller.cancel();
        }
    }

    /// Stop the analysis and forget it.
    pub(crate) fn close_usage(&mut self) {
        if let Some(job) = self.analysing.take() {
            job.canceller.cancel();
        }
    }

    /// The pane's location, if it has one.
    fn pane_location(&self, pane: PaneId) -> Option<Location> {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|tab| tab.location().clone())
    }

    /// Record which row the cursor is on.
    ///
    /// `Tab::active_entry` is a real part of the model - the session stores it,
    /// and `operation_target` falls back to it when nothing is marked or
    /// selected - but nothing ever set it from the interface, so it was always
    /// empty. Anything that asked "what is the cursor on" got no answer:
    /// pressing Enter on an archive did nothing, and `Z` could not find the
    /// folder under the cursor.
    pub(crate) fn set_current_row(&mut self, pane: PaneId, row: Option<usize>) {
        let entry = row.and_then(|row| self.entry_at(pane, row).map(|e| e.location().clone()));
        if let Some(tab) = self
            .workspace
            .pane_mut(pane)
            .and_then(jtf_workspace::Pane::active_tab_mut)
        {
            tab.set_active_entry(entry);
        }
    }

    /// The archive the cursor is on, if it is on one.
    fn archive_under_cursor(&self, pane: PaneId) -> Option<PathBuf> {
        let path = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(jtf_workspace::Tab::active_entry)
            .and_then(|location| location.as_path())?
            .to_path_buf();
        // By the file's own bytes, and only the formats this build can read -
        // ZIP and ISO 9660. A file named `.rar` is not something to open and
        // fail on halfway through, and a `.zip` that is not one should fail to
        // open rather than open into an empty window.
        jtf_viewer::container_of(&path).map(|_| path)
    }

    /// Whether the cursor is on something this build can extract.
    pub(crate) fn cursor_is_archive(&self, pane: PaneId) -> bool {
        self.archive_under_cursor(pane).is_some()
    }

    /// Take whatever the archive thread has said. Returns whether anything
    /// changed.
    pub(crate) fn pump_archive(&mut self) -> bool {
        let Some(job) = self.archiving.as_mut() else {
            return false;
        };
        let mut changed = false;
        loop {
            match job.updates.try_recv() {
                Ok(ArchiveUpdate::Progress { files, bytes }) => {
                    changed = true;
                    job.files = files;
                    job.bytes = bytes;
                }
                Ok(ArchiveUpdate::Done { refused, error }) => {
                    changed = true;
                    job.refused = refused;
                    job.outcome = error.map_or(ArchiveOutcome::Succeeded, ArchiveOutcome::Failed);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // The worker is gone without a verdict - it panicked, or
                    // the channel closed early. Treated as finished rather
                    // than left running forever with a spinner nobody can stop.
                    changed = true;
                    if matches!(job.outcome, ArchiveOutcome::Running) {
                        job.outcome = ArchiveOutcome::Succeeded;
                    }
                    break;
                }
            }
        }
        changed
    }

    /// Whether an extraction or compression is still running.
    pub(crate) fn is_archiving(&self) -> bool {
        self.archiving
            .as_ref()
            .is_some_and(|job| matches!(job.outcome, ArchiveOutcome::Running))
    }

    /// Files, bytes, members refused, and whether this was a compression.
    pub(crate) fn archive_progress(&self) -> (u64, u64, u64, bool) {
        self.archiving.as_ref().map_or((0, 0, 0, false), |job| {
            (job.files, job.bytes, job.refused, job.compressing)
        })
    }

    /// The verdict once finished, and `None` while still running.
    ///
    /// Clears the job, so it is asked for once.
    pub(crate) fn take_archive_result(&mut self) -> Option<ArchiveOutcome> {
        if matches!(self.archiving.as_ref()?.outcome, ArchiveOutcome::Running) {
            return None;
        }
        self.archiving.take().map(|job| job.outcome)
    }

    /// Stop the extraction or compression.
    pub(crate) fn cancel_archive(&mut self) {
        if let Some(job) = self.archiving.take() {
            job.canceller.cancel();
        }
    }

    /// Take whatever the measuring thread has said since last time.
    ///
    /// Returns whether anything changed, so the UI redraws only when there is
    /// something new to draw.
    pub(crate) fn pump_measure(&mut self) -> bool {
        let Some(job) = self.measuring.as_mut() else {
            return false;
        };
        let mut changed = false;
        let mut finished = Vec::new();
        loop {
            match job.updates.try_recv() {
                Ok((path, size, done)) => {
                    changed = true;
                    if done {
                        job.done += 1;
                        finished.push((path, size));
                    } else {
                        // A running total for the folder in progress. Shown,
                        // not stored: only a completed walk is a real answer.
                        job.files = size.files;
                        job.bytes = size.bytes;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    changed = true;
                    self.measuring = None;
                    break;
                }
            }
        }
        for (path, size) in finished {
            self.folder_sizes.insert(path, size);
        }
        if let Some(job) = self.measuring.as_ref() {
            if job.done >= job.total {
                self.measuring = None;
            }
        }
        changed
    }

    /// Whether a measurement is running, for the progress bar.
    pub(crate) const fn is_measuring(&self) -> bool {
        self.measuring.is_some()
    }

    /// Files and bytes counted so far.
    pub(crate) fn measure_progress(&self) -> (u64, u64) {
        self.measuring
            .as_ref()
            .map_or((0, 0), |job| (job.files, job.bytes))
    }

    /// Stop the measurement. The totals already stored stay.
    pub(crate) fn cancel_measure(&mut self) {
        if let Some(job) = self.measuring.take() {
            job.canceller.cancel();
        }
    }

    /// The remembered size of a folder, or None if it has not been measured.
    pub(crate) fn folder_size(&self, path: &Path) -> Option<u64> {
        self.folder_sizes.get(path).map(|size| size.bytes)
    }

    /// Forget every measurement, so the next request re-measures.
    pub(crate) fn clear_folder_sizes(&mut self) {
        self.folder_sizes.clear();
    }

    /// The row the cursor should move to, or -1. Consumed by the call.
    ///
    /// Returned as a row rather than a name so the scan happens here, over
    /// the entries we already hold, instead of the UI asking for every row's
    /// text across the boundary to find one of them.
    pub(crate) fn take_focus_row(&mut self, pane: PaneId) -> isize {
        let parent_row = isize::from(self.has_parent_row(pane));
        let Some(view) = self.views.get_mut(&pane) else {
            return -1;
        };
        let Some(name) = view.focus_name.take() else {
            return -1;
        };
        view.visible
            .iter()
            .position(|&index| {
                view.entries
                    .get(index)
                    .is_some_and(|entry| entry.display_name() == name)
            })
            .map_or(-1, |row| {
                isize::try_from(row).unwrap_or(0).saturating_add(parent_row)
            })
    }

    pub(crate) fn navigate_up(&mut self, pane: PaneId) {
        let parent = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|t| t.location().parent())
            .map(Self::containing_folder);
        if let Some(parent) = parent {
            // Remember which folder we are stepping out of, so the cursor
            // lands on it rather than at the top of a list you were just in
            // the middle of.
            // By `file_name`, which every kind of location can answer, rather
            // than through a local path - a remote folder has none, and the
            // cursor would land at the top of the list you just stepped out of.
            let leaving = self
                .workspace
                .pane(pane)
                .and_then(jtf_workspace::Pane::active_tab)
                .and_then(|tab| tab.location().file_name())
                .map(|name| name.to_string_lossy().into_owned());
            if let Some(p) = self.workspace.pane_mut(pane) {
                if let Some(tab) = p.active_tab_mut() {
                    tab.navigate_to(parent);
                }
            }
            self.start_enumeration(pane);
            if let Some(view) = self.views.get_mut(&pane) {
                view.focus_name = leaving;
            }
        }
    }

    pub(crate) fn go_back(&mut self, pane: PaneId) {
        let moved = self
            .workspace
            .pane_mut(pane)
            .and_then(jtf_workspace::Pane::active_tab_mut)
            .is_some_and(jtf_workspace::Tab::go_back);
        if moved {
            self.start_enumeration(pane);
        }
    }

    pub(crate) fn go_forward(&mut self, pane: PaneId) {
        let moved = self
            .workspace
            .pane_mut(pane)
            .and_then(jtf_workspace::Pane::active_tab_mut)
            .is_some_and(jtf_workspace::Tab::go_forward);
        if moved {
            self.start_enumeration(pane);
        }
    }

    /// Enter a row if it is a directory. Returns whether it navigated.
    pub(crate) fn open_row(&mut self, pane: PaneId, row: usize) -> bool {
        if self.has_parent_row(pane) && row == 0 {
            self.navigate_up(pane);
            return true;
        }
        let Some(entry) = self.entry_at(pane, row) else {
            return false;
        };
        if !entry.kind().is_navigable_by_default() && entry.kind() != FileKind::Symlink {
            return false;
        }

        // A remote entry has no path on this machine, so the guard below - and
        // everything after it, which asks the local filesystem what a path is -
        // does not apply. What it is was already reported by the server when
        // the folder was listed, so the kind is the answer. Without this,
        // pressing Enter on a remote folder did nothing at all.
        if entry.location().is_remote() {
            if entry.kind() != FileKind::Directory {
                // Opening a remote *file* means fetching it first, which is
                // stage two of ADR-0004.
                return false;
            }
            let target = entry.location().clone();
            self.navigate_to_location(pane, target);
            return true;
        }

        let Some(path) = entry.location().as_path().map(std::path::Path::to_path_buf) else {
            return false;
        };
        if path.is_dir() {
            self.navigate(pane, &path.display().to_string());
            return true;
        }
        // A container is entered like a folder rather than handed to whatever
        // application owns .zip or .iso - which would extract it or mount it,
        // not show it.
        if jtf_viewer::container_of(&path).is_some() {
            self.navigate(pane, &path.display().to_string());
            return true;
        }
        false
    }

    // ---------------------------------------------------------------- rows

    /// Identity of the current row set. See [`PaneView::generation`].
    pub(crate) fn row_generation(&self, pane: PaneId) -> u64 {
        self.views.get(&pane).map_or(0, |v| v.generation)
    }

    pub(crate) fn row_count(&self, pane: PaneId) -> usize {
        let listed = self.views.get(&pane).map_or(0, |v| v.visible.len());
        listed + usize::from(self.has_parent_row(pane))
    }

    /// Whether the pane shows a `..` row above its entries.
    ///
    /// Only while browsing a directory that has a parent. It is not shown in
    /// search results, where there is no single folder to be the parent of,
    /// and not while a filter is narrowing the list, where a row that matches
    /// nothing the user typed would be the one exception on screen.
    pub(crate) fn has_parent_row(&self, pane: PaneId) -> bool {
        if !self.settings.parent_row {
            return false;
        }
        let Some(view) = self.views.get(&pane) else {
            return false;
        };
        if view.search.is_some() || !view.query.is_empty() {
            return false;
        }
        let filtering = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(|tab| tab.filter().is_active());
        if filtering {
            return false;
        }
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|tab| tab.location().parent())
            .is_some()
    }

    /// How many rows are real entries, excluding any `..` row.
    ///
    /// The status line counts items; the parent row is a way out of the
    /// folder, not a thing in it.
    pub(crate) fn listed_count(&self, pane: PaneId) -> usize {
        self.views.get(&pane).map_or(0, |v| v.visible.len())
    }

    /// How many entries the directory has before filtering, so the UI can say
    /// "12 of 3400" rather than pretending the rest are not there.
    pub(crate) fn unfiltered_count(&self, pane: PaneId) -> usize {
        self.views.get(&pane).map_or(0, |v| v.entries.len())
    }

    /// The entry a display row refers to.
    fn entry_at(&self, pane: PaneId, row: usize) -> Option<&FileEntry> {
        // The `..` row has no entry behind it, and returning None here is what
        // makes every operation ignore it: marking, sizing, copying and
        // deleting all resolve rows through this one function, so none of them
        // can be talked into acting on the parent directory.
        let row = self.listed_row(pane, row)?;
        let view = self.views.get(&pane)?;
        view.visible
            .get(row)
            .and_then(|index| view.entries.get(*index))
    }

    /// A display row as an index into the listed entries, or None for `..`.
    fn listed_row(&self, pane: PaneId, row: usize) -> Option<usize> {
        listed_row(self.has_parent_row(pane), row)
    }

    /// The pane's filter text.
    pub(crate) fn filter_text(&self, pane: PaneId) -> String {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(String::new, |tab| tab.filter().text.clone())
    }

    /// Narrow the pane to entries whose name contains `text`.
    ///
    /// Applied to what has already been read, so it is instant and does not
    /// touch the disk. Search — which walks a tree — is a different feature
    /// with a different cost, and conflating them would make a filter feel
    /// slow (`docs/SEARCH_AI.md` §1).
    pub(crate) fn set_filter(&mut self, pane: PaneId, text: &str) {
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.filter_mut().text = text.to_string();
            }
        }
        let show_hidden = self.show_hidden;
        let needle = text.to_lowercase();
        if let Some(view) = self.views.get_mut(&pane) {
            Self::recompute_visible(view, &needle, show_hidden);
            view.generation += 1;
        }
    }

    /// Rebuild the visible index from the entries, the filter and the
    /// hidden-file setting.
    fn recompute_visible(view: &mut PaneView, needle: &str, show_hidden: bool) {
        view.visible = view
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| show_hidden || !entry.attributes().hidden)
            .filter(|(_, entry)| {
                needle.is_empty() || entry.display_name().to_lowercase().contains(needle)
            })
            .map(|(index, _)| index)
            .collect();

        let mut folders = 0;
        let mut bytes = 0u64;
        for entry in view.visible.iter().filter_map(|&i| view.entries.get(i)) {
            if entry.kind() == FileKind::Directory {
                folders += 1;
            } else {
                // Saturating: a filesystem reporting nonsense sizes must not
                // wrap this into a small number (docs/SECURITY.md 13).
                bytes = bytes.saturating_add(entry.size().unwrap_or(0));
            }
        }
        view.folder_count = folders;
        view.visible_bytes = bytes;
    }

    /// How many of the shown rows are folders.
    pub(crate) fn folder_count(&self, pane: PaneId) -> usize {
        self.views.get(&pane).map_or(0, |v| v.folder_count)
    }

    /// The size of the shown files, folders excluded.
    pub(crate) fn visible_bytes(&self, pane: PaneId) -> u64 {
        self.views.get(&pane).map_or(0, |v| v.visible_bytes)
    }

    pub(crate) fn is_loading(&self, pane: PaneId) -> bool {
        self.views.get(&pane).is_some_and(|v| v.loading)
    }

    /// What the failure actually said, beyond its category.
    ///
    /// The message key gives "you do not have permission"; the context says
    /// *why* - "the server refused that password", "unknown host key
    /// SHA256:...", "no key was accepted; add one to your agent or to
    /// ~/.ssh". Showing only the category turned every distinct SFTP failure
    /// into the same sentence, and one that pointed at the wrong thing: the
    /// folder was readable, the sign-in was not.
    pub(crate) fn error_detail(&self, pane: PaneId) -> String {
        self.views
            .get(&pane)
            .and_then(|v| v.error.as_ref())
            .map_or_else(String::new, |error| error.context().to_string())
    }

    /// Hold a password for the server this pane is pointed at, and try again.
    ///
    /// Takes the pane rather than a host and a user, so the interface never
    /// has to take apart a displayed path to work out which server it is
    /// talking about.
    pub(crate) fn set_pane_password(&mut self, pane: PaneId, password: &str) -> bool {
        let Some(location) = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|tab| tab.location().clone())
        else {
            return false;
        };
        let Some(endpoint) = jtf_fs::sftp::Endpoint::of(&location) else {
            return false;
        };
        self.sftp.set_password(endpoint, password.to_string());
        self.start_enumeration(pane);
        true
    }

    /// Whether the pane failed because it could not sign in.
    ///
    /// Distinct from an ordinary permission error: a folder you are not
    /// allowed to read is not something a password fixes, and offering to
    /// re-enter one there would be noise. This is the case where the server
    /// refused *the sign-in* - no key accepted, or the password rejected - and
    /// the answer is to ask for a password and try again.
    pub(crate) fn pane_needs_credentials(&self, pane: PaneId) -> bool {
        let remote = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(|tab| tab.location().is_remote());
        if !remote {
            return false;
        }
        self.views
            .get(&pane)
            .and_then(|v| v.error.as_ref())
            .is_some_and(|error| {
                error.code() == jtf_core::ErrorCode::PermissionDenied
                    && (error.context().contains("no key was accepted")
                        || error.context().contains("refused that password"))
            })
    }

    pub(crate) fn error_key(&self, pane: PaneId) -> Option<&'static str> {
        self.views
            .get(&pane)
            .and_then(|v| v.error.as_ref())
            .map(jtf_core::Error::message_key)
    }

    pub(crate) fn row_text(&self, pane: PaneId, row: usize, column: i32) -> String {
        if self.has_parent_row(pane) && row == 0 {
            // Two dots, not a translated phrase: `..` is what the shell calls
            // it and what every file manager shows, and a localized "Parent
            // folder" would sort and read as a file name.
            return if column == COLUMN_NAME {
                "..".to_string()
            } else {
                String::new()
            };
        }
        let Some(entry) = self.entry_at(pane, row) else {
            return String::new();
        };
        match column {
            COLUMN_NAME => entry.display_name(),
            COLUMN_SIZE => entry.size().map_or_else(
                || {
                    if entry.kind().is_directory_on_disk() {
                        // Blank until measured. A folder's size costs a walk
                        // of everything beneath it, so it is asked for rather
                        // than assumed (`docs/BASELINE_FEATURES.md`).
                        entry
                            .location()
                            .as_path()
                            .and_then(|path| self.folder_size(path))
                            .map_or_else(String::new, format_size)
                    } else {
                        "--".to_string()
                    }
                },
                format_size,
            ),
            COLUMN_KIND => self.localizer.text_or_key(entry.kind().label_key()),
            COLUMN_MODIFIED => entry
                .timestamps()
                .modified
                .map_or_else(String::new, |t| format_time(t, self.utc_offset)),
            other => {
                // The columns past the default four, answered from the model's
                // own list so adding one there is enough.
                use jtf_workspace::Column;
                match column_at(other) {
                    Some(Column::Created) => entry
                        .timestamps()
                        .created
                        .map_or_else(String::new, |t| format_time(t, self.utc_offset)),
                    Some(Column::Accessed) => entry
                        .timestamps()
                        .accessed
                        .map_or_else(String::new, |t| format_time(t, self.utc_offset)),
                    Some(Column::Permissions) => {
                        // rwx, the shape everyone already reads, from the
                        // cross-platform summary rather than from a mode bit
                        // that Windows does not have.
                        let p = entry.permissions();
                        let flag = |on: bool, c: char| if on { c } else { '-' };
                        format!(
                            "{}{}{}",
                            flag(p.readable, 'r'),
                            flag(p.writable, 'w'),
                            flag(p.executable, 'x')
                        )
                    }
                    Some(Column::Extension) => entry.extension_hint().unwrap_or_default(),
                    Some(Column::Path) => entry
                        .location()
                        .as_path()
                        .and_then(std::path::Path::parent)
                        .map_or_else(String::new, |p| p.display().to_string()),
                    // Owner and Tags are not carried by the model yet. The
                    // columns exist so the header can offer them, and they
                    // stay blank rather than showing something invented.
                    _ => String::new(),
                }
            }
        }
    }

    /// Full path of a row, for the UI to ask the platform for its icon.
    ///
    /// The icon itself is the toolkit's business: `AGENTS.md` §8 says use
    /// native behaviour where users expect it, and a file's icon is the most
    /// visible instance of that.
    pub(crate) fn row_path(&self, pane: PaneId, row: usize) -> String {
        if self.has_parent_row(pane) && row == 0 {
            return self
                .workspace
                .pane(pane)
                .and_then(jtf_workspace::Pane::active_tab)
                .and_then(|tab| tab.location().parent())
                .map(Self::containing_folder)
                .and_then(|parent| parent.as_path().map(|p| p.display().to_string()))
                .unwrap_or_default();
        }
        let Some(entry) = self.entry_at(pane, row) else {
            return String::new();
        };
        // A remote row has a path on the *server*, returned for showing and
        // for copying as text - never for handing to the platform, because
        // `/srv/data` on a server and `/srv/data` here are different files
        // and the platform cannot tell them apart. The gate is on the pane,
        // not on the row: `jtf_pane_is_remote` decides which commands exist
        // at all (see `kLocalOnlyCommands` in mainwindow.cpp). This comment
        // used to claim every caller checked `row_is_remote` first; none did,
        // and the preview panel was reading server paths off the local disk.
        match entry.location() {
            Location::Remote { path, .. } => path.clone(),
            other => other
                .as_path()
                .map_or_else(String::new, |p| p.display().to_string()),
        }
    }

    /// Whether the row lives on a server rather than on this machine.
    pub(crate) fn row_is_remote(&self, pane: PaneId, row: usize) -> bool {
        if self.has_parent_row(pane) && row == 0 {
            return self.pane_is_remote(pane);
        }
        self.entry_at(pane, row)
            .is_some_and(|entry| entry.location().is_remote())
    }

    /// Whether the pane is showing a folder on a server.
    pub(crate) fn pane_is_remote(&self, pane: PaneId) -> bool {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(|tab| tab.location().is_remote())
    }

    /// Whether the row is a file the platform would run.
    ///
    /// Directories are traversable, which is the same permission bit, so they
    /// are excluded: colouring every folder as executable would tell nobody
    /// anything.
    pub(crate) fn row_is_executable(&self, pane: PaneId, row: usize) -> bool {
        self.entry_at(pane, row).is_some_and(|entry| {
            !entry.kind().is_directory_on_disk() && entry.permissions().executable
        })
    }

    /// Whether the row is hidden by platform convention.
    ///
    /// CView and WinCV both draw a hidden entry dimmer than the rest, so that
    /// a listing with hidden entries shown still reads as "these are the
    /// ordinary files, and those are the others". The model has carried the
    /// flag all along; nothing was asking for it.
    pub(crate) fn row_is_hidden(&self, pane: PaneId, row: usize) -> bool {
        if self.has_parent_row(pane) && row == 0 {
            return false;
        }
        self.entry_at(pane, row)
            .is_some_and(|entry| entry.attributes().hidden)
    }

    pub(crate) fn row_is_directory(&self, pane: PaneId, row: usize) -> bool {
        if self.has_parent_row(pane) && row == 0 {
            return true;
        }
        self.entry_at(pane, row)
            .is_some_and(|e| e.kind().is_directory_on_disk())
    }

    pub(crate) fn row_is_marked(&self, pane: PaneId, row: usize) -> bool {
        let Some(entry) = self.entry_at(pane, row) else {
            return false;
        };
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(|t| t.marks().contains(entry.location()))
    }

    pub(crate) fn toggle_mark(&mut self, pane: PaneId, row: usize) {
        // Through `entry_at`, not `entries[row]`: a display row is an index
        // into what is *shown*, and with a filter active the two lists differ.
        // Indexing the unfiltered list marked whichever file happened to sit
        // at that position in the directory.
        let Some(location) = self.entry_at(pane, row).map(|e| e.location().clone()) else {
            return;
        };
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.marks_mut().toggle(location);
            }
        }
    }

    /// Mark, unmark or invert every row currently listed in the pane.
    ///
    /// Scope is explicit: this acts on what the pane is showing, not on the
    /// whole filesystem and not on some remembered set
    /// (`docs/UI_TEST_PLAN.md` MARK-003).
    pub(crate) fn mark_listed(&mut self, pane: PaneId, action: MarkAction) {
        // "All" means what the pane is showing, filter included: marking
        // three thousand hidden entries the user cannot see would be a
        // surprise (docs/UI_TEST_PLAN.md MARK-003).
        let listed: Vec<Location> = (0..self.row_count(pane))
            .filter_map(|row| self.entry_at(pane, row))
            .map(|entry| entry.location().clone())
            .collect();
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                match action {
                    MarkAction::All => tab.marks_mut().mark_all(listed),
                    MarkAction::None => tab.marks_mut().unmark_all(listed),
                    MarkAction::Invert => tab.marks_mut().invert(listed),
                }
            }
        }
    }

    // ------------------------------------------------------------ selection

    /// Replace a pane's selection with the entries at these row indices.
    ///
    /// The UI owns the widget's selection; the model owns what an operation
    /// will act on. Syncing one into the other is what lets
    /// `OperationTarget` resolve marked-then-selection-then-active without
    /// the C++ side deciding anything (`docs/UI_UX_SPEC.md` §6).
    pub(crate) fn set_selection(&mut self, pane: PaneId, rows: &[usize]) {
        let locations: Vec<Location> = rows
            .iter()
            .filter_map(|row| self.entry_at(pane, *row))
            .map(|entry| entry.location().clone())
            .collect();

        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                let selection = tab.selection_mut();
                selection.clear();
                selection.select_range(locations);
            }
        }
    }

    /// Mark exactly the rows the user just multi-selected.
    ///
    /// Selection and marks remain different states: marks survive navigating
    /// away and back, a selection does not, and you can hold marks with
    /// nothing selected. What is linked is the *gesture* - a deliberate
    /// multi-select, by shift-clicking a range, shift-arrowing one out, or
    /// adding with the platform's modifier - now says "these", which is the
    /// only thing anyone selects several rows in order to mean.
    ///
    /// A single row is deliberately not included: moving the cursor is a
    /// selection of one, and marking as you move would make marks impossible
    /// to keep.
    pub(crate) fn mark_selected_rows(&mut self, pane: PaneId, rows: &[usize]) {
        if rows.len() < 2 {
            return;
        }
        let listed: Vec<Location> = (0..self.row_count(pane))
            .filter_map(|row| self.entry_at(pane, row))
            .map(|entry| entry.location().clone())
            .collect();
        let wanted: Vec<Location> = rows
            .iter()
            .filter_map(|row| self.entry_at(pane, *row))
            .map(|entry| entry.location().clone())
            .collect();
        if let Some(tab) = self
            .workspace
            .pane_mut(pane)
            .and_then(jtf_workspace::Pane::active_tab_mut)
        {
            // The selection replaces the marks within what is on screen, so
            // selecting a different range does not accumulate. Marks on rows
            // the filter is hiding are left alone - they were not part of
            // what the user just pointed at.
            tab.marks_mut().unmark_all(listed);
            tab.marks_mut().mark_all(wanted);
        }
    }

    /// Marks follow the selection exactly.
    ///
    /// Selecting *is* marking: what is highlighted is what is ticked, whether
    /// the rows were picked with the mouse, with Shift and the arrow keys, or
    /// with Space. Decided by the project owner after the two behaved
    /// differently for long enough to be confusing; `AGENTS.md` §10 records
    /// the change.
    ///
    /// The mark set stays the *stored* one - it is what the session keeps and
    /// what an operation reads - and the selection is restored from it on
    /// arriving in a folder, so marks still survive navigating away and back
    /// (`docs/UI_TEST_PLAN.md` MARK-004).
    pub(crate) fn set_marks_from_selection(&mut self, pane: PaneId, rows: &[usize]) {
        let listed: Vec<Location> = (0..self.row_count(pane))
            .filter_map(|row| self.entry_at(pane, row))
            .map(|entry| entry.location().clone())
            .collect();
        let wanted: Vec<Location> = rows
            .iter()
            .filter_map(|row| self.entry_at(pane, *row))
            .map(|entry| entry.location().clone())
            .collect();
        if let Some(tab) = self
            .workspace
            .pane_mut(pane)
            .and_then(jtf_workspace::Pane::active_tab_mut)
        {
            // Cleared within what is on screen only. Marks on rows a filter is
            // hiding were not part of what the user just pointed at, and
            // dropping them would lose work the filter merely hid.
            tab.marks_mut().unmark_all(listed);
            tab.marks_mut().mark_all(wanted);
        }
    }

    /// The rows that are marked, as display rows, for restoring the selection.
    pub(crate) fn marked_rows(&self, pane: PaneId) -> Vec<usize> {
        let Some(tab) = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
        else {
            return Vec::new();
        };
        (0..self.row_count(pane))
            .filter(|row| {
                self.entry_at(pane, *row)
                    .is_some_and(|entry| tab.marks().contains(entry.location()))
            })
            .collect()
    }

    /// What an operation started in this pane would act on, and from where.
    ///
    /// Restricted to the rows on screen, for the same reason the count is: a
    /// mark left behind in another folder is not something the user can see,
    /// and copying it because it is still in the set would move files they are
    /// not looking at. What is left after the filter is the marks in this
    /// folder; the cursor is the fallback when there are none, which is the
    /// precedence `OperationTarget` documents with the selection folded into
    /// the marks (`AGENTS.md` §10).
    fn operation_sources(&self, pane: PaneId) -> Vec<PathBuf> {
        let listed: Vec<PathBuf> = self
            .marked_rows(pane)
            .into_iter()
            .filter_map(|row| self.entry_at(pane, row))
            .filter_map(|entry| entry.location().as_path().map(std::path::Path::to_path_buf))
            .collect();
        if !listed.is_empty() {
            return listed;
        }
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(jtf_workspace::Tab::active_entry)
            .and_then(|location| location.as_path().map(std::path::Path::to_path_buf))
            .into_iter()
            .collect()
    }

    /// The one entry the cursor is on, ignoring marks.
    ///
    /// For commands that rename exactly one thing. `operation_sources` answers
    /// "what is this operation for" with the marked set when there is one,
    /// which is right for copy, move and trash - they take any number. Rename
    /// takes one, and taking it from the marks meant that a row marked and
    /// then left behind won over the row the cursor was visibly sitting on:
    /// press R on the fifth file and the first one's name came up in the box.
    ///
    /// Renaming several things at once is a different command with its own
    /// dialog (`file.batch_rename`), so nothing is lost by this being one.
    fn cursor_source(&self, pane: PaneId) -> Option<PathBuf> {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(jtf_workspace::Tab::active_entry)
            .and_then(|location| location.as_path().map(std::path::Path::to_path_buf))
    }

    /// The name shown in the rename box: the entry the cursor is on.
    pub(crate) fn cursor_name(&self, pane: PaneId) -> String {
        self.cursor_source(pane)
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_default()
    }

    // ----------------------------------------------------------- operations

    /// Build a plan for a copy, move, trash or delete started in `pane`.
    ///
    /// Returns whether there is anything to do. The plan is held until the UI
    /// either starts it or abandons it.
    pub(crate) fn prepare_operation(
        &mut self,
        pane: PaneId,
        kind: crate::operations::OperationKind,
    ) -> bool {
        self.plan_error = None;
        self.pending_plan = None;

        let sources = self.operation_sources(pane);
        if sources.is_empty() {
            // A refusal explains itself; silently doing nothing is worse than
            // an error (docs/UI_CONVENTIONS.md 9).
            //
            // Remote entries produce no sources because a remote location has
            // no local path - which is what stops a copy from grabbing the
            // wrong local file of the same name. But "nothing is selected" is
            // then a lie: something is selected, and it is somewhere this
            // program cannot yet copy from.
            self.plan_error = Some(if self.pane_is_remote(pane) {
                PlanError::Failed(jtf_core::Error::new(
                    jtf_core::ErrorCode::Unsupported,
                    "remote.write_not_built",
                ))
            } else {
                PlanError::NothingToDo
            });
            return false;
        }

        let destination = if kind.needs_destination() {
            let target = self.workspace.target_pane_id();
            match target.and_then(|id| {
                self.workspace
                    .pane(id)
                    .and_then(jtf_workspace::Pane::active_tab)
                    .and_then(|tab| tab.location().as_path())
                    .map(std::path::Path::to_path_buf)
            }) {
                Some(path) => Some(path),
                // With one pane there is no other pane to copy to
                // (docs/UI_TEST_PLAN.md PANE-016).
                None => return false,
            }
        } else {
            None
        };

        self.build_plan(kind, sources, destination)
    }

    /// Prepare a copy or a move into a destination the user named, rather than
    /// into whichever pane happens to be next.
    ///
    /// The two-pane keys are the fast path; this is the one that works when
    /// there is one pane, or when the place you want is a tab that is open
    /// somewhere else, or a folder that is not open at all.
    pub(crate) fn prepare_operation_to(
        &mut self,
        pane: PaneId,
        kind: crate::operations::OperationKind,
        destination: &str,
    ) -> bool {
        self.plan_error = None;
        self.pending_plan = None;

        let sources = self.operation_sources(pane);
        if sources.is_empty() {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }
        if !kind.needs_destination() {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }
        let target = std::path::PathBuf::from(destination);
        if !target.is_dir() {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }
        self.build_plan(kind, sources, Some(target))
    }

    fn build_plan(
        &mut self,
        kind: crate::operations::OperationKind,
        sources: Vec<std::path::PathBuf>,
        destination: Option<std::path::PathBuf>,
    ) -> bool {
        let operation = kind.build(sources, destination);
        match Plan::build(&operation, &CancellationToken::never()) {
            Ok(plan) => {
                self.pending_plan = Some(plan);
                true
            }
            Err(error) => {
                self.plan_error = Some(error);
                false
            }
        }
    }

    /// The location of one tab, so the UI can offer every open tab as a
    /// destination rather than only the pane beside this one.
    pub(crate) fn tab_path(&self, pane: PaneId, index: usize) -> String {
        self.workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .and_then(|tab| tab.location().as_path())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    // ----------------------------------------------------------- folder tree

    /// Child directories of `path`, newline-separated and sorted.
    ///
    /// The tree asks Rust the same way the list does, so the two cannot
    /// disagree about what a directory contains, whether a symlink is a
    /// folder, or whether hidden entries are shown. Two sources of truth about
    /// one filesystem is exactly the drift `AGENTS.md` §4 exists to prevent.
    /// The path arrives as the string the tree stored, which for a server is
    /// the `sftp://` form `display_text` writes, so it is parsed rather than
    /// assumed local: the tree follows whichever pane has focus, and a pane
    /// can be on a server.
    pub(crate) fn child_directories(&self, path: &str) -> String {
        let location = Location::parse_display(path);
        // The tree asks on the interface thread, so it may only ever ask a
        // question that is already answered locally. Listing a server we have
        // not signed in to would sign in from here, and a sign-in takes as
        // long as the network says it does - that is the freeze the pane was
        // moved to a worker thread to avoid. An unconnected server expands to
        // nothing, and expands properly once the pane has connected it.
        if let Location::Remote {
            host, port, user, ..
        } = &location
        {
            let endpoint = jtf_fs::sftp::Endpoint {
                host: host.clone(),
                port: *port,
                user: user.clone(),
            };
            if !self.sftp.is_connected(&endpoint) {
                return String::new();
            }
        }
        let Ok(entries) = self
            .provider_for(&location)
            .list(&location, &CancellationToken::never())
        else {
            return String::new();
        };

        let mut names: Vec<(String, String)> = entries
            .iter()
            .filter(|entry| entry.kind().is_directory_on_disk())
            .filter(|entry| self.show_hidden || !entry.attributes().hidden)
            .map(|entry| {
                (
                    entry.display_name().to_lowercase(),
                    entry.location().display_text(),
                )
            })
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|(_, path)| path)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Whether folders sort ahead of files.
    pub(crate) const fn folders_first(&self) -> bool {
        self.settings.folders_first
    }

    /// Set it, and re-sort every pane so the change is visible immediately
    /// rather than at the next navigation.
    pub(crate) fn set_folders_first(&mut self, folders_first: bool) {
        if self.settings.folders_first == folders_first {
            return;
        }
        self.settings.folders_first = folders_first;
        let panes: Vec<PaneId> = self.views.keys().copied().collect();
        let show_hidden = self.show_hidden;
        for pane in panes {
            let needle = self.filter_text(pane).to_lowercase();
            if let Some(view) = self.views.get_mut(&pane) {
                let sort = view.sort;
                sort_entries_with(&mut view.entries, sort, folders_first);
                Self::recompute_visible(view, &needle, show_hidden);
                view.generation += 1;
            }
        }
    }

    /// Whether the sidebar is shown, and how wide.
    pub(crate) const fn tree_state(&self) -> (bool, u16) {
        (self.settings.tree_visible, self.settings.tree_width)
    }

    /// Remember the sidebar's state.
    pub(crate) fn set_tree_state(&mut self, visible: bool, width: u16) {
        self.settings.tree_visible = visible;
        self.settings.tree_width = width;
    }

    /// Sidebar sections the user has folded away, newline-separated.
    pub(crate) fn collapsed_sections(&self) -> String {
        self.settings.collapsed_sections.join("\n")
    }

    /// Remember which sections are folded, so they are still folded next time.
    pub(crate) fn set_collapsed_sections(&mut self, ids: &str) {
        self.settings.collapsed_sections = ids
            .split('\n')
            .filter(|id| !id.is_empty())
            .map(ToString::to_string)
            .collect();
    }

    /// How many recent folders the sidebar should show.
    pub(crate) fn recent_limit(&self) -> u16 {
        self.settings.recent_limit
    }

    pub(crate) fn set_recent_limit(&mut self, limit: u16) {
        self.settings.recent_limit = limit;
    }

    /// The user's bookmarks.
    pub(crate) fn bookmarks(&self) -> &[Bookmark] {
        self.places.bookmarks()
    }

    /// Whether the pane's folder is bookmarked.
    pub(crate) fn is_bookmarked(&self, pane: PaneId) -> bool {
        let path = self.current_path(pane);
        !path.is_empty() && self.places.is_bookmarked(Path::new(&path))
    }

    /// Bookmark the pane's folder, or remove it. Returns the state afterwards.
    pub(crate) fn toggle_bookmark(&mut self, pane: PaneId) -> bool {
        let path = self.current_path(pane);
        if path.is_empty() {
            return false;
        }
        self.places.toggle_bookmark(path)
    }

    /// Bookmark `path`, or remove it if it is already bookmarked.
    ///
    /// By path, not by pane: the tab strip, the folder tree and a folder row
    /// in the list all offer this, and each of them is pointing at a folder
    /// that may not be the one the pane is currently showing.
    pub(crate) fn toggle_bookmark_path(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        self.places.toggle_bookmark(path)
    }

    /// Whether `path` is bookmarked.
    pub(crate) fn path_is_bookmarked(&self, path: &str) -> bool {
        let path = std::path::Path::new(path);
        self.places
            .bookmarks()
            .iter()
            .any(|bookmark| bookmark.path == path)
    }

    /// Remove the bookmark at `index`.
    pub(crate) fn remove_bookmark(&mut self, index: usize) {
        self.places.remove_bookmark(index);
    }

    /// Rename the bookmark at `index`; an empty name restores the default.
    pub(crate) fn rename_bookmark(&mut self, index: usize, name: &str) {
        self.places.rename_bookmark(index, name);
    }

    /// Reorder a bookmark, for drag reordering in the sidebar.
    pub(crate) fn move_bookmark(&mut self, from: usize, to: usize) {
        self.places.move_bookmark(from, to);
    }

    /// The recent locations, most recent first.
    ///
    /// Trimmed here rather than when they are recorded: the setting decides
    /// how many to *show*, and lowering it should not throw away history that
    /// raising it again would want back.
    pub(crate) fn recent(&self) -> Vec<String> {
        let limit = if self.settings.recent_limit == 0 {
            DEFAULT_RECENT_SHOWN
        } else {
            usize::from(self.settings.recent_limit)
        };
        self.places
            .recent()
            .take(limit)
            .map(|path| path.display().to_string())
            .collect()
    }

    /// Forget where the user has been.
    pub(crate) fn clear_recent(&mut self) {
        self.places.clear_recent();
    }

    /// The command a chord runs, if any. Empty when nothing is bound.
    pub(crate) fn command_for_chord(&self, chord: &str) -> String {
        KeyChord::parse(chord).ok().map_or_else(String::new, |c| {
            self.keymap
                .command_for(&c)
                .map_or_else(String::new, |id| id.as_str().to_string())
        })
    }

    /// Switch to the other keyboard mode, and return its name.
    ///
    /// Two presets, one key. The chord that runs this is bound to the same
    /// command in both of them, which is the whole reason the switch is
    /// usable: a toggle that exists in only one mode is a door that locks
    /// behind you.
    pub(crate) fn toggle_keymap(&mut self) -> String {
        let current = self.keymap.name().to_string();
        let next = if current == KEYMAP_PRESETS[0] {
            KEYMAP_PRESETS[1]
        } else {
            KEYMAP_PRESETS[0]
        };
        self.set_keymap(next);
        next.to_string()
    }

    /// Whether a bare printable key jumps to a file name in this keymap.
    pub(crate) const fn type_ahead(&self) -> bool {
        self.keymap.type_ahead()
    }

    /// Whether the key hint strip is shown.
    pub(crate) const fn key_hints_visible(&self) -> bool {
        self.settings.key_hints_visible
    }

    /// Remember the key hint strip's state.
    pub(crate) fn set_key_hints_visible(&mut self, visible: bool) {
        self.settings.key_hints_visible = visible;
    }

    pub(crate) const fn parent_row(&self) -> bool {
        self.settings.parent_row
    }

    pub(crate) fn set_parent_row(&mut self, shown: bool) {
        self.settings.parent_row = shown;
    }

    pub(crate) const fn inspector_position(&self) -> u8 {
        self.settings.inspector_position
    }

    /// Remember which side the preview panel is on. Anything past the last
    /// position falls back to beside the panes.
    pub(crate) fn set_inspector_position(&mut self, position: u8) {
        self.settings.inspector_position = if position > 1 { 0 } else { position };
    }

    pub(crate) const fn preview_background(&self) -> u8 {
        self.settings.preview_background
    }

    pub(crate) fn preview_background_colour(&self) -> &str {
        &self.settings.preview_background_colour
    }

    /// Remember what the preview area is drawn on. A mode past the last one
    /// falls back to the theme's own surface rather than to nothing.
    pub(crate) fn set_preview_background(&mut self, mode: u8, colour: &str) {
        self.settings.preview_background = if mode > 2 { 0 } else { mode };
        colour.clone_into(&mut self.settings.preview_background_colour);
    }

    pub(crate) const fn key_hints_density(&self) -> u8 {
        self.settings.key_hints_density
    }

    /// Remember how much the strip should say. Values above the last mode
    /// fall back to full rather than showing nothing.
    pub(crate) fn set_key_hints_density(&mut self, density: u8) {
        self.settings.key_hints_density = if density > 2 { 0 } else { density };
    }

    /// The pane's view mode: 0 list, 1 grid.
    pub(crate) fn view_mode(&self, pane: PaneId) -> i32 {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or(0, |tab| {
                // Exhaustive on purpose: a view mode added later should fail
                // to compile here rather than silently report itself as the
                // list.
                match tab.view_mode() {
                    ViewMode::Grid => 1,
                    ViewMode::List => 0,
                }
            })
    }

    /// Switch the pane between the list and the grid.
    pub(crate) fn set_view_mode(&mut self, pane: PaneId, grid: bool) {
        let mode = if grid { ViewMode::Grid } else { ViewMode::List };
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.set_view_mode(mode);
            }
        }
    }

    /// Whether image files show a thumbnail.
    pub(crate) const fn thumbnails(&self) -> bool {
        self.settings.thumbnails
    }

    /// Turn thumbnails on or off.
    pub(crate) fn set_thumbnails(&mut self, on: bool) {
        self.settings.thumbnails = on;
    }

    /// Whether the inspector is shown, and how wide.
    pub(crate) const fn inspector_state(&self) -> (bool, u16) {
        (
            self.settings.inspector_visible,
            self.settings.inspector_width,
        )
    }

    /// Remember the inspector's state.
    pub(crate) fn set_inspector_state(&mut self, visible: bool, width: u16) {
        self.settings.inspector_visible = visible;
        self.settings.inspector_width = width;
    }

    /// The paths an operation started here would act on, newline-separated.
    ///
    /// Used for the clipboard and for "copy path": the same
    /// marked-then-selection-then-active resolution as everything else, so
    /// copying and deleting never disagree about what is targeted.
    pub(crate) fn target_paths(&self, pane: PaneId) -> String {
        self.operation_sources(pane)
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The names of the targeted entries, newline-separated.
    pub(crate) fn target_names(&self, pane: PaneId) -> String {
        self.operation_sources(pane)
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Build a plan that copies the targeted entries beside themselves.
    ///
    /// Always "keep both": a duplicate that overwrote the original would be a
    /// contradiction in terms.
    pub(crate) fn prepare_duplicate(&mut self, pane: PaneId) -> bool {
        self.plan_error = None;
        self.pending_plan = None;

        let sources = self.operation_sources(pane);
        if sources.is_empty() {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }
        let Some(parent) = sources.first().and_then(|path| path.parent()) else {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        };
        self.set_plan(&jtf_ops::Operation::Copy {
            sources: sources.clone(),
            destination: parent.to_path_buf(),
        })
    }

    // --------------------------------------------------- batch rename

    /// Recompute the batch-rename preview for the pane's targeted entries.
    ///
    /// The preview is the same computation the apply uses, so what the user
    /// sees is what happens (`jtf_ops::batch`).
    pub(crate) fn preview_batch(
        &mut self,
        pane: PaneId,
        template: &str,
        find: &str,
        replace: &str,
        regex: bool,
        start: u64,
    ) -> usize {
        let sources = self.operation_sources(pane);
        let pattern = RenamePattern {
            template: template.to_string(),
            find: find.to_string(),
            replace: replace.to_string(),
            regex,
            start,
        };
        let preview = jtf_ops::preview_batch_rename(&sources, &pattern);
        let rows = preview.rows.len();
        self.batch_preview = Some(preview);
        rows
    }

    /// One row of the current preview: original name, new name, issue key.
    pub(crate) fn batch_row(&self, index: usize) -> Option<(String, String, &'static str)> {
        let preview = self.batch_preview.as_ref()?;
        let row = preview.rows.get(index)?;
        Some((row.from.clone(), row.to.clone(), row.issue.label_key()))
    }

    /// Whether the preview can be applied, and how many rows would change.
    pub(crate) fn batch_state(&self) -> (bool, usize) {
        self.batch_preview.as_ref().map_or((false, 0), |preview| {
            (
                !preview.is_blocked() && preview.has_changes(),
                preview.change_count(),
            )
        })
    }

    /// Apply the preview. Returns how many entries were renamed.
    pub(crate) fn apply_batch(&mut self) -> usize {
        let Some(preview) = self.batch_preview.take() else {
            return 0;
        };
        match jtf_ops::apply_batch_rename(&preview) {
            Ok(done) => {
                let renamed = done.len();
                // Reversible like any other rename, through the same stack.
                if renamed > 0 {
                    let report = jtf_ops::Report {
                        outcomes: done
                            .into_iter()
                            .map(|(from, to)| {
                                (
                                    from,
                                    jtf_ops::Outcome::Done {
                                        destination: Some(to),
                                    },
                                )
                            })
                            .collect(),
                        cancelled: false,
                    };
                    let operation = jtf_ops::Operation::Rename {
                        source: PathBuf::new(),
                        new_name: String::new(),
                    };
                    if let Some(record) = UndoRecord::from_report(&operation, &report) {
                        self.undo_stack.push(record);
                    }
                }
                renamed
            }
            Err(error) => {
                self.plan_error = Some(PlanError::Failed(error));
                0
            }
        }
    }

    /// Discard the preview.
    pub(crate) fn clear_batch(&mut self) {
        self.batch_preview = None;
    }

    /// Build a plan for sources that came from somewhere else — a drop from
    /// another pane, or from Finder.
    ///
    /// The destination is the pane being dropped **on**, which is the only
    /// reading of a drop that matches what the user pointed at.
    pub(crate) fn prepare_drop(
        &mut self,
        pane: PaneId,
        kind: crate::operations::OperationKind,
        sources: Vec<PathBuf>,
    ) -> bool {
        self.plan_error = None;
        self.pending_plan = None;
        if sources.is_empty() {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }

        let destination = if kind.needs_destination() {
            match self
                .workspace
                .pane(pane)
                .and_then(jtf_workspace::Pane::active_tab)
                .and_then(|tab| tab.location().as_path())
                .map(std::path::Path::to_path_buf)
            {
                Some(path) => Some(path),
                None => return false,
            }
        } else {
            None
        };

        // Dropping a thing onto the folder it already lives in is a no-op the
        // user almost certainly did by accident, not a request to duplicate.
        if let Some(destination) = &destination {
            if sources
                .iter()
                .all(|source| source.parent() == Some(destination.as_path()))
            {
                return false;
            }
        }

        self.set_plan(&kind.build(sources, destination))
    }

    /// Build a plan over paths the caller names, rather than over a pane's
    /// selection.
    ///
    /// The disc usage window works this way: it is a report about a folder
    /// rather than a pane showing one, so the thing to act on is the row the
    /// cursor is on and there is no selection behind the boundary to read.
    pub(crate) fn prepare_paths(
        &mut self,
        kind: crate::operations::OperationKind,
        sources: Vec<PathBuf>,
        destination: Option<PathBuf>,
    ) -> bool {
        self.plan_error = None;
        self.pending_plan = None;
        if sources.is_empty() || (kind.needs_destination() && destination.is_none()) {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }
        self.set_plan(&kind.build(sources, destination))
    }

    /// Build a rename plan.
    pub(crate) fn prepare_rename(&mut self, pane: PaneId, new_name: &str) -> bool {
        self.plan_error = None;
        self.pending_plan = None;
        // The cursor's row, not the marked set: see `cursor_source`.
        let Some(source) = self.cursor_source(pane) else {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        };
        self.set_plan(&jtf_ops::Operation::Rename {
            source,
            new_name: new_name.to_string(),
        })
    }

    /// Build a new-folder plan.
    /// Prepare setting or clearing read-only on the pane's targets.
    pub(crate) fn prepare_set_read_only(&mut self, pane: PaneId, read_only: bool) -> bool {
        let sources = self.operation_sources(pane);
        if sources.is_empty() {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        }
        self.set_plan(&jtf_ops::Operation::SetReadOnly { sources, read_only })
    }

    /// Whether every target is already read-only, so the dialog opens showing
    /// what is true rather than a guess.
    pub(crate) fn targets_are_read_only(&self, pane: PaneId) -> bool {
        let sources = self.operation_sources(pane);
        !sources.is_empty()
            && sources
                .iter()
                .all(|path| std::fs::metadata(path).is_ok_and(|meta| meta.permissions().readonly()))
    }

    pub(crate) fn prepare_new_file(&mut self, pane: PaneId, name: &str) -> bool {
        let Some(parent) = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|tab| tab.location().as_path())
            .map(std::path::Path::to_path_buf)
        else {
            self.plan_error = Some(PlanError::NothingToDo);
            return false;
        };
        self.set_plan(&jtf_ops::Operation::NewFile {
            parent,
            name: name.to_string(),
        })
    }

    pub(crate) fn prepare_new_folder(&mut self, pane: PaneId, name: &str) -> bool {
        self.plan_error = None;
        self.pending_plan = None;
        let Some(parent) = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|tab| tab.location().as_path())
            .map(std::path::Path::to_path_buf)
        else {
            return false;
        };
        self.set_plan(&jtf_ops::Operation::NewFolder {
            parent,
            name: name.to_string(),
        })
    }

    /// Start building a plan on a worker thread.
    ///
    /// Returns immediately. The caller polls [`Self::poll_planning`] and shows
    /// something while it waits: counting a large folder takes as long as
    /// reading it, and that used to happen on the UI thread.
    fn set_plan(&mut self, operation: &jtf_ops::Operation) -> bool {
        self.planning = Some(crate::operations::Running::start_planning(
            operation.clone(),
        ));
        true
    }

    /// Whether a plan is still being built.
    pub(crate) const fn is_planning(&self) -> bool {
        self.planning.is_some()
    }

    /// Collect a finished plan. Returns 1 when ready, 0 on failure, -1 while
    /// still counting.
    pub(crate) fn poll_planning(&mut self) -> i32 {
        let Some(planning) = self.planning.as_mut() else {
            return i32::from(self.pending_plan.is_some());
        };
        match planning.take() {
            None => -1,
            Some(Ok(plan)) => {
                self.planning = None;
                self.pending_plan = Some(plan);
                1
            }
            Some(Err(error)) => {
                self.planning = None;
                self.plan_error = Some(error);
                0
            }
        }
    }

    /// Stop counting and discard the half-built plan.
    pub(crate) fn cancel_planning(&mut self) {
        if let Some(planning) = self.planning.take() {
            planning.cancel();
        }
        self.pending_plan = None;
    }

    /// The plan waiting for confirmation.
    pub(crate) const fn pending_plan(&self) -> Option<&Plan> {
        self.pending_plan.as_ref()
    }

    /// Why the last prepare failed, as a localization key.
    pub(crate) fn plan_error_key(&self) -> Option<&'static str> {
        self.plan_error.as_ref().map(|error| match error {
            PlanError::DestinationInsideSource(_) => "plan.destination_inside_source",
            PlanError::SourceIsDestination(_) => "plan.source_is_destination",
            PlanError::DestinationNotADirectory(_) => "plan.destination_not_a_directory",
            PlanError::InvalidName(_) => "plan.invalid_name",
            PlanError::NothingToDo => "plan.nothing_to_do",
            // PlanError is non_exhaustive: a variant without its own message
            // reports generically rather than failing to compile here.
            PlanError::Failed(_) | _ => "plan.failed",
        })
    }

    /// Run the pending plan.
    pub(crate) fn start_operation(&mut self, policy: ConflictPolicy) -> bool {
        let Some(plan) = self.pending_plan.take() else {
            return false;
        };
        if self.running.is_some() {
            // Queued rather than refused. Being told "no, something else is
            // copying" is the answer a file manager should never have to
            // give: the second copy is work the user has already decided on.
            if self.queue.len() >= MAX_QUEUED {
                self.plan_error = Some(PlanError::NothingToDo);
                return false;
            }
            self.queue.push_back((plan, policy));
            return true;
        }
        self.last_summary = None;
        self.running = Some(crate::operations::Running::start(plan, policy));
        true
    }

    /// Every job the user has asked for: the running one first, then the ones
    /// waiting, in the order they will run.
    ///
    /// The status bar could only ever say *how many* there were. That is
    /// enough to know something is happening and not enough to know what -
    /// which of two copies is running, how big the one behind it is, or
    /// whether the thing still queued is the one that should be dropped.
    pub(crate) fn job_count(&self) -> usize {
        usize::from(self.running.is_some()) + self.queue.len()
    }

    /// The localization key for job `index`'s label.
    pub(crate) fn job_label_key(&self, index: usize) -> &'static str {
        if self.running.is_some() {
            if index == 0 {
                return self.operation_label_key().unwrap_or("");
            }
            return self
                .queue
                .get(index - 1)
                .map_or("", |(plan, _)| plan.operation.job_kind().label_key());
        }
        self.queue
            .get(index)
            .map_or("", |(plan, _)| plan.operation.job_kind().label_key())
    }

    /// How many entries job `index` will touch, and how many bytes.
    pub(crate) fn job_size(&self, index: usize) -> (u64, u64) {
        let queued = if self.running.is_some() {
            index.checked_sub(1).and_then(|i| self.queue.get(i))
        } else {
            self.queue.get(index)
        };
        queued.map_or((0, 0), |(plan, _)| (plan.total_entries, plan.total_bytes))
    }

    /// Whether job `index` is the one currently running.
    pub(crate) fn job_is_running(&self, index: usize) -> bool {
        self.running.is_some() && index == 0
    }

    /// Drop job `index`. The running one is cancelled; a waiting one is simply
    /// removed, which is not the same decision and does not touch any file.
    pub(crate) fn cancel_job(&mut self, index: usize) {
        if self.job_is_running(index) {
            self.cancel_operation();
            return;
        }
        let at = if self.running.is_some() {
            index.checked_sub(1)
        } else {
            Some(index)
        };
        if let Some(i) = at {
            self.queue.remove(i);
        }
    }

    /// How many operations are waiting behind the running one.
    pub(crate) fn queued_count(&self) -> usize {
        self.queue.len()
    }

    /// Drop everything waiting. The running one is not touched: stopping it
    /// is cancellation, which is a different decision.
    pub(crate) fn clear_queue(&mut self) {
        self.queue.clear();
    }

    /// Whether there is anything to undo.
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() && self.running.is_none()
    }

    /// Localization key naming what undo would reverse.
    pub(crate) fn undo_label_key(&self) -> &'static str {
        self.undo_stack.last().map_or("", UndoRecord::label_key)
    }

    /// Undo the most recent reversible operation.
    pub(crate) fn undo_last(&mut self) -> bool {
        if self.running.is_some() {
            return false;
        }
        let Some(record) = self.undo_stack.pop() else {
            return false;
        };
        self.last_summary = None;
        self.running = Some(crate::operations::Running::start_undo(record));
        true
    }

    /// Whether an operation is in flight.
    pub(crate) const fn operation_running(&self) -> bool {
        self.running.is_some()
    }

    /// Percent complete, or `None` when the total is not yet known.
    pub(crate) fn operation_percent(&self) -> Option<u8> {
        self.running
            .as_ref()
            .and_then(crate::operations::Running::percent)
    }

    /// Localization key for the running operation's label.
    pub(crate) fn operation_label_key(&self) -> Option<&'static str> {
        self.running
            .as_ref()
            .map(|running| running.kind().label_key())
    }

    /// The entry being worked on.
    pub(crate) fn operation_current(&self) -> Option<PathBuf> {
        self.running
            .as_ref()
            .and_then(crate::operations::Running::current)
    }

    /// Ask the running operation to stop.
    pub(crate) fn cancel_operation(&self) {
        if let Some(running) = &self.running {
            running.cancel();
        }
    }

    /// The summary of the last finished operation, if it has not been read.
    pub(crate) const fn last_summary(&self) -> Option<&crate::operations::Summary> {
        self.last_summary.as_ref()
    }

    /// Clear the summary once the UI has shown it.
    pub(crate) fn take_summary(&mut self) {
        self.last_summary = None;
    }

    // --------------------------------------------------------------- viewer

    /// Open the focused row in the viewer.
    ///
    /// The kind comes from the file's own bytes, never its name
    /// (`docs/VIEWER_PREVIEW.md` §1), and anything that is not textual opens
    /// as hex — which is always available and never wrong.
    pub(crate) fn open_viewer(&mut self, pane: PaneId, row: usize) -> bool {
        let Some(path) = self
            .entry_at(pane, row)
            .and_then(|entry| entry.location().as_path())
            .map(std::path::Path::to_path_buf)
        else {
            return false;
        };
        if path.is_dir() {
            return false;
        }

        let kind = detect(&path).unwrap_or(ContentKind::Binary);
        let mut session = ViewerSession {
            path,
            kind,
            text: None,
            hex: None,
            forced_hex: false,
        };
        Self::load_viewer(&mut session);
        let opened = session.text.is_some() || session.hex.is_some();
        self.viewer = Some(session);
        opened
    }

    fn load_viewer(session: &mut ViewerSession) {
        session.text = None;
        session.hex = None;
        if session.kind.is_textual() && !session.forced_hex {
            session.text = TextView::open(&session.path, &CancellationToken::never()).ok();
        }
        if session.text.is_none() {
            session.hex = HexView::open(&session.path).ok();
        }
    }

    /// Close the viewer, releasing its file handle.
    pub(crate) fn close_viewer(&mut self) {
        self.viewer = None;
    }

    /// Open `path` for the inspector's preview. Returns whether it is text.
    ///
    /// Reopening the same path is a no-op, because the inspector is refreshed
    /// on a frame boundary and re-indexing a log file sixty times a second is
    /// not a preview, it is a spin loop.
    pub(crate) fn preview_open(&mut self, path: &str) -> bool {
        let path = PathBuf::from(path);
        if self
            .preview
            .as_ref()
            .is_some_and(|session| session.path == path)
        {
            return self.preview_is_text();
        }
        if path.is_dir() {
            self.preview = None;
            return false;
        }
        let kind = detect(&path).unwrap_or(ContentKind::Binary);
        let mut session = ViewerSession {
            path,
            kind,
            text: None,
            hex: None,
            forced_hex: false,
        };
        // Text only. A preview pane that renders a hex dump of a JPEG is
        // noise where the icon it replaced was an answer.
        if session.kind.is_textual() {
            // A fraction of a large file, not the whole thing. The preview is
            // a glance at what a file is; indexing four gigabytes of log to
            // answer that would read every byte of it and hold eight bytes per
            // line, before one line appeared. The viewer window opens more.
            session.text = TextView::open_bounded(
                &session.path,
                jtf_viewer::PREVIEW_INDEXED_BYTES,
                &CancellationToken::never(),
            )
            .ok();
        }
        let is_text = session.text.is_some();
        self.preview = Some(session);
        is_text
    }

    /// A location, adjusted so it names something you can be *inside*.
    ///
    /// Leaving an archive's root gives the archive file itself, which is
    /// correct as a location and useless as a destination: navigating there
    /// puts you straight back inside it. The folder holding it is what
    /// "up" means from there.
    fn containing_folder(location: Location) -> Location {
        match location.as_path() {
            Some(path) if path.is_file() => path
                .parent()
                .map_or_else(|| location.clone(), Location::local),
            _ => location,
        }
    }

    /// The archive at `path` as display lines, or empty when it is not one.
    ///
    /// Formatted here rather than in the UI so the entry name, which comes
    /// from an untrusted file, is never handed across as something the UI
    /// might treat as markup or a path.
    #[allow(clippy::unused_self)] // reads only the file, but belongs with the other preview calls
    /// Read an archive's listing and hold it for the window to page through.
    ///
    /// Returns how many entries there are. Held rather than re-read per row:
    /// the central directory is one read, and asking for it once per visible
    /// row would re-parse the archive on every scroll.
    pub(crate) fn open_archive_listing(&mut self, path: &str) -> usize {
        self.archive_entries = jtf_fs::list_container(Path::new(path)).unwrap_or_default();
        self.archive_entries.len()
    }

    /// One entry: its stored name, size, whether it is a directory, and
    /// whether extracting it would escape the destination.
    pub(crate) fn archive_entry(&self, index: usize) -> Option<(&str, u64, bool, bool)> {
        self.archive_entries.get(index).map(|entry| {
            (
                entry.name.as_str(),
                entry.size,
                entry.is_directory,
                entry.unsafe_name,
            )
        })
    }

    /// Forget the listing when the window closes.
    pub(crate) fn close_archive_listing(&mut self) {
        self.archive_entries = Vec::new();
    }

    /// A whole archive listing rendered as one block of text, for the preview
    /// panel. Takes no state - the archive is read fresh each time, which is
    /// what a preview of an arbitrary file has to do.
    pub(crate) fn archive_listing(path: &str) -> String {
        use std::fmt::Write as _;

        // Bounded: the parser already caps entries, and this caps what is
        // rendered into one string for a preview pane.
        const MAX_SHOWN: usize = 2000;

        let Ok(entries) = jtf_fs::list_container(Path::new(path)) else {
            return String::new();
        };
        let mut out = String::new();
        for entry in entries.iter().take(MAX_SHOWN) {
            let size = if entry.is_directory {
                String::new()
            } else {
                format_size(entry.size)
            };
            // A name that would escape on extraction is marked, not hidden:
            // seeing it is the point.
            let flag = if entry.unsafe_name { "!" } else { " " };
            let _ = writeln!(out, "{flag} {size:>10}  {}", entry.name);
        }
        if entries.len() > MAX_SHOWN {
            let _ = writeln!(out, "… {} more", entries.len() - MAX_SHOWN);
        }
        out
    }

    /// Release the preview's file handle.
    pub(crate) fn preview_close(&mut self) {
        self.preview = None;
    }

    /// Whether the preview is showing text.
    pub(crate) fn preview_is_text(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|session| session.text.is_some())
    }

    /// How many lines the previewed file has.
    pub(crate) fn preview_line_count(&self) -> u64 {
        self.preview
            .as_ref()
            .and_then(|s| s.text.as_ref())
            .map_or(0, TextView::line_count)
    }

    /// One line of the preview, decoded.
    pub(crate) fn preview_row(&mut self, row: u64) -> String {
        let Some(text) = self.preview.as_mut().and_then(|s| s.text.as_mut()) else {
            return String::new();
        };
        text.window(row, 1)
            .ok()
            .and_then(|window| window.lines.first().cloned())
            .unwrap_or_default()
    }

    /// The preview's encoding, line ending and size, as label keys and bytes.
    pub(crate) fn preview_status(&self) -> (&'static str, &'static str, u64) {
        self.preview.as_ref().and_then(|s| s.text.as_ref()).map_or(
            ("encoding.utf_8", "line_ending.lf", 0),
            |text| {
                (
                    text.effective_encoding().label_key(),
                    text.line_ending().label_key(),
                    text.size(),
                )
            },
        )
    }

    /// Whether the viewer is showing text rather than hex.
    pub(crate) fn viewer_is_text(&self) -> bool {
        self.viewer
            .as_ref()
            .is_some_and(|session| session.text.is_some())
    }

    /// Switch between text and hex, where the file allows it.
    pub(crate) fn viewer_toggle_hex(&mut self) {
        let Some(mut session) = self.viewer.take() else {
            return;
        };
        session.forced_hex = !session.forced_hex;
        Self::load_viewer(&mut session);
        self.viewer = Some(session);
    }

    /// Rows the viewer can scroll through: lines for text, 16-byte rows for
    /// hex.
    pub(crate) fn viewer_row_count(&self) -> u64 {
        self.viewer.as_ref().map_or(0, |session| {
            session.text.as_ref().map_or_else(
                || session.hex.as_ref().map_or(0, HexView::row_count),
                TextView::line_count,
            )
        })
    }

    /// A window of rendered rows.
    pub(crate) fn viewer_rows(&mut self, first: u64, count: usize) -> Vec<String> {
        let Some(session) = self.viewer.as_mut() else {
            return Vec::new();
        };
        if let Some(text) = session.text.as_mut() {
            return text
                .window(first, count)
                .map(|window| window.lines)
                .unwrap_or_default();
        }
        if let Some(hex) = session.hex.as_mut() {
            return hex
                .window(first, count)
                .map(|window| window.rows())
                .unwrap_or_default();
        }
        Vec::new()
    }

    /// Set the text encoding. Ignored while showing hex.
    pub(crate) fn viewer_set_encoding(&mut self, index: usize) {
        let Some(session) = self.viewer.as_mut() else {
            return;
        };
        let Some(text) = session.text.as_mut() else {
            return;
        };
        if let Some(encoding) = Encoding::ALL.get(index) {
            text.set_encoding(*encoding);
        }
    }

    /// The encoding in use, as an index into `Encoding::ALL`.
    pub(crate) fn viewer_encoding(&self) -> usize {
        self.viewer
            .as_ref()
            .and_then(|session| session.text.as_ref())
            .and_then(|text| {
                Encoding::ALL
                    .iter()
                    .position(|e| *e == text.effective_encoding())
            })
            .unwrap_or(0)
    }

    /// A one-line description: path, kind, size, encoding, line endings.
    ///
    /// Assembled from localization keys by the UI, which is handed the parts
    /// rather than a sentence (`AGENTS.md` §11).
    pub(crate) fn viewer_status(&self) -> (String, &'static str, u64, &'static str, &'static str) {
        let Some(session) = self.viewer.as_ref() else {
            return (
                String::new(),
                "content.empty",
                0,
                "encoding.auto",
                "line_ending.none",
            );
        };
        let size = session.text.as_ref().map_or_else(
            || session.hex.as_ref().map_or(0, HexView::size),
            TextView::size,
        );
        let encoding = session.text.as_ref().map_or("encoding.auto", |text| {
            text.effective_encoding().label_key()
        });
        let endings = session
            .text
            .as_ref()
            .map_or("line_ending.none", |text| text.line_ending().label_key());
        (
            session.path.display().to_string(),
            session.kind.label_key(),
            size,
            encoding,
            endings,
        )
    }

    /// Find `needle` from `from_row`, wrapping. Returns the row, if any.
    ///
    /// Searches the rendered rows a window at a time, so a 10 GB file costs a
    /// scan rather than a load.
    pub(crate) fn viewer_find(&mut self, needle: &str, from_row: u64) -> Option<u64> {
        /// Rows fetched per scan step. Large enough that a scan is not a
        /// million round trips, small enough that it is never a load.
        const CHUNK: usize = 512;

        if needle.is_empty() {
            return None;
        }
        let total = self.viewer_row_count();
        if total == 0 {
            return None;
        }
        let needle = needle.to_lowercase();

        let mut scanned = 0u64;
        let mut cursor = from_row;
        while scanned < total {
            let rows = self.viewer_rows(cursor, CHUNK);
            if rows.is_empty() {
                cursor = 0;
                continue;
            }
            for (offset, row) in rows.iter().enumerate() {
                if row.to_lowercase().contains(&needle) {
                    return Some(cursor + offset as u64);
                }
            }
            scanned += rows.len() as u64;
            cursor += rows.len() as u64;
            if cursor >= total {
                cursor = 0; // wrap, so a search from the middle finds the top
            }
        }
        None
    }

    // --------------------------------------------------------------- search

    /// Start a search under the pane's current location.
    ///
    /// Results replace the listing and behave like any other rows: they can be
    /// selected, marked, opened and operated on (`docs/SEARCH_AI.md` §1). The
    /// pane stays on its location, so clearing the search returns to it
    /// without navigating.
    ///
    /// Returns the localization key of a parse error, or an empty string on
    /// success.
    pub(crate) fn start_search(&mut self, pane: PaneId, query: &str) -> &'static str {
        let parsed = match jtf_search::parse(query) {
            Ok(parsed) => parsed,
            Err(error) => return error.message_key(),
        };
        let Some(location) = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|tab| tab.location().clone())
        else {
            return "query.nothing_to_search";
        };

        let view = self.views.entry(pane).or_insert_with(PaneView::new);
        // Dropping the previous handle cancels it: a new search must not race
        // an old one to fill the same pane.
        view.handle = None;
        view.search = None;
        // Listing a directory means the pane is no longer showing results.
        // Leaving the query set made the status line keep claiming "N results"
        // for a directory it had navigated to since.
        view.query.clear();
        view.entries.clear();
        view.visible.clear();
        view.error = None;
        view.loading = true;
        view.generation += 1;
        view.query = query.to_string();

        match jtf_search::search(&location, parsed) {
            Ok(handle) => {
                view.search = Some(handle);
                ""
            }
            Err(error) => {
                view.error = Some(error);
                view.loading = false;
                "query.failed"
            }
        }
    }

    /// The folder a running search is in, or empty when none is running.
    pub(crate) fn search_in(&self, pane: PaneId) -> &str {
        self.views
            .get(&pane)
            .filter(|view| view.search.is_some())
            .map_or("", |view| view.search_in.as_str())
    }

    /// Whether the pane is showing search results.
    pub(crate) fn is_searching(&self, pane: PaneId) -> bool {
        self.views
            .get(&pane)
            .is_some_and(|view| !view.query.is_empty())
    }

    /// The query the pane's results came from.
    pub(crate) fn search_query(&self, pane: PaneId) -> String {
        self.views
            .get(&pane)
            .map_or_else(String::new, |view| view.query.clone())
    }

    /// Abandon the results and go back to showing the directory.
    pub(crate) fn clear_search(&mut self, pane: PaneId) {
        if let Some(view) = self.views.get_mut(&pane) {
            view.search = None;
            view.query.clear();
        }
        self.start_enumeration(pane);
    }

    /// Mark or unmark every listed entry whose name matches a wildcard.
    ///
    /// The CView and WinCV `+` and `-` keys. The pattern applies to what the
    /// pane is showing, filter included, for the same reason "mark all" does:
    /// marking entries the user cannot see would be a surprise.
    ///
    /// Returns how many entries changed.
    pub(crate) fn mark_pattern(&mut self, pane: PaneId, pattern: &str, mark: bool) -> usize {
        let Ok(query) = jtf_search::parse(&format!("glob:{pattern}")) else {
            return 0;
        };
        let now = std::time::SystemTime::now();

        let matched: Vec<Location> = (0..self.row_count(pane))
            .filter_map(|row| self.entry_at(pane, row))
            .filter(|entry| query.matches(entry, now))
            .map(|entry| entry.location().clone())
            .collect();
        let count = matched.len();

        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                if mark {
                    tab.marks_mut().mark_all(matched);
                } else {
                    tab.marks_mut().unmark_all(matched);
                }
            }
        }
        count
    }

    /// Total size of what an operation started here would act on.
    ///
    /// Files only: a directory's size needs a recursive scan, which is a job
    /// rather than a number the status bar can produce while painting.
    pub(crate) fn target_size(&self, pane: PaneId) -> u64 {
        let targets: std::collections::HashSet<Location> = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|tab| tab.operation_target().locations().into_iter().collect())
            .unwrap_or_default();

        (0..self.row_count(pane))
            .filter_map(|row| self.entry_at(pane, row))
            .filter(|entry| targets.contains(entry.location()))
            .filter_map(jtf_core::FileEntry::size)
            .sum()
    }

    /// Re-read the current location.
    pub(crate) fn refresh(&mut self, pane: PaneId) {
        self.start_enumeration(pane);
    }

    /// Try the pane's location again, from a clean connection.
    ///
    /// A plain refresh reuses whatever connection the provider is holding, and
    /// after a failure that is exactly the thing that did not work - a session
    /// the far end has closed, or half of one. Dropping it first means the
    /// retry is a real retry, not a second look at the same broken socket.
    pub(crate) fn reconnect(&mut self, pane: PaneId) {
        let location = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|tab| tab.location().clone());
        if let Some(Location::Remote {
            host, port, user, ..
        }) = location
        {
            self.sftp
                .disconnect(&jtf_fs::sftp::Endpoint { host, port, user });
        }
        self.start_enumeration(pane);
    }

    /// How many of the rows this pane is showing are marked.
    ///
    /// The rows on screen, not the whole stored set. Marks survive navigating
    /// away and back (`docs/UI_TEST_PLAN.md` MARK-004), so the set holds
    /// entries from folders the tab has left - and counting those said「已選取
    /// 6 個」over a folder with nothing ticked in it. A count of things you
    /// cannot see or clear is worse than no count.
    pub(crate) fn marked_count(&self, pane: PaneId) -> usize {
        self.marked_rows(pane).len()
    }

    /// How many marks this tab has turned away for want of room.
    ///
    /// Zero in every ordinary session. Non-zero means a「mark all」over a
    /// folder larger than the set can hold, and the user has to be told:
    /// files that were not marked will not be copied, and「some of them were
    /// silently left out」is not something anyone should have to discover
    /// afterwards.
    pub(crate) fn marks_refused(&self, pane: PaneId) -> usize {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or(0, |tab| tab.marks().refused())
    }

    pub(crate) fn sort_by(&mut self, pane: PaneId, column: i32) {
        let key = match column {
            COLUMN_SIZE => SortKey::Size,
            COLUMN_KIND => SortKey::Kind,
            COLUMN_MODIFIED => SortKey::Modified,
            other => {
                use jtf_workspace::Column;
                match column_at(other) {
                    Some(Column::Created) => SortKey::Created,
                    Some(Column::Accessed) => SortKey::Accessed,
                    Some(Column::Extension) => SortKey::Extension,
                    _ => SortKey::Name,
                }
            }
        };
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.sort_by(key);
            }
        }
        let sort = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(SortSpec::default, jtf_workspace::Tab::sort);
        let needle = self.filter_text(pane).to_lowercase();
        let show_hidden = self.show_hidden;
        let folders_first = self.settings.folders_first;
        if let Some(view) = self.views.get_mut(&pane) {
            view.sort = sort;
            sort_entries_with(&mut view.entries, sort, folders_first);
            Self::recompute_visible(view, &needle, show_hidden);
            view.generation += 1;
        }
    }

    /// Whether the pane's tab has anywhere to go back to.
    ///
    /// A navigation button that is always enabled teaches people that
    /// pressing it does nothing.
    pub(crate) fn can_go_back(&self, pane: PaneId) -> bool {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(jtf_workspace::Tab::can_go_back)
    }

    /// Whether the pane's tab has anywhere to go forward to.
    pub(crate) fn can_go_forward(&self, pane: PaneId) -> bool {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(jtf_workspace::Tab::can_go_forward)
    }

    /// Whether the pane can go up: the root has no parent.
    /// The provider that handles this location.
    fn provider_for(&self, location: &Location) -> &dyn Provider {
        if self.sftp.handles(location) {
            &self.sftp
        } else {
            &self.provider
        }
    }

    /// The saved servers, so the sidebar can list them.
    pub(crate) fn server_count(&self) -> usize {
        self.places.servers().len()
    }

    /// One saved server's label, or empty.
    pub(crate) fn server_name(&self, index: usize) -> String {
        self.places
            .servers()
            .get(index)
            .map(jtf_workspace::Server::display_name)
            .unwrap_or_default()
    }

    /// One saved server's host, port, user and folder.
    pub(crate) fn server_at(&self, index: usize) -> Option<(String, u16, String, String)> {
        self.places.servers().get(index).map(|s| {
            (
                s.host.clone(),
                s.port,
                s.user.clone(),
                if s.path.is_empty() {
                    "/".to_string()
                } else {
                    s.path.clone()
                },
            )
        })
    }

    /// Remember a server, or update the matching one.
    pub(crate) fn add_server(&mut self, host: &str, port: u16, user: &str, path: &str) {
        self.places.add_server(jtf_workspace::Server {
            host: host.to_owned(),
            port,
            user: user.to_owned(),
            path: path.to_owned(),
            name: String::new(),
        });
    }

    /// Whether saved server `index` has a live connection.
    pub(crate) fn server_is_connected(&self, index: usize) -> bool {
        self.places.servers().get(index).is_some_and(|server| {
            self.sftp.is_connected(&jtf_fs::sftp::Endpoint {
                host: server.host.clone(),
                port: server.port,
                user: server.user.clone(),
            })
        })
    }

    /// Close the connection to saved server `index`.
    pub(crate) fn disconnect_server(&self, index: usize) {
        if let Some(server) = self.places.servers().get(index) {
            self.sftp.disconnect(&jtf_fs::sftp::Endpoint {
                host: server.host.clone(),
                port: server.port,
                user: server.user.clone(),
            });
        }
    }

    /// Forget the server at `index`.
    pub(crate) fn remove_server(&mut self, index: usize) {
        self.places.remove_server(index);
    }

    /// Hold a password for the next connection to this endpoint, and no
    /// longer. Never written to the session file.
    pub(crate) fn set_remote_password(&self, host: &str, port: u16, user: &str, password: &str) {
        self.sftp.set_password(
            jtf_fs::sftp::Endpoint {
                host: host.to_owned(),
                port,
                user: user.to_owned(),
            },
            password.to_owned(),
        );
    }

    /// Let the SFTP provider know the user accepted a host's key.
    pub(crate) fn accept_remote_host(&self, host: &str, port: u16, user: &str) {
        self.sftp.accept_host(jtf_fs::sftp::Endpoint {
            host: host.to_owned(),
            port,
            user: user.to_owned(),
        });
    }

    /// Close every remote connection.
    pub(crate) fn disconnect_remote(&self) {
        self.sftp.disconnect_all();
    }

    pub(crate) fn can_go_up(&self, pane: PaneId) -> bool {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|tab| tab.location().parent())
            .is_some()
    }

    /// How many entries are selected in the pane.
    pub(crate) fn selection_count(&self, pane: PaneId) -> usize {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or(0, |tab| tab.selection().len())
    }

    /// The name of the pane's current folder, for the window title.
    pub(crate) fn current_name(&self, pane: PaneId) -> String {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(String::new, |tab| display_name_of(tab.location()))
    }

    /// Whether a column is shown in this pane's active tab.
    pub(crate) fn column_visible(&self, pane: PaneId, column: i32) -> bool {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|tab| tab.columns().get(column_index(column)))
            .is_some_and(|spec| spec.visible)
    }

    /// Show or hide a column. Name is always shown: a list of blank rows is
    /// not a view of anything.
    pub(crate) fn set_column_visible(&mut self, pane: PaneId, column: i32, visible: bool) {
        if column == COLUMN_NAME {
            return;
        }
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                if let Some(spec) = tab.columns_mut().get_mut(column_index(column)) {
                    spec.visible = visible;
                }
            }
        }
    }

    /// Which column the pane is sorted by, as a column index.
    ///
    /// The header needs this to draw its indicator: sorting is done here, not
    /// by the view, so the view has to be told what it is showing.
    pub(crate) fn sort_column(&self, pane: PaneId) -> i32 {
        let sort = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(SortSpec::default, jtf_workspace::Tab::sort);
        match sort.key {
            SortKey::Size => COLUMN_SIZE,
            SortKey::Kind => COLUMN_KIND,
            SortKey::Modified | SortKey::Created => COLUMN_MODIFIED,
            _ => COLUMN_NAME,
        }
    }

    /// Whether the pane's sort is ascending.
    pub(crate) fn sort_ascending(&self, pane: PaneId) -> bool {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_none_or(|tab| tab.sort().ascending)
    }

    pub(crate) const fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub(crate) fn set_show_hidden(&mut self, show: bool) {
        if self.show_hidden != show {
            self.show_hidden = show;
            self.refresh_all_panes();
        }
    }

    // ------------------------------------------------------- background work

    /// Collect whatever the enumerators have produced. Returns whether any
    /// pane changed, so the C++ side repaints only when there is a reason.
    ///
    /// Never blocks: this is called from the Qt event loop
    /// (`AGENTS.md` §3).
    pub(crate) fn pump(&mut self) -> bool {
        let mut changed = false;

        // A finished operation is joined here, on the UI's own tick, rather
        // than by blocking whoever asked about it.
        if self
            .running
            .as_ref()
            .is_some_and(crate::operations::Running::is_finished)
        {
            if let Some(running) = self.running.take() {
                let operation = running.operation().cloned();
                if let Some(report) = running.finish() {
                    if let Some(operation) = operation {
                        if let Some(record) = UndoRecord::from_report(&operation, &report) {
                            // A bounded history: undo is for the mistake you
                            // just made, not an archive.
                            if self.undo_stack.len() >= 32 {
                                self.undo_stack.remove(0);
                            }
                            self.undo_stack.push(record);
                        }
                    }
                    self.last_summary = Some(crate::operations::Summary::from_report(&report));
                }
            }
            // Then the next one, if anything is waiting.
            if let Some((plan, policy)) = self.queue.pop_front() {
                self.running = Some(crate::operations::Running::start(plan, policy));
            }
            // Both panes may have changed on disk.
            let panes = self.workspace.pane_order();
            for pane in panes {
                self.start_enumeration(pane);
            }
            changed = true;
        } else if self.running.is_some() {
            changed = true; // progress moved
        }
        let panes: Vec<PaneId> = self.views.keys().copied().collect();
        let show_hidden = self.show_hidden;
        let folders_first = self.settings.folders_first;
        for pane in panes {
            let filter = self.filter_text(pane).to_lowercase();
            let Some(view) = self.views.get_mut(&pane) else {
                continue;
            };
            // Search results arrive on their own channel; a pane is either
            // listing a directory or showing results, never both.
            if let Some(search) = view.search.as_ref() {
                let (finished, moved) = drain_search(view, search.poll());
                changed |= moved;
                if changed {
                    Self::recompute_visible(view, &filter, show_hidden);
                }
                if finished {
                    view.loading = false;
                    view.search = None;
                    view.search_in.clear();
                    let sort = view.sort;
                    sort_entries_with(&mut view.entries, sort, folders_first);
                    Self::recompute_visible(view, &filter, show_hidden);
                    view.generation += 1;
                }
                continue;
            }

            let Some(handle) = view.handle.as_ref() else {
                continue;
            };

            let mut finished = false;
            for batch in handle.poll() {
                changed = true;
                match batch {
                    // Hidden entries are kept and excluded from the visible
                    // index instead, so toggling the setting is instant and
                    // does not re-scan the directory.
                    Batch::Rows(rows) => view.entries.extend(rows),
                    Batch::Done { .. } => finished = true,
                    Batch::Failed(error) => {
                        view.error = Some(error);
                        finished = true;
                    }
                }
            }
            if changed {
                Self::recompute_visible(view, &filter, show_hidden);
            }
            if finished {
                view.loading = false;
                view.handle = None;
                let sort = view.sort;
                sort_entries_with(&mut view.entries, sort, folders_first);
                Self::recompute_visible(view, &filter, show_hidden);
                // The final sort reorders everything, so the row set has a new
                // identity even though its length did not change.
                view.generation += 1;
            }
        }
        changed
    }

    fn start_enumeration(&mut self, pane: PaneId) {
        let Some(location) = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|t| t.location().clone())
        else {
            return;
        };
        // Every route into a folder ends here - the path field, a double
        // click, the tree, a bookmark, back and forward - so this is the one
        // place the recent list has to be told about.
        if let Some(path) = location.as_path() {
            self.places.visit(path);
        }
        // Read before the mutable borrow of the view below.
        let show_hidden = self.show_hidden;
        let folders_first = self.settings.folders_first;
        let needle = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(String::new, |tab| tab.filter().text.to_lowercase());
        let sort = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(SortSpec::default, jtf_workspace::Tab::sort);

        let view = self.views.entry(pane).or_insert_with(PaneView::new);
        // Dropping the old handle cancels the previous scan, so a fast
        // navigation cannot leave two enumerations racing to fill one pane.
        view.handle = None;
        view.search = None;
        // Listing a directory means the pane is no longer showing results.
        // Leaving the query set made the status line keep claiming "N results"
        // for a directory it had navigated to since.
        view.query.clear();
        view.entries.clear();
        view.visible.clear();
        view.error = None;
        view.sort = sort;
        view.loading = true;
        view.generation += 1;

        // An archive is browsed like a folder. CV.HLP 4: pressing Enter on a
        // ZIP shows what is inside it, and from there you look around. The
        // listing is synthesised here rather than by the provider, because a
        // provider enumerates a filesystem and an archive is a file.
        if let Some(entries) = archive_entries(&location) {
            view.entries = entries;
            view.loading = false;
            sort_entries_with(&mut view.entries, sort, folders_first);
            Self::recompute_visible(view, &needle, show_hidden);
            return;
        }

        // The provider is chosen before the view is borrowed again: picking
        // it needs `&self`, and the view is a mutable borrow of a field of
        // the same `self`.
        let started = self.provider_for(&location).enumerate_async(&location);
        let view = self.views.entry(pane).or_insert_with(PaneView::new);
        match started {
            Ok(handle) => view.handle = Some(handle),
            Err(error) => {
                view.error = Some(error);
                view.loading = false;
            }
        }
    }

    fn refresh_all_panes(&mut self) {
        let panes = self.workspace.pane_order();
        self.views.retain(|id, _| panes.contains(id));
        for pane in panes {
            self.views.entry(pane).or_insert_with(PaneView::new);
            self.start_enumeration(pane);
        }
    }

    // ----------------------------------------------------------- i18n, theme

    /// Show `locale`, or follow the system when it is empty.
    ///
    /// An empty string is how the settings screen says "follow the system":
    /// it clears the stored choice rather than storing today's answer, so the
    /// setting keeps meaning "follow" tomorrow.
    pub(crate) fn set_locale(&mut self, locale: &str) {
        let id = if locale.is_empty() {
            LocaleId::best_match_of(self.system_locale.split(','))
        } else {
            LocaleId::new(locale)
        };
        self.settings.locale = locale.to_string();
        self.localizer
            .set_primary(load_catalog(&self.repo_root, &id));
        self.workspace.set_locale(id.clone());
        self.locale = id;
    }

    /// The user's stored choice, empty when following the system.
    pub(crate) fn locale_preference(&self) -> String {
        self.settings.locale.clone()
    }

    pub(crate) fn locale(&self) -> String {
        self.locale.as_str().to_string()
    }

    pub(crate) fn tr(&self, key: &str) -> String {
        self.localizer.text_or_key(key)
    }

    /// The active keymap's name.
    pub(crate) fn keymap_name(&self) -> String {
        self.keymap.name().to_string()
    }

    /// Switch keymap preset. An unknown name falls back to the platform one
    /// rather than leaving the application with no shortcuts at all.
    pub(crate) fn set_keymap(&mut self, name: &str) {
        self.keymap = load_keymap(&self.repo_root, name);
        self.dropped_bindings = apply_user_overrides(&mut self.keymap, &self.registry);
        self.settings.keymap = self.keymap.name().to_string();
    }

    /// How many stored bindings named a command that no longer exists.
    ///
    /// Surfaced so an upgrade that drops a binding says something changed,
    /// rather than leaving the user with a dead key
    /// (`docs/UPGRADE.md` §4.2).
    pub(crate) const fn dropped_bindings(&self) -> usize {
        self.dropped_bindings
    }

    /// Commands, in registry order, for a settings list.
    pub(crate) fn command_count(&self) -> usize {
        self.registry.len()
    }

    /// One command's id, label key and category key.
    pub(crate) fn command_at(&self, index: usize) -> Option<(String, &'static str, &'static str)> {
        self.registry.iter().nth(index).map(|command| {
            (
                command.id().as_str().to_string(),
                command.label_key(),
                command.category().label_key(),
            )
        })
    }

    /// Whether a command can destroy data, so the settings list can mark it.
    pub(crate) fn command_is_destructive(&self, index: usize) -> bool {
        self.registry
            .iter()
            .nth(index)
            .is_some_and(Command::is_destructive)
    }

    /// Bind a chord to a command.
    ///
    /// Returns `Ok(())`, or the id of the command that already owns the
    /// chord. `docs/UI_TEST_PLAN.md` KEY-005 wants the conflict named, not
    /// merely refused.
    pub(crate) fn bind_shortcut(&mut self, command: &str, chord: &str) -> Result<(), String> {
        let id = CommandId::new(command);
        if !self.registry.contains(&id) {
            return Err(String::new());
        }
        let parsed = KeyChord::parse(chord).map_err(|_| String::new())?;

        match self.keymap.rebind(&id, parsed) {
            Ok(()) => {
                self.save_user_keymap();
                Ok(())
            }
            Err(KeymapError::Conflict { existing, .. }) => Err(existing.as_str().to_string()),
            Err(_) => Err(String::new()),
        }
    }

    /// Remove a command's shortcut.
    pub(crate) fn clear_shortcut(&mut self, command: &str) {
        self.keymap.unbind_command(&CommandId::new(command));
        self.save_user_keymap();
    }

    /// Forget every customisation and go back to the preset.
    pub(crate) fn reset_shortcuts(&mut self) {
        let _ = fs::remove_file(user_keymap_path());
        let name = self.settings.keymap.clone();
        self.keymap = load_keymap(&self.repo_root, &name);
    }

    /// Persist what the user changed, as a diff against their preset.
    ///
    /// A diff rather than a copy: a copy means every command added in a later
    /// release ships unbound for anyone who ever customised anything, because
    /// their file does not mention it and their file wins
    /// (`docs/UPGRADE.md` §4.1).
    fn save_user_keymap(&self) {
        let path = user_keymap_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // Diffed against the shipped preset as it is now, not against
        // whatever is already in the user's file.
        let preset = load_keymap(&self.repo_root, &self.settings.keymap);
        let diff = self.keymap.diff_from(&preset);

        let temporary = path.with_extension("tmp");
        if fs::write(&temporary, diff.to_text()).is_ok() {
            let _ = fs::rename(&temporary, &path);
        }
    }

    // ------------------------------------------------------------- settings

    /// Startup behaviour: 0 last session, 1 home, 2 a fixed location.
    pub(crate) const fn startup_mode(&self) -> i32 {
        match self.settings.restore_on_launch {
            jtf_workspace::RestoreOnLaunch::LastSession => 0,
            jtf_workspace::RestoreOnLaunch::HomeLocation => 1,
            jtf_workspace::RestoreOnLaunch::FixedLocation { .. } => 2,
        }
    }

    /// The fixed start location, when there is one.
    pub(crate) fn startup_location(&self) -> String {
        match &self.settings.restore_on_launch {
            jtf_workspace::RestoreOnLaunch::FixedLocation { location } => location
                .as_path()
                .map_or_else(String::new, |path| path.display().to_string()),
            _ => String::new(),
        }
    }

    /// Set startup behaviour.
    ///
    /// Switching away from remembering the last session **erases** what was
    /// stored, immediately: an off switch that leaves yesterday's paths on
    /// disk is not an off switch (`docs/UI_UX_SPEC.md` §16.2).
    pub(crate) fn set_startup(&mut self, mode: i32, location: &str) {
        self.settings.restore_on_launch = match mode {
            1 => jtf_workspace::RestoreOnLaunch::HomeLocation,
            2 => jtf_workspace::RestoreOnLaunch::FixedLocation {
                location: Location::local(location),
            },
            _ => jtf_workspace::RestoreOnLaunch::LastSession,
        };
        self.save_session();
    }

    /// Whether closed tabs are remembered between runs.
    pub(crate) const fn remember_closed_tabs(&self) -> bool {
        self.settings.remember_closed_tabs
    }

    /// Whether marks are remembered between runs.
    pub(crate) const fn remember_marks(&self) -> bool {
        self.settings.remember_marks
    }

    /// Set the two finer memory switches.
    pub(crate) fn set_remember(&mut self, closed_tabs: bool, marks: bool) {
        self.settings.remember_closed_tabs = closed_tabs;
        self.settings.remember_marks = marks;
        self.save_session();
    }

    /// The shortcut bound to a command, rendered for the toolkit.
    ///
    /// Empty when the command is unbound, which is a normal state: a preset
    /// binds what its users expect and leaves the rest alone.
    pub(crate) fn shortcut_for(&self, command: &str) -> String {
        let id = CommandId::new(command);
        self.keymap
            .chords_for(&id)
            .first()
            .map_or_else(String::new, |chord| chord.to_portable_shortcut())
    }

    /// Whether a command exists at all, so the UI can refuse to build a menu
    /// item for something nothing implements.
    pub(crate) fn has_command(&self, command: &str) -> bool {
        self.registry.contains(&CommandId::new(command))
    }

    /// How the list should be drawn.
    pub(crate) const fn font(&self) -> &FontSettings {
        &self.settings.font
    }

    /// Change the list font. An empty family means the platform's own fixed
    /// font; a zero size means the platform default.
    pub(crate) fn set_font(&mut self, family: &str, point_size: u16, monospace: bool) {
        // The scope is a separate preference and is not part of what this
        // call sets, so it is carried over rather than reset to its default
        // every time the size changes.
        let everywhere = self.settings.font.monospace_everywhere;
        self.settings.font = FontSettings {
            family: family.to_string(),
            point_size,
            monospace,
            monospace_everywhere: everywhere,
        };
    }

    pub(crate) const fn set_monospace_everywhere(&mut self, everywhere: bool) {
        self.settings.font.monospace_everywhere = everywhere;
    }

    pub(crate) const fn theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    pub(crate) fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
        self.workspace.set_theme_mode(mode);
    }

    /// Resolve a token to `0xAARRGGBB`, which is what `QColor::fromRgba`
    /// takes.
    pub(crate) fn theme_color(&self, system_is_dark: bool, token: ThemeToken) -> u32 {
        let system = if system_is_dark {
            SystemAppearance::Dark
        } else {
            SystemAppearance::Light
        };
        let resolved: ResolvedTheme = self.theme_mode.resolve(system);
        let c = Palette::for_theme(resolved).color(token);
        (u32::from(c.a) << 24) | (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
    }

    // --------------------------------------------------------------- session

    /// Persist the session. Called on quit and after a layout change.
    ///
    /// Written to a temporary file and renamed, so a crash mid-write leaves
    /// the previous session loadable (`docs/UI_TEST_PLAN.md` SESS-005).
    pub(crate) fn save_session(&self) {
        let session = Session::capture(&self.workspace, self.settings.clone())
            .with_places(self.places.clone());
        let Ok(json) = session.to_json() else { return };
        if let Some(parent) = self.session_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // A session written by a newer build is left exactly as it is.
        //
        // Running an older build over a newer one is an ordinary thing to do -
        // a downgrade after a bad release, a second machine that has not
        // updated - and the read side already refuses to guess at a format
        // from the future. The write side did not, so the first autosave threw
        // the newer file away and replaced it with one this build understands:
        // every tab, mark and layout the newer build was holding, gone, for no
        // reason the user could see (`docs/UPGRADE.md` §2).
        //
        // This run's own state still goes somewhere, under a name that says
        // what it is, so nothing is lost in either direction.
        let target = session_write_target(&self.session_path, self.session_outcome);

        let temporary = target.with_extension("tmp");
        if fs::write(&temporary, json).is_ok() {
            let _ = fs::rename(&temporary, &target);
        }
    }

    /// The one-off notice about how the last session was read, if there is one.
    ///
    /// Taken rather than read: it is said once, at the launch it applies to,
    /// and a message that comes back on every refresh is a message people
    /// learn to ignore.
    pub(crate) fn take_session_notice(&mut self) -> Option<&'static str> {
        if self.notice_taken || !self.session_outcome.needs_notice() {
            return None;
        }
        self.notice_taken = true;
        Some(match self.session_outcome {
            jtf_workspace::RestoreOutcome::UnsupportedVersion(_) => "session.notice.newer",
            _ => "session.notice.unreadable",
        })
    }
}

impl Default for App {
    /// With no system locale, so the fallback applies.
    fn default() -> Self {
        Self::new("")
    }
}

// ------------------------------------------------------------------ helpers

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// `YYYY-MM-DD HH:MM` in the machine's own time, computed without a date
/// library.
///
/// `offset` is the seconds east of UTC, which the UI asks the platform for and
/// hands down: working it out here would mean either a dependency or reading
/// the timezone database, and the platform already knows.
///
/// Without it every time in the list was UTC. In Taipei that is eight hours
/// out, and the inspector - which formatted the same timestamp through Qt, in
/// local time - disagreed with the row right next to it by exactly that much.
fn format_time(time: std::time::SystemTime, offset: i32) -> String {
    let Ok(since) = time.duration_since(std::time::UNIX_EPOCH) else {
        return String::new();
    };
    let Ok(secs) = i64::try_from(since.as_secs()) else {
        return String::new();
    };
    let secs = secs + i64::from(offset);
    let Ok(secs) = u64::try_from(secs) else {
        // Before 1970 once the offset is applied. Nothing sensible to show.
        return String::new();
    };
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        time_of_day / 3600,
        (time_of_day % 3600) / 60
    )
}

/// Howard Hinnant's days-from-civil, inverted. Public-domain algorithm.
///
/// The casts are the algorithm's own and are safe for the range it is used
/// with here: `z` comes from a `SystemTime` since the Unix epoch, so it is a
/// few tens of thousands of days, nowhere near any of these bounds.
#[allow(
    clippy::many_single_char_names,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "bounded inputs; see above"
)]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Column index in the tab's column list. The list is in `Column::ALL` order,
/// and the viewer's four columns are its first four.
/// A display column is an index into the tab's own column list, which is in
/// the same order. Identity, and stated once so it cannot drift again.
fn column_index(column: i32) -> usize {
    usize::try_from(column).unwrap_or(0)
}

fn display_name_of(location: &Location) -> String {
    location
        .file_name()
        .map_or_else(|| "/".to_string(), |n| n.to_string_lossy().into_owned())
}

fn home_location() -> Location {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    Location::local(home)
}

/// Copy the session aside if it was written by an older format version.
///
/// Named for the version it came from, so a directory with several is
/// readable rather than a pile of `.bak`s. Written once: a second run of the
/// same upgrade must not overwrite the original with the already-migrated
/// copy, which would quietly destroy the thing being kept.
/// Where this run's session may be written.
///
/// The stored path, unless the file already there came from a build this one
/// cannot read - in which case it is left alone and this run writes beside it.
/// Its own function so the rule is testable without a filesystem, and so it
/// cannot be quietly forgotten by a future caller that also saves.
fn session_write_target(session_path: &Path, outcome: jtf_workspace::RestoreOutcome) -> PathBuf {
    if matches!(
        outcome,
        jtf_workspace::RestoreOutcome::UnsupportedVersion(_)
    ) {
        session_path.with_file_name("session.from-older-build.json")
    } else {
        session_path.to_path_buf()
    }
}

fn back_up_before_migrating(session_path: &Path, text: &str) {
    // The version is read with a string scan rather than a JSON parser: the
    // bridge does not otherwise need one, and taking a dependency to read a
    // single integer out of a file we wrote ourselves is a poor trade.
    let Some(version) = stored_format_version(text) else {
        return; // unreadable: there is no version to name a backup after
    };
    if version >= jtf_workspace::SESSION_FORMAT_VERSION {
        return; // current or newer; nothing is being migrated
    }

    let backup = session_path.with_file_name(format!("session.v{version}.backup.json"));
    if backup.exists() {
        return;
    }
    let _ = fs::write(&backup, text);
}

/// The `"version":N` a session file declares, if it declares one.
fn stored_format_version(text: &str) -> Option<u32> {
    let at = text.find("\"version\"")?;
    let rest = &text[at + "\"version\"".len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Where this program keeps what it remembers.
///
/// The platform's own place for it: `~/Library/Application Support` on macOS,
/// `%APPDATA%` on Windows, `$XDG_CONFIG_HOME` on Linux.
///
/// This used to build the macOS path by hand from `HOME`, which on Windows is
/// usually unset - so the fallback took over and the session was written to
/// `%TEMP%\Library\Application Support\jt-filework`, a macOS-shaped path
/// inside a directory the system is entitled to empty. Every tab, mark and
/// window position was one cleanup away from being forgotten.
fn config_dir() -> PathBuf {
    dirs::config_dir().map_or_else(
        || {
            // Only when the platform will not say. Still per-user, and still
            // not the temp directory if there is any alternative.
            std::env::var_os("HOME")
                .map_or_else(std::env::temp_dir, PathBuf::from)
                .join(".jt-filework")
        },
        |base| base.join("jt-filework"),
    )
}

fn session_path() -> PathBuf {
    config_dir().join("session.json")
}

/// Find the repository root so the PoC can load `locales/` from the source
/// tree. A shipped build embeds them instead; this is a PoC affordance.
fn locate_repo_root() -> PathBuf {
    if let Some(explicit) = std::env::var_os("JTF_REPO_ROOT") {
        return PathBuf::from(explicit);
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));

    // A macOS bundle carries its own data: Contents/MacOS/<exe> sits beside
    // Contents/Resources. Checked first, because a shipped app must not
    // depend on finding a source tree above it — and because the walk below
    // once stopped one directory short of the repository root when the
    // executable moved into a bundle, which showed the user every string as
    // its catalogue key.
    let bundled = exe_dir.join("../Resources");
    if bundled.join("locales").join("en").is_dir() {
        return bundled;
    }

    // Everywhere else the data sits beside the executable, because there is no
    // bundle to put it in. Checked before the walk for the same reason: an
    // installed build must not depend on a source tree happening to be above
    // it.
    if exe_dir.join("locales").join("en").is_dir() {
        return exe_dir;
    }

    // Development builds run out of the tree. The bound is generous rather
    // than tight: the cost of one extra `is_dir` at startup is nothing, and
    // the cost of stopping one short is a UI full of identifiers.
    let mut dir = exe_dir;
    for _ in 0..16 {
        if dir.join("locales").join("en").is_dir() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from(".")
}

/// Load a keymap preset by name from `keymaps/<name>.keymap`.
///
/// Keymaps are data (`docs/UI_UX_SPEC.md` §7), so switching preset is reading
/// a different file rather than running different code — which is what makes
/// a settings screen for them a matter of editing data later.
/// Where a user's own keymap lives, separate from the shipped presets.
fn user_keymap_path() -> PathBuf {
    config_dir().join("user.keymap")
}

/// Layer the user's own bindings over the preset.
/// Layer the user's diff over the preset.
///
/// Returns how many bindings were dropped because they named a command that no
/// longer exists — a rename or a removal must not break the rest of the
/// keymap (`docs/UPGRADE.md` §4.2).
fn apply_user_overrides(keymap: &mut Keymap, registry: &CommandRegistry) -> usize {
    let Ok(text) = fs::read_to_string(user_keymap_path()) else {
        return 0;
    };
    let Ok(user) = Keymap::parse(keymap.name(), &text) else {
        return 0;
    };
    keymap.apply_diff(&user, |id| registry.contains(id))
}

/// The profile a fresh install starts with.
///
/// Single-Key, not the host platform's conventions: this program is built for
/// people who have that workflow in their fingers, and Native is one
/// keystroke away for everyone else (`AGENTS.md` §10.2,
/// `docs/KEYBOARD_PROFILE.md`).
pub(crate) const DEFAULT_KEYMAP: &str = "single-key";

/// The two presets the mode toggle switches between.
pub(crate) const KEYMAP_PRESETS: [&str; 2] = ["single-key", "native"];

fn load_keymap(repo_root: &std::path::Path, name: &str) -> Keymap {
    let wanted = if name.is_empty() {
        DEFAULT_KEYMAP
    } else {
        name
    };
    let path = repo_root.join("keymaps").join(format!("{wanted}.keymap"));

    let parsed = fs::read_to_string(&path)
        .ok()
        .and_then(|text| Keymap::parse(wanted, &text).ok());

    match parsed {
        Some(keymap) => keymap,
        None if wanted != "native" => load_keymap(repo_root, "native"),
        None => Keymap::new("native"),
    }
}

fn load_catalog(repo_root: &std::path::Path, locale: &LocaleId) -> Catalog {
    let dir = repo_root.join("locales").join(locale.as_str());
    let mut catalog = Catalog::new(locale.clone());
    let Ok(entries) = fs::read_dir(&dir) else {
        return catalog;
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "catalog"))
        .collect();
    files.sort();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        if let Ok(parsed) = Catalog::parse(locale.clone(), &text) {
            let _ = catalog.merge(parsed);
        }
    }
    catalog
}

/// A display row as an index into the listed entries, or `None` for the `..`
/// row.
///
/// Free-standing and pure so the off-by-one has a test. Every row-to-entry
/// lookup in the bridge goes through here, which is what keeps a synthetic
/// row from shifting one caller and not another.
const fn listed_row(has_parent_row: bool, row: usize) -> Option<usize> {
    if has_parent_row {
        row.checked_sub(1)
    } else {
        Some(row)
    }
}

#[cfg(test)]
mod tests {
    use super::{format_time, listed_row, session_write_target, stored_format_version};
    use jtf_core::ErrorCode;
    use jtf_workspace::RestoreOutcome;
    use std::path::Path;

    /// A timestamp is shown in the machine's own time, not UTC.
    ///
    /// Every date in the list was UTC. In Taipei that is eight hours out, and
    /// the inspector - which formatted the same instant through Qt, in local
    /// time - disagreed with the row next to it by exactly that much.
    #[test]
    fn a_time_is_shown_in_the_machines_own_zone() {
        // 2026-09-01 22:50 UTC.
        let instant = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_788_302_200);
        let utc = format_time(instant, 0);
        let taipei = format_time(instant, 8 * 3600);
        assert_ne!(utc, taipei, "the offset was not applied");
        assert!(!utc.is_empty() && !taipei.is_empty());
    }

    #[test]
    fn an_offset_that_would_land_before_the_epoch_says_nothing_rather_than_wrapping() {
        let instant = std::time::UNIX_EPOCH + std::time::Duration::from_secs(60);
        assert_eq!(format_time(instant, -12 * 3600), "");
    }

    /// The case this exists for: an older build must not throw away what a
    /// newer one wrote. Running an older build is ordinary - a downgrade after
    /// a bad release, a second machine that has not updated yet.
    #[test]
    fn a_session_from_a_newer_build_is_written_beside_rather_than_over() {
        let path = Path::new("/tmp/jtf/session.json");
        let target = session_write_target(path, RestoreOutcome::UnsupportedVersion(99));
        assert_ne!(target, path, "the newer file must be left where it is");
        assert_eq!(
            target.parent(),
            path.parent(),
            "and beside it, not elsewhere"
        );
        assert_eq!(
            target.file_name().and_then(|n| n.to_str()),
            Some("session.from-older-build.json"),
            "under a name that says what it is"
        );
    }

    /// Every other outcome writes where it always did. An unreadable file is
    /// not a file from the future: it is corrupt, already backed up, and
    /// keeping it forever would mean the program never saved again.
    #[test]
    fn every_other_outcome_writes_to_the_session_file() {
        let path = Path::new("/tmp/jtf/session.json");
        for outcome in [
            RestoreOutcome::Restored,
            RestoreOutcome::NothingStored,
            RestoreOutcome::DisabledByPreference,
            RestoreOutcome::Unreadable(ErrorCode::ParseFailed),
        ] {
            assert_eq!(session_write_target(path, outcome), path, "{outcome:?}");
        }
    }

    #[test]
    fn the_stored_format_version_is_found_however_the_file_is_spaced() {
        assert_eq!(stored_format_version(r#"{"version":1,"x":2}"#), Some(1));
        assert_eq!(stored_format_version(r#"{ "version" : 42 }"#), Some(42));
        assert_eq!(
            stored_format_version("{\n  \"version\": 7,\n  \"settings\": {}\n}"),
            Some(7),
            "the file the program actually writes is pretty-printed"
        );
    }

    #[test]
    fn a_file_with_no_version_yields_nothing_rather_than_a_guess() {
        assert_eq!(stored_format_version("{}"), None);
        assert_eq!(stored_format_version(""), None);
        assert_eq!(stored_format_version("not json at all"), None);
        assert_eq!(
            stored_format_version(r#"{"version":"one"}"#),
            None,
            "a version that is not a number is not a version"
        );
    }

    #[test]
    fn without_a_parent_row_display_rows_are_entry_indices() {
        assert_eq!(listed_row(false, 0), Some(0));
        assert_eq!(listed_row(false, 7), Some(7));
    }

    #[test]
    fn the_parent_row_has_no_entry_and_shifts_the_rest() {
        assert_eq!(
            listed_row(true, 0),
            None,
            "row 0 is `..`; resolving it to entry 0 is what would let an \
             operation act on the first file while pointing at the parent"
        );
        assert_eq!(listed_row(true, 1), Some(0));
        assert_eq!(listed_row(true, 8), Some(7));
    }
}

/// The entries of an archive, when `location` names one.
///
/// Returns `None` for anything that is not an archive we can list, so the
/// caller falls through to the ordinary filesystem enumeration.
fn archive_entries(location: &Location) -> Option<Vec<FileEntry>> {
    let path = location.as_path()?;
    if path.is_dir() || !path.is_file() {
        return None;
    }
    jtf_viewer::container_of(path)?;
    let members = jtf_fs::list_container(path).ok()?;

    let mut entries = Vec::with_capacity(members.len());
    for member in members {
        // Flat, with the stored path as the name, which is what CView's
        // archive view shows. Synthesising a folder tree from the stored
        // names would mean inventing directories the archive may not
        // contain, and would hide the very thing worth seeing: a member
        // whose name escapes is displayed exactly as it is stored.
        if member.is_directory {
            continue;
        }
        let name = member.name.trim_end_matches('/');
        if name.is_empty() {
            continue;
        }
        entries.push(
            FileEntry::new(
                Location::archive_member(location.clone(), member.name.clone()),
                jtf_core::RawName::new(name),
                FileKind::File,
            )
            .with_size(member.size),
        );
    }
    Some(entries)
}

// A test asserts by panicking, so the workspace's expect/unwrap lints are
// backwards here: an expect that fails *is* the failure report.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod archive_browsing_tests {
    use super::archive_entries;
    use jtf_core::{FileEntry, Location};

    /// Built by the system's own zip, so this checks the real path a user
    /// takes rather than a fixture shaped to pass.
    fn sample() -> Option<std::path::PathBuf> {
        let dir = std::env::temp_dir().join("jtf-archive-browse");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).ok()?;
        std::fs::write(dir.join("readme.txt"), b"hello").ok()?;
        std::fs::write(dir.join("sub/big.log"), vec![b'x'; 4000]).ok()?;

        let archive = std::env::temp_dir().join("jtf-archive-browse.zip");
        let _ = std::fs::remove_file(&archive);
        let ok = std::process::Command::new("zip")
            .arg("-qr")
            .arg(&archive)
            .arg(".")
            .current_dir(&dir)
            .output()
            .is_ok_and(|out| out.status.success());
        let _ = std::fs::remove_dir_all(&dir);
        ok.then_some(archive)
    }

    #[test]
    fn an_archive_lists_as_entries_a_pane_can_show() {
        let Some(archive) = sample() else {
            return; // no zip(1) here; nothing to check against
        };
        let entries = archive_entries(&Location::local(&archive)).expect("an archive lists");
        let names: Vec<String> = entries.iter().map(FileEntry::display_name).collect();

        assert!(
            names.iter().any(|n| n.ends_with("readme.txt")),
            "expected readme.txt among {names:?}"
        );
        assert!(
            names.iter().any(|n| n.contains("big.log")),
            "expected sub/big.log among {names:?}"
        );
        assert!(
            names.iter().all(|n| !n.ends_with('/')),
            "directory entries are not listed; the view is flat: {names:?}"
        );

        let big = entries
            .iter()
            .find(|e| e.display_name().contains("big.log"))
            .expect("big.log");
        assert_eq!(
            big.size(),
            Some(4000),
            "the size shown is the uncompressed one, which is what a person \
             is asking when they look"
        );
        assert!(
            matches!(big.location(), Location::ArchiveMember { .. }),
            "an entry inside an archive is an archive member, not a path on \
             disk - operations must not treat it as a file they can open"
        );
        let _ = std::fs::remove_file(&archive);
    }

    #[test]
    fn an_ordinary_file_is_not_mistaken_for_an_archive() {
        let path = std::env::temp_dir().join("jtf-not-archive.txt");
        std::fs::write(&path, b"just text").expect("write");
        assert!(
            archive_entries(&Location::local(&path)).is_none(),
            "falling through to the filesystem is what makes every other \
             file still open normally"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_directory_is_not_treated_as_an_archive() {
        let dir = std::env::temp_dir();
        assert!(archive_entries(&Location::local(&dir)).is_none());
    }
}

// --------------------------------------------------- writing an image to disk

/// What the writing thread has to say.
enum BurnUpdate {
    /// A new phase of the work has begun.
    Stage(jtf_imaging::Stage),
    /// Progress within the current phase.
    Progress(Progress),
    /// It finished, one way or the other.
    Done(Box<Result<jtf_imaging::Report, jtf_core::Error>>),
}

/// How the write ended.
pub(crate) enum BurnOutcome {
    /// Still going.
    Running,
    /// Every byte written, and checked if checking was asked for.
    Done(jtf_imaging::Report),
    /// It stopped. The disk holds part of an image and is not usable.
    Failed {
        /// The localization key for what to tell the user.
        key: &'static str,
        /// Developer-facing detail, for the log.
        detail: String,
    },
}

/// A write in flight.
pub(crate) struct Burning {
    updates: std::sync::mpsc::Receiver<BurnUpdate>,
    canceller: Canceller,
    target: String,
    stage: jtf_imaging::Stage,
    progress: Progress,
    outcome: BurnOutcome,
}

/// A watcher that forwards to the channel the UI thread is draining.
struct ChannelWatcher(std::sync::mpsc::Sender<BurnUpdate>);

impl jtf_imaging::Watcher for ChannelWatcher {
    fn stage(&mut self, stage: jtf_imaging::Stage) {
        let _ = self.0.send(BurnUpdate::Stage(stage));
    }
    fn progress(&mut self, progress: Progress) {
        let _ = self.0.send(BurnUpdate::Progress(progress));
    }
}

impl App {
    /// Ask the system what removable disks it has, and remember the answer.
    ///
    /// Returns how many there are. Zero is the normal answer.
    pub(crate) fn refresh_devices(&mut self) -> usize {
        self.devices = jtf_platform_devices::list().unwrap_or_default();
        self.devices.len()
    }

    /// How many disks the last refresh found.
    pub(crate) fn device_count(&self) -> usize {
        self.devices.len()
    }

    fn device(&self, index: usize) -> Option<&jtf_platform_devices::Device> {
        self.devices.get(index)
    }

    /// The node to write to.
    pub(crate) fn device_node(&self, index: usize) -> String {
        self.device(index)
            .map(|d| d.node.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// What to show in the list.
    pub(crate) fn device_name(&self, index: usize) -> String {
        self.device(index)
            .map(|d| d.display_name().to_string())
            .unwrap_or_default()
    }

    /// Capacity in bytes.
    pub(crate) fn device_size(&self, index: usize) -> u64 {
        self.device(index).map_or(0, |d| d.size)
    }

    /// Localization key for how the disk is attached.
    pub(crate) fn device_bus_key(&self, index: usize) -> &'static str {
        self.device(index).map_or("", |d| d.bus.label_key())
    }

    /// The volume labels currently mounted from the disk, comma separated.
    ///
    /// This is what tells two identical sticks apart, so it is worth showing
    /// even though it is not part of the decision.
    pub(crate) fn device_volumes(&self, index: usize) -> String {
        self.device(index).map_or_else(String::new, |d| {
            d.volumes
                .iter()
                .filter_map(|v| {
                    v.label.clone().or_else(|| {
                        v.mount_point
                            .as_ref()
                            .map(|m| m.to_string_lossy().into_owned())
                    })
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
    }

    /// Why this disk cannot take this image, as a localization key.
    ///
    /// Empty when it can. The UI shows the reason next to the disk rather than
    /// hiding it, so that a disk someone expected to see is explained.
    pub(crate) fn device_refusal_key(&self, index: usize, image: &str) -> &'static str {
        let Some(device) = self.device(index) else {
            return "device.refuse.unknown";
        };
        let path = std::path::Path::new(image);
        let Ok(meta) = std::fs::metadata(path) else {
            return "device.refuse.unknown";
        };
        match jtf_platform_devices::check(device, path, meta.len()) {
            Ok(()) => "",
            Err(refusal) => jtf_platform_devices::Refusal::message_key(&refusal),
        }
    }

    /// Begin writing `image` to the disk at `index`.
    ///
    /// Returns false if the plan was refused, which the caller should already
    /// have known from [`Self::device_refusal_key`] - this is the second check,
    /// on the values as they are now rather than as they were when the dialog
    /// was drawn.
    pub(crate) fn start_write(&mut self, index: usize, image: &str, verify: bool) -> bool {
        if matches!(
            self.burning.as_ref().map(|b| &b.outcome),
            Some(BurnOutcome::Running)
        ) {
            return false;
        }
        let Some(device) = self.device(index).cloned() else {
            return false;
        };
        let target = device.display_name().to_string();
        let Ok(mut plan) = jtf_imaging::Plan::new(std::path::Path::new(image), device) else {
            return false;
        };
        plan.verify = verify;

        let (token, canceller) = CancellationToken::new();
        let (sender, updates) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut watcher = ChannelWatcher(sender.clone());
            let outcome = jtf_imaging::run(&plan, &mut watcher, &token);
            let _ = sender.send(BurnUpdate::Done(Box::new(outcome)));
        });
        self.burning = Some(Burning {
            updates,
            canceller,
            target,
            stage: jtf_imaging::Stage::Unmounting,
            progress: Progress::indeterminate(),
            outcome: BurnOutcome::Running,
        });
        true
    }

    /// Take whatever the writing thread has said. Returns whether anything
    /// changed.
    pub(crate) fn pump_write(&mut self) -> bool {
        let Some(job) = self.burning.as_mut() else {
            return false;
        };
        if !matches!(job.outcome, BurnOutcome::Running) {
            return false;
        }
        let mut changed = false;
        // Drained rather than read one at a time: only the latest position is
        // worth drawing, and a slow poll must not crawl through a backlog.
        while let Ok(update) = job.updates.try_recv() {
            changed = true;
            match update {
                BurnUpdate::Stage(stage) => {
                    job.stage = stage;
                    // A new phase starts its own bar. Carrying the old
                    // position over would show the verify pass starting at
                    // 100 %.
                    job.progress = Progress::indeterminate();
                }
                BurnUpdate::Progress(progress) => job.progress = progress,
                BurnUpdate::Done(result) => {
                    job.outcome = match *result {
                        Ok(report) => BurnOutcome::Done(report),
                        Err(error) => BurnOutcome::Failed {
                            key: burn_failure_key(&error),
                            detail: error.to_string(),
                        },
                    };
                    break;
                }
            }
        }
        changed
    }

    /// Whether a write is running.
    pub(crate) fn write_is_running(&self) -> bool {
        matches!(
            self.burning.as_ref().map(|b| &b.outcome),
            Some(BurnOutcome::Running)
        )
    }

    /// Whether a write has finished, successfully or not.
    pub(crate) fn write_is_done(&self) -> bool {
        matches!(
            self.burning.as_ref().map(|b| &b.outcome),
            Some(BurnOutcome::Done(_) | BurnOutcome::Failed { .. })
        )
    }

    /// The name of the phase now running.
    pub(crate) fn write_stage_key(&self) -> &'static str {
        self.burning.as_ref().map_or("", |b| b.stage.label_key())
    }

    /// Progress within the current phase: 0 = done, 1 = total.
    pub(crate) fn write_progress(&self, which: i32) -> u64 {
        self.burning.as_ref().map_or(0, |b| match which {
            0 => b.progress.completed(),
            _ => b.progress.total().unwrap_or(0),
        })
    }

    /// The disk being written to.
    pub(crate) fn write_target(&self) -> &str {
        self.burning.as_ref().map_or("", |b| b.target.as_str())
    }

    /// The message key for how it ended. Empty while it is still running.
    pub(crate) fn write_outcome_key(&self) -> &'static str {
        match self.burning.as_ref().map(|b| &b.outcome) {
            Some(BurnOutcome::Done(report)) => {
                if report.verified.is_some() {
                    "imaging.done"
                } else {
                    "imaging.done_unchecked"
                }
            }
            Some(BurnOutcome::Failed { key, .. }) => key,
            _ => "",
        }
    }

    /// Developer-facing detail of a failure, for the log.
    pub(crate) fn write_failure_detail(&self) -> &str {
        match self.burning.as_ref().map(|b| &b.outcome) {
            Some(BurnOutcome::Failed { detail, .. }) => detail.as_str(),
            _ => "",
        }
    }

    /// Bytes written, on success.
    pub(crate) fn write_bytes(&self) -> u64 {
        match self.burning.as_ref().map(|b| &b.outcome) {
            Some(BurnOutcome::Done(report)) => report.written,
            _ => 0,
        }
    }

    /// The image's CRC-32, on success.
    pub(crate) fn write_checksum(&self) -> u32 {
        match self.burning.as_ref().map(|b| &b.outcome) {
            Some(BurnOutcome::Done(report)) => report.checksum,
            _ => 0,
        }
    }

    /// Stop the write.
    ///
    /// The disk is left holding part of an image, which cannot be undone; the
    /// UI says so rather than implying the disk went back to how it was.
    pub(crate) fn cancel_write(&mut self) {
        if let Some(job) = self.burning.as_ref() {
            job.canceller.cancel();
        }
    }

    /// Forget the finished write.
    pub(crate) fn close_write(&mut self) {
        self.burning = None;
    }
}

/// Turn an error from the write into something worth showing a person.
fn burn_failure_key(error: &jtf_core::Error) -> &'static str {
    match error.code() {
        jtf_core::ErrorCode::PermissionDenied => "imaging.needs_elevation",
        // Everything else means the disk was partly written: cancellation, an
        // I/O failure, a read-back that did not match. The bytes that got there
        // are there, and saying "cancelled" alone would imply otherwise.
        _ => "imaging.failed_midway",
    }
}

impl App {
    /// Move the copy/move target to the next pane round.
    ///
    /// Returns whether anything moved, which is false with fewer than three
    /// panes - with two there is only one other pane and it is already the
    /// target.
    pub(crate) fn cycle_target_pane(&mut self) -> bool {
        let before = self.workspace.target_pane_id();
        let after = self.workspace.cycle_target();
        after.is_some() && after != before
    }
}

impl App {
    /// Tell the core what the machine's UTC offset is, in seconds east.
    ///
    /// Asked of the platform rather than worked out here: the timezone
    /// database is the platform's job and it already has it.
    pub(crate) fn set_utc_offset(&mut self, seconds: i32) {
        self.utc_offset = seconds;
    }
}
