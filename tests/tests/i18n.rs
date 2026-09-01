//! Locale conformance — `docs/TESTING.md` §3.3 and §4.
//!
//! Two different guarantees, and both matter:
//!
//! - **Parity**: `en` and `zh-TW` define exactly the same keys, with exactly
//!   the same placeholders. A locale that is missing a key silently falls back
//!   to English in the product, which is a bug the user sees, not the
//!   developer.
//! - **Coverage**: every key the *code* can ask for actually exists. Parity
//!   alone would happily pass on two identically empty catalogues.

// A test asserts by panicking, so the workspace's unwrap/expect/panic lints
// are exactly backwards here: an `unwrap` that fails *is* the failure report.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use jtf_commands::{Command, CommandCategory, CommandRegistry};
use jtf_core::i18n::{Catalog, LocaleId, Localizer};
use jtf_core::theme::ThemeMode;
use jtf_core::ErrorCode;
use jtf_jobs::{JobKind, JobState};
use jtf_viewer::ContentKind;
use jtf_workspace::{Column, Orientation, RestoreOnLaunch, RestoreOutcome};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Load and merge every catalogue file for a locale.
fn load(locale: &str) -> Catalog {
    let dir = repo_root().join("locales").join(locale);
    let mut catalog = Catalog::new(LocaleId::new(locale));
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "catalog"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no catalogue files in {}", dir.display());

    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let parsed = Catalog::parse(LocaleId::new(locale), &text)
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        catalog
            .merge(parsed)
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    }
    catalog
}

/// Every localization key the code can ask for, gathered from the typed
/// accessors rather than from a hand-maintained list — so adding a variant
/// without a translation fails here.
fn keys_used_by_code() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();

    for code in ErrorCode::ALL {
        keys.insert(code.message_key().to_string());
    }
    for mode in ThemeMode::ALL {
        keys.insert(mode.label_key().to_string());
    }
    for state in JobState::ALL {
        keys.insert(state.label_key().to_string());
    }
    for kind in JobKind::ALL {
        keys.insert(kind.label_key().to_string());
    }
    for column in Column::ALL {
        keys.insert(column.label_key().to_string());
    }
    // What the type column calls each kind of file. `label_key` returns a
    // literal rather than calling `tr_`, so the source scan below cannot see
    // these; walking the enum is the only thing that catches a kind added
    // without its string.
    for kind in ContentKind::ALL {
        keys.insert(kind.label_key().to_string());
    }
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        keys.insert(orientation.label_key().to_string());
    }
    for startup in [
        RestoreOnLaunch::LastSession,
        RestoreOnLaunch::HomeLocation,
        RestoreOnLaunch::FixedLocation {
            location: jtf_core::Location::local("/"),
        },
    ] {
        keys.insert(startup.label_key().to_string());
    }
    for outcome in [
        RestoreOutcome::Unreadable(ErrorCode::ParseFailed),
        RestoreOutcome::UnsupportedVersion(2),
    ] {
        if let Some(key) = outcome.notice_key() {
            keys.insert(key.to_string());
        }
    }
    for key in [
        "target.marked",
        "target.selection",
        "target.active",
        "target.empty",
    ] {
        keys.insert(key.to_string());
    }

    let registry = CommandRegistry::baseline();
    for command in registry.iter() {
        keys.insert(Command::label_key(command).to_string());
    }
    for category in [
        CommandCategory::Workspace,
        CommandCategory::Tabs,
        CommandCategory::Navigation,
        CommandCategory::File,
        CommandCategory::SelectionAndMarks,
        CommandCategory::View,
        CommandCategory::Search,
        CommandCategory::Ai,
        CommandCategory::Jobs,
        CommandCategory::Settings,
    ] {
        keys.insert(category.label_key().to_string());
    }

    keys
}

fn placeholders(catalog: &Catalog) -> BTreeMap<String, BTreeSet<String>> {
    catalog
        .keys()
        .map(|k| {
            let message = catalog.get(k).unwrap();
            (k.to_string(), message.placeholders().clone())
        })
        .collect()
}

#[test]
fn catalogues_parse() {
    assert!(!load(LocaleId::EN).is_empty());
    assert!(!load(LocaleId::ZH_TW).is_empty());
}

#[test]
fn en_and_zh_tw_define_exactly_the_same_keys() {
    let en = load(LocaleId::EN);
    let tw = load(LocaleId::ZH_TW);

    let en_keys: BTreeSet<_> = en.keys().collect();
    let tw_keys: BTreeSet<_> = tw.keys().collect();

    let missing_in_tw: Vec<_> = en_keys.difference(&tw_keys).collect();
    let missing_in_en: Vec<_> = tw_keys.difference(&en_keys).collect();

    assert!(
        missing_in_tw.is_empty(),
        "missing from zh-TW: {missing_in_tw:?}"
    );
    assert!(
        missing_in_en.is_empty(),
        "missing from en: {missing_in_en:?}"
    );
}

#[test]
fn placeholders_match_across_locales() {
    // A translated message that drops {count} produces a sentence with a hole
    // in it; one that invents a placeholder renders it literally.
    let en = placeholders(&load(LocaleId::EN));
    let tw = placeholders(&load(LocaleId::ZH_TW));

    for (key, expected) in &en {
        let Some(actual) = tw.get(key) else {
            continue; // reported by the parity test
        };
        assert_eq!(actual, expected, "placeholder mismatch for `{key}`");
    }
}

#[test]
fn every_key_the_code_can_ask_for_exists_in_both_locales() {
    // Parity alone would pass on two identically empty catalogues. This is the
    // test that makes parity mean something.
    let en = load(LocaleId::EN);
    let tw = load(LocaleId::ZH_TW);

    let mut missing = Vec::new();
    for key in keys_used_by_code() {
        if !en.contains(&key) {
            missing.push(format!("en: {key}"));
        }
        if !tw.contains(&key) {
            missing.push(format!("zh-TW: {key}"));
        }
    }
    assert!(
        missing.is_empty(),
        "keys used by code but not translated: {missing:#?}"
    );
}

#[test]
fn no_key_is_translated_to_an_empty_string() {
    for locale in [LocaleId::EN, LocaleId::ZH_TW] {
        let catalog = load(locale);
        for key in catalog.keys() {
            assert!(
                !catalog.get(key).unwrap().template().trim().is_empty(),
                "{locale}: `{key}` is empty. A blank label is worse than an untranslated one."
            );
        }
    }
}

/// Keys whose value is the same in every locale because it is a technical
/// identifier or a proper noun, not prose.
///
/// An explicit list rather than a looser threshold: "UTF-8" is the same string
/// in Chinese, and pretending otherwise would mean translating it wrongly to
/// satisfy a test.
const UNTRANSLATABLE: &[&str] = &[
    "app.name",
    "command.category.ai",
    "content.pdf",
    "language.english",
    // A product name and a number. "Qt 6.11.1" is what a bug report needs to
    // say and is spelled that way in every language.
    "about.qt",
    // What is printed on the key. A hint strip that said 「空白鍵」 while the
    // keyboard says `Space` would be naming a different key.
    "key.space",
    "key.escape",
];

/// Prefixes whose values are standard names: encodings and line endings.
const UNTRANSLATABLE_PREFIXES: &[&str] = &["encoding.", "line_ending."];

fn is_untranslatable(key: &str) -> bool {
    UNTRANSLATABLE.contains(&key)
        || UNTRANSLATABLE_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix))
}

#[test]
fn zh_tw_is_actually_translated_rather_than_copied_from_english() {
    // A catalogue that passes parity by copying the English values is not a
    // translation. Technical identifiers are exempt by name, so the check
    // stays strict on everything that is actually prose.
    let en = load(LocaleId::EN);
    let tw = load(LocaleId::ZH_TW);

    let identical: Vec<_> = en
        .keys()
        .filter(|k| !is_untranslatable(k))
        .filter(|k| {
            tw.get(k).map(jtf_core::i18n::Message::template)
                == en.get(k).map(jtf_core::i18n::Message::template)
        })
        .collect();

    assert!(
        identical.is_empty(),
        "{} zh-TW values are copied from English: {identical:?}. \
         If one of these genuinely cannot be translated, add it to UNTRANSLATABLE \
         with a reason.",
        identical.len()
    );
}

#[test]
fn the_untranslatable_list_does_not_outlive_its_reason() {
    // An exemption for a key that no longer exists, or that now differs
    // between locales, is an exemption that should be deleted.
    let en = load(LocaleId::EN);
    let tw = load(LocaleId::ZH_TW);
    for key in UNTRANSLATABLE {
        assert!(en.contains(key), "exempted key no longer exists: {key}");
        assert_eq!(
            en.get(key).map(jtf_core::i18n::Message::template),
            tw.get(key).map(jtf_core::i18n::Message::template),
            "{key} is now translated; remove it from UNTRANSLATABLE"
        );
    }
}

#[test]
fn zh_tw_uses_taiwan_terminology() {
    // docs/I18N_THEME.md 5. Mainland-derived wording is the specific failure
    // this project cares about, so it is checked rather than hoped for.
    let tw = load(LocaleId::ZH_TW);
    let all: String = tw
        .keys()
        .filter_map(|k| tw.get(k))
        .map(jtf_core::i18n::Message::template)
        .collect();

    for (wrong, right) in [
        ("文件夹", "資料夾"),
        ("剪切", "剪下"),
        ("回收站", "資源回收筒"),
        ("默认", "預設"),
        ("设置", "設定"),
        ("选项", "選項"),
        ("标签页", "頁籤"),
        ("软件", "軟體"),
        ("信息", "資訊"),
    ] {
        assert!(
            !all.contains(wrong),
            "zh-TW contains `{wrong}`; Taiwan usage is `{right}` (docs/I18N_THEME.md 5)"
        );
    }

    for expected in [
        "檔案",
        "資料夾",
        "設定",
        "重新命名",
        "預覽",
        "搜尋",
        "頁籤",
        "分割",
    ] {
        assert!(
            all.contains(expected),
            "zh-TW never uses the expected term `{expected}`"
        );
    }
}

#[test]
fn the_localizer_resolves_real_catalogues_with_english_fallback() {
    let en = load(LocaleId::EN);
    let localizer = Localizer::new(load(LocaleId::ZH_TW), en);

    // A real key resolves in Chinese.
    let trash = localizer.text("command.file.trash").unwrap();
    assert!(trash.contains("資源回收筒"), "got {trash}");

    // Every code's message resolves without an error.
    for code in ErrorCode::ALL {
        let text = localizer.text(code.message_key()).unwrap();
        assert!(!text.is_empty());
    }
}

/// No Qt widget resolves a catalogue key through `QObject::tr`.
///
/// This one is worth a test because it fails *quietly*. `tr("inspector.kind")`
/// compiles, runs, and returns `"inspector.kind"` — Qt's translation system
/// has no `.ts` file for us, so it hands the key straight back and the panel
/// displays a dotted identifier where a word should be. The catalogue lives in
/// Rust (`AGENTS.md` §11), so every lookup goes through `jtf_tr`, which the
/// C++ side wraps as `tr_`.
#[test]
fn the_qt_layer_never_looks_up_a_catalogue_key_through_qt() {
    let cpp = repo_root().join("src/ui/qt6/cpp");
    let mut offences = Vec::new();
    for entry in fs::read_dir(&cpp)
        .expect("the Qt sources are missing")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "cpp" && e != "mm") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            // A catalogue key is `word.word`; Qt's own tr() is still used for
            // things like tooltips on standard dialogs, which is fine.
            let Some(rest) = find_bare_tr(line) else {
                continue;
            };
            if rest.contains('.') && !rest.contains(' ') {
                offences.push(format!("{}:{}: tr(\"{rest}\")", path.display(), number + 1));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "these resolve to the key itself; use tr_ (jtf_tr):\n  {}",
        offences.join("\n  ")
    );
}

/// The literal inside a `tr("...")` that is not part of a longer identifier.
fn find_bare_tr(line: &str) -> Option<&str> {
    let at = line.find("tr(\"")?;
    // `tr_("...")`, `QObject::tr`, `translate(` and friends all end in a
    // character that makes the call something other than a bare `tr(`.
    let preceded_by = line[..at].chars().next_back();
    if preceded_by.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':') {
        return None;
    }
    let rest = &line[at + 4..];
    rest.find('"').map(|end| &rest[..end])
}

/// No user-visible string names the keyboard mode after another product.
///
/// `docs/KEYBOARD_PROFILE.md`: the mode is `Single-Key Mode`. CView and WinCV
/// are the design's origin and a description of who will find it familiar —
/// they are not the name of a jt-filework feature, and claiming compatibility
/// we have not verified would be a claim about someone else's product.
///
/// The catalogues are checked rather than the source, because the catalogues
/// are what a user actually reads.
#[test]
fn the_keyboard_mode_is_not_named_after_another_product() {
    // Assembled so this file is not itself an offender.
    let (c, w) = ("CView", "WinCV");
    let forbidden = [
        format!("{c} Mode"),
        format!("{w} Mode"),
        format!("{c}-compatible"),
        format!("{c} compatibility"),
        format!("{c}-style Mode"),
    ];

    let mut offences = Vec::new();
    for locale in ["en", "zh-TW"] {
        let path = repo_root()
            .join("locales")
            .join(locale)
            .join("main.catalog");
        let text = fs::read_to_string(&path).expect("a catalogue is readable");
        for (number, line) in text.lines().enumerate() {
            for phrase in &forbidden {
                if line.contains(phrase.as_str()) {
                    offences.push(format!("{locale}:{}: {phrase}", number + 1));
                }
            }
        }
    }
    assert!(
        offences.is_empty(),
        "the mode is called Single-Key; see docs/KEYBOARD_PROFILE.md:\n  {}",
        offences.join("\n  ")
    );
}

/// Every column's header key exists in every catalogue.
///
/// A column added to the model without its catalogue entry does not fail
/// anywhere — the header simply shows the key, `column.accessed`, in the one
/// place a user is guaranteed to look. That shipped once.
#[test]
fn every_column_has_a_header_in_every_locale() {
    let mut missing = Vec::new();
    for locale in ["en", "zh-TW"] {
        let path = repo_root()
            .join("locales")
            .join(locale)
            .join("main.catalog");
        let text = fs::read_to_string(&path).expect("a catalogue is readable");
        for column in jtf_workspace::Column::ALL {
            let key = column.label_key();
            if !text
                .lines()
                .any(|line| line.starts_with(&format!("{key} =")))
            {
                missing.push(format!("{locale}: {key}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "columns with no header text: {missing:?}"
    );
}

/// Chinese text uses full-width punctuation.
///
/// A half-width comma between two Chinese characters is a typographic error,
/// not a style choice: it sets the wrong spacing and reads as a seam. Several
/// crept in while writing this catalogue by hand, and reading for them is not
/// something a person should have to do.
///
/// Half-width punctuation *inside* Latin runs is left alone — `GPL-3.0`,
/// `CView / WinCV` and a URL are all correct as they are.
#[test]
fn zh_tw_does_not_mix_half_width_punctuation_into_chinese() {
    // The marks that have a full-width form and a different meaning at half
    // width. The parenthesis is not here: a Latin aside like `(MIT)` is
    // conventionally half-width even in Chinese text.
    const HALF: [char; 6] = [',', ';', ':', '!', '?', '.'];

    let tw = load(LocaleId::ZH_TW);
    let is_han = |c: char| ('\u{4e00}'..='\u{9fff}').contains(&c);

    let mut wrong: Vec<String> = Vec::new();
    for key in tw.keys() {
        let Some(value) = tw.get(key).map(jtf_core::i18n::Message::template) else {
            continue;
        };
        let chars: Vec<char> = value.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if !HALF.contains(c) {
                continue;
            }
            let before = i.checked_sub(1).and_then(|j| chars.get(j)).copied();
            let after = chars.get(i + 1).copied();
            // Wrong only when it sits against Chinese on either side. A full
            // stop inside `0.1.0` or a colon inside `https://` is between
            // Latin and stays.
            if before.is_some_and(is_han) || after.is_some_and(is_han) {
                wrong.push(format!(
                    "{key}: …{}{c}{}…",
                    before.unwrap_or(' '),
                    after.unwrap_or(' ')
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "half-width punctuation against Chinese; use the full-width form:\n  {}",
        wrong.join("\n  ")
    );
}

/// Traditional Chinese as it is written in Taiwan, not as it is written
/// elsewhere.
///
/// The catalogue is 台灣繁體中文. The two written standards share most
/// characters and differ in vocabulary, and the differences are exactly the
/// words a file manager uses most - 檔案 and 文件, 資料夾 and 文件夾, 儲存
/// and 保存, 複製 and 拷貝. A reader notices immediately, and what they
/// notice is that the program was not written for them.
///
/// Checked here rather than left to review because it arrives one string at a
/// time, and one string at a time is exactly what review misses.
#[test]
fn the_chinese_is_the_one_taiwan_writes() {
    // Mainland usage on the left, what this project says on the right. Only
    // terms that are unambiguous: a word that both standards use for
    // different things - 文件, which is a document here and a file there -
    // cannot be caught by a substring and is left to a human.
    const INSTEAD: &[(&str, &str)] = &[
        ("前綴", "前置 / 開頭"),
        ("缺省", "預設"),
        ("默認", "預設"),
        ("軟件", "軟體"),
        ("硬件", "硬體"),
        ("內存", "記憶體"),
        ("網絡", "網路"),
        ("用戶", "使用者"),
        ("信息", "資訊"),
        ("屏幕", "螢幕"),
        ("視頻", "影片"),
        ("拷貝", "複製"),
        ("激活", "啟用"),
        ("端口", "連接埠"),
        ("磁盤", "磁碟"),
        ("卸載", "解除安裝"),
        ("服務器", "伺服器"),
        ("數據", "資料"),
        ("鏈接", "連結"),
        ("打開", "開啟"),
        ("保存", "儲存"),
        ("刷新", "重新整理"),
        ("剪貼板", "剪貼簿"),
        ("回車", "Enter"),
        ("字節", "位元組"),
        ("線程", "執行緒"),
        ("進程", "行程"),
        ("菜單", "選單"),
        ("窗口", "視窗"),
        ("文件夾", "資料夾"),
        ("查找", "尋找"),
        ("設置", "設定"),
        ("兼容", "相容"),
    ];

    // Everything a reader of Chinese ever sees.
    let surfaces = [
        "locales/zh-TW/main.catalog",
        "README_zh-TW.md",
        "CHANGELOG_zh-TW.md",
        "docs/index.html",
        "docs/features.html",
    ];

    let mut found = Vec::new();
    for surface in surfaces {
        let path = repo_root().join(surface);
        let Ok(text) = fs::read_to_string(&path) else {
            continue; // a surface that does not exist yet is not a failure
        };
        for (line_no, line) in text.lines().enumerate() {
            for (wrong, right) in INSTEAD {
                if line.contains(wrong) {
                    found.push(format!(
                        "{surface}:{}  「{wrong}」 should be 「{right}」\n    {}",
                        line_no + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        found.is_empty(),
        "Chinese that is not the Taiwan variety:\n{}",
        found.join("\n")
    );
}
