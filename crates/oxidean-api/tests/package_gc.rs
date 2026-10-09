//! Package blob GC refcount safety (D-PKG-08 / D-PKG-09).

use oxidean_api::packages::quota::gc_unref_blobs;
use oxidean_api::packages::store::{digest_of_bytes, put_blob};
use oxidean_db::Database;
use uuid::Uuid;

#[tokio::test]
async fn package_gc_keeps_shared_blob_and_removes_unref() {
    let dir = tempfile::tempdir().unwrap();
    let pkg_dir = dir.path().join("pkg");
    std::fs::create_dir_all(&pkg_dir).unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("gc.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();

    let bytes = b"shared-layer-bytes";
    let (digest, size) = put_blob(&pkg_dir, bytes, 1024 * 1024).unwrap();
    assert_eq!(digest, digest_of_bytes(bytes));

    let uid = "user-gc";
    let p1 = Uuid::new_v4().to_string();
    let p2 = Uuid::new_v4().to_string();
    db.insert_package(&p1, "user", uid, "img-a", "oci", "public", None, "")
        .await
        .unwrap();
    db.insert_package(&p2, "user", uid, "img-b", "oci", "public", None, "")
        .await
        .unwrap();
    let v1 = Uuid::new_v4().to_string();
    let v2 = Uuid::new_v4().to_string();
    db.insert_package_version(&v1, &p1, "v1", Some(&digest), "{}", None)
        .await
        .unwrap();
    db.insert_package_version(&v2, &p2, "v1", Some(&digest), "{}", None)
        .await
        .unwrap();
    db.upsert_package_blob(&digest, size as i64).await.unwrap();
    db.adjust_package_blob_refcount(&digest, 2).await.unwrap();
    db.add_package_blob_ref(&v1, &digest, "layer").await.unwrap();
    db.add_package_blob_ref(&v2, &digest, "layer").await.unwrap();

    // Drop one package version — shared blob must survive GC.
    let digests = db.list_package_version_blob_digests(&v1).await.unwrap();
    db.delete_package_version(&v1).await.unwrap();
    for d in digests {
        db.adjust_package_blob_refcount(&d, -1).await.unwrap();
    }

    std::env::set_var("OXIDEAN_PACKAGES_GC_GRACE_SECS", "0");
    let removed = gc_unref_blobs(&db, &pkg_dir).await.unwrap();
    assert_eq!(removed, 0, "shared blob must not GC");
    assert!(pkg_dir.join(digest.replace(':', "/")).exists() || store_exists(&pkg_dir, &digest));

    // Drop remaining ref — now GC can remove.
    let digests = db.list_package_version_blob_digests(&v2).await.unwrap();
    db.delete_package_version(&v2).await.unwrap();
    for d in digests {
        db.adjust_package_blob_refcount(&d, -1).await.unwrap();
    }
    let removed = gc_unref_blobs(&db, &pkg_dir).await.unwrap();
    assert_eq!(removed, 1);
    assert!(!store_exists(&pkg_dir, &digest));
}

fn store_exists(packages_dir: &std::path::Path, digest: &str) -> bool {
    oxidean_api::packages::store::blob_path(packages_dir, digest)
        .map(|p| p.exists())
        .unwrap_or(false)
}
