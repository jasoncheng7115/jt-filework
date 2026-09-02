// C ABI exposed by src/ui/qt6/bridge (Rust).
//
// AGENTS.md 4: no core logic lives on this side of the line. Everything below
// is a question asked of Rust, or an event forwarded to it. The C++ layer
// draws and forwards input; it decides nothing.
#pragma once

#include <cstdint>

extern "C" {

typedef struct JtfApp JtfApp;

JtfApp *jtf_app_new(const char *system_locale);
void jtf_app_free(JtfApp *app);
void jtf_app_save_session(const JtfApp *app);
int jtf_app_pump(JtfApp *app);

// layout
int jtf_layout_json(const JtfApp *app, char *buf, int len);

// Windows. A tab can be torn into one of its own and merged back.
int jtf_window_count(const JtfApp *app);
uint64_t jtf_window_id_at(const JtfApp *app, int index);
int jtf_window_layout_json(const JtfApp *app, uint64_t window, char *buf, int len);
uint64_t jtf_tear_off_tab(JtfApp *app, int pane, int tab_index);
int jtf_merge_tab_into(JtfApp *app, int from, int tab_index, int into);
// The application's version string.
int jtf_app_version(char *buf, int len);
int jtf_session_notice(JtfApp *app, char *buf, int len);
int jtf_pane_count(const JtfApp *app);
// The id of the pane at `index` in visual order, or -1.
int jtf_pane_id_at(const JtfApp *app, int index);
// Whether closing this pane would succeed - the same rule close_pane applies.
int jtf_can_close_pane(const JtfApp *app, int pane);
// The pane a copy or move would go to, or -1 when there is only one.
int jtf_target_pane(const JtfApp *app);
// Moves the copy/move target to the next pane. Returns 1 if it moved, which
// it does not with fewer than three panes.
int jtf_cycle_target_pane(JtfApp *app);
int jtf_active_pane(const JtfApp *app);
void jtf_focus_pane(JtfApp *app, int pane);
void jtf_focus_next_pane(JtfApp *app);
void jtf_split_active(JtfApp *app, int vertical);
int jtf_close_active_pane(JtfApp *app);
// Close one named pane. Closing a torn-off window's panes is what removes the
// window from the workspace, and so from the session.
int jtf_close_pane(JtfApp *app, int pane);
void jtf_apply_preset(JtfApp *app, int preset);

// tabs
int jtf_tab_count(const JtfApp *app, int pane);
int jtf_active_tab(const JtfApp *app, int pane);
int jtf_tab_title(const JtfApp *app, int pane, int index, char *buf, int len);
// One tab's folder, for offering every open tab as a destination.
int jtf_tab_path(const JtfApp *app, int pane, int index, char *buf, int len);
void jtf_new_tab(JtfApp *app);
// Copies the tab at `index`, including where it points, so a tab on a server
// duplicates to the same server.
void jtf_duplicate_tab(JtfApp *app, int pane, int index);
// Pinning. A pinned tab keeps a leading place in the strip, cannot be
// reordered out of it, and refuses to close without force - all modelled, and
// unreachable from the interface until these existed.
int jtf_toggle_tab_pinned(JtfApp *app, int pane, int index);
int jtf_tab_is_pinned(const JtfApp *app, int pane, int index);
void jtf_close_tab(JtfApp *app, int pane, int index);
void jtf_activate_tab(JtfApp *app, int pane, int index);

// navigation
int jtf_current_path(const JtfApp *app, int pane, char *buf, int len);
// What to show: a local path, or `sftp://user@host/path` for a server.
// `jtf_current_path` is empty for a server, which is right for bookmarks and
// wrong for anything a person reads.
int jtf_display_path(const JtfApp *app, int pane, char *buf, int len);
void jtf_navigate(JtfApp *app, int pane, const char *path);
void jtf_navigate_up(JtfApp *app, int pane);
void jtf_go_back(JtfApp *app, int pane);
void jtf_go_forward(JtfApp *app, int pane);
int jtf_open_row(JtfApp *app, int pane, int row);

// rows
uint64_t jtf_row_generation(const JtfApp *app, int pane);
int jtf_row_count(const JtfApp *app, int pane);
int jtf_row_text(const JtfApp *app, int pane, int row, int column, char *buf, int len);
int jtf_row_path(const JtfApp *app, int pane, int row, char *buf, int len);
int jtf_row_is_directory(const JtfApp *app, int pane, int row);
int jtf_row_is_executable(const JtfApp *app, int pane, int row);
// Hidden by platform convention. Drawn dimmer, as CView and WinCV both do.
int jtf_row_is_hidden(const JtfApp *app, int pane, int row);
int jtf_row_is_marked(const JtfApp *app, int pane, int row);
void jtf_toggle_mark(JtfApp *app, int pane, int row);
void jtf_mark_listed(JtfApp *app, int pane, int action);
void jtf_refresh(JtfApp *app, int pane);
// Retry the pane's location from a fresh connection, dropping any the
// provider is holding: after a failure, a plain refresh looks at the same
// broken session again.
void jtf_reconnect(JtfApp *app, int pane);

// Comparing what two panes are showing. The walk runs on a worker thread, so
// the window keeps painting while two trees are read; jtf_pump_compare is
// polled the same way the archive job is.
int jtf_compare_start(JtfApp *app, int left, int right, int recursive);
int jtf_pump_compare(JtfApp *app);
// -1 none, 0 running, 1 done, 2 failed.
int jtf_compare_state(const JtfApp *app);
int jtf_compare_error(const JtfApp *app, char *buf, int len);
int jtf_compare_row_count(const JtfApp *app);
int jtf_compare_difference_count(const JtfApp *app);
int jtf_compare_truncated(const JtfApp *app);
int jtf_compare_is_recursive(const JtfApp *app);
int jtf_compare_left(const JtfApp *app, char *buf, int len);
int jtf_compare_right(const JtfApp *app, char *buf, int len);
int jtf_compare_row_path(const JtfApp *app, int index, char *buf, int len);
// One of `only_left`, `only_right`, `differs`, `same`.
int jtf_compare_row_difference(const JtfApp *app, int index, char *buf, int len);
int jtf_compare_row_is_directory(const JtfApp *app, int index);
// side: 0 left, 1 right. Size is -1 when that side has no such name.
long long jtf_compare_row_size(const JtfApp *app, int index, int side);
long long jtf_compare_row_time(const JtfApp *app, int index, int side);
int jtf_compare_row_has_side(const JtfApp *app, int index, int side);
// Running totals while the walk is going, so the window can say what is
// happening instead of sitting silent for the length of it.
uint64_t jtf_compare_folders_done(const JtfApp *app);
uint64_t jtf_compare_rows_so_far(const JtfApp *app);
uint64_t jtf_compare_differences_so_far(const JtfApp *app);
void jtf_compare_cancel(JtfApp *app);
void jtf_compare_close(JtfApp *app);

// Where the space went: which child folders hold the most, and which kinds of
// file. One walk answers both. It runs on a worker thread and is polled the
// same way the comparison is.
int jtf_usage_start(JtfApp *app, const char *path);
int jtf_pump_usage(JtfApp *app);
int jtf_usage_is_done(const JtfApp *app);
int jtf_usage_root(const JtfApp *app, char *buf, int len);
int jtf_usage_in(const JtfApp *app, char *buf, int len);
// which: 0 bytes, 1 files, 2 folders.
uint64_t jtf_usage_progress(const JtfApp *app, int which);
// which: 0 bytes, 1 files, 2 folders, 3 loose bytes, 4 partial.
uint64_t jtf_usage_total(const JtfApp *app, int which);
int jtf_usage_folder_count(const JtfApp *app);
int jtf_usage_folder_name(const JtfApp *app, int index, char *buf, int len);
int jtf_usage_folder_path(const JtfApp *app, int index, char *buf, int len);
// which: 0 bytes, 1 files.
uint64_t jtf_usage_folder_value(const JtfApp *app, int index, int which);
int jtf_usage_folder_is_directory(const JtfApp *app, int index);
int jtf_usage_kind_count(const JtfApp *app);
int jtf_usage_kind_extension(const JtfApp *app, int index, char *buf, int len);
int jtf_usage_kind_group(const JtfApp *app, int index, char *buf, int len);
uint64_t jtf_usage_kind_value(const JtfApp *app, int index, int which);
void jtf_usage_cancel(JtfApp *app);
void jtf_usage_close(JtfApp *app);

// Writing a disk image to a removable disk. The list is the safety mechanism:
// only removable, external disks that are not carrying the running system are
// ever counted, so an empty list means there is nothing safe to offer.
int jtf_devices_refresh(JtfApp *app);
int jtf_device_count(const JtfApp *app);
int jtf_device_node(const JtfApp *app, int index, char *buf, int len);
int jtf_device_name(const JtfApp *app, int index, char *buf, int len);
uint64_t jtf_device_size(const JtfApp *app, int index);
int jtf_device_bus_key(const JtfApp *app, int index, char *buf, int len);
int jtf_device_volumes(const JtfApp *app, int index, char *buf, int len);
int jtf_device_refusal_key(const JtfApp *app, int index, const char *image, char *buf, int len);
int jtf_write_start(JtfApp *app, int index, const char *image, int verify);
int jtf_pump_write(JtfApp *app);
int jtf_write_is_running(const JtfApp *app);
int jtf_write_is_done(const JtfApp *app);
int jtf_write_stage_key(const JtfApp *app, char *buf, int len);
uint64_t jtf_write_progress(const JtfApp *app, int which);
int jtf_write_target(const JtfApp *app, char *buf, int len);
int jtf_write_outcome_key(const JtfApp *app, char *buf, int len);
int jtf_write_failure_detail(const JtfApp *app, char *buf, int len);
uint64_t jtf_write_bytes(const JtfApp *app);
uint32_t jtf_write_checksum(const JtfApp *app);
int jtf_write_needs_elevation(void);
int jtf_write_is_supported(void);
void jtf_write_cancel(JtfApp *app);
void jtf_write_close(JtfApp *app);
int jtf_marked_count(const JtfApp *app, int pane);
void jtf_sort_by(JtfApp *app, int pane, int column);
int jtf_search_start(JtfApp *app, int pane, const char *query, char *error_buf, int error_len);
int jtf_is_searching(const JtfApp *app, int pane);
int jtf_search_query(const JtfApp *app, int pane, char *buf, int len);
int jtf_search_in(const JtfApp *app, int pane, char *buf, int len);
void jtf_search_clear(JtfApp *app, int pane);
int jtf_filter(const JtfApp *app, int pane, char *buf, int len);
void jtf_set_filter(JtfApp *app, int pane, const char *text);
int jtf_unfiltered_count(const JtfApp *app, int pane);
int jtf_can_go_back(const JtfApp *app, int pane);
int jtf_can_go_forward(const JtfApp *app, int pane);
int jtf_can_go_up(const JtfApp *app, int pane);
int jtf_selection_count(const JtfApp *app, int pane);
int jtf_current_name(const JtfApp *app, int pane, char *buf, int len);
int jtf_column_visible(const JtfApp *app, int pane, int column);
void jtf_set_column_visible(JtfApp *app, int pane, int column, int visible);
int jtf_sort_column(const JtfApp *app, int pane);
int jtf_sort_ascending(const JtfApp *app, int pane);
int jtf_is_loading(const JtfApp *app, int pane);
int jtf_error_key(const JtfApp *app, int pane, char *buf, int len);
// What the failure said beyond its category - which host, which key, what the
// server replied. Not localized: those parts are needed verbatim.
int jtf_error_detail(const JtfApp *app, int pane, char *buf, int len);
// Whether the pane failed because it could not sign in, as opposed to an
// ordinary permission error that no password would fix.
int jtf_pane_needs_credentials(const JtfApp *app, int pane);
// Give that server a password and list it again. Takes the pane, so the
// interface never has to take apart a displayed path to find the host.
int jtf_pane_set_password(JtfApp *app, int pane, const char *password);
void jtf_set_show_hidden(JtfApp *app, int show);
int jtf_show_hidden(const JtfApp *app);

// i18n and theme
void jtf_set_locale(JtfApp *app, const char *locale);
int jtf_locale_preference(const JtfApp *app, char *buf, int len);
int jtf_locale(const JtfApp *app, char *buf, int len);
int jtf_tr(const JtfApp *app, const char *key, char *buf, int len);
// operations
void jtf_set_selection(JtfApp *app, int pane, const int *rows, int count);
int jtf_op_prepare(JtfApp *app, int pane, int kind); // 0 copy 1 move 2 trash 3 delete
// The same, but into a folder the user chose rather than the next pane.
int jtf_op_prepare_to(JtfApp *app, int pane, int kind, const char *destination);
int jtf_batch_preview(JtfApp *app, int pane, const char *template_, const char *find,
                      const char *replace, int regex, int start);
int jtf_batch_row(const JtfApp *app, int index, char *from_buf, int from_len, char *to_buf,
                  int to_len, char *issue_buf, int issue_len);
int jtf_batch_can_apply(const JtfApp *app, int *changes);
int jtf_batch_apply(JtfApp *app);
void jtf_batch_clear(JtfApp *app);
int jtf_child_directories(const JtfApp *app, const char *path, char *buf, int len);
int jtf_folders_first(const JtfApp *app);
void jtf_set_folders_first(JtfApp *app, int folders_first);
int jtf_tree_visible(const JtfApp *app);
int jtf_tree_width(const JtfApp *app);
// The platform's own move-to-trash, installed once at startup.
typedef int (*JtfNativeTrash)(const char *path, char *buf, int len);
void jtf_set_native_trash(JtfNativeTrash callback);

int jtf_view_mode(const JtfApp *app, int pane);
void jtf_set_view_mode(JtfApp *app, int pane, int grid);
int jtf_thumbnails(const JtfApp *app);
void jtf_set_thumbnails(JtfApp *app, int on);
int jtf_key_hints_visible(const JtfApp *app);
void jtf_set_key_hints_visible(JtfApp *app, int visible);
// How much the strip says: 0 full, 1 compact, 2 auto-hide.
// The preview area's background: 0 theme, 1 chequer, 2 a fixed colour.
// Where the preview panel sits: 0 beside the panes, 1 below them.
// Whether the list shows a `..` row. Off by default.
// SFTP (docs/adr/0004-sftp.md). Failures arrive as the pane's error, the same
// way a local one does.
void jtf_navigate_remote(JtfApp *app, int pane, const char *host, int port, const char *user,
                         const char *path);
void jtf_remote_accept_host(const JtfApp *app, const char *host, int port, const char *user);
// Used for one connection and dropped. Never written anywhere.
void jtf_remote_set_password(const JtfApp *app, const char *host, int port, const char *user,
                             const char *password);
void jtf_remote_disconnect(const JtfApp *app);
// Whether a row or a pane lives on a server. Asked before offering anything
// that hands a path to the platform: the same path string means different
// files here and there.
// Saved servers. A host, a port and an account - never a credential.
int jtf_server_count(const JtfApp *app);
int jtf_server_name(const JtfApp *app, int index, char *buf, int len);
void jtf_open_server(JtfApp *app, int pane, int index);
void jtf_add_server(JtfApp *app, const char *host, int port, const char *user, const char *path);
void jtf_remove_server(JtfApp *app, int index);
// Whether a saved server has a live session, and how to end one.
int jtf_server_is_connected(const JtfApp *app, int index);
void jtf_disconnect_server(const JtfApp *app, int index);

int jtf_row_is_remote(const JtfApp *app, int pane, int row);
int jtf_pane_is_remote(const JtfApp *app, int pane);

int jtf_parent_row(const JtfApp *app);
void jtf_set_parent_row(JtfApp *app, int shown);
int jtf_inspector_position(const JtfApp *app);
void jtf_set_inspector_position(JtfApp *app, int position);
int jtf_preview_background(const JtfApp *app);
int jtf_preview_background_colour(const JtfApp *app, char *buf, int len);
void jtf_set_preview_background(JtfApp *app, int mode, const char *colour);
int jtf_key_hints_density(const JtfApp *app);
void jtf_set_key_hints_density(JtfApp *app, int density);
int jtf_inspector_visible(const JtfApp *app);
int jtf_inspector_width(const JtfApp *app);
void jtf_set_inspector_state(JtfApp *app, int visible, int width);

// The inspector's own read of a file, independent of the viewer window.
int jtf_preview_open(JtfApp *app, const char *path);
void jtf_preview_close(JtfApp *app);
int jtf_archive_listing(const JtfApp *app, const char *path, char *buf, int len);
// The archive window's listing: read once, then asked about per row. Reading
// the central directory per visible row would re-parse the file on every
// scroll.
int jtf_open_archive_listing(JtfApp *app, const char *path);
int jtf_archive_entry_name(const JtfApp *app, int index, char *buf, int len);
unsigned long long jtf_archive_entry_size(const JtfApp *app, int index);
int jtf_archive_entry_is_directory(const JtfApp *app, int index);
int jtf_archive_entry_is_unsafe(const JtfApp *app, int index);
void jtf_close_archive_listing(JtfApp *app);
uint64_t jtf_preview_line_count(const JtfApp *app);
int jtf_preview_row(JtfApp *app, uint64_t row, char *buf, int len);
int jtf_preview_encoding_key(const JtfApp *app, char *buf, int len);
int jtf_preview_line_ending_key(const JtfApp *app, char *buf, int len);

int jtf_command_for_chord(const JtfApp *app, const char *chord, char *buf, int len);
int jtf_type_ahead(const JtfApp *app);

int jtf_measure_folder_sizes(JtfApp *app, int pane);
// Measuring runs on a worker thread, because walking a large tree on the UI
// thread freezes the window. These report and stop it.
int jtf_pump_measure(JtfApp *app);
// Archives (ADR-0003). Extraction and compression run on a worker thread for
// the same reason measuring does: a large archive would freeze the window.
int jtf_start_extract(JtfApp *app, int pane, const char *destination);
// Extract named members - newline-separated, empty for all - from one archive.
int jtf_start_extract_from(JtfApp *app, const char *archive, const char *destination,
                           const char *members);
int jtf_start_compress(JtfApp *app, int pane, const char *archive);
// Which row the cursor is on. `Tab::active_entry` is what answers "what is
// the cursor on"; nothing set it until this existed.
void jtf_set_current_row(JtfApp *app, int pane, int row);
// Selecting is marking: the marks become exactly the selected rows, and the
// selection is restored from the marks on arriving in a folder.
void jtf_set_marks_from_selection(JtfApp *app, int pane, const int *rows, int count);
int jtf_marked_rows(const JtfApp *app, int pane, int *out, int len);
int jtf_marks_refused(const JtfApp *app, int pane);
// Mark exactly these rows, for a deliberate multi-select. Fewer than two is
// ignored - moving the cursor is a selection of one.
void jtf_mark_selected_rows(JtfApp *app, int pane, const int *rows, int count);
int jtf_cursor_is_archive(const JtfApp *app, int pane);
int jtf_pump_archive(JtfApp *app);
int jtf_is_archiving(const JtfApp *app);
unsigned long long jtf_archive_files(const JtfApp *app);
unsigned long long jtf_archive_bytes(const JtfApp *app);
unsigned long long jtf_archive_refused(const JtfApp *app);
int jtf_archive_is_compressing(const JtfApp *app);
int jtf_take_archive_result(JtfApp *app, char *buf, int len);
void jtf_cancel_archive(JtfApp *app);
int jtf_is_measuring(const JtfApp *app);
unsigned long long jtf_measure_files(const JtfApp *app);
unsigned long long jtf_measure_bytes(const JtfApp *app);
void jtf_cancel_measure(JtfApp *app);
void jtf_clear_folder_sizes(JtfApp *app);
int jtf_row_is_parent(const JtfApp *app, int pane, int row);
int jtf_take_focus_row(JtfApp *app, int pane);
int jtf_listed_count(const JtfApp *app, int pane);
int jtf_folder_count(const JtfApp *app, int pane);
uint64_t jtf_visible_bytes(const JtfApp *app, int pane);

// Places: the sidebar's bookmarks and recent locations.
int jtf_bookmark_count(const JtfApp *app);
int jtf_bookmark_name(const JtfApp *app, int index, char *buf, int len);
int jtf_bookmark_path(const JtfApp *app, int index, char *buf, int len);
int jtf_is_bookmarked(const JtfApp *app, int pane);
int jtf_toggle_bookmark(JtfApp *app, int pane);
// By path, for the tab strip, the folder tree and a folder row in the list:
// each points at a folder that may not be the one its pane is showing.
// A window of its own showing `path`: a tab on that folder, torn off. Returns
// the new window's id, or 0.
uint64_t jtf_open_in_new_window(JtfApp *app, int pane, const char *path);
int jtf_toggle_bookmark_path(JtfApp *app, const char *path);
int jtf_path_is_bookmarked(const JtfApp *app, const char *path);
void jtf_remove_bookmark(JtfApp *app, int index);
void jtf_rename_bookmark(JtfApp *app, int index, const char *name);
void jtf_move_bookmark(JtfApp *app, int from, int to);
int jtf_recent_count(const JtfApp *app);
int jtf_recent_path(const JtfApp *app, int index, char *buf, int len);
void jtf_clear_recent(JtfApp *app);
void jtf_set_tree_state(JtfApp *app, int visible, int width);
// Sidebar sections the user folded away, one id per line, remembered across
// launches. By id and not by label, so switching language does not reopen them.
int jtf_collapsed_sections(const JtfApp *app, char *buf, int len);
void jtf_set_collapsed_sections(JtfApp *app, const char *ids);
// How many recent folders the sidebar shows. 0 means the default.
int jtf_recent_limit(const JtfApp *app);
void jtf_set_recent_limit(JtfApp *app, int limit);
int jtf_mark_pattern(JtfApp *app, int pane, const char *pattern, int mark);
uint64_t jtf_target_size(const JtfApp *app, int pane);
int jtf_target_paths(const JtfApp *app, int pane, char *buf, int len);
int jtf_target_names(const JtfApp *app, int pane, char *buf, int len);
int jtf_op_prepare_duplicate(JtfApp *app, int pane);
int jtf_op_prepare_drop(JtfApp *app, int pane, int kind, const char *newline_separated);
// Sources named by the caller, for windows that are not panes (disc usage).
int jtf_op_prepare_paths(JtfApp *app, int kind, const char *newline_separated,
                         const char *destination);
int jtf_op_prepare_rename(JtfApp *app, int pane, const char *new_name);
int jtf_op_prepare_new_folder(JtfApp *app, int pane, const char *name);
int jtf_op_prepare_new_file(JtfApp *app, int pane, const char *name);
int jtf_op_prepare_read_only(JtfApp *app, int pane, int read_only);
int jtf_targets_read_only(const JtfApp *app, int pane);
int jtf_op_error_key(const JtfApp *app, char *buf, int len);
int jtf_op_conflicts(const JtfApp *app);
int jtf_op_entries(const JtfApp *app);
uint64_t jtf_op_bytes(const JtfApp *app);
int jtf_op_is_irreversible(const JtfApp *app);
int jtf_op_removes(const JtfApp *app); // trash or delete
int jtf_op_first_conflict(const JtfApp *app, char *buf, int len);
int jtf_op_start(JtfApp *app, int policy);

// Planning runs on a worker thread: counting a large folder takes as long as
// reading it. Poll returns 1 ready, 0 failed, -1 still counting.
int jtf_plan_poll(JtfApp *app);
int jtf_plan_running(const JtfApp *app);
void jtf_plan_cancel(JtfApp *app); // 0 skip 1 overwrite 2 keep both 3 abort
int jtf_can_undo(const JtfApp *app);
int jtf_undo_label_key(const JtfApp *app, char *buf, int len);
int jtf_undo(JtfApp *app);
int jtf_op_running(const JtfApp *app);
int jtf_op_queued(const JtfApp *app);
void jtf_op_clear_queue(JtfApp *app);
// The job queue, so it can be listed rather than only counted. Index 0 is the
// running job when there is one; the rest wait in the order they will run.
int jtf_job_count(const JtfApp *app);
int jtf_job_label_key(const JtfApp *app, int index, char *buf, int len);
uint64_t jtf_job_entries(const JtfApp *app, int index);
uint64_t jtf_job_bytes(const JtfApp *app, int index);
int jtf_job_is_running(const JtfApp *app, int index);
void jtf_job_cancel(JtfApp *app, int index);
int jtf_op_percent(const JtfApp *app);
int jtf_op_label_key(const JtfApp *app, char *buf, int len);
int jtf_op_current(const JtfApp *app, char *buf, int len);
void jtf_op_cancel(const JtfApp *app);
int jtf_op_has_result(const JtfApp *app);
int jtf_op_result(const JtfApp *app, char *key_buf, int key_len, char *error_buf, int error_len,
                  int *succeeded, int *skipped, int *failed);
void jtf_op_clear_result(JtfApp *app);

int jtf_shortcut_for(const JtfApp *app, const char *command, char *buf, int len);
int jtf_has_command(const JtfApp *app, const char *command);
int jtf_keymap_name(const JtfApp *app, char *buf, int len);
void jtf_set_keymap(JtfApp *app, const char *name);
int jtf_toggle_keymap(JtfApp *app, char *buf, int len);
// viewer
int jtf_viewer_open(JtfApp *app, int pane, int row);
void jtf_viewer_close(JtfApp *app);
int jtf_viewer_is_text(const JtfApp *app);
void jtf_viewer_toggle_hex(JtfApp *app);
uint64_t jtf_viewer_row_count(const JtfApp *app);
int jtf_viewer_row(JtfApp *app, uint64_t row, char *buf, int len);
void jtf_viewer_set_encoding(JtfApp *app, int index);
int jtf_viewer_encoding(const JtfApp *app);
int jtf_encoding_count(void);
int jtf_encoding_key(int index, char *buf, int len);
void jtf_viewer_status(const JtfApp *app, char *path_buf, int path_len, char *kind_buf,
                       int kind_len, char *encoding_buf, int encoding_len, char *endings_buf,
                       int endings_len, uint64_t *size);
int64_t jtf_viewer_find(JtfApp *app, const char *needle, uint64_t from_row);

// settings
int jtf_command_count(const JtfApp *app);
int jtf_command_at(const JtfApp *app, int index, char *id_buf, int id_len, char *label_buf,
                   int label_len, char *category_buf, int category_len);
int jtf_command_is_destructive(const JtfApp *app, int index);
int jtf_bind_shortcut(JtfApp *app, const char *command, const char *chord, char *conflict_buf,
                      int conflict_len);
void jtf_clear_shortcut(JtfApp *app, const char *command);
void jtf_reset_shortcuts(JtfApp *app);
int jtf_dropped_bindings(const JtfApp *app);
int jtf_startup_mode(const JtfApp *app);
int jtf_startup_location(const JtfApp *app, char *buf, int len);
void jtf_set_startup(JtfApp *app, int mode, const char *location);
int jtf_remember_closed_tabs(const JtfApp *app);
int jtf_remember_marks(const JtfApp *app);
void jtf_set_remember(JtfApp *app, int closed_tabs, int marks);

int jtf_font_family(const JtfApp *app, char *buf, int len);
int jtf_font_point_size(const JtfApp *app);
int jtf_font_monospace(const JtfApp *app);
// Whether the fixed-width font covers the whole list, or only the columns
// that are read as columns of aligned values.
int jtf_font_monospace_everywhere(const JtfApp *app);
void jtf_set_font_monospace_everywhere(JtfApp *app, int everywhere);
void jtf_set_font(JtfApp *app, const char *family, int point_size, int monospace);
void jtf_set_theme_mode(JtfApp *app, int mode);
int jtf_theme_mode(const JtfApp *app);
uint32_t jtf_theme_color(const JtfApp *app, int system_is_dark, int token);
int jtf_theme_token_count(void);
int jtf_theme_token_name(int index, char *buf, int len);
int jtf_column_count(void);
int jtf_column_key(int column, char *buf, int len);

} // extern "C"

// Token numbering must match ThemeToken::ALL in src/core/src/theme.rs.
// jtf_theme_token_count() is asserted against this at startup, so a mismatch
// is caught immediately rather than becoming a wrong colour.
enum JtfToken {
    TokenSurfaceWindow = 0,
    TokenSurfacePane,
    TokenSurfacePreview,
    TokenSurfaceHeader,
    TokenSurfaceMenu,
    TokenRowAlternate,
    TokenRowHover,
    TokenTextPrimary,
    TokenTextSecondary,
    TokenTextOnAccent,
    TokenBorder,
    TokenSelectionActive,
    TokenSelectionInactive,
    TokenMarkActive,
    TokenFocusRing,
    TokenPaneActiveIndicator,
    TokenTextExecutable,
    TokenStatusError,
    TokenStatusWarning,
    TokenStatusSuccess,
    TokenCount
};

// The names Rust gives these, in the same order. main.cpp checks every one at
// startup: a reordering keeps the count identical and would otherwise recolour
// the whole interface silently.
inline const char *const kTokenNames[] = {
    "surface.window", "surface.pane",   "surface.preview", "surface.header",
    "surface.menu",
    "row.alternate",  "row.hover",      "text.primary",    "text.secondary",
    "text.on_accent", "border",         "selection.active", "selection.inactive",
    "mark.active",    "focus.ring",     "pane.active_indicator",
    "text.executable",
    "status.error",   "status.warning", "status.success",
};
