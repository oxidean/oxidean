//! WI-0001: registry ↔ docs consistency. Every `EXPERIMENTS` key must have a
//! `### \`key\`` section in `docs/FEATURE-FLAGS.md`, and documented keys the
//! registry no longer ships are rejected (retired docs must leave with the
//! flag).

use std::collections::BTreeSet;

fn documented_keys() -> BTreeSet<String> {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/FEATURE-FLAGS.md"
    ))
    .expect("docs/FEATURE-FLAGS.md readable");
    doc.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("### `")?;
            let end = rest.find('`')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

#[test]
fn every_registry_flag_is_documented() {
    let documented = documented_keys();
    for def in oxidean_core::EXPERIMENTS {
        assert!(
            documented.contains(def.key),
            "experiment `{}` missing a `### `{}`` section in docs/FEATURE-FLAGS.md",
            def.key,
            def.key
        );
    }
}

#[test]
fn no_undocumented_flags() {
    let registered: BTreeSet<&str> = oxidean_core::EXPERIMENTS.iter().map(|d| d.key).collect();
    for key in documented_keys() {
        assert!(
            registered.contains(key.as_str()),
            "docs/FEATURE-FLAGS.md documents `{key}` which is not in the EXPERIMENTS registry"
        );
    }
}
