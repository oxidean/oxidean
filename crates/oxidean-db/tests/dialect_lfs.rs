//! LFS migration parity (GIT-12/13 schema) — greened with 0012_lfs.

use std::path::PathBuf;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_lfs_migration(dialect: &str) -> Option<(PathBuf, String)> {
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
        if sql.contains("lfs_objects") && sql.contains("lfs_object_links") {
            return Some((path, sql));
        }
    }
    None
}

fn assert_lfs_sql(dialect: &str, sql: &str) {
    assert!(
        sql.contains("lfs_enabled"),
        "{dialect}: migration must add repositories.lfs_enabled"
    );
    assert!(
        sql.contains("lfs_objects"),
        "{dialect}: must define lfs_objects"
    );
    assert!(
        sql.contains("lfs_object_links"),
        "{dialect}: must define lfs_object_links"
    );
}

#[tokio::test]
async fn dialect_lfs_migrate_schema_presence() {
    let (path, sql) = find_lfs_migration("sqlite").expect("sqlite LFS migration");
    assert_lfs_sql("sqlite", &sql);
    assert!(path.extension().is_some_and(|ext| ext == "sql"));

    for dialect in ["postgres", "mysql"] {
        let (_p, dsql) = find_lfs_migration(dialect)
            .unwrap_or_else(|| panic!("missing {dialect} LFS migration"));
        assert_lfs_sql(dialect, &dsql);
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("lfs.db").display());
    let db = oxidean_db::Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate must apply LFS schema");
    // Smoke: enable flag + empty object lookup.
    assert!(!db.get_repo_lfs_enabled("missing").await.unwrap());
}
