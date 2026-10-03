//! Linguist-lite language breakdown for the About sidebar.
//!
//! Linguist aggregates included file bytes by detected language. We approximate
//! that with the shared `oxidean_core::languages` table (extensions + filenames),
//! vendored-path skips, and prose/data exclusions (`stats: false` rows) —
//! enough for the About bar without shipping full linguist.

use std::collections::HashMap;

use oxidean_core::languages::{language_for_path, language_named};
use oxidean_core::RepoLanguageStat;

/// Soft cap on blobs considered (matches git backend clamp).
pub const MAX_BLOBS: u32 = 50_000;

/// Max distinct languages returned (the long tail collapses into “Other”).
const MAX_NAMED: usize = 8;

/// Aggregate sized blobs into sorted language stats (largest first).
pub fn aggregate_language_stats<'a, I>(blobs: I) -> Vec<RepoLanguageStat>
where
    I: IntoIterator<Item = (&'a str, u64)>,
{
    let mut totals: HashMap<&'static str, u64> = HashMap::new();
    for (path, size) in blobs {
        if size == 0 || is_vendored_path(path) {
            continue;
        }
        if let Some(spec) = language_for_path(path) {
            if spec.stats {
                *totals.entry(spec.stat_name()).or_insert(0) += size;
            }
        }
    }

    let mut rows: Vec<(&str, u64)> = totals.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    if rows.is_empty() {
        return Vec::new();
    }

    let mut languages = Vec::new();
    let mut other_bytes: u64 = 0;
    for (i, (name, bytes)) in rows.into_iter().enumerate() {
        if i < MAX_NAMED {
            languages.push(RepoLanguageStat {
                name: name.to_string(),
                bytes,
                color: language_named(name).and_then(|s| s.color).map(str::to_string),
            });
        } else {
            other_bytes = other_bytes.saturating_add(bytes);
        }
    }
    if other_bytes > 0 {
        languages.push(RepoLanguageStat {
            name: "Other".into(),
            bytes: other_bytes,
            color: Some("#ededed".into()),
        });
    }
    languages
}

pub(crate) fn is_vendored_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
    for seg in &segments {
        match *seg {
            "node_modules" | "vendor" | "third_party" | "third-party" | "bower_components"
            | "dist" | "build" | "target" | "out" | ".yarn" | "__pycache__" | ".tox"
            | "coverage" | ".next" | "pods" | "carthage" | ".git" | "venv" | ".venv" => {
                return true;
            }
            _ => {}
        }
    }
    let file = segments.last().copied().unwrap_or("");
    matches!(
        file,
        "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lock"
            | "bun.lockb"
            | "cargo.lock"
            | "composer.lock"
            | "gemfile.lock"
            | "poetry.lock"
            | "go.sum"
    ) || file.ends_with(".min.js")
        || file.ends_with(".min.css")
        || file.ends_with(".map")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_by_extension_and_sorts() {
        let blobs = [
            ("src/main.rs", 1000u64),
            ("src/lib.rs", 500),
            ("web/app.ts", 200),
            ("web/util.js", 100),
            ("README.md", 9999), // prose — excluded
            ("data.json", 8000), // data — excluded
            ("node_modules/x.js", 50_000), // vendored
        ];
        let stats = aggregate_language_stats(blobs.iter().map(|(p, s)| (*p, *s)));
        assert_eq!(stats[0].name, "Rust");
        assert_eq!(stats[0].bytes, 1500);
        assert_eq!(stats[1].name, "TypeScript");
        assert_eq!(stats[1].bytes, 200);
        assert_eq!(stats[2].name, "JavaScript");
        assert_eq!(stats[2].bytes, 100);
        assert!(stats.iter().all(|s| s.name != "Markdown"));
    }

    #[test]
    fn empty_when_no_code() {
        let blobs = [("README.md", 10u64), ("a.json", 20)];
        assert!(aggregate_language_stats(blobs.iter().map(|(p, s)| (*p, *s))).is_empty());
    }

    #[test]
    fn folds_tail_into_other() {
        let mut blobs = Vec::new();
        // 10 languages with decreasing sizes
        let langs = [
            ("a.rs", 1000u64),
            ("b.ts", 900),
            ("c.py", 800),
            ("d.go", 700),
            ("e.rb", 600),
            ("f.java", 500),
            ("g.kt", 400),
            ("h.swift", 300),
            ("i.lua", 200),
            ("j.dart", 100),
        ];
        for (p, s) in langs {
            blobs.push((p, s));
        }
        let stats = aggregate_language_stats(blobs.iter().map(|(p, s)| (*p, *s)));
        assert_eq!(stats.len(), 9); // 8 named + Other
        assert_eq!(stats.last().unwrap().name, "Other");
        assert_eq!(stats.last().unwrap().bytes, 300); // lua+dart
    }

    #[test]
    fn tsrx_is_first_class_language() {
        let blobs = [
            ("ui/app.tsrx", 500u64),
            ("ui/util.ts", 100),
            ("src/main.rs", 50),
        ];
        let stats = aggregate_language_stats(blobs.iter().map(|(p, s)| (*p, *s)));
        assert_eq!(stats[0].name, "TSRX");
        assert_eq!(stats[0].bytes, 500);
        assert_eq!(stats[0].color.as_deref(), Some("#6f00ff"));
        assert_eq!(stats[1].name, "TypeScript");
        assert_eq!(stats[1].bytes, 100);
    }
}
