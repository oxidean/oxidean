//! DATABASE_URL-gated migrate+probe proof. CI sets DATABASE_URL per dialect leg;
//! local runs skip gracefully with no Docker (matches `Database::skipped()` philosophy).

use oxidean_db::{dialect, Database};

/// Both tests share one `DATABASE_URL` target; serialize them so SQLite's
/// single-writer model doesn't race two schema-touching connections.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => None,
    }
}

#[tokio::test]
async fn migrate_and_probe_round_trip() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let first = db.probe().await.expect("first probe");
    let second = db.probe().await.expect("second probe");

    assert_eq!(second.probe_count, first.probe_count + 1);
    assert_eq!(first.dialect, dialect::from_url(&url).unwrap().as_str());
    assert!(
        first.probed_at.ends_with('Z'),
        "probed_at not UTC-normalized: {}",
        first.probed_at
    );
    assert_eq!(
        first.probed_at.len(),
        20,
        "unexpected probed_at length: {}",
        first.probed_at
    );
}

/// `package_blobs.refcount` is INT4 on Postgres/MySQL — exercises the decode
/// path that must read it as i32 and widen (regression: i64 read panicked on PG).
#[tokio::test]
async fn package_blob_refcount_round_trip() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let digest = format!("sha256:dialect-probe-{}", std::process::id());
    db.upsert_package_blob(&digest, 42)
        .await
        .expect("upsert blob");
    let v = db
        .adjust_package_blob_refcount(&digest, 3)
        .await
        .expect("adjust refcount");
    assert_eq!(v, 3);
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;

    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("first migrate");
    db.migrate()
        .await
        .expect("second migrate should be a no-op");
}
