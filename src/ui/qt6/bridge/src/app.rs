//! Application state.
//!
//! Everything the UI shows lives here, in Rust. The C++ side holds one
//! pointer to an [`App`] and asks it questions.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use jtf_core::i18n::{Catalog, LocaleId, Localizer};
use jtf_core::theme::{Palette, ResolvedTheme, SystemAppearance, ThemeMode, ThemeToken};
use jtf_core::{Error, FileEntry, FileKind, Location};
use jtf_fs::{Batch, EnumerationHandle, LocalProvider, Provider};
use jtf_workspace::{
    LayoutPreset, Orientation, PaneId, Session, SessionSettings, SortKey, SortSpec, Workspace,
};

/// Columns the PoC shows. Kept in sync with the C++ header by
/// `docs/adr/0001-gui-stack.md`'s PoC scope, not by cleverness.
pub(crate) const COLUMN_NAME: i32 = 0;
pub(crate) const COLUMN_SIZE: i32 = 1;
pub(crate) const COLUMN_KIND: i32 = 2;
pub(crate) const COLUMN_MODIFIED: i32 = 3;
pub(crate) const COLUMN_COUNT: i32 = 4;

/// What one pane is currently showing.
struct PaneView {
    entries: Vec<FileEntry>,
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
        let session = Session::capture(&self.workspace, SessionSettings::default());
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

fn sort_entries(entries: &mut [FileEntry], sort: SortSpec) {
    entries.sort_by(|a, b| {
        // Directories first, as every desktop file manager does.
        let dir_order = b
            .kind()
            .is_directory_on_disk()
            .cmp(&a.kind().is_directory_on_disk());
        if dir_order != std::cmp::Ordering::Equal {
            return dir_order;
        }
        let ordering = match sort.key {
            SortKey::Size => a.size().unwrap_or(0).cmp(&b.size().unwrap_or(0)),
            SortKey::Modified => a.timestamps().modified.cmp(&b.timestamps().modified),
            SortKey::Created => a.timestamps().created.cmp(&b.timestamps().created),
            SortKey::Kind | SortKey::Extension => a.extension_hint().cmp(&b.extension_hint()),
            // SortKey is non_exhaustive; a new key sorts by name until it is
            // implemented, which is wrong-but-harmless rather than a panic.
            SortKey::Name | _ => a
                .display_name()
                .to_lowercase()
                .cmp(&b.display_name().to_lowercase()),
        };
        if sort.ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}

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
