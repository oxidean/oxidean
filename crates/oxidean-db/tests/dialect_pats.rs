//! 08-03: `0008_pats` + personal_access_tokens / personal_access_token_repos CRUD.

use oxidean_core::Role;
use oxidean_db::Database;

/// Expect sqlite `0008_pats.sql` with PAT tables (hash-at-rest; no plaintext column),
/// then create/find/list/revoke/touch round-trip.
#[tokio::test]
async fn dialect_pats_migrate_0008_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0008_pats.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0008_pats.sql must exist (personal_access_tokens + personal_access_token_repos)"
    );
    assert!(
        sql.contains("personal_access_tokens"),
        "0008 must define personal_access_tokens"
    );
    assert!(
        sql.contains("personal_access_token_repos"),
        "0008 must define personal_access_token_repos (FG selected repos)"
    );
    assert!(
        sql.contains("token_hash"),
        "0008 must store token_hash only (no plaintext at rest)"
    );
    assert!(
        !sql.to_ascii_lowercase().contains("plaintext")
            && !sql.contains("token_secret")
            && !sql.contains("raw_token"),
        "0008 must not store plaintext PAT columns"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pats.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-pat-owner",
            "patowner@example.com",
            "patowner",
            Some("hash"),
            "Pat Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner");

    let repo = db
        .insert_repository(
            "r-pat-1", &owner.id, "user", "pat_demo", "private", "pat demo", "main",
        )
        .await
        .expect("insert repo");

    let classic_hash = "a".repeat(64);
    db.create_pat(
        "pat-classic-1",
        &owner.id,
        "classic",
        "laptop",
        "oxidean_pat_",
        &classic_hash,
        Some(r#"["repo"]"#),
        None,
        None,
        None,
        &[],
    )
    .await
    .expect("create classic pat");

    let found = db
        .find_pat_by_token_hash(&classic_hash)
        .await
        .expect("find")
        .expect("classic present");
    assert_eq!(found.id, "pat-classic-1");
    assert_eq!(found.kind, "classic");
    assert_eq!(found.name, "laptop");
    assert_eq!(found.token_prefix, "oxidean_pat_");
    assert_eq!(found.scopes_json.as_deref(), Some(r#"["repo"]"#));
    assert!(found.revoked_at.is_none());

    let fg_hash = "b".repeat(64);
    db.create_pat(
        "pat-fg-1",
        &owner.id,
        "fine_grained",
        "ci",
        "oxidean_fg_",
        &fg_hash,
        None,
        Some("write"),
        Some("selected"),
        None,
        std::slice::from_ref(&repo.id),
    )
    .await
    .expect("create fg pat");

    let found_fg = db
        .find_pat_by_token_hash(&fg_hash)
        .await
        .expect("find fg")
        .expect("fg present");
    assert_eq!(found_fg.kind, "fine_grained");
    assert_eq!(found_fg.contents_perm.as_deref(), Some("write"));
    assert_eq!(found_fg.repo_access.as_deref(), Some("selected"));
    assert_eq!(found_fg.repository_ids, vec![repo.id.clone()]);

    let listed = db.list_pats_for_user(&owner.id).await.expect("list");
    assert_eq!(listed.len(), 2, "both active PATs listed");
    assert_eq!(listed[0].id, "pat-fg-1", "newest first (created_at DESC)");
    assert_eq!(listed[1].id, "pat-classic-1");

    db.touch_pat_last_used("pat-classic-1", "2030-01-02T03:04:05Z", Some("203.0.113.9"))
        .await
        .expect("touch");
    let touched = db
        .find_pat_by_token_hash(&classic_hash)
        .await
        .expect("find touched")
        .expect("still present");
    assert!(touched.last_used_at.is_some());
    assert_eq!(touched.last_used_ip.as_deref(), Some("203.0.113.9"));

    db.revoke_pat("pat-classic-1", "2030-01-03T00:00:00Z")
        .await
        .expect("revoke");
    let after_revoke = db
        .find_pat_by_token_hash(&classic_hash)
        .await
        .expect("find revoked");
    assert!(
        after_revoke.is_none(),
        "revoked PATs must not resolve by token_hash"
    );
    let listed_after = db
        .list_pats_for_user(&owner.id)
        .await
        .expect("list after revoke");
    assert_eq!(listed_after.len(), 1);
    assert_eq!(listed_after[0].id, "pat-fg-1");
}

/// Tri-dialect parity: postgres and mysql siblings must exist alongside sqlite.
#[test]
fn dialect_pats_tri_dialect_files() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!("{root}/{dialect}/0008_pats.sql");
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "missing {path} — tri-dialect 0008_pats required"
        );
        assert!(
            sql.contains("personal_access_tokens"),
            "{dialect} 0008 must mention personal_access_tokens"
        );
        assert!(
            sql.contains("personal_access_token_repos"),
            "{dialect} 0008 must mention personal_access_token_repos"
        );
        assert!(
            sql.contains("token_hash"),
            "{dialect} 0008 must mention token_hash"
        );
    }
}
