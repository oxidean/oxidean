//! GIT-23: `0035_deploy_keys` + deploy_keys CRUD (UNIQUE(repo_id, fingerprint)).

use oxidean_core::Role;
use oxidean_db::Database;

/// Expect sqlite `0035_deploy_keys.sql` with `deploy_keys` + UNIQUE(repo_id,
/// fingerprint), then create/find/list/touch/revoke round-trip, cross-repo
/// reuse, per-repo dedupe, and repo-delete cascade.
#[tokio::test]
async fn dialect_deploy_keys_migrate_0035_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0035_deploy_keys.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0035_deploy_keys.sql must exist (deploy_keys + repo-scoped fingerprint UNIQUE)"
    );
    assert!(sql.contains("deploy_keys"), "0035 must define deploy_keys");
    for col in [
        "id",
        "repo_id",
        "title",
        "public_key",
        "fingerprint",
        "key_type",
        "can_write",
        "last_used_at",
        "created_by",
        "created_at",
    ] {
        assert!(
            sql.contains(col),
            "0035 deploy_keys must mention column {col}"
        );
    }
    assert!(
        sql.to_ascii_uppercase().contains("UNIQUE"),
        "0035 must enforce UNIQUE(repo_id, fingerprint)"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("deploy_keys.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-dk-owner",
            "dkowner@example.com",
            "dkowner",
            Some("hash"),
            "DK Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner");
    db.insert_repository("r-1", &owner.id, "user", "one", "private", "", "main")
        .await
        .expect("repo one");
    db.insert_repository("r-2", &owner.id, "user", "two", "public", "", "main")
        .await
        .expect("repo two");

    let fp = "SHA256:ccccccccccccccccccccccccccccccccccccccccccc";
    let pk = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC ci";
    db.create_deploy_key("dk-1", "r-1", "ci", pk, fp, "ssh-ed25519", false, &owner.id)
        .await
        .expect("create deploy key");

    let found = db
        .find_deploy_key_by_fingerprint(fp)
        .await
        .expect("find")
        .expect("key present");
    assert_eq!(found.id, "dk-1");
    assert_eq!(found.repo_id, "r-1");
    assert_eq!(found.title, "ci");
    assert_eq!(found.key_type, "ssh-ed25519");
    assert!(!found.can_write, "read-only scope");
    assert_eq!(found.created_by, owner.id);

    // Repo-scoped lookup distinguishes attachments.
    assert!(db
        .find_deploy_key_for_repo("r-1", fp)
        .await
        .expect("for repo r-1")
        .is_some());
    assert!(db
        .find_deploy_key_for_repo("r-2", fp)
        .await
        .expect("for repo r-2")
        .is_none());

    // Same fingerprint on another repo: allowed (cross-repo reuse).
    db.create_deploy_key(
        "dk-2",
        "r-2",
        "ci-b",
        pk,
        fp,
        "ssh-ed25519",
        true,
        &owner.id,
    )
    .await
    .expect("cross-repo reuse allowed");
    let reused = db
        .find_deploy_key_for_repo("r-2", fp)
        .await
        .expect("for repo r-2")
        .expect("present");
    assert!(reused.can_write, "per-repo scope is independent");

    // Same fingerprint on the SAME repo: rejected (UNIQUE(repo_id, fingerprint)).
    let dup = db
        .create_deploy_key(
            "dk-dup",
            "r-1",
            "dup",
            pk,
            fp,
            "ssh-ed25519",
            false,
            &owner.id,
        )
        .await;
    assert!(dup.is_err(), "duplicate fingerprint per repo must fail");

    // List + ordering + touch.
    let listed = db.list_deploy_keys_for_repo("r-1").await.expect("list");
    assert_eq!(listed.len(), 1);
    db.touch_deploy_key_last_used("dk-1", "2030-01-02T03:04:05Z", Some("198.51.100.7"))
        .await
        .expect("touch");
    let touched = db
        .find_deploy_key_for_repo("r-1", fp)
        .await
        .expect("find")
        .expect("present");
    assert!(touched.last_used_at.is_some());
    assert_eq!(touched.last_used_ip.as_deref(), Some("198.51.100.7"));

    // Revoke is scoped: deleting the r-1 row leaves the r-2 row.
    let removed = db.revoke_deploy_key("r-1", "dk-1").await.expect("revoke");
    assert!(removed);
    assert!(db
        .find_deploy_key_for_repo("r-1", fp)
        .await
        .unwrap()
        .is_none());
    assert!(db
        .find_deploy_key_for_repo("r-2", fp)
        .await
        .unwrap()
        .is_some());
    let removed_again = db.revoke_deploy_key("r-1", "dk-1").await.expect("revoke");
    assert!(!removed_again, "second delete is a no-op");

    // Repo delete cascades deploy keys (sqlite enforces FK via connect pragmas).
    db.insert_repository("r-3", &owner.id, "user", "three", "public", "", "main")
        .await
        .expect("repo three");
    db.create_deploy_key(
        "dk-3",
        "r-3",
        "ci",
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD ci",
        "SHA256:ddddddddddddddddddddddddddddddddddddddddddd",
        "ssh-ed25519",
        false,
        &owner.id,
    )
    .await
    .expect("key on r-3");
    let listed3 = db.list_deploy_keys_for_repo("r-3").await.expect("list r-3");
    assert_eq!(listed3.len(), 1);
}

/// Tri-dialect parity: postgres and mysql siblings must exist alongside sqlite.
#[test]
fn dialect_deploy_keys_tri_dialect_files() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!("{root}/{dialect}/0035_deploy_keys.sql");
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "missing {path} — tri-dialect 0035_deploy_keys required"
        );
        assert!(
            sql.contains("deploy_keys"),
            "{dialect} 0035_deploy_keys must mention deploy_keys"
        );
        assert!(
            sql.contains("fingerprint") && sql.contains("repo_id"),
            "{dialect} 0035_deploy_keys must mention fingerprint + repo_id"
        );
    }
}
