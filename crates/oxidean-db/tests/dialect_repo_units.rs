//! COL-13: `0043_repo_unit_toggles` — `repositories.issues_enabled` /
//! `pulls_enabled` per-repo unit flags (SQLite + Postgres + MySQL parity,
//! sqlite round-trip).

use std::path::PathBuf;

use oxidean_core::Role;
use oxidean_db::Database;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_unit_toggles_migration(dialect: &str) -> Option<(PathBuf, String)> {
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
        if sql.contains("issues_enabled") && sql.contains("pulls_enabled") {
            return Some((path, sql));
        }
    }
    None
}

#[tokio::test]
async fn dialect_repo_unit_toggles_schema_presence() {
    for dialect in ["sqlite", "postgres", "mysql"] {
        let (path, sql) = find_unit_toggles_migration(dialect)
            .unwrap_or_else(|| panic!("{dialect}: missing 0043_repo_unit_toggles migration"));
        assert!(
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains("repo_unit_toggles")),
            "{dialect}: unexpected migration path {}",
            path.display()
        );
        for needle in [
            "ALTER TABLE repositories",
            "issues_enabled",
            "pulls_enabled",
        ] {
            assert!(
                sql.contains(needle),
                "{dialect}: unit-toggles migration must define {needle}"
            );
        }
        // Disabled units must never delete data — flags only.
        assert!(
            !sql.to_lowercase().contains("drop table")
                && !sql.to_lowercase().contains("delete from"),
            "{dialect}: unit-toggles migration must not remove data"
        );
    }
}

#[tokio::test]
async fn sqlite_repo_unit_flags_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("units.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-unit-owner",
            "unitowner@example.com",
            "unitowner",
            Some("hash"),
            "Unit Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner");

    let repo = db
        .insert_repository(
            "r-unit-1",
            &owner.id,
            "user",
            "units-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");

    // Columns default enabled.
    let flags = db.get_repo_unit_flags(&repo.id).await.expect("flags");
    assert!(flags.issues_enabled);
    assert!(flags.pulls_enabled);

    db.set_repo_issues_enabled(&repo.id, false)
        .await
        .expect("disable issues");
    let flags = db.get_repo_unit_flags(&repo.id).await.expect("flags");
    assert!(!flags.issues_enabled);
    assert!(flags.pulls_enabled, "pulls unaffected by issues toggle");

    db.set_repo_pulls_enabled(&repo.id, false)
        .await
        .expect("disable pulls");
    let flags = db.get_repo_unit_flags(&repo.id).await.expect("flags");
    assert!(!flags.pulls_enabled);

    db.set_repo_issues_enabled(&repo.id, true)
        .await
        .expect("re-enable issues");
    db.set_repo_pulls_enabled(&repo.id, true)
        .await
        .expect("re-enable pulls");
    let flags = db.get_repo_unit_flags(&repo.id).await.expect("flags");
    assert!(flags.issues_enabled);
    assert!(flags.pulls_enabled);
}
