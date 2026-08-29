//! Bookmarks and recent locations — the sidebar's "places".
//!
//! Both are lists of locations the user can get back to, and both are the
//! user's work in the sense of `AGENTS.md` §10.4: an upgrade that loses a
//! bookmark list someone curated over a year has lost something real.
//!
//! They differ in who maintains them. A bookmark is added deliberately and
//! stays until removed. A recent location is added by walking around and is
//! evicted by walking around more.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many recent locations are kept.
///
/// Bounded because it is appended to on every navigation, and an unbounded
/// list that is written to disk on every keystroke is a disk-space bug with a
/// slow fuse (`docs/SECURITY.md` §13: bound what input can grow).
pub const MAX_RECENT: usize = 32;

/// How many bookmarks are kept.
///
/// High enough that nobody meets it by curating, low enough that a program
/// bug adding them in a loop cannot fill the disk.
pub const MAX_BOOKMARKS: usize = 512;

/// A saved location with a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    /// Where it goes.
    pub path: PathBuf,
    /// What to call it. Empty means "use the folder's own name".
    #[serde(default)]
    pub name: String,
}

impl Bookmark {
    /// A bookmark to `path`, named after the folder.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            name: String::new(),
        }
    }

    /// What the sidebar shows.
    ///
    /// The folder's own name unless the user renamed the bookmark. A path
    /// whose last component is not valid UTF-8 falls back to the lossy
    /// rendering rather than vanishing from the list — `AGENTS.md` §9 keeps
    /// the raw name for operations and the display name for display.
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.path.file_name().map_or_else(
            || self.path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

/// The user's bookmarks and recent locations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Places {
    #[serde(default)]
    bookmarks: Vec<Bookmark>,
    #[serde(default)]
    recent: VecDeque<PathBuf>,
}

impl Places {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// The bookmarks, in the order the user arranged them.
    pub fn bookmarks(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Whether `path` is bookmarked.
    pub fn is_bookmarked(&self, path: &Path) -> bool {
        self.bookmarks.iter().any(|b| b.path == path)
    }

    /// Bookmark `path`, or remove it if it is already bookmarked.
    ///
    /// Returns whether it is bookmarked afterwards, which is what a toggle
    /// button needs to know to draw itself.
    pub fn toggle_bookmark(&mut self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        if let Some(at) = self.bookmarks.iter().position(|b| b.path == path) {
            self.bookmarks.remove(at);
            return false;
        }
        if self.bookmarks.len() >= MAX_BOOKMARKS {
            return false;
        }
        self.bookmarks.push(Bookmark::new(path));
        true
    }

    /// Remove the bookmark at `index`, if there is one.
    pub fn remove_bookmark(&mut self, index: usize) {
        if index < self.bookmarks.len() {
            self.bookmarks.remove(index);
        }
    }

    /// Rename the bookmark at `index`. An empty name restores the default.
    pub fn rename_bookmark(&mut self, index: usize, name: impl Into<String>) {
        if let Some(bookmark) = self.bookmarks.get_mut(index) {
            bookmark.name = name.into();
        }
    }

    /// Move the bookmark at `from` to `to`, for drag reordering.
    pub fn move_bookmark(&mut self, from: usize, to: usize) {
        if from >= self.bookmarks.len() || to >= self.bookmarks.len() || from == to {
            return;
        }
        let moved = self.bookmarks.remove(from);
        self.bookmarks.insert(to, moved);
    }

    /// The recent locations, most recent first.
    pub fn recent(&self) -> impl Iterator<Item = &PathBuf> {
        self.recent.iter()
    }

    /// Record a visit.
    ///
    /// A location already in the list moves to the front rather than
    /// appearing twice: a list where your home folder fills every slot is not
    /// a list of where you have been.
    pub fn visit(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        if let Some(at) = self.recent.iter().position(|p| p == path) {
            self.recent.remove(at);
        }
        self.recent.push_front(path.to_path_buf());
        while self.recent.len() > MAX_RECENT {
            self.recent.pop_back();
        }
    }

    /// Forget where the user has been. Bookmarks are deliberate and stay.
    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bookmark_is_named_after_its_folder_until_it_is_renamed() {
        let mut places = Places::new();
        places.toggle_bookmark("/Users/someone/Projects");
        assert_eq!(places.bookmarks()[0].display_name(), "Projects");
        places.rename_bookmark(0, "Work");
        assert_eq!(places.bookmarks()[0].display_name(), "Work");
        places.rename_bookmark(0, "");
        assert_eq!(
            places.bookmarks()[0].display_name(),
            "Projects",
            "clearing the name restores the folder's own, rather than leaving \
             the entry blank"
        );
    }

    #[test]
    fn toggling_reports_the_state_a_button_should_draw() {
        let mut places = Places::new();
        assert!(places.toggle_bookmark("/tmp"), "added");
        assert!(places.is_bookmarked(Path::new("/tmp")));
        assert!(!places.toggle_bookmark("/tmp"), "removed");
        assert!(!places.is_bookmarked(Path::new("/tmp")));
        assert!(places.bookmarks().is_empty());
    }

    #[test]
    fn revisiting_moves_a_location_up_rather_than_duplicating_it() {
        let mut places = Places::new();
        places.visit("/a");
        places.visit("/b");
        places.visit("/a");
        let recent: Vec<_> = places.recent().collect();
        assert_eq!(
            recent,
            [Path::new("/a"), Path::new("/b")],
            "the list is where you have been, not how often"
        );
    }

    #[test]
    fn recent_locations_are_bounded() {
        let mut places = Places::new();
        for i in 0..(MAX_RECENT * 3) {
            places.visit(format!("/folder/{i}"));
        }
        assert_eq!(places.recent().count(), MAX_RECENT);
        assert_eq!(
            places.recent().next().unwrap(),
            Path::new(&format!("/folder/{}", MAX_RECENT * 3 - 1)),
            "the newest visit is still at the front after eviction"
        );
    }

    #[test]
    fn bookmarks_are_bounded_too() {
        let mut places = Places::new();
        for i in 0..(MAX_BOOKMARKS + 10) {
            places.toggle_bookmark(format!("/folder/{i}"));
        }
        assert_eq!(places.bookmarks().len(), MAX_BOOKMARKS);
    }

    #[test]
    fn clearing_recent_leaves_bookmarks_alone() {
        let mut places = Places::new();
        places.toggle_bookmark("/keep");
        places.visit("/forget");
        places.clear_recent();
        assert_eq!(places.recent().count(), 0);
        assert_eq!(places.bookmarks().len(), 1, "bookmarks are deliberate");
    }

    #[test]
    fn reordering_a_bookmark_keeps_every_other_one() {
        let mut places = Places::new();
        for name in ["/a", "/b", "/c"] {
            places.toggle_bookmark(name);
        }
        places.move_bookmark(2, 0);
        let paths: Vec<_> = places.bookmarks().iter().map(|b| b.path.clone()).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("/c"),
                PathBuf::from("/a"),
                PathBuf::from("/b")
            ]
        );
        places.move_bookmark(9, 0);
        assert_eq!(
            places.bookmarks().len(),
            3,
            "an out-of-range move does nothing"
        );
    }
}
