//! Package quota tests (D-PKG-09).

use oxidean_api::packages::quota::{check_can_store, DEFAULT_OWNER_QUOTA_BYTES};
use oxidean_api::packages::store::{put_blob, StoreError};
use oxidean_db::Database;

#[tokio::test]
async fn package_quota_rejects_over_owner_limit() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("q.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let uid = "user-1";
    db.upsert_package_quota_override("user", uid, 10)
        .await
        .unwrap();
    let err = check_can_store(&db, "user", uid, 11).await.unwrap_err();
    assert!(err.contains("quota"), "{err}");
    const { assert!(DEFAULT_OWNER_QUOTA_BYTES > 0) };
}

#[tokio::test]
async fn package_quota_max_blob_store_reject() {
    let dir = tempfile::tempdir().unwrap();
    let err = put_blob(dir.path(), b"abcdef", 4).unwrap_err();
    assert!(matches!(err, StoreError::TooLarge { .. }));
}
