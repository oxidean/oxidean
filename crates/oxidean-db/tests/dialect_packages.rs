//! 20-02: `0015_packages` + packages / versions / blobs / refs / quota tables.

use oxidean_core::Role;
use oxidean_db::Database;

/// Expect sqlite `0015_packages.sql` with packages domain tables, then migrate applies.
#[tokio::test]
async fn dialect_packages_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0015_packages.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0015_packages.sql must exist (packages domain)"
    );
    for table in [
        "packages",
        "package_versions",
        "package_blobs",
        "package_blob_refs",
        "package_quota_overrides",
    ] {
        assert!(sql.contains(table), "0012 must define {}", table);
    }
    assert!(
        sql.contains("owner_type") && sql.contains("owner_id") && sql.contains("format"),
        "packages must carry owner + format (D-PKG-02)"
    );
    assert!(
        sql.contains("repository_id"),
        "packages must allow optional repository link"
    );
    assert!(
        sql.contains("refcount"),
        "package_blobs must track refcount (D-PKG-08)"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("packages.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    // Prove DB is usable after packages migration (insert a user — init schema intact).
    let _owner = db
        .create_user(
            "u-pkg-owner",
            "pkgowner@example.com",
            "pkgowner",
            Some("hash"),
            "Pkg Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner after packages migrate");
}

/// Deleting a package cascades versions; empty packages are hidden from owner
/// and repository listings (last-version delete removes the
/// package, and orphan rows never render).
#[tokio::test]
async fn dialect_packages_delete_and_empty_listing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("packages.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-pkg-owner",
            "pkgowner@example.com",
            "pkgowner",
            Some("hash"),
            "Pkg Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner");

    db.insert_package(
        "pkg-1", "user", &owner.id, "demo", "generic", "public", None, "",
    )
    .await
    .expect("insert package");
    db.insert_package_version("pv-1", "pkg-1", "1.0.0", None, "{}", Some(&owner.id))
        .await
        .expect("insert version");

    assert_eq!(
        db.list_packages_by_owner("user", &owner.id)
            .await
            .expect("list")
            .len(),
        1
    );

    db.delete_package_version("pv-1")
        .await
        .expect("delete version");

    // Empty package no longer lists for owner or repository queries.
    assert!(db
        .list_packages_by_owner("user", &owner.id)
        .await
        .expect("list empty")
        .is_empty());

    // Explicit package delete cascades remaining versions.
    db.insert_package(
        "pkg-2", "user", &owner.id, "demo2", "generic", "public", None, "",
    )
    .await
    .expect("insert package 2");
    db.insert_package_version("pv-2", "pkg-2", "2.0.0", None, "{}", Some(&owner.id))
        .await
        .expect("insert version 2");
    db.delete_package("pkg-2").await.expect("delete package");
    assert!(db
        .list_package_versions("pkg-2")
        .await
        .expect("versions")
        .is_empty());
    assert!(db
        .find_package_by_id("pkg-2")
        .await
        .expect("find")
        .is_none());
}
