//! Docker-free SQLite proof: parent-dir creation (D-18) and migrate+probe round trip.
//! Runs everywhere, no env gating.

use oxidean_db::Database;

#[tokio::test]
async fn sqlite_creates_parent_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("nested/deeper/oxidean.db");
    let url = format!("sqlite:{}", db_path.display());

    let result = Database::connect(&url).await;
    assert!(result.is_ok(), "connect failed: {:?}", result.err());
    assert!(
        db_path.exists(),
        "sqlite file was not created at {db_path:?}"
    );
}

#[tokio::test]
async fn sqlite_probe_round_trip_in_tempdir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("oxidean.db");
    let url = format!("sqlite:{}", db_path.display());

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let first = db.probe().await.expect("first probe");
    let second = db.probe().await.expect("second probe");

    assert_eq!(second.probe_count, first.probe_count + 1);
    assert_eq!(first.dialect, "sqlite");
}
