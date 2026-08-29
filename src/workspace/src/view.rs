//! Per-tab view state: sorting, filtering, columns, view mode and scroll.
//!
//! `AGENTS.md` §7 requires every tab to own these independently, and
//! `docs/UI_TEST_PLAN.md` TAB-008 requires them all to travel with a tab that
//! moves to another pane.

use serde::{Deserialize, Serialize};

/// What a list is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SortKey {
    /// Display name, locale-aware.
    Name,
    /// Size in bytes; directories sort by the platform convention.
    Size,
    /// Entry kind.
    Kind,
    /// Modification time.
    Modified,
    /// Creation time.
    Created,
    /// Extension, then name.
    Extension,
}

/// Sort key plus direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortSpec {
    /// What to sort by.
    pub key: SortKey,
    /// Ascending when true.
    pub ascending: bool,
}

impl Default for SortSpec {
    fn default() -> Self {
        Self {
            key: SortKey::Name,
            ascending: true,
        }
    }
}

impl SortSpec {
    /// Clicking the sorted column flips direction; clicking another column
    /// switches to it, ascending.
    #[must_use]
    pub fn toggled_by(self, key: SortKey) -> Self {
        if self.key == key {
            Self {
                key,
                ascending: !self.ascending,
            }
        } else {
            Self {
                key,
                ascending: true,
            }
        }
    }
}

/// How filter text is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    /// Case-insensitive substring.
    #[default]
    Substring,
    /// Shell-style wildcard.
    Glob,
    /// Regular expression.
    Regex,
}

/// The tab's live filter.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Filter {
    /// The text the user typed. Empty means no filtering.
    pub text: String,
    /// How to interpret it.
    pub mode: FilterMode,
    /// Whether hidden entries are listed.
    pub show_hidden: bool,
}

impl Filter {
    /// Whether the filter excludes anything.
    pub fn is_active(&self) -> bool {
        !self.text.is_empty()
    }
}

/// A column in the detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Column {
    /// Display name.
    Name,
    /// Size.
    Size,
    /// Kind.
    Kind,
    /// Modification time.
    Modified,
    /// Creation time.
    Created,
    /// Permission summary.
    Permissions,
    /// Owner.
    Owner,
    /// Extension.
    Extension,
    /// Platform tags.
    Tags,
    /// Full path. Shown in search results.
    Path,
}

impl Column {
    /// Localization key for the header.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Name => "column.name",
            Self::Size => "column.size",
            Self::Kind => "column.kind",
            Self::Modified => "column.modified",
            Self::Created => "column.created",
            Self::Permissions => "column.permissions",
            Self::Owner => "column.owner",
            Self::Extension => "column.extension",
            Self::Tags => "column.tags",
            Self::Path => "column.path",
        }
    }

    /// Every column, for exhaustive tests and catalogue parity.
    pub const ALL: &'static [Self] = &[
        Self::Name,
        Self::Size,
        Self::Kind,
        Self::Modified,
        Self::Created,
        Self::Permissions,
        Self::Owner,
        Self::Extension,
        Self::Tags,
        Self::Path,
    ];
}

/// A column's presentation in one tab.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColumnSpec {
    /// Which column.
    pub column: Column,
    /// Width in logical pixels.
    pub width: f32,
    /// Whether it is shown.
    pub visible: bool,
}

impl ColumnSpec {
    /// A visible column at a default width.
    pub const fn visible(column: Column, width: f32) -> Self {
        Self {
            column,
            width,
            visible: true,
        }
    }
}

/// List or grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    /// Detail list with columns. The primary mode.
    #[default]
    List,
    /// Icon grid.
    Grid,
}

/// Where the list is scrolled to.
///
/// Stored as a row index plus a sub-row offset so restoring a position does
/// not depend on pixel metrics that change with font or DPI.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ScrollPosition {
    /// Index of the first visible row.
    pub first_visible_row: usize,
    /// Fraction of that row scrolled past, in `0.0..1.0`.
    pub row_offset: f32,
}

/// The default column layout for a directory listing.
pub(crate) fn default_columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::visible(Column::Name, 320.0),
        ColumnSpec::visible(Column::Size, 90.0),
        ColumnSpec::visible(Column::Modified, 160.0),
        ColumnSpec::visible(Column::Kind, 120.0),
        ColumnSpec {
            column: Column::Created,
            width: 160.0,
            visible: false,
        },
        ColumnSpec {
            column: Column::Permissions,
            width: 110.0,
            visible: false,
        },
        ColumnSpec {
            column: Column::Owner,
            width: 110.0,
            visible: false,
        },
        ColumnSpec {
            column: Column::Extension,
            width: 80.0,
            visible: false,
        },
        ColumnSpec {
            column: Column::Tags,
            width: 120.0,
            visible: false,
        },
        ColumnSpec {
            column: Column::Path,
            width: 400.0,
            visible: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sort_is_name_ascending() {
        assert_eq!(
            SortSpec::default(),
            SortSpec {
                key: SortKey::Name,
                ascending: true
            }
        );
    }

    #[test]
    fn clicking_the_same_column_flips_direction_and_another_resets_it() {
        let s = SortSpec::default();
        let s = s.toggled_by(SortKey::Name);
        assert!(!s.ascending);
        let s = s.toggled_by(SortKey::Size);
        assert_eq!(
            s,
            SortSpec {
                key: SortKey::Size,
                ascending: true
            }
        );
    }

    #[test]
    fn a_filter_is_inactive_until_there_is_text() {
        let mut f = Filter::default();
        assert!(!f.is_active());
        f.text = "log".to_string();
        assert!(f.is_active());
    }

    #[test]
    fn default_view_is_the_detail_list() {
        assert_eq!(ViewMode::default(), ViewMode::List);
    }

    #[test]
    fn default_columns_cover_every_column_exactly_once() {
        let columns = default_columns();
        assert_eq!(columns.len(), Column::ALL.len());
        for &column in Column::ALL {
            assert_eq!(
                columns.iter().filter(|c| c.column == column).count(),
                1,
                "{column:?} must appear exactly once"
            );
        }
    }

    #[test]
    fn name_is_visible_by_default_and_path_is_not() {
        let columns = default_columns();
        let find = |c: Column| columns.iter().find(|s| s.column == c).unwrap();
        assert!(find(Column::Name).visible);
        assert!(!find(Column::Path).visible, "path is for search results");
    }

    #[test]
    fn every_column_has_a_distinct_label_key() {
        let mut keys: Vec<_> = Column::ALL.iter().map(|c| c.label_key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn scroll_position_is_row_based_not_pixel_based() {
        // So restoring a session is stable across font size and DPI changes.
        let p = ScrollPosition {
            first_visible_row: 4200,
            row_offset: 0.5,
        };
        let back: ScrollPosition =
            serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p, back);
    }
}
