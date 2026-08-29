// C ABI exposed by src/ui/qt6/bridge (Rust).
//
// AGENTS.md 4: no core logic lives on this side of the line. Everything below
// is a question asked of Rust, or an event forwarded to it. The C++ layer
// draws and forwards input; it decides nothing.
#pragma once

#include <cstdint>

extern "C" {

typedef struct JtfApp JtfApp;

JtfApp *jtf_app_new(void);
void jtf_app_free(JtfApp *app);
void jtf_app_save_session(const JtfApp *app);
int jtf_app_pump(JtfApp *app);

// layout
int jtf_layout_json(const JtfApp *app, char *buf, int len);
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
int jtf_row_is_marked(const JtfApp *app, int pane, int row);
void jtf_toggle_mark(JtfApp *app, int pane, int row);
void jtf_mark_listed(JtfApp *app, int pane, int action);
void jtf_refresh(JtfApp *app, int pane);
int jtf_marked_count(const JtfApp *app, int pane);
void jtf_sort_by(JtfApp *app, int pane, int column);
int jtf_is_loading(const JtfApp *app, int pane);
int jtf_error_key(const JtfApp *app, int pane, char *buf, int len);
void jtf_set_show_hidden(JtfApp *app, int show);
int jtf_show_hidden(const JtfApp *app);

// i18n and theme
void jtf_set_locale(JtfApp *app, const char *locale);
int jtf_locale(const JtfApp *app, char *buf, int len);
int jtf_tr(const JtfApp *app, const char *key, char *buf, int len);
int jtf_shortcut_for(const JtfApp *app, const char *command, char *buf, int len);
int jtf_has_command(const JtfApp *app, const char *command);
int jtf_keymap_name(const JtfApp *app, char *buf, int len);
void jtf_set_keymap(JtfApp *app, const char *name);
int jtf_font_family(const JtfApp *app, char *buf, int len);
int jtf_font_point_size(const JtfApp *app);
int jtf_font_monospace(const JtfApp *app);
void jtf_set_font(JtfApp *app, const char *family, int point_size, int monospace);
void jtf_set_theme_mode(JtfApp *app, int mode);
int jtf_theme_mode(const JtfApp *app);
uint32_t jtf_theme_color(const JtfApp *app, int system_is_dark, int token);
int jtf_theme_token_count(void);
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
    TokenStatusError,
    TokenStatusWarning,
    TokenStatusSuccess,
    TokenCount
};
