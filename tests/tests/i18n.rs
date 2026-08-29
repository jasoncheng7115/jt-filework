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

#[test]
fn zh_tw_is_actually_translated_rather_than_copied_from_english() {
    // A catalogue that passes parity by copying the English values is not a
    // translation. Labels that are genuinely identical across locales - "AI" -
    // are the exception, not the rule.
    let en = load(LocaleId::EN);
    let tw = load(LocaleId::ZH_TW);

    let identical: Vec<_> = en
        .keys()
        .filter(|k| {
            tw.get(k).map(jtf_core::i18n::Message::template)
                == en.get(k).map(jtf_core::i18n::Message::template)
        })
        .collect();

    assert!(
        identical.len() * 20 < en.len(),
        "{} of {} zh-TW values are identical to English: {identical:?}",
        identical.len(),
        en.len()
    );
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
