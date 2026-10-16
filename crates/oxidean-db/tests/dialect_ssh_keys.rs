//! 09-02: `0009_ssh_keys` + ssh_public_keys (UNIQUE fingerprint) CRUD.

use oxidean_core::Role;
use oxidean_db::Database;

/// Expect sqlite `0009_ssh_keys.sql` with `ssh_public_keys` + UNIQUE fingerprint,
/// then create/find/list/touch/revoke (hard-delete) round-trip.
#[tokio::test]
async fn dialect_ssh_keys_migrate_0009_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0009_ssh_keys.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0009_ssh_keys.sql must exist (ssh_public_keys + UNIQUE fingerprint)"
    );
    assert!(
        sql.contains("ssh_public_keys"),
        "0009 must define ssh_public_keys"
    );
    for col in [
        "id",
        "user_id",
        "title",
        "public_key",
        "fingerprint",
        "key_type",
        "last_used_at",
        "created_at",
    ] {
        assert!(
            sql.contains(col),
            "0009 ssh_public_keys must mention column {col}"
        );
    }
    assert!(
        sql.to_ascii_uppercase().contains("UNIQUE") || sql.contains("unique"),
        "0009 fingerprint must be UNIQUE (D-SSH-05)"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("ssh_keys.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-ssh-owner",
            "sshowner@example.com",
            "sshowner",
            Some("hash"),
            "SSH Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner");

    let fp1 = "SHA256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let pk1 =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA laptop";
    db.create_ssh_key(
        "ssh-key-1",
        &owner.id,
        "laptop",
        pk1,
        fp1,
        "ssh-ed25519",
        true,
        true,
    )
    .await
    .expect("create ssh key");

    let found = db
        .find_ssh_key_by_fingerprint(fp1)
        .await
        .expect("find")
        .expect("key present");
    assert_eq!(found.id, "ssh-key-1");
    assert_eq!(found.title, "laptop");
    assert_eq!(found.fingerprint, fp1);
    assert_eq!(found.key_type, "ssh-ed25519");
    assert_eq!(found.public_key, pk1);
    assert!(found.can_authenticate);
    assert!(found.can_sign);

    let fp2 = "SHA256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let pk2 = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB ci";
    db.create_ssh_key(
        "ssh-key-2",
        &owner.id,
        "ci",
        pk2,
        fp2,
        "ssh-ed25519",
        true,
        true,
    )
    .await
    .expect("create second key");

    let listed = db.list_ssh_keys_for_user(&owner.id).await.expect("list");
    assert_eq!(listed.len(), 2, "both keys listed");
    assert_eq!(listed[0].id, "ssh-key-2", "newest first (created_at DESC)");
    assert_eq!(listed[1].id, "ssh-key-1");

    db.touch_ssh_key_last_used("ssh-key-1", "2030-01-02T03:04:05Z", Some("203.0.113.9"))
        .await
        .expect("touch");
    let touched = db
        .find_ssh_key_by_fingerprint(fp1)
        .await
        .expect("find touched")
        .expect("still present");
    assert!(touched.last_used_at.is_some());
    assert_eq!(touched.last_used_ip.as_deref(), Some("203.0.113.9"));

    db.revoke_ssh_key("ssh-key-1").await.expect("revoke");
    let after_revoke = db
        .find_ssh_key_by_fingerprint(fp1)
        .await
        .expect("find revoked");
    assert!(
        after_revoke.is_none(),
        "revoked SSH keys must hard-delete (no fingerprint resolve)"
    );

    let listed_after = db
        .list_ssh_keys_for_user(&owner.id)
        .await
        .expect("list after");
    assert_eq!(listed_after.len(), 1);
    assert_eq!(listed_after[0].id, "ssh-key-2");

    // UNIQUE fingerprint: duplicate insert must fail
    let dup = db
        .create_ssh_key(
            "ssh-key-dup",
            &owner.id,
            "dup",
            pk2,
            fp2,
            "ssh-ed25519",
            true,
            true,
        )
        .await;
    assert!(dup.is_err(), "duplicate fingerprint must be rejected");
}

/// Tri-dialect parity: postgres and mysql siblings must exist alongside sqlite.
#[test]
fn dialect_ssh_keys_tri_dialect_files() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!("{root}/{dialect}/0009_ssh_keys.sql");
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "missing {path} — tri-dialect 0009_ssh_keys required"
        );
        assert!(
            sql.contains("ssh_public_keys"),
            "{dialect} 0009_ssh_keys must mention ssh_public_keys"
        );
        assert!(
            sql.contains("fingerprint"),
            "{dialect} 0009_ssh_keys must mention fingerprint"
        );
    }
}
