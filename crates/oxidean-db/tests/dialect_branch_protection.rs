//! Branch protection + commit_statuses migration parity (ORG-05/06, D-01, D-11).

use std::path::PathBuf;

use sqlx::Row;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_protection_migration(dialect: &str) -> Option<(PathBuf, String)> {
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
        if sql.contains("branch_protection_rules") {
            return Some((path, sql));
        }
    }
    None
}

fn find_migration_containing(dialect: &str, needle: &str) -> Option<(PathBuf, String)> {
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
        if sql.contains(needle) {
            return Some((path, sql));
        }
    }
    None
}

fn assert_protection_sql(dialect: &str, sql: &str) {
    assert!(
        sql.contains("branch_protection_rules"),
        "{dialect}: must define branch_protection_rules"
    );
    assert!(
        sql.contains("pattern"),
        "{dialect}: rule must include branch pattern column"
    );
    assert!(
        sql.contains("commit_statuses"),
        "{dialect}: must define commit_statuses"
    );
    assert!(
        sql.contains("require_reviews") || sql.contains("required_approving_review_count"),
        "{dialect}: review columns required"
    );
}

#[tokio::test]
async fn dialect_branch_protection_migrate_schema_presence() {
    let (path, sql) =
        find_protection_migration("sqlite").expect("sqlite branch_protection migration");
    assert_protection_sql("sqlite", &sql);
    assert!(path.extension().is_some_and(|ext| ext == "sql"));

    for dialect in ["postgres", "mysql"] {
        let (_p, dsql) = find_protection_migration(dialect)
            .unwrap_or_else(|| panic!("missing {dialect} branch_protection migration"));
        assert_protection_sql(dialect, &dsql);
    }

    // GIT-22: require_signed_commits column migration exists in all dialects.
    for dialect in ["sqlite", "postgres", "mysql"] {
        let (_p, dsql) = find_migration_containing(dialect, "require_signed_commits")
            .unwrap_or_else(|| panic!("missing {dialect} require_signed_commits migration"));
        assert!(
            dsql.contains("branch_protection_rules"),
            "{dialect}: require_signed_commits must alter branch_protection_rules"
        );
    }

    for dialect in ["sqlite", "postgres", "mysql"] {
        let (_path, sql) = find_migration_containing(dialect, "require_code_owner_reviews")
            .unwrap_or_else(|| panic!("missing {dialect} require_code_owner_reviews migration"));
        let normalized = sql
            .to_ascii_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let expected_column = match dialect {
            "sqlite" => "require_code_owner_reviews integer not null default 0",
            "postgres" => "require_code_owner_reviews boolean not null default false",
            "mysql" => "require_code_owner_reviews tinyint(1) not null default 0",
            _ => unreachable!(),
        };
        assert!(
            normalized.contains("branch_protection_rules") && normalized.contains(expected_column),
            "{dialect}: CODEOWNERS protection must be a false-by-default branch_protection_rules column"
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("bp.db").display());
    let db = oxidean_db::Database::connect(&url).await.expect("connect");
    db.migrate()
        .await
        .expect("migrate must apply branch protection schema");
    let pool = sqlx::SqlitePool::connect(&url).await.expect("SQLite pool");
    let columns = sqlx::query("PRAGMA table_info(branch_protection_rules)")
        .fetch_all(&pool)
        .await
        .expect("branch protection column metadata");
    let codeowners_default = columns
        .iter()
        .find(|row| {
            row.try_get::<String, _>("name").ok().as_deref() == Some("require_code_owner_reviews")
        })
        .expect("CODEOWNERS protection column")
        .try_get::<Option<String>, _>("dflt_value")
        .expect("CODEOWNERS protection default");
    assert_eq!(codeowners_default.as_deref(), Some("0"));
    let rules = db
        .list_branch_protection_rules("missing")
        .await
        .expect("list rules on empty");
    assert!(rules.is_empty());
}
