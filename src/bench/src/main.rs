//! Measures the core against the budgets in `docs/TESTING.md` §8.2.
//!
//! `AGENTS.md` §18.3 requires these numbers to be measured rather than
//! assumed, on every platform, and recorded. This binary produces them.
//!
//! ```text
//! cargo run --release -p jtf-bench            # 100K
//! cargo run --release -p jtf-bench -- 1000000 # 1M
//! ```
//!
//! Fixtures are generated once into a cache directory and reused, because
//! creating a million files is slower than every measurement that follows it.
//! They are never created inside the repository or inside a synced folder.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a measurement tool reports to a person on stdout"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jtf_core::{FileEntry, Location};
use jtf_fs::{Batch, LocalProvider, Provider};
use jtf_jobs::CancellationToken;
use jtf_workspace::{sort_entries, SortKey, SortSpec};

/// One measured budget from `docs/TESTING.md` §8.2.
struct Budget {
    label: &'static str,
    limit: Duration,
}

const BUDGETS: &[Budget] = &[
    Budget {
        label: "first rows visible",
        limit: Duration::from_millis(150),
    },
    Budget {
        label: "full enumeration",
        limit: Duration::from_secs(2),
    },
    Budget {
        label: "sort",
        limit: Duration::from_millis(250),
    },
    Budget {
        label: "filter",
        limit: Duration::from_millis(100),
    },
];

fn budget(label: &str) -> Option<Duration> {
    BUDGETS.iter().find(|b| b.label == label).map(|b| b.limit)
}

fn main() {
    let count: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(100_000);

    let dir = fixture(count);
    println!("\x1b[1mjt-filework core benchmark\x1b[0m");
    println!("entries: {count}");
    println!("fixture: {}", dir.display());
    println!(
        "build:   {}\n",
        if cfg!(debug_assertions) {
            "debug (numbers are meaningless)"
        } else {
            "release"
        }
    );

    println!(
        "{:<34} {:>10}  {:>10}  verdict",
        "measurement", "result", "budget"
    );
    println!("{}", "-".repeat(74));

    let entries = measure_enumeration(&dir);
    measure_sorts(&entries);
    measure_filter(&entries);
    report_memory(&entries);
}

// ------------------------------------------------------------------ fixtures

fn cache_root() -> PathBuf {
    // Never inside the repository: build and fixture data must not end up in
    // a synced folder (docs/DEVELOPMENT_ENVIRONMENT.md).
    std::env::var_os("JTF_BENCH_DIR").map_or_else(
        || {
            std::env::var_os("HOME")
                .map_or_else(std::env::temp_dir, PathBuf::from)
                .join(".cache/jt-filework-bench")
        },
        PathBuf::from,
    )
}

/// A realistic mix of extensions, a few directories and varied name lengths.
/// A directory of identically named files would flatter sorting.
const EXTENSIONS: [&str; 8] = ["txt", "rs", "log", "png", "json", "md", "zip", ""];

fn fixture(count: usize) -> PathBuf {
    let dir = cache_root().join(format!("flat-{count}"));
    let marker = dir.join(".complete");
    if marker.is_file() {
        return dir;
    }

    println!("generating {count} entries in {} ...", dir.display());
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture directory");

    let started = Instant::now();
    for i in 0..count {
        if i % 97 == 0 {
            let _ = fs::create_dir(dir.join(format!("dir-{i:07}")));
            continue;
        }
        let ext = EXTENSIONS[i % EXTENSIONS.len()];
        let name = if ext.is_empty() {
            format!("entry-{i:07}")
        } else {
            format!("entry-{i:07}-{}.{ext}", "x".repeat(i % 17))
        };
        let _ = fs::write(dir.join(name), b"");
    }
    let _ = fs::write(&marker, b"");
    println!("generated in {:.1}s\n", started.elapsed().as_secs_f64());
    dir
}

// -------------------------------------------------------------- measurements

fn row(label: &str, taken: Duration) {
    let millis = taken.as_secs_f64() * 1000.0;
    match budget(label) {
        Some(limit) => {
            let limit_ms = limit.as_secs_f64() * 1000.0;
            let ok = taken <= limit;
            println!(
                "{label:<34} {millis:>8.1}ms  {limit_ms:>8.0}ms  {}",
                if ok {
                    "\x1b[32mpass\x1b[0m"
                } else {
                    "\x1b[31mFAIL\x1b[0m"
                }
            );
        }
        None => println!("{label:<34} {millis:>8.1}ms  {:>10}  -", ""),
    }
}

fn measure_enumeration(dir: &Path) -> Vec<FileEntry> {
    let provider = LocalProvider::new();
    let location = Location::local(dir);

    let started = Instant::now();
    let handle = provider
        .enumerate_async(&location)
        .expect("start enumeration");

    let mut first_batch = None;
    let mut entries = Vec::new();
    while let Some(batch) = handle.recv() {
        match batch {
            Batch::Rows(rows) => {
                if first_batch.is_none() {
                    first_batch = Some(started.elapsed());
                }
                entries.extend(rows);
            }
            Batch::Done { .. } => break,
            Batch::Failed(error) => {
                eprintln!("enumeration failed: {error}");
                std::process::exit(1);
            }
        }
    }
    let full = started.elapsed();

    row("first rows visible", first_batch.unwrap_or(full));
    row("full enumeration", full);

    // The synchronous path, for comparison only. It is not what the UI uses.
    let started = Instant::now();
    let sync = provider
        .list(&location, &CancellationToken::never())
        .expect("list");
    row("full enumeration (blocking)", started.elapsed());
    assert_eq!(sync.len(), entries.len(), "the two paths must agree");

    println!();
    entries
}

fn measure_sorts(entries: &[FileEntry]) {
    // Exactly the function the application calls, so the number means
    // something (jtf_workspace::sort_entries).
    for (label, key) in [
        ("sort by name", SortKey::Name),
        ("sort by size", SortKey::Size),
        ("sort by modified", SortKey::Modified),
        ("sort by extension", SortKey::Extension),
    ] {
        let mut copy = entries.to_vec();
        let spec = SortSpec {
            key,
            ascending: true,
        };
        let started = Instant::now();
        sort_entries(&mut copy, spec);
        let taken = started.elapsed();
        // Every sort is measured against the one "sort" budget.
        let limit = budget("sort").expect("sort budget");
        let millis = taken.as_secs_f64() * 1000.0;
        println!(
            "{label:<34} {millis:>8.1}ms  {:>8.0}ms  {}",
            limit.as_secs_f64() * 1000.0,
            if taken <= limit {
                "\x1b[32mpass\x1b[0m"
            } else {
                "\x1b[31mFAIL\x1b[0m"
            }
        );
    }
    println!();
}

fn measure_filter(entries: &[FileEntry]) {
    for (label, needle) in [
        ("filter substring 'entry-0001'", "entry-0001"),
        ("filter substring '.log'", ".log"),
    ] {
        let started = Instant::now();
        let matched = entries
            .iter()
            .filter(|e| e.display_name().to_lowercase().contains(needle))
            .count();
        let taken = started.elapsed();
        let limit = budget("filter").expect("filter budget");
        let millis = taken.as_secs_f64() * 1000.0;
        println!(
            "{label:<34} {millis:>8.1}ms  {:>8.0}ms  {}   ({matched} matched)",
            limit.as_secs_f64() * 1000.0,
            if taken <= limit {
                "\x1b[32mpass\x1b[0m"
            } else {
                "\x1b[31mFAIL\x1b[0m"
            }
        );
    }
    println!();
}

fn report_memory(entries: &[FileEntry]) {
    // A shallow figure: the vector plus each entry's own heap strings. It is
    // an estimate and is labelled as one, which is more useful than a precise
    // number of the wrong thing.
    let shallow = std::mem::size_of_val(entries);
    let names: usize = entries.iter().map(|e| e.raw_name().as_os_str().len()).sum();
    let paths: usize = entries
        .iter()
        .filter_map(|e| e.location().as_path())
        .map(|p| p.as_os_str().len())
        .sum();
    let total = shallow + names + paths;

    println!("model footprint (estimate)");
    println!("  FileEntry struct           {:>10.1} MB", mb(shallow));
    println!("  names                      {:>10.1} MB", mb(names));
    println!("  paths                      {:>10.1} MB", mb(paths));
    println!(
        "  total                      {:>10.1} MB  ({:.0} bytes/entry)",
        mb(total),
        bytes_per_entry(total, entries.len())
    );
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a reported average, not a computation"
)]
fn bytes_per_entry(total: usize, entries: usize) -> f64 {
    if entries == 0 {
        return 0.0;
    }
    total as f64 / entries as f64
}

#[allow(
    clippy::cast_precision_loss,
    reason = "reporting megabytes to one decimal"
)]
fn mb(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
