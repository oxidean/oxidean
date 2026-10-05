//! AGT-03: `0039_instance_mcp_settings` exists for every dialect and keeps the
//! singleton-settings shape (id = 1 + nullable `enabled` override).

use std::path::PathBuf;

fn migrations_dir(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

fn find_mcp_migration(dialect: &str) -> Option<(PathBuf, String)> {
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
        sql.contains("instance_mcp_settings").then_some((path, sql))
    })
}

#[test]
fn dialect_mcp_settings_migration_parity() {
    for dialect in ["postgres", "mysql", "sqlite"] {
        let (path, sql) = find_mcp_migration(dialect)
            .unwrap_or_else(|| panic!("{dialect}: instance_mcp_settings migration missing"));
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with("0039_"),
            "{dialect}: migration must be numbered 0039_+ (found {name})"
        );
        assert!(sql.contains("enabled"), "{dialect}: must define enabled");
        assert!(sql.contains("id = 1"), "{dialect}: singleton guard");
        assert!(
            sql.contains("INSERT"),
            "{dialect}: must seed the singleton row"
        );
    }
}
