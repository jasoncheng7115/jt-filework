//! The C entry points.
//!
//! Conventions, applied without exception:
//!
//! - a null `App` pointer is a no-op or a zero, never a crash
//! - indices out of range are a no-op or a zero, never a panic
//! - text is copied into a caller buffer; the return is the byte length that
//!   *would* have been written, so the caller can detect truncation
//! - nothing allocated in Rust crosses the boundary, so C++ has nothing to
//!   free
//!
//! `unreachable_pub` is allowed for the module: these items are unreachable
//! from Rust by design, because their consumer is a linker, not a crate.

#![allow(
    unreachable_pub,
    reason = "these are ABI exports, reached by the linker"
)]

use std::ffi::{c_char, c_int, CStr};

use jtf_core::theme::{ThemeMode, ThemeToken};
use jtf_workspace::{LayoutPreset, PaneId};

use crate::app::{App, MarkAction};
use crate::operations::OperationKind;

/// Borrow the app, or do nothing.
///
/// # Safety
/// `app` must be null or a pointer returned by [`jtf_app_new`] that has not
/// been freed.
unsafe fn app_ref<'a>(app: *const App) -> Option<&'a App> {
    if app.is_null() {
        None
    } else {
        // SAFETY: caller contract above; the pointer came from Box::into_raw
        // and is not aliased mutably while a &App exists, because the C++
        // side is single-threaded on the UI thread.
        Some(unsafe { &*app })
    }
}

/// # Safety
/// As [`app_ref`], and no other reference to the app may be live.
unsafe fn app_mut<'a>(app: *mut App) -> Option<&'a mut App> {
    if app.is_null() {
        None
    } else {
        // SAFETY: see app_ref.
        Some(unsafe { &mut *app })
    }
}

/// Copy `text` into `buf`, NUL-terminated. Returns the byte length of `text`.
///
/// # Safety
/// `buf` must be null or writable for `len` bytes.
unsafe fn write_str(text: &str, buf: *mut c_char, len: c_int) -> c_int {
    let needed = text.len();
    if buf.is_null() || len <= 0 {
        return c_int::try_from(needed).unwrap_or(c_int::MAX);
    }
    let capacity = usize::try_from(len).unwrap_or(0).saturating_sub(1);
    let copy = needed.min(capacity);
    // Never split a UTF-8 character: truncate to a boundary.
    let mut copy = copy;
    while copy > 0 && !text.is_char_boundary(copy) {
        copy -= 1;
    }
    // SAFETY: `buf` is writable for `len` bytes by the caller contract, and
    // `copy < len`.
    unsafe {
        std::ptr::copy_nonoverlapping(text.as_ptr().cast::<c_char>(), buf, copy);
        *buf.add(copy) = 0;
    }
    c_int::try_from(needed).unwrap_or(c_int::MAX)
}

/// # Safety
/// `s` must be null or a valid NUL-terminated C string.
unsafe fn read_str<'a>(s: *const c_char) -> Option<&'a str> {
    if s.is_null() {
        return None;
    }
    // SAFETY: caller contract.
    unsafe { CStr::from_ptr(s) }.to_str().ok()
}

fn pane(index: c_int) -> PaneId {
    PaneId::new(u64::try_from(index).unwrap_or(0))
}

// ------------------------------------------------------------- lifecycle

/// Create the application. Never returns null.
#[no_mangle]
pub extern "C" fn jtf_app_new() -> *mut App {
    Box::into_raw(Box::new(App::new()))
}

/// Save the session and destroy the application.
///
/// # Safety
/// `app` must be null or a pointer from [`jtf_app_new`], freed only once.
#[no_mangle]
pub unsafe extern "C" fn jtf_app_free(app: *mut App) {
    if app.is_null() {
        return;
    }
    // SAFETY: caller contract.
    let boxed = unsafe { Box::from_raw(app) };
    boxed.save_session();
}

/// Persist the session without quitting.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_app_save_session(app: *const App) {
    if let Some(app) = unsafe { app_ref(app) } {
        app.save_session();
    }
}

/// Collect background results. Returns 1 if anything visible changed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_app_pump(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.pump()))
}

// ---------------------------------------------------------------- layout

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_layout_json(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let json = app.layout_json();
    unsafe { write_str(&json, buf, len) }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pane_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.pane_ids().len()).unwrap_or(0))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_active_pane(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.active_pane().get()).unwrap_or(0))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_focus_pane(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.focus_pane(pane(pane_id));
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_focus_next_pane(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.focus_next_pane();
    }
}

/// `vertical` non-zero splits top/bottom.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_split_active(app: *mut App, vertical: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.split_active(vertical != 0);
    }
}

/// Returns 1 if a pane was closed, 0 if it was the last one.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_close_active_pane(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.close_active_pane()))
}

/// 0 single, 1 two columns, 2 two rows, 3 quad.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_apply_preset(app: *mut App, preset: c_int) {
    let preset = match preset {
        1 => LayoutPreset::TwoColumns,
        2 => LayoutPreset::TwoRows,
        3 => LayoutPreset::Quad,
        _ => LayoutPreset::Single,
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.apply_preset(preset);
    }
}

// ------------------------------------------------------------------ tabs

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tab_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.tab_count(pane(pane_id))).unwrap_or(0)
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_active_tab(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.active_tab_index(pane(pane_id))).unwrap_or(0)
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tab_title(
    app: *const App,
    pane_id: c_int,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let title = app.tab_title(pane(pane_id), usize::try_from(index).unwrap_or(0));
    unsafe { write_str(&title, buf, len) }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_new_tab(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.new_tab();
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_close_tab(app: *mut App, pane_id: c_int, index: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.close_tab(pane(pane_id), usize::try_from(index).unwrap_or(0));
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_activate_tab(app: *mut App, pane_id: c_int, index: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.activate_tab(pane(pane_id), usize::try_from(index).unwrap_or(0));
    }
}

// ------------------------------------------------------------ navigation

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_current_path(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let path = app.current_path(pane(pane_id));
    unsafe { write_str(&path, buf, len) }
}

/// # Safety
/// See [`jtf_app_free`]; `path` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_navigate(app: *mut App, pane_id: c_int, path: *const c_char) {
    let Some(path) = (unsafe { read_str(path) }) else {
        return;
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.navigate(pane(pane_id), path);
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_navigate_up(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.navigate_up(pane(pane_id));
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_go_back(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.go_back(pane(pane_id));
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_go_forward(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.go_forward(pane(pane_id));
    }
}

/// Returns 1 if the row was a directory and the pane navigated into it.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_open_row(app: *mut App, pane_id: c_int, row: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.open_row(pane(pane_id), usize::try_from(row).unwrap_or(0)))
    })
}

// ------------------------------------------------------------------ rows

/// Identity of the pane's current row set.
///
/// Unchanged while rows are merely being appended; bumped on a new location,
/// a re-sort or a filter change. Lets the UI append rows instead of rebuilding
/// its model on every batch.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_generation(app: *const App, pane_id: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.row_generation(pane(pane_id)))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.row_count(pane(pane_id))).unwrap_or(c_int::MAX)
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_text(
    app: *const App,
    pane_id: c_int,
    row: c_int,
    column: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let text = app.row_text(pane(pane_id), usize::try_from(row).unwrap_or(0), column);
    unsafe { write_str(&text, buf, len) }
}

/// Full path of a row, for the platform's own icon lookup.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_path(
    app: *const App,
    pane_id: c_int,
    row: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let path = app.row_path(pane(pane_id), usize::try_from(row).unwrap_or(0));
    unsafe { write_str(&path, buf, len) }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_is_directory(
    app: *const App,
    pane_id: c_int,
    row: c_int,
) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.row_is_directory(pane(pane_id), usize::try_from(row).unwrap_or(0)))
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_is_marked(app: *const App, pane_id: c_int, row: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.row_is_marked(pane(pane_id), usize::try_from(row).unwrap_or(0)))
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_toggle_mark(app: *mut App, pane_id: c_int, row: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.toggle_mark(pane(pane_id), usize::try_from(row).unwrap_or(0));
    }
}

/// 0 mark all listed, 1 unmark all listed, 2 invert listed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_mark_listed(app: *mut App, pane_id: c_int, action: c_int) {
    let action = match action {
        1 => MarkAction::None,
        2 => MarkAction::Invert,
        _ => MarkAction::All,
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.mark_listed(pane(pane_id), action);
    }
}

/// Re-read the pane's current location.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_refresh(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.refresh(pane(pane_id));
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_marked_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.marked_count(pane(pane_id))).unwrap_or(0)
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_sort_by(app: *mut App, pane_id: c_int, column: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.sort_by(pane(pane_id), column);
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_is_loading(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_loading(pane(pane_id))))
}

/// Localization key for the pane's error, empty if there is none.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_error_key(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let key = app.error_key(pane(pane_id)).unwrap_or("");
    unsafe { write_str(key, buf, len) }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_show_hidden(app: *mut App, show: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_show_hidden(show != 0);
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_show_hidden(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.show_hidden()))
}

// ------------------------------------------------------------ i18n, theme

/// # Safety
/// See [`jtf_app_free`]; `locale` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_locale(app: *mut App, locale: *const c_char) {
    let Some(locale) = (unsafe { read_str(locale) }) else {
        return;
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_locale(locale);
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_locale(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.locale(), buf, len) }
}

/// Resolve a localization key. Falls back to the key itself, never to English
/// text invented at the call site (`AGENTS.md` §11).
///
/// # Safety
/// See [`jtf_app_free`]; `key` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_tr(
    app: *const App,
    key: *const c_char,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(key) = (unsafe { read_str(key) }) else {
        return 0;
    };
    unsafe { write_str(&app.tr(key), buf, len) }
}

// ------------------------------------------------------------- operations

/// Tell the model which rows are selected in a pane.
///
/// # Safety
/// See [`jtf_app_free`]; `rows` must point to `count` readable `c_int`s, or be
/// null when `count` is zero.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_selection(
    app: *mut App,
    pane_id: c_int,
    rows: *const c_int,
    count: c_int,
) {
    let Some(app) = (unsafe { app_mut(app) }) else {
        return;
    };
    let len = usize::try_from(count).unwrap_or(0);
    let indices: Vec<usize> = if rows.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: caller contract above.
        let slice = unsafe { std::slice::from_raw_parts(rows, len) };
        slice
            .iter()
            .filter_map(|row| usize::try_from(*row).ok())
            .collect()
    };
    app.set_selection(pane(pane_id), &indices);
}

/// Build a plan. 0 copy, 1 move, 2 trash, 3 delete.
///
/// Returns 1 when there is a plan waiting, 0 otherwise; in the 0 case
/// [`jtf_op_error_key`] says why.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare(app: *mut App, pane_id: c_int, kind: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.prepare_operation(pane(pane_id), OperationKind::from_code(kind)))
    })
}

/// Build a plan for dropped sources, newline-separated.
///
/// A newline-separated list rather than an array of pointers: paths cannot
/// contain a newline on any platform this targets, the encoding is obvious in
/// a debugger, and there is no array lifetime to get wrong across the
/// boundary.
///
/// # Safety
/// See [`jtf_app_free`]; `paths` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_drop(
    app: *mut App,
    pane_id: c_int,
    kind: c_int,
    paths: *const c_char,
) -> c_int {
    let Some(paths) = (unsafe { read_str(paths) }) else {
        return 0;
    };
    let sources: Vec<std::path::PathBuf> = paths
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(std::path::PathBuf::from)
        .collect();

    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.prepare_drop(pane(pane_id), OperationKind::from_code(kind), sources))
    })
}

/// # Safety
/// See [`jtf_app_free`]; `new_name` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_rename(
    app: *mut App,
    pane_id: c_int,
    new_name: *const c_char,
) -> c_int {
    let Some(name) = (unsafe { read_str(new_name) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.prepare_rename(pane(pane_id), name)))
}

/// # Safety
/// See [`jtf_app_free`]; `name` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_new_folder(
    app: *mut App,
    pane_id: c_int,
    name: *const c_char,
) -> c_int {
    let Some(name) = (unsafe { read_str(name) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.prepare_new_folder(pane(pane_id), name))
    })
}

/// Localization key explaining why the last prepare produced no plan.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_error_key(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(app.plan_error_key().unwrap_or(""), buf, len) }
}

/// How many destinations the pending plan would collide with.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_conflicts(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.pending_plan()
            .and_then(|plan| c_int::try_from(plan.conflicts.len()).ok())
            .unwrap_or(0)
    })
}

/// How many entries the pending plan covers.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_entries(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.pending_plan()
            .and_then(|plan| c_int::try_from(plan.total_entries).ok())
            .unwrap_or(0)
    })
}

/// Bytes the pending plan would move.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_bytes(app: *const App) -> u64 {
    unsafe { app_ref(app) }
        .and_then(|a| a.pending_plan().map(|plan| plan.total_bytes))
        .unwrap_or(0)
}

/// Whether the pending plan destroys data that cannot be recovered.
///
/// The UI must warn differently for this: `docs/UI_UX_SPEC.md` §10 asks it to
/// say so **before** the action, not after.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_is_irreversible(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(
            a.pending_plan()
                .is_some_and(|plan| plan.operation.is_irreversible()),
        )
    })
}

/// The first colliding destination, for the conflict dialog.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_first_conflict(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let text = app
        .pending_plan()
        .and_then(|plan| plan.conflicts.first())
        .map_or_else(String::new, |conflict| {
            conflict.destination.display().to_string()
        });
    unsafe { write_str(&text, buf, len) }
}

/// Run the pending plan. 0 skip, 1 overwrite, 2 keep both, 3 abort.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_start(app: *mut App, policy: c_int) -> c_int {
    let policy = match policy {
        1 => jtf_ops::ConflictPolicy::Overwrite,
        2 => jtf_ops::ConflictPolicy::KeepBoth,
        3 => jtf_ops::ConflictPolicy::Abort,
        _ => jtf_ops::ConflictPolicy::Skip,
    };
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.start_operation(policy)))
}

/// Whether an operation is in flight.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_running(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.operation_running()))
}

/// Percent complete, or -1 while the total is unknown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_percent(app: *const App) -> c_int {
    unsafe { app_ref(app) }
        .and_then(App::operation_percent)
        .map_or(-1, c_int::from)
}

/// Localization key for the running operation's label.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_label_key(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(app.operation_label_key().unwrap_or(""), buf, len) }
}

/// The entry currently being worked on.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_current(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let text = app
        .operation_current()
        .map_or_else(String::new, |path| path.display().to_string());
    unsafe { write_str(&text, buf, len) }
}

/// Ask the running operation to stop.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_cancel(app: *const App) {
    if let Some(app) = unsafe { app_ref(app) } {
        app.cancel_operation();
    }
}

/// Whether a finished operation has a result the UI has not shown yet.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_has_result(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.last_summary().is_some()))
}

/// The result's message key, its counts, and the first failure.
///
/// # Safety
/// See [`jtf_app_free`]; the out pointers must be writable or null.
#[no_mangle]
pub unsafe extern "C" fn jtf_op_result(
    app: *const App,
    key_buf: *mut c_char,
    key_len: c_int,
    error_buf: *mut c_char,
    error_len: c_int,
    succeeded: *mut c_int,
    skipped: *mut c_int,
    failed: *mut c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(summary) = app.last_summary() else {
        return 0;
    };

    unsafe { write_str(summary.key, key_buf, key_len) };
    // The message key and the failing entry together: "permission denied"
    // without a path is not a report a user can act on.
    let detail = match (&summary.first_error_key, &summary.first_error_path) {
        (Some(key), Some(path)) => format!("{key}\t{}", path.display()),
        (Some(key), None) => (*key).to_string(),
        _ => String::new(),
    };
    unsafe { write_str(&detail, error_buf, error_len) };

    // SAFETY: caller contract; each pointer is checked before writing.
    unsafe {
        if !succeeded.is_null() {
            *succeeded = c_int::try_from(summary.succeeded).unwrap_or(0);
        }
        if !skipped.is_null() {
            *skipped = c_int::try_from(summary.skipped).unwrap_or(0);
        }
        if !failed.is_null() {
            *failed = c_int::try_from(summary.failed).unwrap_or(0);
        }
    }
    1
}

/// Discard the result once it has been shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_clear_result(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.take_summary();
    }
}

/// The shortcut bound to a command id, as a `QKeySequence` string.
///
/// Empty when the command is unbound.
///
/// # Safety
/// See [`jtf_app_free`]; `command` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_shortcut_for(
    app: *const App,
    command: *const c_char,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(command) = (unsafe { read_str(command) }) else {
        return 0;
    };
    unsafe { write_str(&app.shortcut_for(command), buf, len) }
}

/// Whether a command id is registered.
///
/// # Safety
/// See [`jtf_app_free`]; `command` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_has_command(app: *const App, command: *const c_char) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(command) = (unsafe { read_str(command) }) else {
        return 0;
    };
    c_int::from(app.has_command(command))
}

/// Name of the active keymap preset.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_keymap_name(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.keymap_name(), buf, len) }
}

/// Switch keymap preset.
///
/// # Safety
/// See [`jtf_app_free`]; `name` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_keymap(app: *mut App, name: *const c_char) {
    let Some(name) = (unsafe { read_str(name) }) else {
        return;
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_keymap(name);
    }
}

// --------------------------------------------------------------- settings

/// How many commands the registry holds.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_command_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.command_count()).unwrap_or(0))
}

/// One command's id, label key and category key, for a settings list.
///
/// # Safety
/// See [`jtf_app_free`]; the buffers must be writable or null.
#[no_mangle]
pub unsafe extern "C" fn jtf_command_at(
    app: *const App,
    index: c_int,
    id_buf: *mut c_char,
    id_len: c_int,
    label_buf: *mut c_char,
    label_len: c_int,
    category_buf: *mut c_char,
    category_len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some((id, label, category)) = app.command_at(usize::try_from(index).unwrap_or(0)) else {
        return 0;
    };
    unsafe { write_str(&id, id_buf, id_len) };
    unsafe { write_str(label, label_buf, label_len) };
    unsafe { write_str(category, category_buf, category_len) };
    1
}

/// Whether a command can destroy data.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_command_is_destructive(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.command_is_destructive(usize::try_from(index).unwrap_or(0)))
    })
}

/// Bind a chord to a command.
///
/// Returns 1 on success. On a conflict returns 0 and writes the id of the
/// command that already owns the chord into `conflict_buf`, so the UI can name
/// it rather than merely refusing.
///
/// # Safety
/// See [`jtf_app_free`]; `command` and `chord` must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_bind_shortcut(
    app: *mut App,
    command: *const c_char,
    chord: *const c_char,
    conflict_buf: *mut c_char,
    conflict_len: c_int,
) -> c_int {
    let Some(command) = (unsafe { read_str(command) }) else {
        return 0;
    };
    let Some(chord) = (unsafe { read_str(chord) }) else {
        return 0;
    };
    let Some(app) = (unsafe { app_mut(app) }) else {
        return 0;
    };

    match app.bind_shortcut(command, chord) {
        Ok(()) => 1,
        Err(conflict) => {
            unsafe { write_str(&conflict, conflict_buf, conflict_len) };
            0
        }
    }
}

/// Remove a command's shortcut.
///
/// # Safety
/// See [`jtf_app_free`]; `command` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_clear_shortcut(app: *mut App, command: *const c_char) {
    let Some(command) = (unsafe { read_str(command) }) else {
        return;
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.clear_shortcut(command);
    }
}

/// Discard every customisation and go back to the preset.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_reset_shortcuts(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.reset_shortcuts();
    }
}

/// Startup behaviour: 0 last session, 1 home, 2 a fixed location.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_startup_mode(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, App::startup_mode)
}

/// The fixed start location, when there is one.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_startup_location(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.startup_location(), buf, len) }
}

/// Set startup behaviour. Switching away from the last session erases it.
///
/// # Safety
/// See [`jtf_app_free`]; `location` must be null or a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_startup(app: *mut App, mode: c_int, location: *const c_char) {
    let location = unsafe { read_str(location) }.unwrap_or("");
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_startup(mode, location);
    }
}

/// Whether closed tabs are remembered between runs.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_remember_closed_tabs(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.remember_closed_tabs()))
}

/// Whether marks are remembered between runs.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_remember_marks(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.remember_marks()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_remember(app: *mut App, closed_tabs: c_int, marks: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_remember(closed_tabs != 0, marks != 0);
    }
}

/// List font family, empty for the platform's own fixed-width font.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_font_family(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.font().family, buf, len) }
}

/// Point size, or 0 for the platform default.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_font_point_size(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.font().point_size))
}

/// Whether the list uses a fixed-width font.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_font_monospace(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.font().monospace))
}

/// # Safety
/// See [`jtf_app_free`]; `family` must be null or a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_font(
    app: *mut App,
    family: *const c_char,
    point_size: c_int,
    monospace: c_int,
) {
    let family = unsafe { read_str(family) }.unwrap_or("");
    let size = u16::try_from(point_size.max(0)).unwrap_or(0);
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_font(family, size, monospace != 0);
    }
}

/// 0 system, 1 light, 2 dark.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_theme_mode(app: *mut App, mode: c_int) {
    let mode = match mode {
        1 => ThemeMode::Light,
        2 => ThemeMode::Dark,
        _ => ThemeMode::System,
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_theme_mode(mode);
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_theme_mode(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| match a.theme_mode() {
        ThemeMode::System => 0,
        ThemeMode::Light => 1,
        ThemeMode::Dark => 2,
    })
}

/// Resolve a semantic token to `0xAARRGGBB`.
///
/// Token numbering matches `ThemeToken::ALL`, and the C++ header states the
/// same order. This is the only channel through which a colour reaches the
/// UI: `AGENTS.md` §12 forbids literal colours in UI code, and
/// `docs/TESTING.md` §3.4 tests for them.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_theme_color(
    app: *const App,
    system_is_dark: c_int,
    token: c_int,
) -> u32 {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(&token) = ThemeToken::ALL.get(usize::try_from(token).unwrap_or(usize::MAX)) else {
        return 0;
    };
    app.theme_color(system_is_dark != 0, token)
}

/// How many tokens exist, so the C++ side can assert its header matches.
#[no_mangle]
pub extern "C" fn jtf_theme_token_count() -> c_int {
    c_int::try_from(ThemeToken::ALL.len()).unwrap_or(0)
}

/// Number of list columns.
#[no_mangle]
pub extern "C" fn jtf_column_count() -> c_int {
    crate::app::COLUMN_COUNT
}

/// Localization key for a column header.
///
/// # Safety
/// `buf` must be writable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_column_key(column: c_int, buf: *mut c_char, len: c_int) -> c_int {
    let key = match column {
        crate::app::COLUMN_SIZE => "column.size",
        crate::app::COLUMN_KIND => "column.kind",
        crate::app::COLUMN_MODIFIED => "column.modified",
        _ => "column.name",
    };
    unsafe { write_str(key, buf, len) }
}
