//! Tag protection rulesets migration parity (GIT-21).

use std::path::PathBuf;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_tag_protection_migration(dialect: &str) -> Option<(PathBuf, String)> {
    let dir = migrations_dir(dialect);
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    entries.sort();
    for path in entries {
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        if sql.contains("tag_protection_rules") {
            return Some((path, sql));
        }
    }
    None
}

fn assert_tag_protection_sql(dialect: &str, sql: &str) {
    assert!(
        sql.contains("tag_protection_rules"),
        "{dialect}: must define tag_protection_rules"
    );
    assert!(
        sql.contains("pattern"),
        "{dialect}: rule must include tag pattern column"
    );
    for col in ["allow_create", "allow_update", "allow_delete", "enforce_admins"] {
        assert!(sql.contains(col), "{dialect}: rule must include {col}");
    }
}

#[tokio::test]
async fn dialect_tag_protection_migrate_schema_presence() {
    let (path, sql) =
        find_tag_protection_migration("sqlite").expect("sqlite tag_protection migration");
    assert_tag_protection_sql("sqlite", &sql);
    assert!(path.extension().is_some_and(|ext| ext == "sql"));

    for dialect in ["postgres", "mysql"] {
        let (_p, dsql) = find_tag_protection_migration(dialect)
            .unwrap_or_else(|| panic!("missing {dialect} tag_protection migration"));
        assert_tag_protection_sql(dialect, &dsql);
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("tp.db").display());
    let db = oxidean_db::Database::connect(&url)
        .await
        .expect("connect");
    db.migrate()
        .await
        .expect("migrate must apply tag protection schema");
    let rules = db
        .list_tag_protection_rules("missing")
        .await
        .expect("list rules on empty");
    assert!(rules.is_empty());
}
