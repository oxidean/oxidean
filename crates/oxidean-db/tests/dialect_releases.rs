//! Phase 15: `0014_releases_redirects` tri-dialect parity + round-trip.

use oxidean_core::Role;
use oxidean_db::Database;

#[tokio::test]
async fn dialect_releases_migrate_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0014_releases_redirects.sql"
    );
    let sql = std::fs::read_to_string(migration_path).expect("0014_releases_redirects.sql");
    assert!(sql.contains("releases"));
    assert!(sql.contains("release_assets"));
    assert!(sql.contains("repository_redirects"));

    for dialect in ["sqlite", "postgres", "mysql"] {
        let p = format!(
            "{}/migrations/{}/0014_releases_redirects.sql",
            env!("CARGO_MANIFEST_DIR"),
            dialect
        );
        assert!(std::path::Path::new(&p).exists(), "missing {p}");
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("releases.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate 0012");

    let user = db
        .create_user(
            "u-rel",
            "rel@example.com",
            "reluser",
            Some("hash"),
            "Rel",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository("r-rel-1", &user.id, "user", "demo", "public", "", "main")
        .await
        .expect("repo");
    let release = db
        .insert_release(
            "rel-1", &repo.id, "v1.0.0", "First", "notes", false, false, &user.id,
        )
        .await
        .expect("insert release");
    assert_eq!(release.tag_name, "v1.0.0");
    assert!(!release.draft);

    let asset = db
        .insert_release_asset(
            "asset-1",
            &release.id,
            "bin.tar.gz",
            "application/gzip",
            12,
            &user.id,
        )
        .await
        .expect("insert asset");
    assert_eq!(asset.filename, "bin.tar.gz");

    let redir = db
        .insert_repository_redirect(
            "redir-1",
            "oldowner",
            "oldname",
            &repo.id,
            "2099-01-01T00:00:00Z",
        )
        .await
        .expect("insert redirect");
    assert_eq!(redir.old_owner_slug, "oldowner");
    assert_eq!(redir.old_name, "oldname");
}

#[test]
fn dialect_releases_tri_dialect_files() {
    for dialect in ["sqlite", "postgres", "mysql"] {
        let p = format!(
            "{}/migrations/{}/0014_releases_redirects.sql",
            env!("CARGO_MANIFEST_DIR"),
            dialect
        );
        let sql = std::fs::read_to_string(&p).unwrap_or_default();
        assert!(!sql.is_empty(), "{dialect} 0012 missing");
        assert!(sql.contains("releases"));
        assert!(sql.contains("release_assets"));
        assert!(sql.contains("repository_redirects"));
    }
}
