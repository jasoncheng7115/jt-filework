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

use crate::app::{App, CompareOutcome, MarkAction};
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
///
/// `system_locale` is the platform's ordered list of preferred languages,
/// comma-separated, used when the user has not chosen one. A list rather than
/// a single tag because macOS reports a single "locale" that mixes the region
/// with the language the application was launched in - on a Traditional
/// Chinese machine with a Taiwan region it says `en_TW`.
///
/// # Safety
/// `system_locale` must be null or a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_app_new(system_locale: *const c_char) -> *mut App {
    let tag = unsafe { read_str(system_locale) }.unwrap_or("");
    Box::into_raw(Box::new(App::new(tag)))
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

/// Point a pane at a folder on a host reached over SFTP.
///
/// The connection is opened lazily by the enumeration, so this returns
/// immediately and any failure - a refused host key, no accepted key, an
/// unreachable address - arrives as the pane's error the way a local failure
/// does.
///
/// # Safety
/// See [`jtf_app_free`]; the string arguments must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_navigate_remote(
    app: *mut App,
    pane_id: c_int,
    host: *const c_char,
    port: c_int,
    user: *const c_char,
    path: *const c_char,
) {
    let (Some(host), Some(user), Some(path)) = (
        unsafe { read_str(host) },
        unsafe { read_str(user) },
        unsafe { read_str(path) },
    ) else {
        return;
    };
    let port = u16::try_from(port).unwrap_or(22);
    if let Some(a) = unsafe { app_mut(app) } {
        a.navigate_to_location(
            pane(pane_id),
            jtf_core::Location::remote(host, port, user, path),
        );
    }
}

/// How many servers are saved.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_server_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.server_count()).unwrap_or(0))
}

/// The label for saved server `index`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_server_name(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(a) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let name = a.server_name(usize::try_from(index).unwrap_or(0));
    unsafe { write_str(&name, buf, len) }
}

/// Open saved server `index` in `pane`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_open_server(app: *mut App, pane_id: c_int, index: c_int) {
    let Some(a) = (unsafe { app_mut(app) }) else {
        return;
    };
    let Some((host, port, user, path)) = a.server_at(usize::try_from(index).unwrap_or(0)) else {
        return;
    };
    a.navigate_to_location(
        pane(pane_id),
        jtf_core::Location::remote(host, port, user, path),
    );
}

/// Remember a server, or update the one that matches host, port and user.
///
/// # Safety
/// See [`jtf_app_free`]; the string arguments must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_add_server(
    app: *mut App,
    host: *const c_char,
    port: c_int,
    user: *const c_char,
    path: *const c_char,
) {
    let (Some(host), Some(user), Some(path)) = (
        unsafe { read_str(host) },
        unsafe { read_str(user) },
        unsafe { read_str(path) },
    ) else {
        return;
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.add_server(host, u16::try_from(port).unwrap_or(22), user, path);
    }
}

/// Whether saved server `index` has a live connection.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_server_is_connected(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.server_is_connected(usize::try_from(index).unwrap_or(0)))
    })
}

/// Close the connection to saved server `index`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_disconnect_server(app: *const App, index: c_int) {
    if let Some(a) = unsafe { app_ref(app) } {
        a.disconnect_server(usize::try_from(index).unwrap_or(0));
    }
}

/// Forget saved server `index`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_remove_server(app: *mut App, index: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.remove_server(usize::try_from(index).unwrap_or(0));
    }
}

/// Whether the row lives on a server rather than on this machine.
///
/// The UI asks before offering anything that hands a path to the platform:
/// Quick Look, Reveal, Open With, a terminal. `/srv/data` on a server and
/// `/srv/data` here are different files, and the platform cannot tell.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_is_remote(app: *const App, pane_id: c_int, row: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.row_is_remote(pane(pane_id), usize::try_from(row).unwrap_or(0)))
    })
}

/// Whether the pane is showing a folder on a server.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pane_is_remote(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.pane_is_remote(pane(pane_id))))
}

/// Hand over a password for the next connection to this host.
///
/// Used once and dropped. It is never written to the session file, never kept
/// on the connection, and never logged: a server that only accepts passwords
/// is common, and remembering the password is a separate decision this program
/// does not make.
///
/// # Safety
/// See [`jtf_app_free`]; the string arguments must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_remote_set_password(
    app: *const App,
    host: *const c_char,
    port: c_int,
    user: *const c_char,
    password: *const c_char,
) {
    let (Some(host), Some(user), Some(password)) = (
        unsafe { read_str(host) },
        unsafe { read_str(user) },
        unsafe { read_str(password) },
    ) else {
        return;
    };
    if let Some(a) = unsafe { app_ref(app) } {
        a.set_remote_password(host, u16::try_from(port).unwrap_or(22), user, password);
    }
}

/// Record that the user accepted a host's key, so the next attempt writes it
/// to `known_hosts` instead of refusing.
///
/// # Safety
/// See [`jtf_app_free`]; the string arguments must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_remote_accept_host(
    app: *const App,
    host: *const c_char,
    port: c_int,
    user: *const c_char,
) {
    let (Some(host), Some(user)) = (unsafe { read_str(host) }, unsafe { read_str(user) }) else {
        return;
    };
    if let Some(a) = unsafe { app_ref(app) } {
        a.accept_remote_host(host, u16::try_from(port).unwrap_or(22), user);
    }
}

/// Close every remote connection.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_remote_disconnect(app: *const App) {
    if let Some(a) = unsafe { app_ref(app) } {
        a.disconnect_remote();
    }
}

/// Whether the list shows a `..` row.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_parent_row(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.parent_row()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_parent_row(app: *mut App, shown: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_parent_row(shown != 0);
    }
}

/// Where the preview panel sits: 0 beside the panes, 1 below them.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_inspector_position(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.inspector_position()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_inspector_position(app: *mut App, position: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_inspector_position(u8::try_from(position).unwrap_or(0));
    }
}

/// What the preview area is drawn on: 0 theme, 1 chequer, 2 a fixed colour.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_background(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.preview_background()))
}

/// The colour used when the mode is 2, as `#rrggbb`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_background_colour(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(a) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(a.preview_background_colour(), buf, len) }
}

/// # Safety
/// See [`jtf_app_free`]; `colour` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_preview_background(
    app: *mut App,
    mode: c_int,
    colour: *const c_char,
) {
    let text = unsafe { read_str(colour) }.unwrap_or("");
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_preview_background(u8::try_from(mode).unwrap_or(0), text);
    }
}

/// How much the key hint strip says: 0 full, 1 compact, 2 auto-hide.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_key_hints_density(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.key_hints_density()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_key_hints_density(app: *mut App, density: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_key_hints_density(u8::try_from(density).unwrap_or(0));
    }
}

/// The application's version, as the crate records it.
///
/// Read from `CARGO_PKG_VERSION` rather than repeated as a constant: a
/// version that has to be updated by hand is a version that is wrong.
///
/// # Safety
/// The one-off notice about how the last session was read, as a catalogue key.
///
/// Empty when there is nothing to say, and empty on every call after the
/// first: this is said once, at the launch it is about.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_session_notice(app: *mut App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(a) = (unsafe { app_mut(app) }) else {
        return 0;
    };
    let key = a.take_session_notice().unwrap_or("");
    unsafe { write_str(key, buf, len) }
}

/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_app_version(buf: *mut c_char, len: c_int) -> c_int {
    unsafe { write_str(env!("CARGO_PKG_VERSION"), buf, len) }
}

/// The id of the pane at `index` in visual order, or -1.
///
/// Panes are addressed by id everywhere else; this is how a caller that wants
/// to walk all of them gets those ids without inventing an index space.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pane_id_at(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(-1, |a| {
        usize::try_from(index)
            .ok()
            .and_then(|i| a.pane_ids().get(i).copied())
            .and_then(|id| c_int::try_from(id.get()).ok())
            .unwrap_or(-1)
    })
}

/// The pane a copy or a move would go to, or -1 when there is only one.
///
/// The UI has to be able to say where the files are about to land, and the
/// answer has to come from the same place the operation takes it from.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_target_pane(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(-1, |a| {
        a.target_pane()
            .and_then(|id| c_int::try_from(id.get()).ok())
            .unwrap_or(-1)
    })
}

/// Whether closing `pane` would succeed, so the UI can decide whether to
/// offer the control at all.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_can_close_pane(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.can_close_pane(pane(pane_id))))
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
pub unsafe extern "C" fn jtf_close_pane(app: *mut App, pane_id: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.close_pane(pane(pane_id))))
}

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
pub unsafe extern "C" fn jtf_toggle_tab_pinned(
    app: *mut App,
    pane_id: c_int,
    index: c_int,
) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.toggle_tab_pinned(pane(pane_id), usize::try_from(index).unwrap_or(0)))
    })
}

/// Whether the tab at `index` is pinned.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tab_is_pinned(app: *const App, pane_id: c_int, index: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.tab_is_pinned(pane(pane_id), usize::try_from(index).unwrap_or(0)))
    })
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_duplicate_tab(app: *mut App, pane_id: c_int, index: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.duplicate_tab(pane(pane_id), usize::try_from(index).unwrap_or(0));
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

/// The pane's folder as text to show a person, which for a server is
/// `sftp://user@host/path` rather than the empty string `jtf_current_path`
/// correctly returns for it.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_display_path(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let path = app.display_path(pane(pane_id));
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

// ------------------------------------------------------- where the space went

/// Start analysing what fills `path`. Returns 1 if it started.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 string. See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_start(app: *mut App, path: *const c_char) -> c_int {
    let Some(text) = (unsafe { read_str(path) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.start_usage(text)))
}

/// Take whatever the analysis thread has said. Returns 1 if anything changed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pump_usage(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.pump_usage()))
}

/// Whether the analysis has finished.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_is_done(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.usage_is_done()))
}

/// What is being analysed.
///
/// # Safety
/// The folder the running walk is in. Empty when it is not running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_in(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.usage_in(), buf, len) }
}

/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_root(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", App::usage_root);
    unsafe { write_str(text, buf, len) }
}

/// Running totals while the walk is going: 0 bytes, 1 files, 2 folders.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_progress(app: *const App, which: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| {
        let (bytes, files, folders) = a.usage_progress();
        match which {
            0 => bytes,
            1 => files,
            _ => folders,
        }
    })
}

/// Totals of the finished breakdown: 0 bytes, 1 files, 2 folders, 3 loose
/// bytes (files sitting in the root itself), 4 whether it is partial.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_total(app: *const App, which: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.usage().map_or(0, |usage| match which {
            0 => usage.bytes,
            1 => usage.files,
            2 => usage.folder_count,
            3 => usage.loose_bytes,
            _ => u64::from(usage.partial),
        })
    })
}

/// How many folder rows the breakdown has.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_folder_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.usage().map_or(0, |u| {
            c_int::try_from(u.folders.len()).unwrap_or(c_int::MAX)
        })
    })
}

/// A folder row's name.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_folder_name(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.folders.get(i)))
        })
        .map_or("", |folder| folder.name.as_str());
    unsafe { write_str(text, buf, len) }
}

/// A folder row's full path, so the window can go there.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_folder_path(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.folders.get(i)))
        })
        .map_or_else(String::new, |folder| folder.path.display().to_string());
    unsafe { write_str(&text, buf, len) }
}

/// A folder row's bytes (`which` 0) or file count (1).
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_folder_value(
    app: *const App,
    index: c_int,
    which: c_int,
) -> u64 {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.folders.get(i)))
        })
        .map_or(0, |folder| {
            if which == 0 {
                folder.bytes
            } else {
                folder.files
            }
        })
}

/// Whether an entry row is a folder. A file cannot be descended into.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_folder_is_directory(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.folders.get(i)))
        })
        .map_or(0, |folder| c_int::from(folder.is_directory))
}

/// How many kind rows the breakdown has.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_kind_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.usage()
            .map_or(0, |u| c_int::try_from(u.kinds.len()).unwrap_or(c_int::MAX))
    })
}

/// A kind row's extension, lowercased and without a dot. Empty means the
/// files had none.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_kind_extension(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.kinds.get(i)))
        })
        .map_or("", |kind| kind.extension.as_str());
    unsafe { write_str(text, buf, len) }
}

/// A kind row's catalogue key, so the window names the group in the user's
/// language rather than showing a bare extension.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_kind_group(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.kinds.get(i)))
        })
        .map_or("", |kind| kind.group_key);
    unsafe { write_str(text, buf, len) }
}

/// A kind row's bytes (`which` 0) or file count (1).
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_kind_value(app: *const App, index: c_int, which: c_int) -> u64 {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.usage().and_then(|u| u.kinds.get(i)))
        })
        .map_or(0, |kind| if which == 0 { kind.bytes } else { kind.files })
}

/// Stop a running analysis.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_cancel(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_usage();
    }
}

/// Stop the analysis and forget it.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_usage_close(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.close_usage();
    }
}

// ----------------------------------------------------- comparing two folders

/// Start comparing what two panes are showing. Returns 1 if it started.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_start(
    app: *mut App,
    left: c_int,
    right: c_int,
    recursive: c_int,
) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.start_compare(pane(left), pane(right), recursive != 0))
    })
}

/// Take whatever the comparison thread has said. Returns 1 if anything changed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pump_compare(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.pump_compare()))
}

/// Where the comparison has got to: -1 none, 0 running, 1 done, 2 failed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_state(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(-1, |a| match a.compare_outcome() {
        None => -1,
        Some(CompareOutcome::Running) => 0,
        Some(CompareOutcome::Done(_)) => 1,
        Some(CompareOutcome::Failed(_)) => 2,
    })
}

/// Why the comparison failed, if it did.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_error(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", |a| match a.compare_outcome() {
        Some(CompareOutcome::Failed(reason)) => reason.as_str(),
        _ => "",
    });
    unsafe { write_str(text, buf, len) }
}

/// How many rows the comparison produced.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.comparison()
            .map_or(0, |c| c_int::try_from(c.rows.len()).unwrap_or(c_int::MAX))
    })
}

/// How many of those rows are an actual difference.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_difference_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.comparison().map_or(0, |c| {
            c_int::try_from(c.difference_count()).unwrap_or(c_int::MAX)
        })
    })
}

/// Whether the walk was cut short by the row limit.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_truncated(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        a.comparison().map_or(0, |c| c_int::from(c.truncated))
    })
}

/// Whether the comparison walked subfolders.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_is_recursive(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.compare_is_recursive()))
}

/// The left folder's path, as shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_left(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", |a| a.compare_sides().0);
    unsafe { write_str(text, buf, len) }
}

/// The right folder's path, as shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_right(app: *const App, buf: *mut c_char, len: c_int) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", |a| a.compare_sides().1);
    unsafe { write_str(text, buf, len) }
}

/// A row's path, relative to both folders.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_path(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.comparison_row(i))
        })
        .map_or("", |row| row.relative.as_str());
    unsafe { write_str(text, buf, len) }
}

/// A row's verdict, as one of `only_left`, `only_right`, `differs`, `same`.
///
/// A name rather than a number: the interface looks the wording up by it, and
/// a number would have to be kept in step by hand at both ends.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_difference(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.comparison_row(i))
        })
        .map_or("", |row| row.difference.id());
    unsafe { write_str(text, buf, len) }
}

/// Whether a row is a folder on either side.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_is_directory(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.comparison_row(i))
        })
        .map_or(0, |row| c_int::from(row.is_directory()))
}

/// A row's size on one side, or -1 when that side has no such name.
///
/// `side` is 0 for left and 1 for right.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_size(app: *const App, index: c_int, side: c_int) -> i64 {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.comparison_row(i))
        })
        .and_then(|row| if side == 0 { row.left } else { row.right })
        .and_then(|s| s.size)
        .and_then(|size| i64::try_from(size).ok())
        .unwrap_or(-1)
}

/// A row's modification time on one side as a Unix timestamp, or 0.
///
/// `side` is 0 for left and 1 for right.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_time(app: *const App, index: c_int, side: c_int) -> i64 {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.comparison_row(i))
        })
        .and_then(|row| if side == 0 { row.left } else { row.right })
        .and_then(|s| s.modified)
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0)
}

/// Whether one side has this name at all.
///
/// `side` is 0 for left and 1 for right.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_row_has_side(
    app: *const App,
    index: c_int,
    side: c_int,
) -> c_int {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.comparison_row(i))
        })
        .map_or(0, |row| {
            c_int::from(if side == 0 {
                row.left.is_some()
            } else {
                row.right.is_some()
            })
        })
}

/// How many folders the running comparison has read.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_folders_done(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.compare_progress().0)
}

/// How many rows the running comparison has produced so far.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_rows_so_far(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.compare_progress().1)
}

/// How many differences the running comparison has found so far.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_differences_so_far(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.compare_progress().2)
}

/// Stop a running comparison, keeping what it has.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_cancel(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_compare();
    }
}

/// Stop the comparison and forget it.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_compare_close(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.close_compare();
    }
}

/// Open `path` in a window of its own. Returns the new window's id, or 0.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 string. See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_open_in_new_window(
    app: *mut App,
    pane_id: c_int,
    path: *const c_char,
) -> u64 {
    let Some(text) = (unsafe { read_str(path) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| a.open_in_new_window(pane(pane_id), text))
}

/// Bookmark `path`, or remove it if it is already bookmarked. Returns 1 if it
/// is bookmarked afterwards.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 string. See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_toggle_bookmark_path(app: *mut App, path: *const c_char) -> c_int {
    let Some(a) = (unsafe { app_mut(app) }) else {
        return 0;
    };
    let Some(text) = (unsafe { read_str(path) }) else {
        return 0;
    };
    c_int::from(a.toggle_bookmark_path(text))
}

/// Whether `path` is bookmarked.
///
/// # Safety
/// `path` must be a NUL-terminated UTF-8 string. See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_path_is_bookmarked(app: *const App, path: *const c_char) -> c_int {
    let Some(a) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(text) = (unsafe { read_str(path) }) else {
        return 0;
    };
    c_int::from(a.path_is_bookmarked(text))
}

/// Retry the pane's location from a fresh connection.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_reconnect(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.reconnect(pane(pane_id));
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

/// Start a search under the pane's current location.
///
/// Returns the localization key of a parse error, or an empty string on
/// success, so the UI can say what is wrong with the query rather than only
/// that it failed.
///
/// # Safety
/// See [`jtf_app_free`]; `query` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_search_start(
    app: *mut App,
    pane_id: c_int,
    query: *const c_char,
    error_buf: *mut c_char,
    error_len: c_int,
) -> c_int {
    let Some(query) = (unsafe { read_str(query) }) else {
        return 0;
    };
    let Some(app) = (unsafe { app_mut(app) }) else {
        return 0;
    };
    let key = app.start_search(pane(pane_id), query);
    unsafe { write_str(key, error_buf, error_len) };
    c_int::from(key.is_empty())
}

/// Whether the pane is showing search results.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_is_searching(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_searching(pane(pane_id))))
}

/// The query the pane's results came from.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_search_query(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.search_query(pane(pane_id)), buf, len) }
}

/// The folder a running search is currently in. Empty when none is running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_search_in(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(app.search_in(pane(pane_id)), buf, len) }
}

/// Abandon the results and show the directory again.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_search_clear(app: *mut App, pane_id: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.clear_search(pane(pane_id));
    }
}

/// The pane's filter text.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_filter(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.filter_text(pane(pane_id)), buf, len) }
}

/// Narrow the pane to entries whose name contains `text`.
///
/// # Safety
/// See [`jtf_app_free`]; `text` must be null or a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_filter(app: *mut App, pane_id: c_int, text: *const c_char) {
    let text = unsafe { read_str(text) }.unwrap_or("");
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_filter(pane(pane_id), text);
    }
}

/// How many entries the directory has before filtering.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_unfiltered_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.unfiltered_count(pane(pane_id))).unwrap_or(0)
    })
}

/// Whether the pane can go back, forward or up.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_can_go_back(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.can_go_back(pane(pane_id))))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_can_go_forward(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.can_go_forward(pane(pane_id))))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_can_go_up(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.can_go_up(pane(pane_id))))
}

/// How many entries are selected.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_selection_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.selection_count(pane(pane_id))).unwrap_or(0)
    })
}

/// The pane's folder name, for the window title.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_current_name(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.current_name(pane(pane_id)), buf, len) }
}

/// Whether a column is shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_column_visible(
    app: *const App,
    pane_id: c_int,
    column: c_int,
) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.column_visible(pane(pane_id), column)))
}

/// Show or hide a column.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_column_visible(
    app: *mut App,
    pane_id: c_int,
    column: c_int,
    visible: c_int,
) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_column_visible(pane(pane_id), column, visible != 0);
    }
}

/// Which column a pane is sorted by.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_sort_column(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| a.sort_column(pane(pane_id)))
}

/// Whether a pane's sort is ascending.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_sort_ascending(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.sort_ascending(pane(pane_id))))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_is_loading(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_loading(pane(pane_id))))
}

/// What the pane's error said beyond its category, empty if there is none.
///
/// Not localized: it names hosts, key fingerprints and what the server
/// replied, which are the parts a person needs verbatim to act on.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_error_detail(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let detail = app.error_detail(pane(pane_id));
    unsafe { write_str(&detail, buf, len) }
}

/// Give the pane's server a password and list it again.
///
/// # Safety
/// See [`jtf_app_free`]; `password` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_pane_set_password(
    app: *mut App,
    pane_id: c_int,
    password: *const c_char,
) -> c_int {
    let Some(password) = (unsafe { read_str(password) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.set_pane_password(pane(pane_id), password))
    })
}

/// Whether the pane failed because it could not sign in to the server.
///
/// The interface offers to ask for a password when this is true, and does not
/// when the failure was an ordinary permission error a password cannot fix.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pane_needs_credentials(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.pane_needs_credentials(pane(pane_id))))
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

/// The user's stored language choice, empty when following the system.
///
/// Distinct from [`jtf_locale`], which reports what is actually being shown.
/// The two differ exactly when following the system, which is the case the
/// settings screen needs to be able to display.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_locale_preference(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or_else(String::new, App::locale_preference);
    unsafe { write_str(&text, buf, len) }
}

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

/// Build a plan whose destination is a folder the user named.
///
/// Same result codes as [`jtf_op_prepare`]. `destination` must be an existing
/// directory; the caller is offering a chooser, and a plan against a path that
/// is not there would fail later with a worse message.
///
/// # Safety
/// See [`jtf_app_free`]; `destination` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_to(
    app: *mut App,
    pane_id: c_int,
    kind: c_int,
    destination: *const c_char,
) -> c_int {
    let Some(path) = (unsafe { read_str(destination) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.prepare_operation_to(pane(pane_id), OperationKind::from_code(kind), path))
    })
}

/// One tab's folder, so the UI can list every open tab as a destination.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tab_path(
    app: *const App,
    pane_id: c_int,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let path = app.tab_path(pane(pane_id), usize::try_from(index).unwrap_or(0));
    unsafe { write_str(&path, buf, len) }
}

/// Recompute the batch-rename preview. Returns how many rows it has.
///
/// # Safety
/// See [`jtf_app_free`]; the string arguments must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_batch_preview(
    app: *mut App,
    pane_id: c_int,
    template: *const c_char,
    find: *const c_char,
    replace: *const c_char,
    regex: c_int,
    start: c_int,
) -> c_int {
    let template = unsafe { read_str(template) }.unwrap_or("");
    let find = unsafe { read_str(find) }.unwrap_or("");
    let replace = unsafe { read_str(replace) }.unwrap_or("");
    let start = u64::try_from(start.max(0)).unwrap_or(1);

    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::try_from(a.preview_batch(pane(pane_id), template, find, replace, regex != 0, start))
            .unwrap_or(0)
    })
}

/// One preview row: the old name, the new name and an issue key.
///
/// # Safety
/// See [`jtf_app_free`]; the buffers must be writable or null.
#[no_mangle]
pub unsafe extern "C" fn jtf_batch_row(
    app: *const App,
    index: c_int,
    from_buf: *mut c_char,
    from_len: c_int,
    to_buf: *mut c_char,
    to_len: c_int,
    issue_buf: *mut c_char,
    issue_len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some((from, to, issue)) = app.batch_row(usize::try_from(index).unwrap_or(0)) else {
        return 0;
    };
    unsafe {
        write_str(&from, from_buf, from_len);
        write_str(&to, to_buf, to_len);
        write_str(issue, issue_buf, issue_len);
    }
    1
}

/// Whether the preview can be applied, and how many rows would change.
///
/// # Safety
/// See [`jtf_app_free`]; `changes` must be writable or null.
#[no_mangle]
pub unsafe extern "C" fn jtf_batch_can_apply(app: *const App, changes: *mut c_int) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let (can, count) = app.batch_state();
    // SAFETY: caller contract; checked before writing.
    unsafe {
        if !changes.is_null() {
            *changes = c_int::try_from(count).unwrap_or(0);
        }
    }
    c_int::from(can)
}

/// Apply the preview. Returns how many entries were renamed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_batch_apply(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::try_from(a.apply_batch()).unwrap_or(0))
}

/// Discard the preview.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_batch_clear(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.clear_batch();
    }
}

/// Child directories of a path, newline-separated.
///
/// # Safety
/// See [`jtf_app_free`]; `path` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_child_directories(
    app: *const App,
    path: *const c_char,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let Some(path) = (unsafe { read_str(path) }) else {
        return 0;
    };
    unsafe { write_str(&app.child_directories(path), buf, len) }
}

/// Whether folders sort ahead of files.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_folders_first(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.folders_first()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_folders_first(app: *mut App, folders_first: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_folders_first(folders_first != 0);
    }
}

/// Whether the folder tree is shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tree_visible(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.tree_state().0))
}

/// Its remembered width, or 0 for the default.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tree_width(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.tree_state().1))
}

/// Remember the folder tree's state.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_tree_state(app: *mut App, visible: c_int, width: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_tree_state(visible != 0, u16::try_from(width.max(0)).unwrap_or(0));
    }
}

/// Sidebar sections the user has folded away, one id per line.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_collapsed_sections(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let text = app.collapsed_sections();
    unsafe { write_str(&text, buf, len) }
}

/// # Safety
/// See [`jtf_app_free`]; `ids` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_collapsed_sections(app: *mut App, ids: *const c_char) {
    let Some(ids) = (unsafe { read_str(ids) }) else {
        return;
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_collapsed_sections(ids);
    }
}

/// How many recent folders the sidebar shows. 0 means the default.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_recent_limit(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.recent_limit()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_recent_limit(app: *mut App, limit: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_recent_limit(u16::try_from(limit.max(0)).unwrap_or(0));
    }
}

/// Prepare creating an empty file.
///
/// # Safety
/// See [`jtf_app_free`]; `name` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_new_file(
    app: *mut App,
    pane_id: c_int,
    name: *const c_char,
) -> c_int {
    let Some(name) = (unsafe { read_str(name) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.prepare_new_file(pane(pane_id), name)))
}

/// Install the platform's "move to trash".
///
/// The callback is given a NUL-terminated path and writes where the item went
/// into `buf`, returning its length, or 0 when the platform declined. Called
/// once at startup by the Qt layer, which is where the platform code lives.
///
/// # Safety
/// See [`jtf_app_free`]. `callback` must remain valid for the process's
/// lifetime, which it does: it is a plain function.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_native_trash(
    callback: Option<extern "C" fn(*const c_char, *mut c_char, c_int) -> c_int>,
) {
    let Some(callback) = callback else {
        return;
    };
    // Stored rather than closed over, because the hook jtf-ops takes is a
    // plain fn pointer - it has no state and must not capture any.
    let _ = NATIVE_TRASH_CALLBACK.set(callback);
    jtf_ops::set_native_trash(native_trash_bridge);
}

static NATIVE_TRASH_CALLBACK: std::sync::OnceLock<
    extern "C" fn(*const c_char, *mut c_char, c_int) -> c_int,
> = std::sync::OnceLock::new();

/// Calls the installed C callback, converting at the boundary.
fn native_trash_bridge(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let callback = NATIVE_TRASH_CALLBACK.get()?;
    let source = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut buffer = vec![0_u8; 4096];
    // The callback writes into our buffer; nothing crosses the boundary
    // owning memory, which is the rule for every other call here too.
    let written = callback(
        source.as_ptr(),
        buffer.as_mut_ptr().cast::<c_char>(),
        c_int::try_from(buffer.len()).unwrap_or(0),
    );
    if written <= 0 {
        return None;
    }
    let len = usize::try_from(written).ok()?.min(buffer.len());
    let text = String::from_utf8(buffer[..len].to_vec()).ok()?;
    Some(std::path::PathBuf::from(text))
}

/// How many operations are waiting behind the running one.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_queued(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.queued_count()).unwrap_or(0))
}

/// How many jobs there are: the running one, plus those waiting.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_job_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.job_count()).unwrap_or(0))
}

/// The localization key for job `index`'s label.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_job_label_key(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(a) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let key = a.job_label_key(usize::try_from(index).unwrap_or(0));
    unsafe { write_str(key, buf, len) }
}

/// Entries in job `index`, or 0 for the running one, whose count is reported
/// by the progress functions instead.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_job_entries(app: *const App, index: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.job_size(usize::try_from(index).unwrap_or(0)).0)
}

/// Bytes in job `index`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_job_bytes(app: *const App, index: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.job_size(usize::try_from(index).unwrap_or(0)).1)
}

/// Whether job `index` is the one running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_job_is_running(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(a.job_is_running(usize::try_from(index).unwrap_or(0)))
    })
}

/// Cancel the running job, or drop a waiting one.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_job_cancel(app: *mut App, index: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_job(usize::try_from(index).unwrap_or(0));
    }
}

/// Drop everything waiting. The running operation is untouched.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_clear_queue(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.clear_queue();
    }
}

/// Prepare setting or clearing read-only on the pane's targets.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_read_only(
    app: *mut App,
    pane_id: c_int,
    read_only: c_int,
) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.prepare_set_read_only(pane(pane_id), read_only != 0))
    })
}

/// Whether every target is already read-only.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_targets_read_only(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.targets_are_read_only(pane(pane_id))))
}

/// The pane's view mode: 0 list, 1 grid.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_view_mode(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| a.view_mode(pane(pane_id)))
}

/// Switch the pane between the list and the grid.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_view_mode(app: *mut App, pane_id: c_int, grid: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_view_mode(pane(pane_id), grid != 0);
    }
}

/// Whether image files show a thumbnail.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_thumbnails(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.thumbnails()))
}

/// Turn thumbnails on or off.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_thumbnails(app: *mut App, on: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_thumbnails(on != 0);
    }
}

/// Whether the key hint strip is shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_key_hints_visible(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.key_hints_visible()))
}

/// Remember the key hint strip's state.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_key_hints_visible(app: *mut App, visible: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_key_hints_visible(visible != 0);
    }
}

/// Whether the inspector panel is shown.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_inspector_visible(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.inspector_state().0))
}

/// Its remembered width, or 0 for the default.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_inspector_width(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.inspector_state().1))
}

/// Remember the inspector's state.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_inspector_state(app: *mut App, visible: c_int, width: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_inspector_state(visible != 0, u16::try_from(width.max(0)).unwrap_or(0));
    }
}

/// Open `path` for the inspector preview. Returns whether it is text.
///
/// # Safety
/// See [`jtf_app_free`]; `path` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_open(app: *mut App, path: *const c_char) -> c_int {
    let Some(path) = (unsafe { read_str(path) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.preview_open(path)))
}

/// Collect a finished plan: 1 ready, 0 failed, -1 still counting.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_plan_poll(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, App::poll_planning)
}

/// Whether a plan is still being built.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_plan_running(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_planning()))
}

/// Stop counting and discard the half-built plan.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_plan_cancel(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_planning();
    }
}

/// How many top-level windows the workspace has.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_window_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.window_ids().len()).unwrap_or(0))
}

/// The id of window `index`, or 0.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_window_id_at(app: *const App, index: c_int) -> u64 {
    unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.window_ids().get(i).copied())
        })
        .unwrap_or(0)
}

/// The layout tree of one window, as JSON. Empty when it does not exist.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_window_layout_json(
    app: *const App,
    window: u64,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or_else(String::new, |a| a.layout_json_for(window));
    unsafe { write_str(&text, buf, len) }
}

/// Move a tab into a window of its own. Returns the new window id, or 0.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_tear_off_tab(app: *mut App, pane_id: c_int, tab_index: c_int) -> u64 {
    let Ok(index) = usize::try_from(tab_index) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| a.tear_off_tab(pane(pane_id), index))
}

/// Move a tab from one pane into another, possibly in another window.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_merge_tab_into(
    app: *mut App,
    from: c_int,
    tab_index: c_int,
    into: c_int,
) -> c_int {
    let Ok(index) = usize::try_from(tab_index) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.merge_tab_into(pane(from), index, pane(into)))
    })
}

/// The archive at `path` as display lines. Empty when it is not an archive.
///
/// # Safety
/// See [`jtf_app_free`]; `path` must be a valid C string and `buf` must have
/// room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_listing(
    app: *const App,
    path: *const c_char,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(path) = (unsafe { read_str(path) }) else {
        return 0;
    };
    // The listing does not depend on application state; the app pointer is
    // still checked so a null one is a no-op rather than a read of nothing.
    let text = unsafe { app_ref(app) }.map_or_else(String::new, |_| App::archive_listing(path));
    unsafe { write_str(&text, buf, len) }
}

/// Read an archive's listing for the archive window. Returns the entry count.
///
/// # Safety
/// See [`jtf_app_free`]; `path` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_open_archive_listing(app: *mut App, path: *const c_char) -> c_int {
    let Some(path) = (unsafe { read_str(path) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::try_from(a.open_archive_listing(path)).unwrap_or(0)
    })
}

/// One listed entry's stored name.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_entry_name(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let name = usize::try_from(index)
        .ok()
        .and_then(|i| app.archive_entry(i))
        .map_or("", |entry| entry.0);
    unsafe { write_str(name, buf, len) }
}

/// Its uncompressed size.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_entry_size(app: *const App, index: c_int) -> u64 {
    unsafe { app_ref(app) }
        .and_then(|a| usize::try_from(index).ok().and_then(|i| a.archive_entry(i)))
        .map_or(0, |entry| entry.1)
}

/// Whether it is a directory entry.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_entry_is_directory(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }
        .and_then(|a| usize::try_from(index).ok().and_then(|i| a.archive_entry(i)))
        .map_or(0, |entry| c_int::from(entry.2))
}

/// Whether extracting it would land outside the chosen folder.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_entry_is_unsafe(app: *const App, index: c_int) -> c_int {
    unsafe { app_ref(app) }
        .and_then(|a| usize::try_from(index).ok().and_then(|i| a.archive_entry(i)))
        .map_or(0, |entry| c_int::from(entry.3))
}

/// Forget the listing.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_close_archive_listing(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.close_archive_listing();
    }
}

/// Release the preview's file handle.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_close(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.preview_close();
    }
}

/// How many lines the previewed file has.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_line_count(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, App::preview_line_count)
}

/// One decoded line of the preview.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_row(
    app: *mut App,
    row: u64,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_mut(app) }.map_or_else(String::new, |a| a.preview_row(row));
    unsafe { write_str(&text, buf, len) }
}

/// The preview's encoding label key.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_encoding_key(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let key = unsafe { app_ref(app) }.map_or("", |a| a.preview_status().0);
    unsafe { write_str(key, buf, len) }
}

/// The preview's line-ending label key.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_preview_line_ending_key(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let key = unsafe { app_ref(app) }.map_or("", |a| a.preview_status().1);
    unsafe { write_str(key, buf, len) }
}

/// The command a chord runs. Empty when nothing is bound.
///
/// # Safety
/// See [`jtf_app_free`]; `chord` must be a valid C string and `buf` must have
/// room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_command_for_chord(
    app: *const App,
    chord: *const c_char,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(chord) = (unsafe { read_str(chord) }) else {
        return 0;
    };
    let id = unsafe { app_ref(app) }.map_or_else(String::new, |a| a.command_for_chord(chord));
    unsafe { write_str(&id, buf, len) }
}

/// Switch to the other keyboard mode. Writes its name and returns the length.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_toggle_keymap(app: *mut App, buf: *mut c_char, len: c_int) -> c_int {
    let name = unsafe { app_mut(app) }.map_or_else(String::new, App::toggle_keymap);
    unsafe { write_str(&name, buf, len) }
}

/// Whether a bare printable key jumps to a file name in the active keymap.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_type_ahead(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(1, |a| c_int::from(a.type_ahead()))
}

/// Whether the row is an executable file.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_is_executable(
    app: *const App,
    pane_id: c_int,
    row: c_int,
) -> c_int {
    let Ok(row) = usize::try_from(row) else {
        return 0;
    };
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.row_is_executable(pane(pane_id), row)))
}

/// Whether the pane's row `row` is hidden by platform convention.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_is_hidden(app: *const App, pane_id: c_int, row: c_int) -> c_int {
    let Ok(row) = usize::try_from(row) else {
        return 0;
    };
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.row_is_hidden(pane(pane_id), row)))
}

/// Whether the pane's row `row` is the synthetic `..` row.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_row_is_parent(app: *const App, pane_id: c_int, row: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(row == 0 && a.has_parent_row(pane(pane_id)))
    })
}

/// Measure the folders among the pane's targets. Returns how many.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_start_extract(
    app: *mut App,
    pane_id: c_int,
    destination: *const c_char,
) -> c_int {
    let Some(destination) = (unsafe { read_str(destination) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.start_extract(pane(pane_id), destination))
    })
}

/// Extract `members` (newline-separated, empty for all) from `archive`.
///
/// # Safety
/// See [`jtf_app_free`]; all three strings must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn jtf_start_extract_from(
    app: *mut App,
    archive: *const c_char,
    destination: *const c_char,
    members: *const c_char,
) -> c_int {
    let (Some(archive), Some(destination)) = (unsafe { read_str(archive) }, unsafe {
        read_str(destination)
    }) else {
        return 0;
    };
    let wanted: Vec<String> = unsafe { read_str(members) }
        .unwrap_or("")
        .split('\n')
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .collect();
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.start_extract_from(archive, destination, wanted))
    })
}

/// # Safety
/// See [`jtf_app_free`]; `archive` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_start_compress(
    app: *mut App,
    pane_id: c_int,
    archive: *const c_char,
) -> c_int {
    let Some(archive) = (unsafe { read_str(archive) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.start_compress(pane(pane_id), archive)))
}

/// Mark exactly these rows, when the user has multi-selected them.
///
/// Fewer than two rows is ignored: moving the cursor is a selection of one.
///
/// # Safety
/// See [`jtf_app_free`]; `rows` must point to `count` valid ints.
#[no_mangle]
pub unsafe extern "C" fn jtf_mark_selected_rows(
    app: *mut App,
    pane_id: c_int,
    rows: *const c_int,
    count: c_int,
) {
    let Ok(count) = usize::try_from(count) else {
        return;
    };
    if rows.is_null() || count == 0 {
        return;
    }
    // SAFETY: caller contract - `rows` points to `count` ints.
    let slice = unsafe { std::slice::from_raw_parts(rows, count) };
    let rows: Vec<usize> = slice
        .iter()
        .filter_map(|r| usize::try_from(*r).ok())
        .collect();
    if let Some(a) = unsafe { app_mut(app) } {
        a.mark_selected_rows(pane(pane_id), &rows);
    }
}

/// Set the marks to exactly these rows. Selecting is marking.
///
/// # Safety
/// See [`jtf_app_free`]; `rows` must point to `count` valid ints.
#[no_mangle]
pub unsafe extern "C" fn jtf_set_marks_from_selection(
    app: *mut App,
    pane_id: c_int,
    rows: *const c_int,
    count: c_int,
) {
    let Ok(count) = usize::try_from(count) else {
        return;
    };
    let rows: Vec<usize> = if rows.is_null() || count == 0 {
        Vec::new()
    } else {
        // SAFETY: caller contract - `rows` points to `count` ints.
        let slice = unsafe { std::slice::from_raw_parts(rows, count) };
        slice
            .iter()
            .filter_map(|r| usize::try_from(*r).ok())
            .collect()
    };
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_marks_from_selection(pane(pane_id), &rows);
    }
}

/// How many rows are marked, and which, so the selection can be restored.
///
/// Writes up to `len` row numbers and returns how many there are in total.
///
/// # Safety
/// How many marks the pane's tab refused for want of room. Zero, normally.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_marks_refused(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.marks_refused(pane(pane_id))).unwrap_or(c_int::MAX)
    })
}

/// See [`jtf_app_free`]; `out` must have room for `len` ints.
#[no_mangle]
pub unsafe extern "C" fn jtf_marked_rows(
    app: *const App,
    pane_id: c_int,
    out: *mut c_int,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    let rows = app.marked_rows(pane(pane_id));
    let Ok(len) = usize::try_from(len) else {
        return 0;
    };
    if !out.is_null() {
        for (i, row) in rows.iter().take(len).enumerate() {
            // SAFETY: `i` is below `len`, and the caller promises that much room.
            unsafe { out.add(i).write(c_int::try_from(*row).unwrap_or(0)) };
        }
    }
    c_int::try_from(rows.len()).unwrap_or(0)
}

/// Tell the core which row the cursor is on. A negative row means none.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_current_row(app: *mut App, pane_id: c_int, row: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_current_row(pane(pane_id), usize::try_from(row).ok());
    }
}

/// Whether the cursor is on an archive this build can extract.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_cursor_is_archive(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.cursor_is_archive(pane(pane_id))))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pump_archive(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.pump_archive()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_is_archiving(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_archiving()))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_files(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.archive_progress().0)
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_bytes(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.archive_progress().1)
}

/// How many members were refused for trying to escape the destination.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_refused(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.archive_progress().2)
}

/// Whether the job in flight is a compression rather than an extraction.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_archive_is_compressing(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.archive_progress().3))
}

/// Once finished: 1 and an empty `buf` on success, 1 and the reason on
/// failure, 0 while still running. Clears the job.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_take_archive_result(
    app: *mut App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_mut(app) }) else {
        return 0;
    };
    match app.take_archive_result() {
        // Still running, and the same answer either way: nothing to report
        // yet. `take_archive_result` already returns `None` while running, so
        // the second spelling is unreachable and named only for exhaustiveness.
        None | Some(crate::app::ArchiveOutcome::Running) => 0,
        Some(crate::app::ArchiveOutcome::Succeeded) => {
            unsafe { write_str("", buf, len) };
            1
        }
        Some(crate::app::ArchiveOutcome::Failed(reason)) => {
            unsafe { write_str(&reason, buf, len) };
            1
        }
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_cancel_archive(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_archive();
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pump_measure(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.pump_measure()))
}

/// Whether a folder measurement is running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_is_measuring(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_measuring()))
}

/// Files counted so far by the running measurement.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_measure_files(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.measure_progress().0)
}

/// Bytes counted so far by the running measurement.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_measure_bytes(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.measure_progress().1)
}

/// Stop the running measurement. What it already counted stays.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_cancel_measure(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_measure();
    }
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_measure_folder_sizes(app: *mut App, pane_id: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::try_from(a.measure_folder_sizes(pane(pane_id))).unwrap_or(0)
    })
}

/// Forget every folder measurement.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_clear_folder_sizes(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.clear_folder_sizes();
    }
}

/// The row the cursor should move to after a navigation, or -1.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_take_focus_row(app: *mut App, pane_id: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(-1, |a| {
        c_int::try_from(a.take_focus_row(pane(pane_id))).unwrap_or(-1)
    })
}

/// How many of the pane's rows are real entries, excluding any `..` row.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_listed_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.listed_count(pane(pane_id))).unwrap_or(0)
    })
}

/// How many of the pane's shown rows are folders.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_folder_count(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::try_from(a.folder_count(pane(pane_id))).unwrap_or(0)
    })
}

/// The size of the pane's shown files, folders excluded.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_visible_bytes(app: *const App, pane_id: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.visible_bytes(pane(pane_id)))
}

/// How many bookmarks there are.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_bookmark_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.bookmarks().len()).unwrap_or(0))
}

/// The name to show for bookmark `index`.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_bookmark_name(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.bookmarks().get(i))
        })
        .map_or_else(String::new, jtf_workspace::Bookmark::display_name);
    unsafe { write_str(&text, buf, len) }
}

/// Where bookmark `index` goes.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_bookmark_path(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.bookmarks().get(i))
        })
        .map_or_else(String::new, |b| b.path.display().to_string());
    unsafe { write_str(&text, buf, len) }
}

/// Whether the pane's folder is bookmarked.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_is_bookmarked(app: *const App, pane_id: c_int) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.is_bookmarked(pane(pane_id))))
}

/// Bookmark the pane's folder, or remove it. Returns the state afterwards.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_toggle_bookmark(app: *mut App, pane_id: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.toggle_bookmark(pane(pane_id))))
}

/// Remove bookmark `index`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_remove_bookmark(app: *mut App, index: c_int) {
    if let (Some(a), Ok(i)) = (unsafe { app_mut(app) }, usize::try_from(index)) {
        a.remove_bookmark(i);
    }
}

/// Rename bookmark `index`. An empty name restores the folder's own.
///
/// # Safety
/// See [`jtf_app_free`]; `name` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_rename_bookmark(app: *mut App, index: c_int, name: *const c_char) {
    let Some(name) = (unsafe { read_str(name) }) else {
        return;
    };
    if let (Some(a), Ok(i)) = (unsafe { app_mut(app) }, usize::try_from(index)) {
        a.rename_bookmark(i, name);
    }
}

/// Move bookmark `from` to `to`.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_move_bookmark(app: *mut App, from: c_int, to: c_int) {
    if let (Some(a), Ok(f), Ok(t)) = (
        unsafe { app_mut(app) },
        usize::try_from(from),
        usize::try_from(to),
    ) {
        a.move_bookmark(f, t);
    }
}

/// How many recent locations there are.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_recent_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.recent().len()).unwrap_or(0))
}

/// Recent location `index`, most recent first.
///
/// # Safety
/// See [`jtf_app_free`]; `buf` must have room for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_recent_path(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .and_then(|a| {
            usize::try_from(index)
                .ok()
                .and_then(|i| a.recent().get(i).cloned())
        })
        .unwrap_or_default();
    unsafe { write_str(&text, buf, len) }
}

/// Forget where the user has been. Bookmarks are untouched.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_clear_recent(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.clear_recent();
    }
}

/// Mark or unmark listed entries matching a wildcard. Returns how many.
///
/// # Safety
/// See [`jtf_app_free`]; `pattern` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_mark_pattern(
    app: *mut App,
    pane_id: c_int,
    pattern: *const c_char,
    mark: c_int,
) -> c_int {
    let Some(pattern) = (unsafe { read_str(pattern) }) else {
        return 0;
    };
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::try_from(a.mark_pattern(pane(pane_id), pattern, mark != 0)).unwrap_or(0)
    })
}

/// Total size of what an operation started here would act on.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_target_size(app: *const App, pane_id: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.target_size(pane(pane_id)))
}

/// The paths an operation started in this pane would act on.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_target_paths(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.target_paths(pane(pane_id)), buf, len) }
}

/// The names of those entries.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_target_names(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.target_names(pane(pane_id)), buf, len) }
}

/// Plan a duplicate-in-place of the targeted entries.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_duplicate(app: *mut App, pane_id: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.prepare_duplicate(pane(pane_id))))
}

/// Build a plan over the newline-separated `sources`, into `destination`.
///
/// `destination` may be empty for a kind that does not need one (trash,
/// delete). Used by windows that are not panes - the disc usage report acts
/// on the row you are looking at, and there is no pane selection to read.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_prepare_paths(
    app: *mut App,
    kind: c_int,
    sources: *const c_char,
    destination: *const c_char,
) -> c_int {
    let Some(a) = (unsafe { app_mut(app) }) else {
        return 0;
    };
    let Some(list) = (unsafe { read_str(sources) }) else {
        return 0;
    };
    let paths: Vec<std::path::PathBuf> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(std::path::PathBuf::from)
        .collect();
    let into = unsafe { read_str(destination) }
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(std::path::PathBuf::from);
    c_int::from(a.prepare_paths(OperationKind::from_code(kind), paths, into))
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
/// Whether the pending plan takes things away - trash or delete.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_op_removes(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| {
        c_int::from(
            a.pending_plan()
                .is_some_and(|plan| plan.operation.removes()),
        )
    })
}

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

/// Whether there is anything to undo.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_can_undo(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.can_undo()))
}

/// Localization key naming what undo would reverse.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_undo_label_key(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(app.undo_label_key(), buf, len) }
}

/// Undo the most recent reversible operation.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_undo(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.undo_last()))
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

// ----------------------------------------------------------------- viewer

/// Open the focused row in the viewer.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_open(app: *mut App, pane_id: c_int, row: c_int) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| {
        c_int::from(a.open_viewer(pane(pane_id), usize::try_from(row).unwrap_or(0)))
    })
}

/// Close the viewer and release its file handle.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_close(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.close_viewer();
    }
}

/// Whether the viewer is showing text rather than hex.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_is_text(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.viewer_is_text()))
}

/// Switch between text and hex.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_toggle_hex(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.viewer_toggle_hex();
    }
}

/// How many rows the viewer has.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_row_count(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, App::viewer_row_count)
}

/// One rendered row.
///
/// The window is fetched per row rather than in bulk because the view only
/// ever asks for what it paints, and a bulk API would invite loading more.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_row(
    app: *mut App,
    row: u64,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_mut(app) }) else {
        return 0;
    };
    let rows = app.viewer_rows(row, 1);
    unsafe { write_str(rows.first().map_or("", String::as_str), buf, len) }
}

/// Set the text encoding, as an index into the encoding list.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_set_encoding(app: *mut App, index: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.viewer_set_encoding(usize::try_from(index).unwrap_or(0));
    }
}

/// The encoding in use, as an index.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_encoding(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.viewer_encoding()).unwrap_or(0))
}

/// How many encodings the list offers.
#[no_mangle]
pub extern "C" fn jtf_encoding_count() -> c_int {
    c_int::try_from(jtf_viewer::Encoding::ALL.len()).unwrap_or(0)
}

/// Localization key for one encoding.
///
/// # Safety
/// `buf` must be writable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_encoding_key(index: c_int, buf: *mut c_char, len: c_int) -> c_int {
    let key = jtf_viewer::Encoding::ALL
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .map_or("", |encoding| encoding.label_key());
    unsafe { write_str(key, buf, len) }
}

/// The viewer's status parts: path, kind key, size, encoding key, endings key.
///
/// # Safety
/// See [`jtf_app_free`]; the buffers must be writable or null.
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_status(
    app: *const App,
    path_buf: *mut c_char,
    path_len: c_int,
    kind_buf: *mut c_char,
    kind_len: c_int,
    encoding_buf: *mut c_char,
    encoding_len: c_int,
    endings_buf: *mut c_char,
    endings_len: c_int,
    size: *mut u64,
) {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return;
    };
    let (path, kind, bytes, encoding, endings) = app.viewer_status();
    unsafe {
        write_str(&path, path_buf, path_len);
        write_str(kind, kind_buf, kind_len);
        write_str(encoding, encoding_buf, encoding_len);
        write_str(endings, endings_buf, endings_len);
        if !size.is_null() {
            *size = bytes;
        }
    }
}

/// Find text from a row, wrapping. Returns the row, or -1.
///
/// # Safety
/// See [`jtf_app_free`]; `needle` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn jtf_viewer_find(
    app: *mut App,
    needle: *const c_char,
    from_row: u64,
) -> i64 {
    let Some(needle) = (unsafe { read_str(needle) }) else {
        return -1;
    };
    unsafe { app_mut(app) }
        .and_then(|a| a.viewer_find(needle, from_row))
        .and_then(|row| i64::try_from(row).ok())
        .unwrap_or(-1)
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

/// How many stored bindings named a command that no longer exists.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_dropped_bindings(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.dropped_bindings()).unwrap_or(0))
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

/// Whether the fixed-width font covers every column or only the aligned ones.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_font_monospace_everywhere(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.font().monospace_everywhere))
}

/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_font_monospace_everywhere(app: *mut App, everywhere: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_monospace_everywhere(everywhere != 0);
    }
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

/// The stable name of one token.
///
/// The count alone is not enough: reordering `ThemeToken::ALL` keeps the count
/// identical and silently recolours the entire interface. Checking names
/// against the C++ header turns that into a startup failure.
///
/// # Safety
/// `buf` must be writable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn jtf_theme_token_name(index: c_int, buf: *mut c_char, len: c_int) -> c_int {
    let name = ThemeToken::ALL
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .map_or("", |token| token.as_str());
    unsafe { write_str(name, buf, len) }
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
    // Straight from the model's own list, so a column added there appears in
    // the header and its menu without a second table to keep in step.
    let key = crate::app::column_at(column).map_or("column.name", |c| c.label_key());
    unsafe { write_str(key, buf, len) }
}

// ------------------------------------------------- writing an image to a disk

/// Ask the system what removable disks it has. Returns how many.
///
/// Zero is a normal answer and means nothing removable is attached. Only
/// removable, external disks that are not carrying the running system are ever
/// counted.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_devices_refresh(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::try_from(a.refresh_devices()).unwrap_or(0))
}

/// How many disks the last refresh found.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_count(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::try_from(a.device_count()).unwrap_or(0))
}

/// The node the write would go to.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_node(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .map(|a| a.device_node(usize::try_from(index).unwrap_or(0)))
        .unwrap_or_default();
    unsafe { write_str(&text, buf, len) }
}

/// What to show for the disk.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_name(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .map(|a| a.device_name(usize::try_from(index).unwrap_or(0)))
        .unwrap_or_default();
    unsafe { write_str(&text, buf, len) }
}

/// The disk's capacity in bytes.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_size(app: *const App, index: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.device_size(usize::try_from(index).unwrap_or(0)))
}

/// Localization key for how the disk is attached.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_bus_key(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", |a| a.device_bus_key(usize::try_from(index).unwrap_or(0)));
    unsafe { write_str(text, buf, len) }
}

/// The volumes mounted from the disk right now, comma separated.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_volumes(
    app: *const App,
    index: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }
        .map(|a| a.device_volumes(usize::try_from(index).unwrap_or(0)))
        .unwrap_or_default();
    unsafe { write_str(&text, buf, len) }
}

/// Why this disk cannot take this image, as a localization key. Empty if it
/// can.
///
/// # Safety
/// `image` must be a NUL-terminated UTF-8 string. See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_device_refusal_key(
    app: *const App,
    index: c_int,
    image: *const c_char,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(path) = (unsafe { read_str(image) }) else {
        return 0;
    };
    let text =
        unsafe { app_ref(app) }.map_or("", |a| a.device_refusal_key(usize::try_from(index).unwrap_or(0), path));
    unsafe { write_str(text, buf, len) }
}

/// Start writing `image` to the disk at `index`. Returns 1 if it started.
///
/// # Safety
/// `image` must be a NUL-terminated UTF-8 string. See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_start(
    app: *mut App,
    index: c_int,
    image: *const c_char,
    verify: c_int,
) -> c_int {
    let Some(path) = (unsafe { read_str(image) }) else {
        return 0;
    };
    unsafe { app_mut(app) }
        .map_or(0, |a| c_int::from(a.start_write(usize::try_from(index).unwrap_or(0), path, verify != 0)))
}

/// Take whatever the writing thread has said. Returns 1 if anything changed.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_pump_write(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.pump_write()))
}

/// Whether a write is running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_is_running(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.write_is_running()))
}

/// Whether a write has finished, successfully or not.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_is_done(app: *const App) -> c_int {
    unsafe { app_ref(app) }.map_or(0, |a| c_int::from(a.write_is_done()))
}

/// Localization key for the phase now running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_stage_key(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", App::write_stage_key);
    unsafe { write_str(text, buf, len) }
}

/// Progress in the current phase: `which` 0 for done, 1 for total.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_progress(app: *const App, which: c_int) -> u64 {
    unsafe { app_ref(app) }.map_or(0, |a| a.write_progress(which))
}

/// The disk being written to.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_target(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", App::write_target);
    unsafe { write_str(text, buf, len) }
}

/// Localization key for how it ended. Empty while it is still running.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_outcome_key(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", App::write_outcome_key);
    unsafe { write_str(text, buf, len) }
}

/// Developer-facing detail of a failure, for the log.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_failure_detail(
    app: *const App,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let text = unsafe { app_ref(app) }.map_or("", App::write_failure_detail);
    unsafe { write_str(text, buf, len) }
}

/// Bytes written, on success.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_bytes(app: *const App) -> u64 {
    unsafe { app_ref(app) }.map_or(0, App::write_bytes)
}

/// The image's CRC-32, on success.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_checksum(app: *const App) -> u32 {
    unsafe { app_ref(app) }.map_or(0, App::write_checksum)
}

/// Whether this platform needs a separately elevated process to write a disk.
#[no_mangle]
pub extern "C" fn jtf_write_needs_elevation() -> c_int {
    c_int::from(jtf_platform_devices::needs_elevation())
}

/// Whether this platform can write disks at all.
#[no_mangle]
pub extern "C" fn jtf_write_is_supported() -> c_int {
    c_int::from(jtf_platform_devices::is_supported())
}

/// Stop the write. The disk is left partly written.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_cancel(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.cancel_write();
    }
}

/// Forget the finished write.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_write_close(app: *mut App) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.close_write();
    }
}

/// Move the copy/move target to the next pane. Returns 1 if it moved.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_cycle_target_pane(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.cycle_target_pane()))
}

/// The name of the entry the cursor is on, ignoring marks.
///
/// What the rename box opens with. `jtf_target_names` answers with the marked
/// set, which is right for the operations that take any number and wrong for
/// the one that takes exactly one.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_cursor_name(
    app: *const App,
    pane_id: c_int,
    buf: *mut c_char,
    len: c_int,
) -> c_int {
    let Some(app) = (unsafe { app_ref(app) }) else {
        return 0;
    };
    unsafe { write_str(&app.cursor_name(pane(pane_id)), buf, len) }
}

/// Tell the core the machine's UTC offset in seconds east.
///
/// Called at startup and whenever the platform says the zone changed. Without
/// it every timestamp in the list is UTC.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_set_utc_offset(app: *mut App, seconds: c_int) {
    if let Some(a) = unsafe { app_mut(app) } {
        a.set_utc_offset(seconds);
    }
}

/// Re-list any pane whose folder has changed underneath it. Returns 1 if any
/// pane was re-read.
///
/// The caller decides *when* it is safe to ask: never while someone is typing
/// into a rename box, a filter or the path field, because re-listing under a
/// text field moves the thing being named.
///
/// # Safety
/// See [`jtf_app_free`].
#[no_mangle]
pub unsafe extern "C" fn jtf_poll_folders(app: *mut App) -> c_int {
    unsafe { app_mut(app) }.map_or(0, |a| c_int::from(a.poll_folders()))
}
