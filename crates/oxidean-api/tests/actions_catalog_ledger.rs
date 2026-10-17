//! Phase 0 — the catalog compatibility ledger stays honest.
//!
//! Every vendored workflow template gets a `compat` entry in
//! `assets/workflow-templates/catalog.json` produced by the same
//! `classify_template_content` the API uses for `unsupported_actions` /
//! `parse_error`. This test recomputes the classification from disk and fails
//! if the ledger has drifted (feature landed, parser changed, template
//! vendored). Regenerate with:
//!
//!   OXIDEAN_UPDATE_LEDGER=1 cargo test -p oxidean-api --test actions_catalog_ledger

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use oxidean_api::actions::templates::classify_template_content;

const STATUSES: &[&str] = &[
    "runnable",
    "partial",
    "unsupported-runtime",
    "reusable-workflow",
    "restricted",
    "parse_error",
];

fn templates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/workflow-templates")
}

/// Every vendored `<dir>/<stem>.yml|yaml` → its ledger key.
fn vendored_templates() -> BTreeMap<String, PathBuf> {
    let mut out = BTreeMap::new();
    for dir in std::fs::read_dir(templates_dir()).unwrap() {
        let dir = dir.unwrap().path();
        if !dir.is_dir() || dir.file_name().unwrap() == "icons" {
            continue;
        }
        for ent in std::fs::read_dir(&dir).unwrap() {
            let p = ent.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy();
            if !(name.ends_with(".yml") || name.ends_with(".yaml")) {
                continue;
            }
            let stem = name.rsplit_once('.').unwrap().0.to_string();
            let key = format!("{}/{stem}", dir.file_name().unwrap().to_string_lossy());
            out.insert(key, p);
        }
    }
    out
}

fn expected_compat() -> BTreeMap<String, serde_json::Value> {
    vendored_templates()
        .iter()
        .map(|(key, path)| {
            let content = std::fs::read_to_string(path).unwrap();
            let c = classify_template_content(&content);
            (
                key.clone(),
                serde_json::json!({
                    "status": c.status,
                    "missing_features": c.missing_features,
                    "unsupported_actions": c.unsupported_actions,
                    "warnings": c.warnings,
                }),
            )
        })
        .collect()
}

#[test]
fn ledger_matches_recomputation() {
    let catalog_path = templates_dir().join("catalog.json");
    let mut catalog: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&catalog_path).unwrap()).unwrap();
    let expected = expected_compat();

    if std::env::var_os("OXIDEAN_UPDATE_LEDGER").is_some() {
        catalog["compat"] = serde_json::to_value(&expected).unwrap();
        std::fs::write(
            &catalog_path,
            serde_json::to_string_pretty(&catalog).unwrap() + "\n",
        )
        .unwrap();
        eprintln!("ledger: rewrote compat for {} templates", expected.len());
    }

    let compat = catalog["compat"].as_object().cloned().unwrap_or_default();
    let ledger_keys: BTreeSet<_> = compat.keys().cloned().collect();
    let disk_keys: BTreeSet<_> = expected.keys().cloned().collect();
    assert_eq!(
        ledger_keys, disk_keys,
        "compat keys must cover every vendored template (run with OXIDEAN_UPDATE_LEDGER=1)"
    );

    let mut mismatches = Vec::new();
    for (key, want) in &expected {
        if compat.get(key) != Some(want) {
            mismatches.push(format!("{key}: ledger={:?} want={want}", compat.get(key)));
        }
    }
    assert!(
        mismatches.is_empty(),
        "compat ledger drifted (run with OXIDEAN_UPDATE_LEDGER=1):\n{}",
        mismatches.join("\n")
    );

    for (key, entry) in &compat {
        let status = entry["status"].as_str().unwrap_or_default();
        assert!(
            STATUSES.contains(&status),
            "{key}: unknown compat status '{status}'"
        );
        assert!(
            entry["missing_features"].is_array(),
            "{key}: missing_features must be an array"
        );
        assert!(
            entry["unsupported_actions"].is_array(),
            "{key}: unsupported_actions must be an array"
        );
        assert!(
            entry["warnings"].is_array(),
            "{key}: warnings must be an array"
        );
    }
}
