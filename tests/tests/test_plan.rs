//! The UI test plan is a document the release gate reads.
//!
//! A plan with two cases called MARK-010 cannot be referred to, and a plan
//! that still describes a rule the program no longer follows is worse than no
//! plan: someone runs it, it disagrees with the build, and the plan loses.
//!
//! `docs/UI_TEST_PLAN.md` §22.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn plan() -> String {
    let path = repo_root().join("docs/UI_TEST_PLAN.md");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Rows that begin with a case id: `| MARK-002 | ... | H2 |`.
fn cases(text: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }
        let id = cells[0];
        let looks_like_id = id.split_once('-').is_some_and(|(prefix, number)| {
            !prefix.is_empty()
                && prefix
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && !number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit())
        });
        if looks_like_id {
            out.push((id.to_string(), cells[1].to_string(), cells[2].to_string()));
        }
    }
    out
}

#[test]
fn the_plan_has_cases_at_all() {
    // A parser that silently matches nothing would make every test below pass.
    let found = cases(&plan());
    assert!(
        found.len() > 300,
        "only {} cases parsed out of the plan; the reader is probably broken",
        found.len()
    );
}

/// Two cases with one id cannot both be referred to, and the one that gets
/// referred to is whichever the reader saw first.
#[test]
fn no_case_id_is_used_twice() {
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, case, _) in cases(&plan()) {
        seen.entry(id).or_default().push(case);
    }
    let dupes: Vec<(&String, &Vec<String>)> =
        seen.iter().filter(|(_, uses)| uses.len() > 1).collect();
    assert!(dupes.is_empty(), "case ids used more than once: {dupes:#?}");
}

/// Every case says which harness layer proves it. A case with no layer is a
/// wish rather than a test (`docs/UI_TEST_PLAN.md` §0.1).
///
/// The performance table is exempt: its rows are scenarios run under the
/// watchdog rather than cases with a layer, and it says so in its own heading.
#[test]
fn every_case_names_a_layer() {
    let allowed = ["H0", "H1", "H2", "H3", "H4", "H5", "H6"];
    for (id, case, layer) in cases(&plan()) {
        if id.starts_with("PERF-") {
            continue;
        }
        // A case for something not built yet is listed without a layer, so it
        // cannot be mistaken for one that passes.
        if layer == "—" {
            continue;
        }
        let named: Vec<&str> = layer.split('/').map(str::trim).collect();
        assert!(
            !named.is_empty() && named.iter().all(|l| allowed.contains(l)),
            "{id} ({case}) names layer {layer:?}, which is not one of {allowed:?}. \
             Manual cases are H5; a case for something not built carries an em dash."
        );
    }
}

/// A case has to say something. An empty description is an id nobody can run.
#[test]
fn every_case_says_what_it_checks() {
    for (id, case, _) in cases(&plan()) {
        assert!(
            case.len() > 15,
            "{id} says only {case:?}, which is not a case anyone could carry out"
        );
    }
}

/// The plan and the rule it is written against must agree about marking.
///
/// `AGENTS.md` §10 was changed - selection *is* the mark - and the plan kept
/// asserting the opposite for a long time afterwards. This is the specific
/// disagreement that happened; the test exists so it cannot happen quietly
/// again.
#[test]
fn the_plan_does_not_contradict_the_marking_rule() {
    let agents = fs::read_to_string(repo_root().join("AGENTS.md")).unwrap();
    let unified = agents.contains("Selection is the mark")
        || agents.contains("selection is the mark")
        || agents.contains("選取就是標記");
    if !unified {
        return; // the rule is the other way; nothing to check
    }
    let text = plan();
    for wrong in [
        "Mark toggle does not change selection",
        "Selection change does not change marks",
        "the two states stay separate",
    ] {
        assert!(
            !text.contains(wrong),
            "the plan still says {wrong:?}, which the marking rule no longer allows"
        );
    }
}
