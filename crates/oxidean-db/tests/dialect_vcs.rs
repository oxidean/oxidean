//! DATABASE_URL-gated VCS facade proof (u4 / TD4.8): `db.migrate()` runs the
//! real 0054 ALTERs on the dialect under test, then the facade is exercised —
//! changeset-create round trip + exact sentinel strings, every CAS arm, and
//! case-variant coexistence under the binary key collation (utf8mb4_bin is the
//! behavior the migrate-only CI lane cannot prove).
//!
//! MySQL additionally asserts `information_schema` collations + the widened
//! `index_key` length via a raw sqlx connection; PG/SQLite are bytewise by
//! construction so the case-variant round-trips carry the proof there.

use oxidean_core::Role;
use oxidean_db::{
    dialect, CasOutcome, Database, Dialect, VcsChangesetInsert, VcsDerivedIndexUpsert,
    VcsFileChangeInsert,
};

/// All tests share one `DATABASE_URL` target; serialize them so SQLite's
/// single-writer model doesn't race schema-touching connections.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => None,
    }
}

fn hash(byte: u8) -> String {
    format!("{:02x}", byte).repeat(32)
}

// vcs_changesets.id is a GLOBAL primary key — each test gets its own ids;
// a cross-test reuse suppresses the insert with no same-repo row to reselect.
const CS_A: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";
const CS_B: &str = "cs_02J9W5MHZ1M3C0R8Y6G7N8PQST";
const CS_RT: &str = "cs_11H8V4JGZ0K2B9Q7X3F5M6NPRT";
const CS_RT2: &str = "cs_12H8V4JGZ0K2B9Q7X3F5M6NPRW";
const CS_CV: &str = "cs_22H8V4JGZ0K2B9Q7X3F5M6NPQV";
const CS_MISSING: &str = "cs_03K0X6MJA2N4D1S9Z7H8P9QRTV";
/// `hash(0xcc)` as a 'static literal (borrowed by `VcsChangesetInsert`).
const MH: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn changeset<'a>(id: &'a str, repo_id: &'a str, parents: &'a str) -> VcsChangesetInsert<'a> {
    VcsChangesetInsert {
        id,
        repo_id,
        parents,
        author: "Dialect Test <dialect@example.com>",
        message: "dialect seed",
        committed_at: "2026-01-02T03:04:05Z",
        manifest_hash: MH,
        idem_key: None,
    }
}

async fn seed_repo(db: &Database, repo_id: &str) {
    let uid = format!("u-{repo_id}");
    db.create_user(
        &uid,
        &format!("{repo_id}@example.com"),
        &format!("u{repo_id}"),
        None,
        "Dialect Seed",
        "",
        None,
        Role::User,
    )
    .await
    .expect("seed user");
    db.insert_repository(repo_id, &uid, "user", repo_id, "private", "", "main")
        .await
        .expect("seed repo");
}

#[tokio::test]
async fn changeset_tx_round_trip_and_exact_sentinels() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repo = "dcs-sentinels";
    seed_repo(&db, repo).await;

    let blob = hash(0xaa);
    db.vcs_insert_object_if_absent(&blob, 7)
        .await
        .expect("object row");

    let upserts = [VcsDerivedIndexUpsert {
        kind: "path_touch",
        index_key: "pt\x01key",
        payload: "{}",
        covering_changeset: CS_RT,
    }];
    let changes = [VcsFileChangeInsert {
        path: "Src/Main.rs",
        kind: "add",
        mode: 33188,
        old_path: None,
        blob_hash: Some(&blob),
    }];
    let id = db
        .vcs_create_changeset_tx(
            &changeset(CS_RT, repo, "[]"),
            &changes,
            &[blob.as_str()],
            &upserts,
        )
        .await
        .expect("create_tx");
    assert_eq!(id, CS_RT);

    let row = db
        .vcs_get_changeset(repo, CS_RT)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(row.author, "Dialect Test <dialect@example.com>");
    assert_eq!(row.committed_at, "2026-01-02T03:04:05Z");
    assert_eq!(
        db.vcs_changeset_parents(repo, CS_RT)
            .await
            .expect("parents"),
        Vec::<String>::new()
    );
    let fcs = db.vcs_list_file_changes(CS_RT).await.expect("file changes");
    assert_eq!(fcs.len(), 1);
    assert_eq!(fcs[0].path, "Src/Main.rs");
    assert_eq!(fcs[0].blob_hash.as_deref(), Some(blob.as_str()));

    // The four sentinel literals — whole-string equality is the decode
    // contract; assert them verbatim.
    let missing_parent = db
        .vcs_create_changeset_tx(
            &changeset(CS_RT2, repo, &format!("[\"{CS_MISSING}\"]")),
            &[],
            &[],
            &[],
        )
        .await
        .expect_err("missing parent must sentinel");
    assert_eq!(missing_parent, "vcs.not_found:parent");

    let bad_parents = db
        .vcs_create_changeset_tx(&changeset(CS_RT2, repo, "not-json"), &[], &[], &[])
        .await
        .expect_err("malformed parents json must sentinel");
    assert_eq!(bad_parents, "vcs.invalid_input:parents");

    let missing_obj = db
        .vcs_create_changeset_tx(
            &changeset(CS_RT2, repo, "[]"),
            &[VcsFileChangeInsert {
                path: "a",
                kind: "add",
                mode: 33188,
                old_path: None,
                blob_hash: Some(&hash(0xdd)),
            }],
            &[&hash(0xdd)],
            &[],
        )
        .await
        .expect_err("missing object must sentinel");
    assert_eq!(missing_obj, "vcs.not_found:object");

    let missing_blob_field = db
        .vcs_create_changeset_tx(
            &changeset(CS_RT2, repo, "[]"),
            &[VcsFileChangeInsert {
                path: "a",
                kind: "add",
                mode: 33188,
                old_path: None,
                blob_hash: None,
            }],
            &[],
            &[],
        )
        .await
        .expect_err("non-delete without blob must sentinel");
    assert_eq!(missing_blob_field, "vcs.invalid_input:file_change");
}

#[tokio::test]
async fn cas_all_arms() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repo = "dcs-cas";
    seed_repo(&db, repo).await;
    for (cs, blob) in [(CS_A, 0xa1u8), (CS_B, 0xb1u8)] {
        let h = hash(blob);
        db.vcs_insert_object_if_absent(&h, 1).await.expect("object");
        let changes = [VcsFileChangeInsert {
            path: "f",
            kind: "add",
            mode: 33188,
            old_path: None,
            blob_hash: Some(&h),
        }];
        db.vcs_create_changeset_tx(&changeset(cs, repo, "[]"), &changes, &[&h], &[])
            .await
            .expect("create");
    }

    // create-only on absent
    match db
        .vcs_cas_ref_update(repo, "main", None, Some(CS_A))
        .await
        .expect("cas create")
    {
        CasOutcome::Applied(row) => {
            assert_eq!(row.changeset_id.as_deref(), Some(CS_A));
        }
        other => panic!("expected Applied, got {other:?}"),
    }
    // create-only on existing
    match db
        .vcs_cas_ref_update(repo, "main", None, Some(CS_B))
        .await
        .expect("cas")
    {
        CasOutcome::Conflict { actual } => {
            assert_eq!(actual.as_deref(), Some(CS_A));
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    // move with correct expectation
    match db
        .vcs_cas_ref_update(repo, "main", Some(CS_A), Some(CS_B))
        .await
        .expect("cas move")
    {
        CasOutcome::Applied(row) => {
            assert_eq!(row.changeset_id.as_deref(), Some(CS_B));
        }
        other => panic!("expected Applied, got {other:?}"),
    }
    // move with stale expectation → conflict carries loser-visible head
    match db
        .vcs_cas_ref_update(repo, "main", Some(CS_A), Some(CS_A))
        .await
        .expect("cas stale")
    {
        CasOutcome::Conflict { actual } => {
            assert_eq!(actual.as_deref(), Some(CS_B));
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    // delete with matching expectation
    match db
        .vcs_cas_ref_update(repo, "main", Some(CS_B), None)
        .await
        .expect("cas delete")
    {
        CasOutcome::Applied(row) => {
            assert_eq!(row.changeset_id, None);
        }
        other => panic!("expected Applied, got {other:?}"),
    }
    // absent-delete → Conflict{actual:None} (registry maps to RefNotFound)
    match db
        .vcs_cas_ref_update(repo, "never", Some(CS_A), None)
        .await
        .expect("cas absent delete")
    {
        CasOutcome::Conflict { actual } => assert_eq!(actual, None),
        other => panic!("expected Conflict, got {other:?}"),
    }
    // assert-absent noop on absent → Conflict{actual:None} (registry: Ok)
    match db
        .vcs_cas_ref_update(repo, "never", None, None)
        .await
        .expect("cas noop")
    {
        CasOutcome::Conflict { actual } => assert_eq!(actual, None),
        other => panic!("expected Conflict, got {other:?}"),
    }
    // assert-absent on a present ref → Conflict{actual:Some(head)}
    db.vcs_cas_ref_update(repo, "main", None, Some(CS_A))
        .await
        .expect("recreate");
    match db
        .vcs_cas_ref_update(repo, "main", None, None)
        .await
        .expect("cas noop on present")
    {
        CasOutcome::Conflict { actual } => {
            assert_eq!(actual.as_deref(), Some(CS_A));
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    // unconditional delete + absent-delete
    assert!(db.vcs_delete_ref(repo, "main").await.expect("delete"));
    assert!(!db
        .vcs_delete_ref(repo, "main")
        .await
        .expect("delete absent"));
}

#[tokio::test]
async fn case_variant_coexistence_and_keyset() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repo = "dcs-case";
    seed_repo(&db, repo).await;
    let blob = hash(0xe1);
    db.vcs_insert_object_if_absent(&blob, 3)
        .await
        .expect("object");

    // Two case-variant paths in ONE changeset — under a case-insensitive
    // collation the (changeset_id, path) PK collides; utf8mb4_bin keeps both.
    let changes = [
        VcsFileChangeInsert {
            path: "Src/App.rs",
            kind: "add",
            mode: 33188,
            old_path: None,
            blob_hash: Some(&blob),
        },
        VcsFileChangeInsert {
            path: "src/app.rs",
            kind: "add",
            mode: 33188,
            old_path: None,
            blob_hash: Some(&blob),
        },
        VcsFileChangeInsert {
            path: "X/moved.rs",
            kind: "rename",
            mode: 33188,
            old_path: Some("SRC/Old.rs"),
            blob_hash: Some(&blob),
        },
    ];
    db.vcs_create_changeset_tx(&changeset(CS_CV, repo, "[]"), &changes, &[&blob], &[])
        .await
        .expect("case-variant paths must coexist");
    let fcs = db.vcs_list_file_changes(CS_CV).await.expect("file changes");
    assert_eq!(fcs.len(), 3, "case-variant paths must not merge");
    let moved = fcs
        .iter()
        .find(|f| f.path == "X/moved.rs")
        .expect("rename row");
    assert_eq!(moved.old_path.as_deref(), Some("SRC/Old.rs"));

    // Ref names "Main" vs "main" must coexist AND sort bytewise — under a
    // ci collation the second insert collides, and a `>` cursor on "Main"
    // would skip "main".
    for name in ["Main", "main"] {
        match db
            .vcs_cas_ref_update(repo, name, None, Some(CS_CV))
            .await
            .expect("cas create")
        {
            CasOutcome::Applied(_) => {}
            CasOutcome::Conflict { actual } => {
                panic!("case-variant ref {name} collided: {actual:?}")
            }
        }
    }
    let page = db.vcs_list_refs(repo, None, 10).await.expect("list");
    let names: Vec<&str> = page.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Main", "main"], "bytewise ref order");
    let after = db
        .vcs_list_refs(repo, Some("Main"), 10)
        .await
        .expect("keyset after Main");
    let names: Vec<&str> = after.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["main"], "keyset '>' must not skip case variants");

    // Derived-index keys differ only by case → both must survive the upsert.
    let upserts = [
        VcsDerivedIndexUpsert {
            kind: "path_touch",
            index_key: "K\x01A",
            payload: "{\"v\":1}",
            covering_changeset: CS_CV,
        },
        VcsDerivedIndexUpsert {
            kind: "path_touch",
            index_key: "K\x01a",
            payload: "{\"v\":2}",
            covering_changeset: CS_CV,
        },
    ];
    db.vcs_upsert_derived_index_rows(repo, &upserts)
        .await
        .expect("index upserts");
    for (key, want) in [("K\x01A", "{\"v\":1}"), ("K\x01a", "{\"v\":2}")] {
        let rows = db
            .vcs_query_derived_index(repo, "path_touch", Some(key), None, 1)
            .await
            .expect("index probe");
        assert_eq!(rows.len(), 1, "case-variant index key {key:?} must land");
        assert_eq!(rows[0].payload, want);
    }

    // MySQL-only: assert the column collations are utf8mb4_bin + index_key
    // widened to VARCHAR(560) (the behavior migrate-only coverage can't
    // prove; PG/SQLite are bytewise by construction).
    if dialect::from_url(&url).unwrap() == Dialect::MySql {
        let pool = sqlx::MySqlPool::connect(&url)
            .await
            .expect("raw mysql connect");
        for (table, column, want_len) in [
            ("vcs_derived_index", "index_key", Some(560i64)),
            ("vcs_refs", "name", Some(255)),
            ("vcs_file_changes", "path", Some(512)),
            ("vcs_file_changes", "old_path", Some(512)),
        ] {
            let (collation, len): (Option<String>, Option<i64>) = sqlx::query_as(
                "SELECT collation_name, character_maximum_length \
                 FROM information_schema.columns \
                 WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?",
            )
            .bind(table)
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("information_schema");
            assert_eq!(
                collation.as_deref(),
                Some("utf8mb4_bin"),
                "{table}.{column} must be utf8mb4_bin"
            );
            assert_eq!(len, want_len, "{table}.{column} length");
        }
    }
}
