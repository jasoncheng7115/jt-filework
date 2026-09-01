//! Where the space went.
//!
//! `folder_size` answers "how big is this folder"; this answers "and what is
//! it made of". Two breakdowns of one walk:
//!
//! * **By folder** — every immediate child of the root with everything beneath
//!   it counted into it, so one screen says which branch to go and look at.
//!   That is what a disc-usage tool is for, and going one level at a time is
//!   how you find the thing rather than being shown a picture of everything.
//! * **By kind** — every file grouped by what it is, so「照片佔了 40 GB」is a
//!   question that has an answer. The tools people use for this mostly cannot
//!   say it, and it is the more useful half as often as not.
//!
//! One walk produces both, because walking a large tree twice to answer two
//! questions about it is the expensive mistake here.
//!
//! Iterative rather than recursive, and symlinks are never followed: a
//! directory tree is untrusted input, and a link into a parent must not make
//! the walk loop (`AGENTS.md` §20.2, `docs/SECURITY.md` §3.1). That last is
//! checked in `tests/symlinks_unix.rs`, where making a link is possible.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use jtf_jobs::CancellationToken;

/// How deep the walk will go. The same bound the size measurement uses.
pub const MAX_DEPTH: usize = 64;

/// How many child folders are reported before the rest are gathered into one
/// row.
///
/// A folder with ten thousand children is a real thing, and a window that
/// draws a row and a bar for each of them is a window that takes a second to
/// open and cannot be read anyway. The ones that matter are at the top; the
/// rest become one line so the numbers still add up.
pub const MAX_FOLDERS: usize = 200;

/// How many kinds are reported before the rest are gathered into one row.
///
/// A disc holds a long tail of extensions nobody has heard of. Twenty rows is
/// the part worth reading; the tail is reported as one line so the total still
/// adds up.
pub const MAX_KINDS: usize = 20;

/// One thing sitting directly in the analysed folder.
///
/// A child folder, carrying everything beneath it, or a file carrying its own
/// size. Both, because「這個資料夾裡什麼最大」is as often answered by one
/// enormous file as by a branch, and a breakdown that lists only folders
/// cannot say so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderUsage {
    /// Its own name, not its path: the window shows it under a root it
    /// already names.
    pub name: String,
    /// Its full path, so the window can descend into it or go there.
    pub path: PathBuf,
    /// Bytes: everything beneath a folder, or the file's own size.
    pub bytes: u64,
    /// How many files that was. One, for a file.
    pub files: u64,
    /// Whether this is a folder. A file cannot be descended into.
    pub is_directory: bool,
}

/// What one kind of file adds up to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindUsage {
    /// The extension, lowercased, without a dot. Empty means "no extension".
    pub extension: String,
    /// The catalogue key for the group it falls in, so the window can name it
    /// in the user's language rather than showing a bare extension.
    pub group_key: &'static str,
    /// Bytes.
    pub bytes: u64,
    /// How many files.
    pub files: u64,
}

/// What a walk found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Usage {
    /// What sits directly in the folder - child folders and files - largest
    /// first.
    pub folders: Vec<FolderUsage>,
    /// Kinds, largest first, at most [`MAX_KINDS`] plus a gathered remainder.
    pub kinds: Vec<KindUsage>,
    /// Bytes of every file beneath the root.
    pub bytes: u64,
    /// Files counted.
    pub files: u64,
    /// Directories entered.
    pub folder_count: u64,
    /// Bytes in files sitting directly in the root.
    ///
    /// Those files now have rows of their own, so this is reported for the
    /// caller's arithmetic rather than for a row of its own.
    pub loose_bytes: u64,
    /// Whether the walk stopped early: cancelled, too deep, or blocked by a
    /// directory it could not read. Reported rather than hidden — a breakdown
    /// that silently omits part of a disc is worse than one labelled
    /// incomplete.
    pub partial: bool,
}

/// How far a walk has got.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// Bytes counted so far.
    pub bytes: u64,
    /// Files counted so far.
    pub files: u64,
    /// Directories entered so far.
    pub folders: u64,
    /// The directory this update is about.
    ///
    /// A count says the walk is alive; it does not say where it has got to,
    /// and on a home folder「32894 個檔案」reads the same whether it is in
    /// Downloads or four levels into a cache. The path is what tells someone
    /// whether to wait.
    pub directory: PathBuf,
}

/// Which group an extension belongs to.
///
/// Deliberately coarse. The question this answers is「什麼吃掉了空間」, and
/// twelve buckets answer it; a hundred exact types do not. The catalogue key
/// is returned rather than a name, so the window says it in the user's
/// language.
#[must_use]
pub fn group_of(extension: &str) -> &'static str {
    const VIDEO: &[&str] = &[
        "mp4", "mkv", "mov", "avi", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts",
    ];
    const IMAGE: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "heic", "heif", "webp", "raw", "cr2",
        "nef", "arw", "dng", "svg", "psd",
    ];
    const AUDIO: &[&str] = &[
        "mp3", "aac", "flac", "wav", "aiff", "m4a", "ogg", "opus", "wma",
    ];
    const ARCHIVE: &[&str] = &[
        "zip", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "tar", "jar", "war", "cab",
    ];
    const DISK_IMAGE: &[&str] = &["iso", "dmg", "img", "vdi", "vmdk", "qcow2", "vhd", "vhdx"];
    const DOCUMENT: &[&str] = &[
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "pages",
        "numbers", "key", "rtf", "epub",
    ];
    const CODE: &[&str] = &[
        "rs", "c", "h", "cpp", "hpp", "cc", "py", "js", "ts", "tsx", "jsx", "go", "java", "kt",
        "swift", "rb", "php", "sh", "pl", "lua", "cs", "m", "mm",
    ];
    const TEXT: &[&str] = &[
        "txt", "md", "log", "json", "xml", "yaml", "yml", "toml", "ini", "csv", "html", "css",
    ];
    const DATABASE: &[&str] = &["db", "sqlite", "sqlite3", "mdb", "dbf", "parquet"];
    const BINARY: &[&str] = &[
        "exe", "dll", "so", "dylib", "a", "o", "bin", "app", "pkg", "deb",
    ];
    const FONT: &[&str] = &["ttf", "otf", "woff", "woff2", "ttc"];

    if extension.is_empty() {
        return "usage.kind.none";
    }
    for (list, key) in [
        (VIDEO, "usage.kind.video"),
        (IMAGE, "usage.kind.image"),
        (AUDIO, "usage.kind.audio"),
        (ARCHIVE, "usage.kind.archive"),
        (DISK_IMAGE, "usage.kind.disk_image"),
        (DOCUMENT, "usage.kind.document"),
        (CODE, "usage.kind.code"),
        (TEXT, "usage.kind.text"),
        (DATABASE, "usage.kind.database"),
        (BINARY, "usage.kind.binary"),
        (FONT, "usage.kind.font"),
    ] {
        if list.contains(&extension) {
            return key;
        }
    }
    "usage.kind.other"
}

/// Analyse `root`, stopping early if `cancel` is triggered.
#[must_use]
pub fn analyse(root: &Path, cancel: &CancellationToken) -> Usage {
    analyse_with(root, cancel, |_| {})
}

/// Analyse `root`, reporting as it goes.
///
/// There is no percentage: the total is not known until the walk ends, which
/// is the whole reason the walk exists. What a caller can show is how much has
/// been counted so far. Called once per directory entered rather than once per
/// file, so a folder of a hundred thousand files does not send a hundred
/// thousand updates to a screen that redraws sixty times a second.
#[must_use]
pub fn analyse_with(
    root: &Path,
    cancel: &CancellationToken,
    mut progress: impl FnMut(Progress),
) -> Usage {
    let mut usage = Usage::default();
    // Bytes and files per immediate child, keyed by that child's name.
    let mut per_child: HashMap<PathBuf, (u64, u64)> = HashMap::new();
    // Files directly in the root, each a row of its own.
    let mut loose_files: Vec<FolderUsage> = Vec::new();
    let mut per_extension: HashMap<String, (u64, u64)> = HashMap::new();

    // (path, depth, which immediate child of the root this is under). An
    // explicit stack, so depth costs heap and not stack.
    let mut pending: Vec<(PathBuf, usize, Option<PathBuf>)> = vec![(root.to_path_buf(), 0, None)];

    while let Some((dir, depth, branch)) = pending.pop() {
        if cancel.is_cancelled() {
            usage.partial = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            // An unreadable directory is a permissions fact, not a failure of
            // the analysis: count what we can and say the total is partial.
            usage.partial = true;
            continue;
        };
        progress(Progress {
            bytes: usage.bytes,
            files: usage.files,
            folders: usage.folder_count,
            directory: dir.clone(),
        });

        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                usage.partial = true;
                continue;
            };
            // Never followed: a link into a parent would make the walk loop,
            // and a link to something huge elsewhere is not space used here.
            if meta.is_symlink() {
                continue;
            }
            let path = entry.path();
            if meta.is_dir() {
                usage.folder_count += 1;
                if depth + 1 > MAX_DEPTH {
                    usage.partial = true;
                    continue;
                }
                // At the root, this directory *is* a branch; deeper down it
                // belongs to whichever branch we came in through.
                let child = branch.clone().or_else(|| Some(path.clone()));
                per_child
                    .entry(child.clone().unwrap_or_default())
                    .or_insert((0, 0));
                pending.push((path, depth + 1, child));
                continue;
            }

            usage.files += 1;
            let bytes = meta.len();
            usage.bytes = usage.bytes.saturating_add(bytes);
            if let Some(child) = &branch {
                let counter = per_child.entry(child.clone()).or_insert((0, 0));
                counter.0 = counter.0.saturating_add(bytes);
                counter.1 += 1;
            } else {
                // A file sitting directly in the root is its own row: it
                // belongs to no child, and one enormous file is exactly the
                // thing a breakdown of folders alone would hide.
                usage.loose_bytes = usage.loose_bytes.saturating_add(bytes);
                loose_files.push(FolderUsage {
                    name: path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    path: path.clone(),
                    bytes,
                    files: 1,
                    is_directory: false,
                });
            }

            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase)
                .unwrap_or_default();
            let counter = per_extension.entry(extension).or_insert((0, 0));
            counter.0 = counter.0.saturating_add(bytes);
            counter.1 += 1;
        }
    }

    usage.folders = ranked_folders(per_child, loose_files);
    usage.kinds = ranked_kinds(per_extension);
    usage
}

/// The children as rows, largest first.
///
/// Ties break by name, so running the same folder twice gives the same order
/// rather than shuffling whichever the map happened to hand over first.
fn ranked_folders(
    per_child: HashMap<PathBuf, (u64, u64)>,
    loose_files: Vec<FolderUsage>,
) -> Vec<FolderUsage> {
    let mut folders: Vec<FolderUsage> = per_child
        .into_iter()
        .filter(|(path, _)| !path.as_os_str().is_empty())
        .map(|(path, (bytes, files))| FolderUsage {
            name: path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            path,
            bytes,
            files,
            is_directory: true,
        })
        .collect();
    // Folders and files ranked together: the question is what is biggest, not
    // what is biggest of each kind.
    folders.extend(loose_files);
    folders.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    if folders.len() > MAX_FOLDERS {
        // Gathered rather than dropped, for the same reason the kinds are:
        // what is on screen has to add up to the total above it.
        let tail: Vec<FolderUsage> = folders.split_off(MAX_FOLDERS);
        let bytes = tail.iter().map(|folder| folder.bytes).sum();
        let files = tail.iter().map(|folder| folder.files).sum();
        folders.push(FolderUsage {
            // No name and no path: this row is a total, not an entry, and the
            // window must not offer to navigate to it.
            name: String::new(),
            path: PathBuf::new(),
            bytes,
            files,
            is_directory: false,
        });
    }
    folders
}

/// The kinds as rows, largest first, with the long tail gathered into one.
fn ranked_kinds(per_extension: HashMap<String, (u64, u64)>) -> Vec<KindUsage> {
    let mut kinds: Vec<KindUsage> = per_extension
        .into_iter()
        .map(|(extension, (bytes, files))| KindUsage {
            group_key: group_of(&extension),
            extension,
            bytes,
            files,
        })
        .collect();
    kinds.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    if kinds.len() > MAX_KINDS {
        // The tail becomes one row rather than being dropped, so the numbers
        // on screen still add up to the total above them.
        let tail: Vec<KindUsage> = kinds.split_off(MAX_KINDS);
        let bytes = tail.iter().map(|kind| kind.bytes).sum();
        let files = tail.iter().map(|kind| kind.files).sum();
        kinds.push(KindUsage {
            extension: String::new(),
            group_key: "usage.kind.rest",
            bytes,
            files,
        });
    }
    kinds
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jtf-usage-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn write(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        let mut file = fs::File::create(path).expect("create");
        file.write_all(&vec![b'x'; bytes]).expect("write");
    }

    #[test]
    fn totals_every_file_beneath_the_root() {
        let root = temp_dir("total");
        write(&root.join("a.txt"), 10);
        write(&root.join("sub/b.txt"), 20);
        write(&root.join("sub/deeper/c.txt"), 30);

        let usage = analyse(&root, &CancellationToken::never());
        assert_eq!(usage.bytes, 60);
        assert_eq!(usage.files, 3);
        assert!(!usage.partial);
    }

    /// The point of the folder breakdown: one row per branch, everything
    /// beneath it counted into it, biggest first.
    #[test]
    fn each_child_carries_everything_beneath_it() {
        let root = temp_dir("children");
        write(&root.join("small/a"), 10);
        write(&root.join("big/b"), 100);
        write(&root.join("big/deeper/c"), 200);

        let usage = analyse(&root, &CancellationToken::never());
        let names: Vec<&str> = usage.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["big", "small"], "largest first");
        assert_eq!(usage.folders[0].bytes, 300, "everything beneath big");
        assert_eq!(usage.folders[0].files, 2);
        assert_eq!(usage.folders[1].bytes, 10);
    }

    /// A file sitting in the folder gets a row of its own, beside the child
    /// folders and ranked against them. One enormous file is exactly what a
    /// breakdown of folders alone would hide.
    #[test]
    fn files_in_the_folder_are_listed_beside_the_child_folders() {
        let root = temp_dir("loose");
        write(&root.join("big.bin"), 500);
        write(&root.join("sub/inside.bin"), 70);

        let usage = analyse(&root, &CancellationToken::never());
        let names: Vec<&str> = usage.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["big.bin", "sub"], "largest first, both kinds");

        let file = &usage.folders[0];
        assert!(!file.is_directory, "a file must not look like a folder");
        assert_eq!(file.bytes, 500);
        assert_eq!(file.files, 1);
        assert!(usage.folders[1].is_directory);

        // The rows are now the whole of the folder, so they add up on their
        // own - there is no separate remainder to add in.
        let listed: u64 = usage.folders.iter().map(|f| f.bytes).sum();
        assert_eq!(listed, usage.bytes);
        assert_eq!(usage.loose_bytes, 500, "still reported, for the caller");
    }

    #[test]
    fn groups_files_by_kind_largest_first() {
        let root = temp_dir("kinds");
        write(&root.join("clip.mp4"), 500);
        write(&root.join("photo.jpg"), 200);
        write(&root.join("notes.txt"), 10);

        let usage = analyse(&root, &CancellationToken::never());
        let order: Vec<&str> = usage.kinds.iter().map(|k| k.extension.as_str()).collect();
        assert_eq!(order, vec!["mp4", "jpg", "txt"]);
        assert_eq!(usage.kinds[0].group_key, "usage.kind.video");
        assert_eq!(usage.kinds[1].group_key, "usage.kind.image");
        assert_eq!(usage.kinds[2].group_key, "usage.kind.text");
    }

    /// The same extension in different folders is one kind, and case is not a
    /// distinction: `.JPG` and `.jpg` are the same thing to a person.
    #[test]
    fn a_kind_gathers_across_folders_and_ignores_case() {
        let root = temp_dir("gather");
        write(&root.join("a/one.JPG"), 100);
        write(&root.join("b/two.jpg"), 50);

        let usage = analyse(&root, &CancellationToken::never());
        assert_eq!(usage.kinds.len(), 1, "{:?}", usage.kinds);
        assert_eq!(usage.kinds[0].extension, "jpg");
        assert_eq!(usage.kinds[0].bytes, 150);
        assert_eq!(usage.kinds[0].files, 2);
    }

    #[test]
    fn a_file_with_no_extension_is_its_own_kind() {
        let root = temp_dir("noext");
        write(&root.join("README"), 42);
        let usage = analyse(&root, &CancellationToken::never());
        assert_eq!(usage.kinds[0].extension, "");
        assert_eq!(usage.kinds[0].group_key, "usage.kind.none");
    }

    /// The tail becomes one row instead of being dropped, so what is on screen
    /// still adds up to the total above it.
    #[test]
    fn the_long_tail_is_gathered_rather_than_dropped() {
        let root = temp_dir("tail");
        for index in 0..MAX_KINDS + 5 {
            write(&root.join(format!("file{index}.e{index}")), index + 1);
        }
        let usage = analyse(&root, &CancellationToken::never());
        assert_eq!(usage.kinds.len(), MAX_KINDS + 1);
        assert_eq!(
            usage.kinds.last().expect("tail").group_key,
            "usage.kind.rest"
        );
        let shown: u64 = usage.kinds.iter().map(|k| k.bytes).sum();
        assert_eq!(shown, usage.bytes, "the rows must add up to the total");
    }

    /// A folder with more children than anyone can read is capped, and the
    /// remainder becomes one row so the numbers still add up.
    #[test]
    fn a_very_wide_folder_is_capped_and_the_rest_gathered() {
        let root = temp_dir("wide");
        for index in 0..MAX_FOLDERS + 5 {
            write(&root.join(format!("child{index:04}/file")), index + 1);
        }
        let usage = analyse(&root, &CancellationToken::never());
        assert_eq!(usage.folders.len(), MAX_FOLDERS + 1);
        let gathered = usage.folders.last().expect("tail");
        assert!(
            gathered.name.is_empty(),
            "the tail row is a total, not a folder"
        );
        let shown: u64 = usage.folders.iter().map(|f| f.bytes).sum();
        assert_eq!(shown, usage.bytes, "the rows must add up to the total");
    }

    #[test]
    fn cancelling_says_the_answer_is_partial() {
        let root = temp_dir("cancel");
        write(&root.join("a.txt"), 10);
        let usage = analyse(&root, &CancellationToken::cancelled());
        assert!(usage.partial);
    }

    #[test]
    fn every_group_has_a_key_and_unknown_extensions_fall_through() {
        assert_eq!(group_of("mp4"), "usage.kind.video");
        assert_eq!(group_of("MP4-not-lowercased"), "usage.kind.other");
        assert_eq!(group_of("wibble"), "usage.kind.other");
        assert_eq!(group_of(""), "usage.kind.none");
    }
}
