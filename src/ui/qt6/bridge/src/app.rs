//! Application state.
//!
//! Everything the UI shows lives here, in Rust. The C++ side holds one
//! pointer to an [`App`] and asks it questions.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use jtf_commands::{Command, CommandId, CommandRegistry, KeyChord, Keymap, KeymapError};
use jtf_core::i18n::{Catalog, LocaleId, Localizer};
use jtf_core::theme::{Palette, ResolvedTheme, SystemAppearance, ThemeMode, ThemeToken};
use jtf_core::{Error, FileEntry, FileKind, Location};
use jtf_fs::{Batch, EnumerationHandle, LocalProvider, Provider};
use jtf_jobs::CancellationToken;
use jtf_ops::{ConflictPolicy, Plan, PlanError};
use jtf_viewer::{detect, ContentKind, Encoding, HexView, TextView};
use jtf_workspace::{
    sort_entries, FontSettings, LayoutPreset, Orientation, PaneId, Session, SessionSettings,
    SortKey, SortSpec, Workspace,
};

/// Columns the PoC shows. Kept in sync with the C++ header by
/// `docs/adr/0001-gui-stack.md`'s PoC scope, not by cleverness.
pub(crate) const COLUMN_NAME: i32 = 0;
pub(crate) const COLUMN_SIZE: i32 = 1;
pub(crate) const COLUMN_KIND: i32 = 2;
pub(crate) const COLUMN_MODIFIED: i32 = 3;
pub(crate) const COLUMN_COUNT: i32 = 4;

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
    entries: Vec<FileEntry>,
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
}

impl PaneView {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            generation: 0,
            handle: None,
            sort: SortSpec::default(),
            error: None,
            loading: false,
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
    provider: LocalProvider,
    localizer: Localizer,
    locale: LocaleId,
    theme_mode: ThemeMode,
    show_hidden: bool,
    settings: SessionSettings,
    pending_plan: Option<Plan>,
    plan_error: Option<PlanError>,
    running: Option<crate::operations::Running>,
    viewer: Option<ViewerSession>,
    last_summary: Option<crate::operations::Summary>,
    registry: CommandRegistry,
    keymap: Keymap,
    repo_root: PathBuf,
    session_path: PathBuf,
}

impl App {
    /// Start the application, restoring the previous session if the user's
    /// preference allows it (`docs/PRODUCT_SPEC.md` §5.1).
    pub(crate) fn new() -> Self {
        let repo_root = locate_repo_root();
        let session_path = session_path();
        let home = home_location();

        let stored = fs::read_to_string(&session_path).ok();
        let restored = Session::restore(stored.as_deref(), &home);
        let settings = restored.settings.clone();

        let locale = restored.workspace.locale().clone();
        let localizer = Localizer::new(
            load_catalog(&repo_root, &locale),
            load_catalog(&repo_root, &LocaleId::english()),
        );
        let theme_mode = restored.workspace.theme_mode();

        let mut app = Self {
            workspace: restored.workspace,
            views: BTreeMap::new(),
            provider: LocalProvider::new(),
            localizer,
            locale,
            theme_mode,
            show_hidden: false,
            pending_plan: None,
            plan_error: None,
            running: None,
            viewer: None,
            last_summary: None,
            registry: CommandRegistry::baseline(),
            keymap: {
                let mut keymap = load_keymap(&repo_root, &settings.keymap);
                apply_user_overrides(&mut keymap);
                keymap
            },
            settings,
            repo_root,
            session_path,
        };
        app.refresh_all_panes();
        app
    }

    // ---------------------------------------------------------------- layout

    /// The layout tree, as JSON, for the C++ side to build splitters from.
    ///
    /// Parsed only when the layout changes, never per frame, so the cost of
    /// going through text is irrelevant and the alternative — a second
    /// hand-written tree ABI — would be worse.
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

    pub(crate) fn close_active_pane(&mut self) -> bool {
        let pane = self.workspace.active_pane_id();
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

    pub(crate) fn close_tab(&mut self, pane: PaneId, index: usize) {
        let Some(id) = self
            .workspace
            .pane(pane)
            .and_then(|p| p.tabs().get(index))
            .map(jtf_workspace::Tab::id)
        else {
            return;
        };
        if let Some(p) = self.workspace.pane_mut(pane) {
            if p.tab_count() > 1 {
                p.close_tab(id, true);
            }
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

    pub(crate) fn current_path(&self, pane: PaneId) -> String {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|t| t.location().as_path())
            .map_or_else(String::new, |p| p.display().to_string())
    }

    pub(crate) fn navigate(&mut self, pane: PaneId, path: &str) {
        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                tab.navigate_to(Location::local(path));
            }
        }
        self.start_enumeration(pane);
    }

    pub(crate) fn navigate_up(&mut self, pane: PaneId) {
        let parent = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .and_then(|t| t.location().parent());
        if let Some(parent) = parent {
            if let Some(p) = self.workspace.pane_mut(pane) {
                if let Some(tab) = p.active_tab_mut() {
                    tab.navigate_to(parent);
                }
            }
            self.start_enumeration(pane);
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
        let Some(entry) = self.views.get(&pane).and_then(|v| v.entries.get(row)) else {
            return false;
        };
        if !entry.kind().is_navigable_by_default() && entry.kind() != FileKind::Symlink {
            return false;
        }
        let Some(path) = entry.location().as_path().map(std::path::Path::to_path_buf) else {
            return false;
        };
        if !path.is_dir() {
            return false;
        }
        self.navigate(pane, &path.display().to_string());
        true
    }

    // ---------------------------------------------------------------- rows

    /// Identity of the current row set. See [`PaneView::generation`].
    pub(crate) fn row_generation(&self, pane: PaneId) -> u64 {
        self.views.get(&pane).map_or(0, |v| v.generation)
    }

    pub(crate) fn row_count(&self, pane: PaneId) -> usize {
        self.views.get(&pane).map_or(0, |v| v.entries.len())
    }

    pub(crate) fn is_loading(&self, pane: PaneId) -> bool {
        self.views.get(&pane).is_some_and(|v| v.loading)
    }

    pub(crate) fn error_key(&self, pane: PaneId) -> Option<&'static str> {
        self.views
            .get(&pane)
            .and_then(|v| v.error.as_ref())
            .map(jtf_core::Error::message_key)
    }

    pub(crate) fn row_text(&self, pane: PaneId, row: usize, column: i32) -> String {
        let Some(entry) = self.views.get(&pane).and_then(|v| v.entries.get(row)) else {
            return String::new();
        };
        match column {
            COLUMN_NAME => entry.display_name(),
            COLUMN_SIZE => entry.size().map_or_else(
                || {
                    if entry.kind().is_directory_on_disk() {
                        String::new()
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
                .map_or_else(String::new, format_time),
            _ => String::new(),
        }
    }

    /// Full path of a row, for the UI to ask the platform for its icon.
    ///
    /// The icon itself is the toolkit's business: `AGENTS.md` §8 says use
    /// native behaviour where users expect it, and a file's icon is the most
    /// visible instance of that.
    pub(crate) fn row_path(&self, pane: PaneId, row: usize) -> String {
        self.views
            .get(&pane)
            .and_then(|v| v.entries.get(row))
            .and_then(|e| e.location().as_path())
            .map_or_else(String::new, |p| p.display().to_string())
    }

    pub(crate) fn row_is_directory(&self, pane: PaneId, row: usize) -> bool {
        self.views
            .get(&pane)
            .and_then(|v| v.entries.get(row))
            .is_some_and(|e| e.kind().is_directory_on_disk())
    }

    pub(crate) fn row_is_marked(&self, pane: PaneId, row: usize) -> bool {
        let Some(entry) = self.views.get(&pane).and_then(|v| v.entries.get(row)) else {
            return false;
        };
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .is_some_and(|t| t.marks().contains(entry.location()))
    }

    pub(crate) fn toggle_mark(&mut self, pane: PaneId, row: usize) {
        let Some(location) = self
            .views
            .get(&pane)
            .and_then(|v| v.entries.get(row))
            .map(|e| e.location().clone())
        else {
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
        let listed: Vec<Location> = self
            .views
            .get(&pane)
            .map(|v| v.entries.iter().map(|e| e.location().clone()).collect())
            .unwrap_or_default();
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
        let locations: Vec<Location> = self
            .views
            .get(&pane)
            .map(|view| {
                rows.iter()
                    .filter_map(|row| view.entries.get(*row))
                    .map(|entry| entry.location().clone())
                    .collect()
            })
            .unwrap_or_default();

        if let Some(p) = self.workspace.pane_mut(pane) {
            if let Some(tab) = p.active_tab_mut() {
                let selection = tab.selection_mut();
                selection.clear();
                selection.select_range(locations);
            }
        }
    }

    /// What an operation started in this pane would act on, and from where.
    fn operation_sources(&self, pane: PaneId) -> Vec<PathBuf> {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map(|tab| {
                tab.operation_target()
                    .locations()
                    .iter()
                    .filter_map(|location| location.as_path().map(std::path::Path::to_path_buf))
                    .collect()
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

    /// Build a rename plan.
    pub(crate) fn prepare_rename(&mut self, pane: PaneId, new_name: &str) -> bool {
        self.plan_error = None;
        self.pending_plan = None;
        let Some(source) = self.operation_sources(pane).first().cloned() else {
            return false;
        };
        self.set_plan(&jtf_ops::Operation::Rename {
            source,
            new_name: new_name.to_string(),
        })
    }

    /// Build a new-folder plan.
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

    fn set_plan(&mut self, operation: &jtf_ops::Operation) -> bool {
        match Plan::build(operation, &CancellationToken::never()) {
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
            return false;
        }
        self.last_summary = None;
        self.running = Some(crate::operations::Running::start(plan, policy));
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
            .views
            .get(&pane)
            .and_then(|view| view.entries.get(row))
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

    /// Re-read the current location.
    pub(crate) fn refresh(&mut self, pane: PaneId) {
        self.start_enumeration(pane);
    }

    pub(crate) fn marked_count(&self, pane: PaneId) -> usize {
        self.workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or(0, |t| t.marks().len())
    }

    pub(crate) fn sort_by(&mut self, pane: PaneId, column: i32) {
        let key = match column {
            COLUMN_SIZE => SortKey::Size,
            COLUMN_KIND => SortKey::Kind,
            COLUMN_MODIFIED => SortKey::Modified,
            _ => SortKey::Name,
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
        if let Some(view) = self.views.get_mut(&pane) {
            view.sort = sort;
            sort_entries(&mut view.entries, sort);
            view.generation += 1;
        }
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
                if let Some(report) = running.finish() {
                    self.last_summary = Some(crate::operations::Summary::from_report(&report));
                }
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
        for pane in panes {
            let Some(view) = self.views.get_mut(&pane) else {
                continue;
            };
            let Some(handle) = view.handle.as_ref() else {
                continue;
            };

            let mut finished = false;
            for batch in handle.poll() {
                changed = true;
                match batch {
                    Batch::Rows(rows) => {
                        view.entries.extend(
                            rows.into_iter()
                                .filter(|e| self.show_hidden || !e.attributes().hidden),
                        );
                    }
                    Batch::Done { .. } => finished = true,
                    Batch::Failed(error) => {
                        view.error = Some(error);
                        finished = true;
                    }
                }
            }
            if finished {
                view.loading = false;
                view.handle = None;
                let sort = view.sort;
                sort_entries(&mut view.entries, sort);
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
        let sort = self
            .workspace
            .pane(pane)
            .and_then(jtf_workspace::Pane::active_tab)
            .map_or_else(SortSpec::default, jtf_workspace::Tab::sort);

        let view = self.views.entry(pane).or_insert_with(PaneView::new);
        // Dropping the old handle cancels the previous scan, so a fast
        // navigation cannot leave two enumerations racing to fill one pane.
        view.handle = None;
        view.entries.clear();
        view.error = None;
        view.sort = sort;
        view.loading = true;
        view.generation += 1;

        match self.provider.enumerate_async(&location) {
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

    pub(crate) fn set_locale(&mut self, locale: &str) {
        let id = LocaleId::new(locale);
        self.localizer
            .set_primary(load_catalog(&self.repo_root, &id));
        self.workspace.set_locale(id.clone());
        self.locale = id;
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
        apply_user_overrides(&mut self.keymap);
        self.settings.keymap = self.keymap.name().to_string();
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

    /// Persist the whole keymap as the user's own.
    ///
    /// The full map rather than a diff: a preset that changes underneath a
    /// stored diff would silently move the user's shortcuts around.
    fn save_user_keymap(&self) {
        let path = user_keymap_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let temporary = path.with_extension("tmp");
        if fs::write(&temporary, self.keymap.to_text()).is_ok() {
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
        self.settings.font = FontSettings {
            family: family.to_string(),
            point_size,
            monospace,
        };
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
        let session = Session::capture(&self.workspace, self.settings.clone());
        let Ok(json) = session.to_json() else { return };
        if let Some(parent) = self.session_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let temporary = self.session_path.with_extension("tmp");
        if fs::write(&temporary, json).is_ok() {
            let _ = fs::rename(&temporary, &self.session_path);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
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

/// `YYYY-MM-DD HH:MM`, computed without a date library.
///
/// A real implementation formats with the platform's locale-aware API
/// (`docs/PRODUCT_SPEC.md` §14); this is a PoC placeholder and is marked as
/// one in `TODO.md`.
fn format_time(time: std::time::SystemTime) -> String {
    let Ok(since) = time.duration_since(std::time::UNIX_EPOCH) else {
        return String::new();
    };
    let secs = since.as_secs();
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

fn display_name_of(location: &Location) -> String {
    location
        .file_name()
        .map_or_else(|| "/".to_string(), |n| n.to_string_lossy().into_owned())
}

fn home_location() -> Location {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
    Location::local(home)
}

fn session_path() -> PathBuf {
    let base = std::env::var_os("HOME").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("Library/Application Support/jt-filework/session.json")
}

/// Find the repository root so the PoC can load `locales/` from the source
/// tree. A shipped build embeds them instead; this is a PoC affordance.
fn locate_repo_root() -> PathBuf {
    if let Some(explicit) = std::env::var_os("JTF_REPO_ROOT") {
        return PathBuf::from(explicit);
    }
    let mut dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    for _ in 0..8 {
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
    let base = std::env::var_os("HOME").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("Library/Application Support/jt-filework/user.keymap")
}

/// Layer the user's own bindings over the preset.
fn apply_user_overrides(keymap: &mut Keymap) {
    let Ok(text) = fs::read_to_string(user_keymap_path()) else {
        return;
    };
    let Ok(user) = Keymap::parse(keymap.name(), &text) else {
        return;
    };
    // The user's file is authoritative where it says anything at all.
    for (chord, command) in user.iter() {
        let _ = keymap.rebind(command, chord.clone());
    }
}

fn load_keymap(repo_root: &std::path::Path, name: &str) -> Keymap {
    let wanted = if name.is_empty() { "platform" } else { name };
    let path = repo_root.join("keymaps").join(format!("{wanted}.keymap"));

    let parsed = fs::read_to_string(&path)
        .ok()
        .and_then(|text| Keymap::parse(wanted, &text).ok());

    match parsed {
        Some(keymap) => keymap,
        None if wanted != "platform" => load_keymap(repo_root, "platform"),
        None => Keymap::new("platform"),
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
