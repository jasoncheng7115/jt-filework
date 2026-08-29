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
int jtf_pane_count(const JtfApp *app);
int jtf_active_pane(const JtfApp *app);
void jtf_focus_pane(JtfApp *app, int pane);
void jtf_focus_next_pane(JtfApp *app);
void jtf_split_active(JtfApp *app, int vertical);
int jtf_close_active_pane(JtfApp *app);
void jtf_apply_preset(JtfApp *app, int preset);

// tabs
int jtf_tab_count(const JtfApp *app, int pane);
int jtf_active_tab(const JtfApp *app, int pane);
int jtf_tab_title(const JtfApp *app, int pane, int index, char *buf, int len);
void jtf_new_tab(JtfApp *app);
void jtf_close_tab(JtfApp *app, int pane, int index);
void jtf_activate_tab(JtfApp *app, int pane, int index);

// navigation
int jtf_current_path(const JtfApp *app, int pane, char *buf, int len);
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
int jtf_row_is_marked(const JtfApp *app, int pane, int row);
void jtf_toggle_mark(JtfApp *app, int pane, int row);
void jtf_mark_listed(JtfApp *app, int pane, int action);
void jtf_refresh(JtfApp *app, int pane);
int jtf_marked_count(const JtfApp *app, int pane);
void jtf_sort_by(JtfApp *app, int pane, int column);
int jtf_search_start(JtfApp *app, int pane, const char *query, char *error_buf, int error_len);
int jtf_is_searching(const JtfApp *app, int pane);
int jtf_search_query(const JtfApp *app, int pane, char *buf, int len);
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
int jtf_view_mode(const JtfApp *app, int pane);
void jtf_set_view_mode(JtfApp *app, int pane, int grid);
int jtf_thumbnails(const JtfApp *app);
void jtf_set_thumbnails(JtfApp *app, int on);
int jtf_key_hints_visible(const JtfApp *app);
void jtf_set_key_hints_visible(JtfApp *app, int visible);
int jtf_inspector_visible(const JtfApp *app);
int jtf_inspector_width(const JtfApp *app);
void jtf_set_inspector_state(JtfApp *app, int visible, int width);

// The inspector's own read of a file, independent of the viewer window.
int jtf_preview_open(JtfApp *app, const char *path);
void jtf_preview_close(JtfApp *app);
int jtf_archive_listing(const JtfApp *app, const char *path, char *buf, int len);
uint64_t jtf_preview_line_count(const JtfApp *app);
int jtf_preview_row(JtfApp *app, uint64_t row, char *buf, int len);
int jtf_preview_encoding_key(const JtfApp *app, char *buf, int len);
int jtf_preview_line_ending_key(const JtfApp *app, char *buf, int len);

int jtf_command_for_chord(const JtfApp *app, const char *chord, char *buf, int len);
int jtf_type_ahead(const JtfApp *app);

int jtf_measure_folder_sizes(JtfApp *app, int pane);
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
void jtf_remove_bookmark(JtfApp *app, int index);
void jtf_rename_bookmark(JtfApp *app, int index, const char *name);
void jtf_move_bookmark(JtfApp *app, int from, int to);
int jtf_recent_count(const JtfApp *app);
int jtf_recent_path(const JtfApp *app, int index, char *buf, int len);
void jtf_clear_recent(JtfApp *app);
void jtf_set_tree_state(JtfApp *app, int visible, int width);
int jtf_mark_pattern(JtfApp *app, int pane, const char *pattern, int mark);
uint64_t jtf_target_size(const JtfApp *app, int pane);
int jtf_target_paths(const JtfApp *app, int pane, char *buf, int len);
int jtf_target_names(const JtfApp *app, int pane, char *buf, int len);
int jtf_op_prepare_duplicate(JtfApp *app, int pane);
int jtf_op_prepare_drop(JtfApp *app, int pane, int kind, const char *newline_separated);
int jtf_op_prepare_rename(JtfApp *app, int pane, const char *new_name);
int jtf_op_prepare_new_folder(JtfApp *app, int pane, const char *name);
int jtf_op_prepare_new_file(JtfApp *app, int pane, const char *name);
int jtf_op_error_key(const JtfApp *app, char *buf, int len);
int jtf_op_conflicts(const JtfApp *app);
int jtf_op_entries(const JtfApp *app);
uint64_t jtf_op_bytes(const JtfApp *app);
int jtf_op_is_irreversible(const JtfApp *app);
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
    "row.alternate",  "row.hover",      "text.primary",    "text.secondary",
    "text.on_accent", "border",         "selection.active", "selection.inactive",
    "mark.active",    "focus.ring",     "pane.active_indicator",
    "text.executable",
    "status.error",   "status.warning", "status.success",
};
