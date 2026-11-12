//! WI-0001: `0100_user_experiments` exists for every dialect with the same
//! `(user_id, flag_key)` PK shape and a cascading user FK.

use std::path::PathBuf;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_experiments_migration(dialect: &str) -> Option<(PathBuf, String)> {
    let dir = migrations_dir(dialect);
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    entries.sort();
    entries.into_iter().find_map(|path| {
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        sql.contains("user_experiments").then_some((path, sql))
    })
}

#[test]
fn dialect_user_experiments_migration_parity() {
    for dialect in ["postgres", "mysql", "sqlite"] {
        let (path, sql) = find_experiments_migration(dialect)
            .unwrap_or_else(|| panic!("{dialect}: user_experiments migration missing"));
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("0100_"),
            "{dialect}: migration must be numbered 0100_+ (found {name})"
        );
        let lower = sql.to_lowercase();
        assert!(
            lower.contains("primary key (user_id, flag_key)"),
            "{dialect}: composite (user_id, flag_key) PK"
        );
        assert!(
            lower.contains("references users(id)"),
            "{dialect}: user FK required"
        );
        assert!(
            lower.contains("on delete cascade"),
            "{dialect}: flags must not outlive the user row"
        );
    }
}
