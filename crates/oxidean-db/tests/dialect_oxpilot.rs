//! DATABASE_URL-gated Oxpilot schema proof (U1): `db.migrate()` runs the
//! real 0085 migration on the dialect under test, then the store is
//! exercised — session insert/find/list ordering, message insert/seq/snapshot
//! ordering, `client_msg_id` dedupe lookup, `(session_id, seq)` uniqueness,
//! and the streaming-orphan sweep — on all three engines.
//!
//! All tests share one `DATABASE_URL` target; serialize them so SQLite's
//! single-writer model doesn't race schema-touching connections.

use oxidean_db::{
    dialect, Database, Dialect, NewOxpilotAction, NewOxpilotGrant, NewOxpilotMessage,
    NewOxpilotPairing, NewOxpilotSession, NewOxpilotTarget, OxpilotSettingsRow,
};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) if !url.is_empty() => Some(url),
        _ => None,
    }
}

/// Tri-dialect raw-pool shim — schema-shape assertions that prove columns
/// by SELECT-listing them can't ride the `Database` facade (its store
/// methods are the *next* layer); each leg connects the dialect's native
/// pool the same way `dialect_vcs.rs`'s information_schema leg does.
enum RawPool {
    Sqlite(sqlx::SqlitePool),
    Postgres(sqlx::PgPool),
    MySql(sqlx::MySqlPool),
}

impl RawPool {
    async fn connect(url: &str) -> Self {
        match dialect::from_url(url).expect("dialect") {
            Dialect::Sqlite => {
                RawPool::Sqlite(sqlx::SqlitePool::connect(url).await.expect("sqlite pool"))
            }
            Dialect::Postgres => {
                RawPool::Postgres(sqlx::PgPool::connect(url).await.expect("pg pool"))
            }
            Dialect::MySql => {
                RawPool::MySql(sqlx::MySqlPool::connect(url).await.expect("mysql pool"))
            }
        }
    }

    /// Run a statement returning no rows; error propagates so callers can
    /// assert constraint rejections. sqlx 0.9's dynamic path pins
    /// `&'static str` — every call site below passes a literal.
    async fn exec(&self, sql: &'static str) -> Result<(), sqlx::Error> {
        match self {
            RawPool::Sqlite(p) => sqlx::query(sql).execute(p).await.map(|_| ()),
            RawPool::Postgres(p) => sqlx::query(sql).execute(p).await.map(|_| ()),
            RawPool::MySql(p) => sqlx::query(sql).execute(p).await.map(|_| ()),
        }
    }

    /// COUNT(*) style scalar — i64 on every dialect.
    async fn count(&self, sql: &'static str) -> i64 {
        match self {
            RawPool::Sqlite(p) => sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(p)
                .await
                .expect("count"),
            RawPool::Postgres(p) => sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(p)
                .await
                .expect("count"),
            RawPool::MySql(p) => sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(p)
                .await
                .expect("count"),
        }
    }
}

async fn seed(db: &Database, tag: &str) -> (String, String) {
    let uid = format!("u-oxp-{tag}");
    db.create_user(
        &uid,
        &format!("oxp-{tag}@example.com"),
        &format!("oxp{tag}"),
        None,
        "Dialect Seed",
        "",
        None,
        oxidean_core::Role::User,
    )
    .await
    .expect("seed user");
    let repo = format!("repo-oxp-{tag}");
    db.insert_repository(&repo, &uid, "user", &repo, "private", "", "main")
        .await
        .expect("seed repo");
    (uid, repo)
}

fn session<'a>(id: &'a str, uid: &'a str, repo: &'a str, title: &'a str) -> NewOxpilotSession<'a> {
    NewOxpilotSession {
        id,
        user_id: uid,
        repo_owner: "oxpilot",
        repo_name: repo,
        repo_id: repo,
        branch: "main",
        goal: None,
        title,
    }
}

fn msg<'a>(
    id: &'a str,
    session_id: &'a str,
    seq: i64,
    role: &'a str,
    content: &'a str,
    client: Option<&'a str>,
    status: &'a str,
) -> NewOxpilotMessage<'a> {
    NewOxpilotMessage {
        id,
        session_id,
        seq,
        role,
        content,
        client_msg_id: client,
        status,
    }
}

#[tokio::test]
async fn session_insert_find_and_list_ordering() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "listord").await;

    let first = db
        .insert_oxpilot_session(&session("oxps-listord-1", &uid, &repo, "first"))
        .await
        .expect("insert first");
    assert_eq!(first.status, "active");
    assert_eq!(first.branch, "main");
    assert_eq!(first.title, "first");

    let second = db
        .insert_oxpilot_session(&session("oxps-listord-2", &uid, &repo, "second"))
        .await
        .expect("insert second");

    // Bump the older row so ordering follows last_activity_at, not insert order.
    db.touch_oxpilot_session_activity(&first.id)
        .await
        .expect("touch");

    let list = db.list_oxpilot_sessions_for_user(&uid).await.expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, first.id, "most-recently-active first");
    assert_eq!(list[1].id, second.id);

    // Ownership scoping — a foreign user sees neither row.
    let other = db
        .list_oxpilot_sessions_for_user("u-oxp-other")
        .await
        .expect("list other");
    assert!(other.is_empty());
    let owned = db
        .find_owned_oxpilot_session(&first.id, "u-oxp-other")
        .await
        .expect("owned find");
    assert!(owned.is_none(), "cross-user find must resolve None");
    let owned = db
        .find_owned_oxpilot_session(&first.id, &uid)
        .await
        .expect("owned find");
    assert!(owned.is_some());

    assert_eq!(
        dialect::from_url(&url).unwrap().as_str(),
        db.dialect().unwrap().as_str()
    );
}

#[tokio::test]
async fn message_seq_snapshot_and_client_dedupe() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "msgseq").await;
    let sid = "oxps-msgseq-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("insert session");

    assert_eq!(db.next_oxpilot_message_seq(sid).await.unwrap(), 1);
    let user_row = db
        .insert_oxpilot_message(&msg(
            "oxpm-msgseq-1",
            sid,
            1,
            "user",
            "hello",
            Some("cm-1"),
            "complete",
        ))
        .await
        .expect("user row");
    assert_eq!(user_row.seq, 1);
    assert_eq!(db.next_oxpilot_message_seq(sid).await.unwrap(), 2);
    db.insert_oxpilot_message(&msg(
        "oxpm-msgseq-2",
        sid,
        2,
        "assistant",
        "partial",
        None,
        "streaming",
    ))
    .await
    .expect("assistant row");

    // Snapshot ordering.
    let snap = db.list_oxpilot_messages(sid).await.expect("snapshot");
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[0].role, "user");
    assert_eq!(snap[1].status, "streaming");

    // client_msg_id dedupe lookup + unique constraint.
    let found = db
        .find_oxpilot_message_by_client_msg_id(sid, "cm-1")
        .await
        .expect("dedupe find");
    assert_eq!(found.unwrap().id, "oxpm-msgseq-1");
    assert!(db
        .find_oxpilot_message_by_client_msg_id(sid, "cm-2")
        .await
        .expect("absent")
        .is_none());
    let dup = db
        .insert_oxpilot_message(&msg(
            "oxpm-msgseq-3",
            sid,
            3,
            "user",
            "again",
            Some("cm-1"),
            "complete",
        ))
        .await;
    assert!(
        dup.is_err(),
        "(session_id, client_msg_id) unique must reject"
    );

    // Streaming-content accumulation + status transition.
    db.set_oxpilot_message_content("oxpm-msgseq-2", "partial more")
        .await
        .expect("append content");
    db.set_oxpilot_message_status("oxpm-msgseq-2", "complete")
        .await
        .expect("finalize");
    let done = db
        .find_oxpilot_message_by_id("oxpm-msgseq-2")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(done.content, "partial more");
    assert_eq!(done.status, "complete");
}

#[tokio::test]
async fn streaming_orphan_sweep_marks_error() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "sweep").await;
    let sid = "oxps-sweep-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("insert session");
    db.insert_oxpilot_message(&msg(
        "oxpm-sweep-1",
        sid,
        1,
        "assistant",
        "partial",
        None,
        "streaming",
    ))
    .await
    .expect("streaming row");
    db.insert_oxpilot_message(&msg("oxpm-sweep-2", sid, 2, "user", "ok", None, "complete"))
        .await
        .expect("complete row");

    let swept = db.sweep_oxpilot_streaming_messages().await.expect("sweep");
    assert_eq!(swept, 1);
    let orphan = db
        .find_oxpilot_message_by_id("oxpm-sweep-1")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(orphan.status, "error");
    let untouched = db
        .find_oxpilot_message_by_id("oxpm-sweep-2")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(untouched.status, "complete");
    assert_eq!(db.sweep_oxpilot_streaming_messages().await.unwrap(), 0);
}

// ---- 0086 oxpilot_settings (U4) — singleton sparse-override table ----

/// The migrate seeds exactly one row (id = 1) whose every override column
/// is NULL (pure-env posture) and `updated_at` is server-set. The SELECT
/// list itself is the column-shape assertion — a missing column fails the
/// query on every dialect. `clear_oxpilot_settings` normalizes the shared
/// singleton first so store tests running earlier in the same DATABASE_URL
/// can't leave this assertion racing a non-NULL row.
#[tokio::test]
async fn settings_table_shape_and_nullable_columns() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    db.clear_oxpilot_settings().await.expect("normalize row");
    let pool = RawPool::connect(&url).await;

    // Column shape: every override column must exist and the (seeded /
    // just-cleared — same all-NULL posture) row must carry NULL in each
    // (NULL = follow env, the sparse contract).
    let seeded = pool
        .count(
            "SELECT COUNT(*) FROM oxpilot_settings \
             WHERE id = 1 \
               AND enabled_override IS NULL \
               AND provider_id IS NULL \
               AND model IS NULL \
               AND base_url IS NULL \
               AND key_ciphertext IS NULL \
               AND key_last4 IS NULL \
               AND user_budget IS NULL \
               AND instance_budget IS NULL \
               AND updated_at IS NOT NULL",
        )
        .await;
    assert_eq!(seeded, 1, "id=1 row must carry all-NULL overrides");

    // Nullability is symmetric: a set value must also write + read back —
    // BOOLEAN/BIGINT/TEXT columns on each dialect accept the write.
    pool.exec(
        "UPDATE oxpilot_settings \
         SET provider_id = 'openai', model = 'm', user_budget = 42, enabled_override = 1 \
         WHERE id = 1",
    )
    .await
    .expect("override write");
    let written = pool
        .count(
            "SELECT COUNT(*) FROM oxpilot_settings \
             WHERE id = 1 AND provider_id = 'openai' AND model = 'm' \
               AND user_budget = 42 AND enabled_override IS NOT NULL",
        )
        .await;
    assert_eq!(written, 1, "override columns must accept and return writes");

    // Restore the all-NULL posture for the singleton constraint test.
    pool.exec(
        "UPDATE oxpilot_settings \
         SET provider_id = NULL, model = NULL, user_budget = NULL, enabled_override = NULL \
         WHERE id = 1",
    )
    .await
    .expect("restore nulls");
}

/// CHECK (id = 1) + the PK are the singleton — a second row at any other
/// id fails on the check, and a duplicate id=1 fails on the key, on every
/// dialect.
#[tokio::test]
async fn settings_singleton_constraint_id_eq_1() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let pool = RawPool::connect(&url).await;

    let rows = pool.count("SELECT COUNT(*) FROM oxpilot_settings").await;
    assert_eq!(rows, 1, "singleton table must carry exactly one row");

    let second = pool
        .exec("INSERT INTO oxpilot_settings (id) VALUES (2)")
        .await;
    assert!(second.is_err(), "CHECK (id = 1) must reject id = 2");

    let dup = pool
        .exec("INSERT INTO oxpilot_settings (id) VALUES (1)")
        .await;
    assert!(dup.is_err(), "PK must reject a duplicate singleton row");

    assert_eq!(pool.count("SELECT COUNT(*) FROM oxpilot_settings").await, 1);
}

// ---- 0086 oxpilot_settings store (U4 Step 6) — facade round-trips ----

fn full_row() -> OxpilotSettingsRow {
    OxpilotSettingsRow {
        enabled_override: Some(true),
        provider_id: Some("openai".to_string()),
        model: Some("gpt-4o".to_string()),
        base_url: Some("https://api.example.com".to_string()),
        key_ciphertext: Some("aabbcc".to_string()),
        key_last4: Some("wxyz".to_string()),
        user_budget: Some(1_000_000),
        instance_budget: Some(5_000_000),
        updated_at: String::new(),
    }
}

/// get → all-NULL posture: `clear_oxpilot_settings` normalizes the shared
/// row first (tests share one DATABASE_URL), then the read must decode
/// every override column as `None` and still surface `updated_at`.
#[tokio::test]
async fn settings_store_default_row_decodes_all_null() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    db.clear_oxpilot_settings().await.expect("clear");

    let row = db
        .get_oxpilot_settings()
        .await
        .expect("get")
        .expect("singleton row");
    assert_eq!(
        row,
        OxpilotSettingsRow {
            updated_at: row.updated_at.clone(),
            ..OxpilotSettingsRow::default()
        },
        "cleared row must decode every override as NULL"
    );
    assert!(!row.updated_at.is_empty(), "server-set updated_at");
}

/// Write→read round trip: every sparse field survives the dialect's
/// BOOLEAN/BIGINT/TEXT decode chain, and the returned `updated_at` is
/// server-set (not caller-supplied).
#[tokio::test]
async fn settings_store_upsert_round_trip() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let written = db
        .upsert_oxpilot_settings(&full_row())
        .await
        .expect("upsert");
    assert_eq!(written.enabled_override, Some(true));
    assert_eq!(written.provider_id.as_deref(), Some("openai"));
    assert_eq!(written.model.as_deref(), Some("gpt-4o"));
    assert_eq!(written.base_url.as_deref(), Some("https://api.example.com"));
    assert_eq!(written.key_ciphertext.as_deref(), Some("aabbcc"));
    assert_eq!(written.key_last4.as_deref(), Some("wxyz"));
    assert_eq!(written.user_budget, Some(1_000_000));
    assert_eq!(written.instance_budget, Some(5_000_000));
    assert!(!written.updated_at.is_empty());

    let read = db.get_oxpilot_settings().await.expect("get").expect("row");
    assert_eq!(read, written, "re-read must match the upsert return");

    // Second upsert overwrites — sparse write semantics are the caller's
    // (settings::apply), the store always persists the full candidate.
    let mut again = full_row();
    again.provider_id = Some("anthropic".to_string());
    again.model = None;
    let rewritten = db.upsert_oxpilot_settings(&again).await.expect("upsert2");
    assert_eq!(rewritten.provider_id.as_deref(), Some("anthropic"));
    assert_eq!(rewritten.model, None);
    assert_eq!(rewritten.enabled_override, Some(true));
}

/// `clear_row` NULLs every override in one statement while the singleton
/// row — and its server-stamped `updated_at` — survives.
#[tokio::test]
async fn settings_store_clear_row_nulls_all_overrides() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    db.upsert_oxpilot_settings(&full_row())
        .await
        .expect("seed non-null");

    db.clear_oxpilot_settings().await.expect("clear");
    let row = db
        .get_oxpilot_settings()
        .await
        .expect("get")
        .expect("row survives clear");
    assert_eq!(row.enabled_override, None);
    assert_eq!(row.provider_id, None);
    assert_eq!(row.model, None);
    assert_eq!(row.base_url, None);
    assert_eq!(row.key_ciphertext, None);
    assert_eq!(row.key_last4, None);
    assert_eq!(row.user_budget, None);
    assert_eq!(row.instance_budget, None);
    assert!(!row.updated_at.is_empty());
}

/// NFR1.5: ciphertext+last4 is one custody pair — they land together on
/// write and NULL out together; the store never sees half a pair.
#[tokio::test]
async fn settings_store_key_pair_moves_together() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let mut row = OxpilotSettingsRow {
        key_ciphertext: Some("deadbeef".to_string()),
        key_last4: Some("1234".to_string()),
        ..Default::default()
    };
    let written = db.upsert_oxpilot_settings(&row).await.expect("write key");
    assert_eq!(written.key_ciphertext.as_deref(), Some("deadbeef"));
    assert_eq!(written.key_last4.as_deref(), Some("1234"));

    // A new-key write replaces both halves in the same statement.
    row.key_ciphertext = Some("feedface".to_string());
    row.key_last4 = Some("9876".to_string());
    let rotated = db.upsert_oxpilot_settings(&row).await.expect("rotate");
    assert_eq!(rotated.key_ciphertext.as_deref(), Some("feedface"));
    assert_eq!(rotated.key_last4.as_deref(), Some("9876"));

    // clear_key clears the pair atomically — never ciphertext alone.
    row.key_ciphertext = None;
    row.key_last4 = None;
    let cleared = db.upsert_oxpilot_settings(&row).await.expect("clear key");
    assert_eq!(cleared.key_ciphertext, None);
    assert_eq!(cleared.key_last4, None);
}

/// The store self-heals a missing singleton: delete the row out from
/// under it and `upsert_row` re-creates id = 1 (dialect-native upsert —
/// the CHECK constraint would reject any other id).
#[tokio::test]
async fn settings_store_upsert_recreates_missing_singleton() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let pool = RawPool::connect(&url).await;

    pool.exec("DELETE FROM oxpilot_settings WHERE id = 1")
        .await
        .expect("delete singleton");
    assert!(db
        .get_oxpilot_settings()
        .await
        .expect("get absent")
        .is_none());

    let row = OxpilotSettingsRow {
        provider_id: Some("ollama".to_string()),
        ..Default::default()
    };
    let recreated = db.upsert_oxpilot_settings(&row).await.expect("upsert");
    assert_eq!(recreated.provider_id.as_deref(), Some("ollama"));

    assert_eq!(
        pool.count("SELECT COUNT(*) FROM oxpilot_settings").await,
        1,
        "upsert recreates exactly the singleton row"
    );
}

/// Repeated upserts/clears never grow the table — id stays pinned at 1.
#[tokio::test]
async fn settings_store_singleton_survives_writes() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let pool = RawPool::connect(&url).await;

    for i in 0..3 {
        let mut row = full_row();
        row.model = Some(format!("m-{i}"));
        db.upsert_oxpilot_settings(&row).await.expect("upsert");
    }
    db.clear_oxpilot_settings().await.expect("clear");
    db.upsert_oxpilot_settings(&full_row())
        .await
        .expect("final upsert");

    assert_eq!(
        pool.count("SELECT COUNT(*) FROM oxpilot_settings").await,
        1,
        "writes must never add rows"
    );
}

/// Boolean decode is dialect-tolerant: a raw `enabled_override = 0`
/// (MySQL TINYINT(1) / SQLite INTEGER storage shape) must still decode
/// `Some(false)` through the facade's bool/i64/i8 fallback chain.
#[tokio::test]
async fn settings_store_bool_decode_from_int_storage() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let pool = RawPool::connect(&url).await;

    pool.exec("UPDATE oxpilot_settings SET enabled_override = 0, user_budget = 7 WHERE id = 1")
        .await
        .expect("raw int write");
    let row = db.get_oxpilot_settings().await.expect("get").expect("row");
    assert_eq!(row.enabled_override, Some(false));
    assert_eq!(row.user_budget, Some(7));
    db.clear_oxpilot_settings().await.expect("restore nulls");
}

// ---- 0087 oxpilot_session_core (U2) — queue/tool_calls/proposals/usage ----

/// Table presence + the pinned unique keys: `(session_id, client_msg_id)`
/// and `(session_id, queue_seq)` on queued, `(session_id, call_id)` on
/// tool_calls, `(scope, scope_key, window)` on usage_counters — each pair
/// must reject on every dialect.
#[tokio::test]
async fn session_core_tables_shape_and_unique_keys() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "coreuq").await;
    let sid = "oxps-coreuq-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("insert session");
    let pool = RawPool::connect(&url).await;

    // Presence — the SELECT list itself is the column-shape proof.
    assert_eq!(
        pool.count(
            "SELECT COUNT(*) FROM oxpilot_queued_messages \
             WHERE session_id = 'oxps-coreuq-1' AND queue_seq IS NOT NULL \
               AND content IS NOT NULL AND created_at IS NOT NULL",
        )
        .await,
        0
    );
    assert_eq!(
        pool.count(
            "SELECT COUNT(*) FROM oxpilot_tool_calls \
             WHERE session_id = 'oxps-coreuq-1' AND call_index IS NOT NULL \
               AND args_summary IS NOT NULL AND output_summary IS NOT NULL",
        )
        .await,
        0
    );
    assert_eq!(
        pool.count(
            "SELECT COUNT(*) FROM oxpilot_proposals \
             WHERE session_id = 'oxps-coreuq-1' AND status = 'pending' AND pr_ref IS NULL",
        )
        .await,
        0
    );
    assert_eq!(
        pool.count(
            "SELECT COUNT(*) FROM oxpilot_usage_counters \
             WHERE scope = 'instance' AND requests IS NOT NULL AND tokens IS NOT NULL",
        )
        .await,
        0
    );

    // (session_id, client_msg_id) + (session_id, queue_seq) on queued.
    db.enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
        id: "oxpq-coreuq-1",
        session_id: sid,
        queue_seq: 1,
        content: "first",
        client_msg_id: Some("cm-uq"),
    })
    .await
    .expect("enqueue first");
    let dup_cmid = db
        .enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
            id: "oxpq-coreuq-2",
            session_id: sid,
            queue_seq: 2,
            content: "again",
            client_msg_id: Some("cm-uq"),
        })
        .await;
    assert!(dup_cmid.is_err(), "(session_id, client_msg_id) must reject");
    let dup_qseq = db
        .enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
            id: "oxpq-coreuq-3",
            session_id: sid,
            queue_seq: 1,
            content: "again",
            client_msg_id: Some("cm-other"),
        })
        .await;
    assert!(dup_qseq.is_err(), "(session_id, queue_seq) must reject");

    // (session_id, call_id) on tool_calls.
    db.insert_oxpilot_tool_call(&oxidean_db::NewOxpilotToolCall {
        id: "oxpt-coreuq-1",
        session_id: sid,
        message_id: "oxpm-coreuq-m",
        call_id: "call-uq",
        seq: 7,
        call_index: 0,
        tool: "fs_read",
        args_summary: "{}",
        status: "running",
    })
    .await
    .expect("tool call");
    let dup_call = db
        .insert_oxpilot_tool_call(&oxidean_db::NewOxpilotToolCall {
            id: "oxpt-coreuq-2",
            session_id: sid,
            message_id: "oxpm-coreuq-m",
            call_id: "call-uq",
            seq: 7,
            call_index: 1,
            tool: "fs_read",
            args_summary: "{}",
            status: "running",
        })
        .await;
    assert!(dup_call.is_err(), "(session_id, call_id) must reject");

    // (scope, scope_key, window) on usage_counters — raw insert of the
    // same key triple under a different pk must violate the unique key.
    pool.exec(
        "INSERT INTO oxpilot_usage_counters \
             (id, scope, scope_key, window, requests, tokens) \
         VALUES ('raw-usage-1', 'user', 'u-x', '2026-01-01', 1, 1)",
    )
    .await
    .expect("first usage row");
    let dup_scope = pool
        .exec(
            "INSERT INTO oxpilot_usage_counters \
                 (id, scope, scope_key, window, requests, tokens) \
             VALUES ('raw-usage-2', 'user', 'u-x', '2026-01-01', 1, 1)",
        )
        .await;
    assert!(
        dup_scope.is_err(),
        "(scope, scope_key, window) must reject duplicates"
    );
    let other_window = pool
        .exec(
            "INSERT INTO oxpilot_usage_counters \
                 (id, scope, scope_key, window, requests, tokens) \
             VALUES ('raw-usage-3', 'user', 'u-x', '2026-01-02', 1, 1)",
        )
        .await;
    assert!(other_window.is_ok(), "a different window is a new row");
}

/// BR5.3/AC1.5.x — one session delete cascades messages, tool_calls,
/// queued rows, and proposals; usage counters carry no FK and survive.
#[tokio::test]
async fn session_delete_cascades_children_but_keeps_usage() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "casc").await;
    let sid = "oxps-casc-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("insert session");
    db.insert_oxpilot_message(&msg("oxpm-casc-1", sid, 1, "user", "hi", None, "complete"))
        .await
        .expect("message");
    db.enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
        id: "oxpq-casc-1",
        session_id: sid,
        queue_seq: 1,
        content: "later",
        client_msg_id: Some("cm-casc"),
    })
    .await
    .expect("queued");
    db.insert_oxpilot_tool_call(&oxidean_db::NewOxpilotToolCall {
        id: "oxpt-casc-1",
        session_id: sid,
        message_id: "oxpm-casc-1",
        call_id: "call-1",
        seq: 1,
        call_index: 0,
        tool: "fs_read",
        args_summary: "{}",
        status: "done",
    })
    .await
    .expect("tool call");
    db.insert_oxpilot_proposal(&oxidean_db::NewOxpilotProposal {
        id: "oxpp-casc-1",
        session_id: sid,
        title: "diff",
        base: "main",
        diff_body: "diff --git a/x b/x",
        ops_json: "[]",
    })
    .await
    .expect("proposal");
    db.upsert_oxpilot_usage(&uid, "2026-01-03", 1, 10)
        .await
        .expect("usage");

    // The user_id predicate is the depth guard — a mismatched owner
    // deletes nothing.
    assert_eq!(
        db.delete_oxpilot_session_cascade(sid, "oxpu-other")
            .await
            .expect("foreign delete"),
        0
    );
    let deleted = db
        .delete_oxpilot_session_cascade(sid, &uid)
        .await
        .expect("delete");
    assert_eq!(deleted, 1);
    let pool = RawPool::connect(&url).await;
    for (table, count_sql) in [
        (
            "oxpilot_sessions",
            "SELECT COUNT(*) FROM oxpilot_sessions WHERE id = 'oxps-casc-1'",
        ),
        (
            "oxpilot_messages",
            "SELECT COUNT(*) FROM oxpilot_messages WHERE session_id = 'oxps-casc-1'",
        ),
        (
            "oxpilot_tool_calls",
            "SELECT COUNT(*) FROM oxpilot_tool_calls WHERE session_id = 'oxps-casc-1'",
        ),
        (
            "oxpilot_queued_messages",
            "SELECT COUNT(*) FROM oxpilot_queued_messages WHERE session_id = 'oxps-casc-1'",
        ),
        (
            "oxpilot_proposals",
            "SELECT COUNT(*) FROM oxpilot_proposals WHERE session_id = 'oxps-casc-1'",
        ),
    ] {
        assert_eq!(
            pool.count(count_sql).await,
            0,
            "{table} must cascade-empty with the session"
        );
    }
    // The audit-grade rollup is standalone by design — deletion-proof.
    let (user_counter, instance_counter) = db
        .read_oxpilot_usage_counters(&uid, "2026-01-03")
        .await
        .expect("counters");
    assert_eq!(user_counter.unwrap().requests, 1);
    assert_eq!(instance_counter.unwrap().requests, 1);
}

/// BR4.2 exactly-once: the drain claims+deletes every queued row in
/// `queue_seq` order and appends `role=user` transcript rows minting `seq`
/// in the same transaction — queue empties, delivery order holds, a
/// second drain is a no-op. Gaps in `queue_seq` (a drained head) still
/// deliver in order — `queue_seq` is ordering, not position.
#[tokio::test]
async fn queue_drain_tx_delivers_in_order_exactly_once() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "drain").await;
    let sid = "oxps-drain-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("insert session");
    db.insert_oxpilot_message(&msg(
        "oxpm-drain-1",
        sid,
        1,
        "user",
        "first",
        None,
        "complete",
    ))
    .await
    .expect("seed transcript");

    for (id, qs, cmid) in [
        ("oxpq-drain-1", 1_i64, Some("cm-d1")),
        ("oxpq-drain-2", 2_i64, Some("cm-d2")),
        ("oxpq-drain-3", 3_i64, None),
    ] {
        db.enqueue_oxpilot_queued(&oxidean_db::NewOxpilotQueued {
            id,
            session_id: sid,
            queue_seq: qs,
            content: "queued",
            client_msg_id: cmid,
        })
        .await
        .expect("enqueue");
    }

    let mut mint = 100_i64;
    let delivered = db
        .drain_oxpilot_queue_tx(sid, || {
            mint += 1;
            format!("oxpm-drain-{mint}")
        })
        .await
        .expect("drain");
    assert_eq!(delivered.len(), 3);
    assert_eq!(
        delivered.iter().map(|m| m.seq).collect::<Vec<_>>(),
        vec![2, 3, 4],
        "seq mints inside the tx continuing the transcript"
    );
    assert!(delivered
        .iter()
        .all(|m| m.role == "user" && m.status == "complete"));
    assert_eq!(
        delivered[0].client_msg_id.as_deref(),
        Some("cm-d1"),
        "client_msg_id carried through the drain"
    );

    // Queue is empty (deleted in-tx) — a re-drain is a clean no-op.
    assert!(db
        .list_oxpilot_queued_for_session(sid)
        .await
        .expect("queue read")
        .is_empty());
    let mut mint2 = 200_i64;
    let again = db
        .drain_oxpilot_queue_tx(sid, || {
            mint2 += 1;
            format!("oxpm-drain-{mint2}")
        })
        .await
        .expect("re-drain");
    assert!(again.is_empty(), "nothing queued ⇒ nothing delivered");
    // Drained resends still dedupe through the carried client_msg_id.
    let found = db
        .find_oxpilot_message_by_client_msg_id(sid, "cm-d1")
        .await
        .expect("dedupe");
    assert!(found.is_some(), "carried cmid must still dedupe");
}

/// US5.2/BR6.1 — `upsert_usage` lands `(user, user_id)` and
/// `(instance, '__instance__')` rows in one transaction and
/// upsert-increments on repeat; a second user's write shares the same
/// instance row, never the user row.
#[tokio::test]
async fn usage_upsert_dual_scope_and_shared_instance() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, _repo) = seed(&db, "usage").await;
    let window = "2026-01-04";

    // Absent rows read as None — the service treats that as zero.
    let (u0, i0) = db
        .read_oxpilot_usage_counters(&uid, window)
        .await
        .expect("read");
    assert!(u0.is_none());
    assert!(i0.is_none());

    db.upsert_oxpilot_usage(&uid, window, 1, 40)
        .await
        .expect("upsert 1");
    db.upsert_oxpilot_usage(&uid, window, 1, 25)
        .await
        .expect("upsert 2");
    let (user, instance) = db
        .read_oxpilot_usage_counters(&uid, window)
        .await
        .expect("read");
    let user = user.expect("user row");
    assert_eq!(user.scope, "user");
    assert_eq!(user.scope_key, uid);
    assert_eq!(user.requests, 2);
    assert_eq!(user.tokens, 65);
    let instance = instance.expect("instance row");
    assert_eq!(instance.scope_key, oxidean_db::oxpilot::INSTANCE_SCOPE_KEY);
    assert_eq!(instance.requests, 2);
    assert_eq!(instance.tokens, 65);

    // Second user's window gets its own user row; the instance row is
    // shared — its counters accumulate across users.
    db.upsert_oxpilot_usage("u-oxp-usage-b", window, 3, 7)
        .await
        .expect("other user upsert");
    let (u_b, i_b) = db
        .read_oxpilot_usage_counters("u-oxp-usage-b", window)
        .await
        .expect("read b");
    assert_eq!(u_b.unwrap().requests, 3);
    let i_b = i_b.expect("shared instance row");
    assert_eq!(i_b.requests, 5, "instance accumulates across users");
    assert_eq!(i_b.tokens, 72);
}

// ---------- connector gateway (0088, U6) ----------
//
// Tri-dialect connector-table proofs: pairing mint/claim atomicity,
// verifier-hash partial-unique semantics (NULL tombstones coexist on
// every engine), grant/action state transitions, FIFO dequeue + claim
// CAS, disconnect/revoke close paths, and the session-delete cascade.

const FAR_FUTURE: &str = "2999-01-01T00:00:00Z";
const FAR_PAST: &str = "2000-01-01T00:00:00Z";
const NOW: &str = "2026-01-01T00:00:00Z";

fn pairing<'a>(
    id: &'a str,
    uid: &'a str,
    code_hash: &'a str,
    name: &'a str,
    caps: &'a str,
    expires: &'a str,
) -> NewOxpilotPairing<'a> {
    NewOxpilotPairing {
        id,
        user_id: uid,
        code_hash,
        target_name: name,
        requested_caps: caps,
        expires_at: expires,
    }
}

fn target<'a>(
    id: &'a str,
    uid: &'a str,
    pairing_id: &'a str,
    name: &'a str,
    caps: &'a str,
    verifier_hash: &'a str,
) -> NewOxpilotTarget<'a> {
    NewOxpilotTarget {
        id,
        user_id: uid,
        pairing_id,
        name,
        caps,
        verifier_hash,
    }
}

#[allow(clippy::too_many_arguments)]
fn grant<'a>(
    id: &'a str,
    target_id: &'a str,
    uid: &'a str,
    session_id: Option<&'a str>,
    kind: &'a str,
    action_kind: Option<&'a str>,
    action_args: Option<&'a str>,
    action_summary: Option<&'a str>,
    expires: &'a str,
) -> NewOxpilotGrant<'a> {
    NewOxpilotGrant {
        id,
        target_id,
        user_id: uid,
        session_id,
        kind,
        action_kind,
        action_args,
        action_summary,
        expires_at: expires,
    }
}

#[allow(clippy::too_many_arguments)]
fn action<'a>(
    id: &'a str,
    target_id: &'a str,
    uid: &'a str,
    session_id: &'a str,
    grant_id: &'a str,
    kind: &'a str,
    args: &'a str,
    summary: &'a str,
) -> NewOxpilotAction<'a> {
    NewOxpilotAction {
        id,
        target_id,
        user_id: uid,
        session_id,
        grant_id,
        kind,
        args,
        args_summary: summary,
    }
}

/// Mint a pairing + redeem it through the tx — returns the target row
/// whose `pending_consent` lifecycle + live verifier the later legs
/// exercise. The connect grant minted alongside always carries
/// `session_id NULL` (BR6.6).
async fn claimed_target(
    db: &Database,
    tag: &str,
    uid: &str,
    code_hash: &str,
    verifier_hash: &str,
) -> oxidean_db::OxpilotTargetRow {
    db.insert_oxpilot_pairing(&pairing(
        &format!("oxpp-{tag}"),
        uid,
        code_hash,
        "desk",
        r#"["exec_shell","fs_read"]"#,
        FAR_FUTURE,
    ))
    .await
    .expect("mint pairing");
    db.claim_oxpilot_pairing_tx(
        code_hash,
        NOW,
        &target(
            &format!("oxpt-{tag}"),
            uid,
            &format!("oxpp-{tag}"),
            "desk",
            r#"["exec_shell","fs_read"]"#,
            verifier_hash,
        ),
        &grant(
            &format!("oxpg-{tag}-connect"),
            &format!("oxpt-{tag}"),
            uid,
            None,
            "connect",
            None,
            None,
            None,
            FAR_FUTURE,
        ),
    )
    .await
    .expect("claim tx")
    .expect("claim wins")
}

/// 0088.1 — pairing insert/find/lazy-expire + `UNIQUE(code_hash)`.
#[tokio::test]
async fn connector_pairing_mint_lookup_expire() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, _repo) = seed(&db, "pairmint").await;

    let p = db
        .insert_oxpilot_pairing(&pairing(
            "oxpp-pairmint-1",
            &uid,
            "hash-pairmint-1",
            "desk",
            r#"["exec_shell"]"#,
            FAR_FUTURE,
        ))
        .await
        .expect("insert pairing");
    assert_eq!(p.state, "pending");
    assert_eq!(p.target_name, "desk");

    let found = db
        .find_oxpilot_pairing_by_code_hash("hash-pairmint-1")
        .await
        .expect("find")
        .expect("row");
    assert_eq!(found.id, "oxpp-pairmint-1");
    assert!(db
        .find_oxpilot_pairing_by_code_hash("hash-absent")
        .await
        .expect("absent")
        .is_none());

    // UNIQUE(code_hash) rejects a second mint under the same hash.
    let dup = db
        .insert_oxpilot_pairing(&pairing(
            "oxpp-pairmint-2",
            &uid,
            "hash-pairmint-1",
            "desk2",
            r#"["exec_shell"]"#,
            FAR_FUTURE,
        ))
        .await;
    assert!(dup.is_err(), "UNIQUE(code_hash) must reject: {dup:?}");

    // Lazy expire is a pending-only CAS: 1 stamps, re-run answers 0.
    assert_eq!(
        db.expire_oxpilot_pairing("oxpp-pairmint-1").await.unwrap(),
        1
    );
    assert_eq!(
        db.expire_oxpilot_pairing("oxpp-pairmint-1").await.unwrap(),
        0
    );
    let expired = db
        .find_oxpilot_pairing_by_code_hash("hash-pairmint-1")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(expired.state, "expired");
}

/// 0088.2 — `claim_pairing_tx` is atomic (pairing + target + connect
/// grant in one tx); the CAS loses on re-claim; the verifier unique
/// index rejects a duplicate live hash AND rolls the whole claim back;
/// NULL tombstones coexist on every engine.
#[tokio::test]
async fn connector_pairing_claim_tx_atomic_and_verifier_uniqueness() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, _repo) = seed(&db, "pairclaim").await;

    let t = claimed_target(&db, "pairclaim-1", &uid, "hash-pc-1", "vh-pc-1").await;
    assert_eq!(t.status, "pending_consent");
    assert_eq!(t.verifier_hash.as_deref(), Some("vh-pc-1"));

    // Pairing flipped claimed + connect grant materialized in the tx.
    let pairing_row = db
        .find_oxpilot_pairing_by_code_hash("hash-pc-1")
        .await
        .expect("find pairing")
        .unwrap();
    assert_eq!(pairing_row.state, "claimed");
    let connect = db
        .find_oxpilot_grant("oxpg-pairclaim-1-connect")
        .await
        .expect("find grant")
        .expect("connect grant minted");
    assert_eq!(connect.kind, "connect");
    assert_eq!(connect.state, "pending");
    assert!(connect.session_id.is_none());

    // Re-claim loses the CAS — None, no second target row.
    let second = db
        .claim_oxpilot_pairing_tx(
            "hash-pc-1",
            NOW,
            &target(
                "oxpt-pairclaim-dup",
                &uid,
                "oxpp-pairclaim-1",
                "desk",
                r#"["exec_shell"]"#,
                "vh-other",
            ),
            &grant(
                "oxpg-pairclaim-dup-connect",
                "oxpt-pairclaim-dup",
                &uid,
                None,
                "connect",
                None,
                None,
                None,
                FAR_FUTURE,
            ),
        )
        .await
        .expect("second claim");
    assert!(second.is_none(), "claimed pairing must lose the CAS");
    assert!(db
        .find_oxpilot_target("oxpt-pairclaim-dup")
        .await
        .expect("find")
        .is_none());

    // Verifier-hash lookup resolves; a second live target under the
    // SAME hash violates the unique index and the whole claim rolls
    // back (pairing stays pending — atomicity proof).
    let resolved = db
        .find_oxpilot_target_by_verifier_hash("vh-pc-1")
        .await
        .expect("by hash")
        .unwrap();
    assert_eq!(resolved.id, t.id);
    db.insert_oxpilot_pairing(&pairing(
        "oxpp-pairclaim-2",
        &uid,
        "hash-pc-2",
        "desk",
        r#"["exec_shell"]"#,
        FAR_FUTURE,
    ))
    .await
    .expect("mint p2");
    let dup_claim = db
        .claim_oxpilot_pairing_tx(
            "hash-pc-2",
            NOW,
            &target(
                "oxpt-pairclaim-2",
                &uid,
                "oxpp-pairclaim-2",
                "desk",
                r#"["exec_shell"]"#,
                "vh-pc-1", // duplicate live verifier hash
            ),
            &grant(
                "oxpg-pairclaim-2-connect",
                "oxpt-pairclaim-2",
                &uid,
                None,
                "connect",
                None,
                None,
                None,
                FAR_FUTURE,
            ),
        )
        .await;
    assert!(dup_claim.is_err(), "duplicate verifier hash must fail");
    let p2 = db
        .find_oxpilot_pairing_by_code_hash("hash-pc-2")
        .await
        .expect("find p2")
        .unwrap();
    assert_eq!(p2.state, "pending", "failed claim must roll back fully");

    // NULL tombstones coexist: two disconnected targets share NULL
    // verifier_hash on every engine (partial-unique semantics).
    let t2 = claimed_target(&db, "pairclaim-3", &uid, "hash-pc-3", "vh-pc-3").await;
    db.disconnect_oxpilot_target_tx(&t.id, NOW)
        .await
        .expect("disconnect 1")
        .expect("outcome");
    db.disconnect_oxpilot_target_tx(&t2.id, NOW)
        .await
        .expect("disconnect 2")
        .expect("outcome");
    assert!(db
        .find_oxpilot_target_by_verifier_hash("vh-pc-1")
        .await
        .expect("tombstoned lookup")
        .is_none());
    let gone = db.find_oxpilot_target(&t.id).await.expect("find").unwrap();
    assert!(gone.verifier_hash.is_none());
    assert_eq!(gone.status, "disconnected");
}

/// 0088.3 — grant allow paths (connect → target connected; action →
/// Action materialized), lazy expiry, deny CAS, and the pending→expired
/// stamp.
#[tokio::test]
async fn connector_grant_state_machine() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "grantsm").await;
    let t = claimed_target(&db, "grantsm", &uid, "hash-gs", "vh-gs").await;

    // Connect-grant allow flips target → connected in the same tx.
    assert!(db
        .allow_oxpilot_connect_grant_tx("oxpg-grantsm-connect", &t.id, NOW)
        .await
        .expect("allow connect"));
    let connected = db.find_oxpilot_target(&t.id).await.expect("find").unwrap();
    assert_eq!(connected.status, "connected");
    let cg = db
        .find_oxpilot_grant("oxpg-grantsm-connect")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(cg.state, "allowed");
    // CAS re-run loses — already allowed.
    assert!(!db
        .allow_oxpilot_connect_grant_tx("oxpg-grantsm-connect", &t.id, NOW)
        .await
        .expect("re-allow"));

    let sid = "oxps-grantsm-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("session");

    // Pending action grant → allow(session) materializes the Action.
    db.insert_oxpilot_grant(&grant(
        "oxpg-grantsm-a1",
        &t.id,
        &uid,
        Some(sid),
        "action",
        Some("exec_shell"),
        Some(r#"{"cmd":"ls"}"#),
        Some("run ls"),
        FAR_FUTURE,
    ))
    .await
    .expect("mint action grant");
    let materialized = db
        .allow_oxpilot_action_grant_tx(
            "oxpg-grantsm-a1",
            "action_session",
            &action(
                "oxpa-grantsm-1",
                &t.id,
                &uid,
                sid,
                "oxpg-grantsm-a1",
                "exec_shell",
                r#"{"cmd":"ls"}"#,
                "run ls",
            ),
            NOW,
        )
        .await
        .expect("allow action")
        .expect("materialized");
    assert_eq!(materialized.state, "queued");
    let ag = db
        .find_oxpilot_grant("oxpg-grantsm-a1")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(ag.kind, "action_session");
    assert_eq!(ag.state, "allowed");

    // Live session grant resolves for dispatch.
    let live = db
        .live_oxpilot_session_grant(&t.id, sid, NOW)
        .await
        .expect("live");
    assert_eq!(live.unwrap().id, "oxpg-grantsm-a1");

    // Pending dedupe key finds the pending twin.
    db.insert_oxpilot_grant(&grant(
        "oxpg-grantsm-a2",
        &t.id,
        &uid,
        Some(sid),
        "action",
        Some("fs_read"),
        Some(r#"{"path":"/tmp/a"}"#),
        Some("read /tmp/a"),
        FAR_FUTURE,
    ))
    .await
    .expect("mint a2");
    let dup = db
        .pending_oxpilot_action_grant(&t.id, sid, "fs_read", r#"{"path":"/tmp/a"}"#)
        .await
        .expect("dedupe");
    assert_eq!(dup.unwrap().id, "oxpg-grantsm-a2");
    assert!(db
        .pending_oxpilot_action_grant(&t.id, sid, "fs_read", r#"{"path":"/other"}"#)
        .await
        .expect("distinct args")
        .is_none());

    // Deny CAS: 1 denies, re-run 0.
    assert_eq!(
        db.deny_oxpilot_grant("oxpg-grantsm-a2", NOW).await.unwrap(),
        1
    );
    assert_eq!(
        db.deny_oxpilot_grant("oxpg-grantsm-a2", NOW).await.unwrap(),
        0
    );

    // Lazy expiry stamps a past-due pending grant.
    db.insert_oxpilot_grant(&grant(
        "oxpg-grantsm-exp",
        &t.id,
        &uid,
        Some(sid),
        "action",
        Some("fs_read"),
        Some("{}"),
        Some("x"),
        FAR_PAST,
    ))
    .await
    .expect("mint exp");
    assert_eq!(
        db.expire_oxpilot_grant("oxpg-grantsm-exp", NOW)
            .await
            .unwrap(),
        1
    );
    let exp = db
        .find_oxpilot_grant("oxpg-grantsm-exp")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(exp.state, "expired");
}

/// 0088.4 — FIFO dequeue ordering, `queued→claimed` CAS single-winner,
/// `claimed→running` touch flip, idempotent report close.
#[tokio::test]
async fn connector_action_dequeue_fifo_and_report_idempotent() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "fifo").await;
    let t = claimed_target(&db, "fifo", &uid, "hash-fifo", "vh-fifo").await;
    let sid = "oxps-fifo-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("session");
    db.insert_oxpilot_grant(&grant(
        "oxpg-fifo-g1",
        &t.id,
        &uid,
        Some(sid),
        "action_session",
        Some("exec_shell"),
        None,
        None,
        FAR_FUTURE,
    ))
    .await
    .expect("grant");
    // Direct grant inserts stay `pending` — the candidate join surfaces
    // whatever state the row carries.
    db.insert_oxpilot_grant(&grant(
        "oxpg-fifo-g2",
        &t.id,
        &uid,
        Some(sid),
        "action_session",
        Some("exec_shell"),
        None,
        None,
        FAR_FUTURE,
    ))
    .await
    .expect("grant2");
    for (i, gid) in ["oxpa-fifo-1", "oxpa-fifo-2", "oxpa-fifo-3"]
        .iter()
        .enumerate()
    {
        db.insert_oxpilot_action(&action(
            gid,
            &t.id,
            &uid,
            sid,
            if i == 2 {
                "oxpg-fifo-g2"
            } else {
                "oxpg-fifo-g1"
            },
            "exec_shell",
            &format!(r#"{{"n":{i}}}"#),
            "x",
        ))
        .await
        .expect("insert action");
    }

    // FIFO candidate scan — created_at order, grant state joined.
    let cands = db
        .queued_oxpilot_action_candidates(&t.id, 50)
        .await
        .expect("candidates");
    assert_eq!(cands.len(), 3);
    assert_eq!(cands[0].action.id, "oxpa-fifo-1");
    assert_eq!(cands[1].action.id, "oxpa-fifo-2");
    assert_eq!(cands[2].action.id, "oxpa-fifo-3");
    assert_eq!(cands[0].grant_state, "pending"); // joined grant state surfaces

    // Claim CAS: first wins, re-claim loses.
    assert_eq!(
        db.claim_oxpilot_action("oxpa-fifo-1", NOW).await.unwrap(),
        1
    );
    assert_eq!(
        db.claim_oxpilot_action("oxpa-fifo-1", NOW).await.unwrap(),
        0
    );
    // Claimed rows leave the queued scan.
    let cands = db
        .queued_oxpilot_action_candidates(&t.id, 50)
        .await
        .expect("candidates");
    assert_eq!(cands.len(), 2);
    assert_eq!(cands[0].action.id, "oxpa-fifo-2");

    // Touch proof: claimed → running.
    assert_eq!(db.flip_oxpilot_claimed_to_running(&t.id).await.unwrap(), 1);
    let a1 = db
        .find_oxpilot_action("oxpa-fifo-1")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(a1.state, "running");

    // Report close: first lands terminal, second is an idempotent no-op.
    assert_eq!(
        db.close_oxpilot_action_report("oxpa-fifo-1", "done", Some("exit 0"), NOW)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        db.close_oxpilot_action_report("oxpa-fifo-1", "failed", Some("x"), NOW)
            .await
            .unwrap(),
        0
    );
    let a1 = db
        .find_oxpilot_action("oxpa-fifo-1")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(a1.state, "done");
    assert_eq!(a1.result_summary.as_deref(), Some("exit 0"));

    // Dead-grant dequeue close: g2's queued row cancels at observation.
    assert_eq!(
        db.cancel_queued_oxpilot_action("oxpa-fifo-3", NOW)
            .await
            .unwrap(),
        1
    );
    let a3 = db
        .find_oxpilot_action("oxpa-fifo-3")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(a3.state, "cancelled");
}

/// 0088.5 — disconnect tombstones the verifier and directly cancels
/// every live row; revoke keeps the verifier, revokes grants, and moves
/// in-flight to `cancel_requested` (the heartbeat cancel set). Both are
/// idempotent on terminal targets (BR6.7).
#[tokio::test]
async fn connector_disconnect_and_revoke_close_paths() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "close").await;
    let sid = "oxps-close-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("session");

    // --- revoke arm: verifier kept, grants revoked, in-flight moves. ---
    let t = claimed_target(&db, "close-rev", &uid, "hash-cr", "vh-cr").await;
    for (gid, kind, st) in [
        ("oxpg-cr-pend", "action", "pending"),
        ("oxpg-cr-allow", "action_session", "allowed"),
    ] {
        db.insert_oxpilot_grant(&grant(
            gid,
            &t.id,
            &uid,
            Some(sid),
            kind,
            Some("exec_shell"),
            Some("{}"),
            Some("x"),
            FAR_FUTURE,
        ))
        .await
        .expect("grant");
        if st == "allowed" {
            db.allow_oxpilot_action_grant_tx(
                gid,
                "action_session",
                &action(
                    &format!("{gid}-seed"),
                    &t.id,
                    &uid,
                    sid,
                    gid,
                    "exec_shell",
                    "{}",
                    "x",
                ),
                NOW,
            )
            .await
            .expect("allow")
            .expect("action");
        }
    }
    for aid in ["oxpa-cr-q", "oxpa-cr-run"] {
        db.insert_oxpilot_action(&action(
            aid,
            &t.id,
            &uid,
            sid,
            "oxpg-cr-allow",
            "exec_shell",
            "{}",
            "x",
        ))
        .await
        .expect("action");
    }
    db.claim_oxpilot_action("oxpa-cr-run", NOW).await.unwrap();
    db.flip_oxpilot_claimed_to_running(&t.id).await.unwrap();

    let outcome = db
        .revoke_oxpilot_target_tx(&t.id, NOW)
        .await
        .expect("revoke")
        .expect("outcome");
    assert!(!outcome.grants.is_empty(), "revoked grants returned");
    assert!(!outcome.cancelled_actions.is_empty(), "queued cancelled");
    assert!(
        !outcome.cancel_requested_actions.is_empty(),
        "in-flight cancel_requested"
    );
    let revoked = db.find_oxpilot_target(&t.id).await.expect("find").unwrap();
    assert_eq!(revoked.status, "revoked");
    // The verifier is NOT tombstoned — heartbeat still authenticates.
    assert_eq!(revoked.verifier_hash.as_deref(), Some("vh-cr"));
    assert!(db
        .find_oxpilot_target_by_verifier_hash("vh-cr")
        .await
        .expect("lookup")
        .is_some());
    let cancel_set = db.oxpilot_cancel_requested_ids(&t.id).await.expect("set");
    assert!(cancel_set.contains(&"oxpa-cr-run".to_string()));
    let a3 = db
        .find_oxpilot_action("oxpa-cr-q")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(a3.state, "cancelled");
    let g = db
        .find_oxpilot_grant("oxpg-cr-pend")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(g.state, "revoked");
    // BR6.7: already-terminal → idempotent None.
    assert!(db
        .revoke_oxpilot_target_tx(&t.id, NOW)
        .await
        .expect("re-revoke")
        .is_none());
    assert!(db
        .disconnect_oxpilot_target_tx(&t.id, NOW)
        .await
        .expect("disconnect on revoked")
        .is_none());

    // --- disconnect arm: verifier tombstoned, in-flight cancelled direct. ---
    let t2 = claimed_target(&db, "close-dis", &uid, "hash-cd", "vh-cd").await;
    db.insert_oxpilot_grant(&grant(
        "oxpg-cd-pend",
        &t2.id,
        &uid,
        Some(sid),
        "action",
        Some("exec_shell"),
        Some("{}"),
        Some("x"),
        FAR_FUTURE,
    ))
    .await
    .expect("grant");
    db.insert_oxpilot_action(&action(
        "oxpa-cd-run",
        &t2.id,
        &uid,
        sid,
        "oxpg-cd-pend",
        "exec_shell",
        "{}",
        "x",
    ))
    .await
    .expect("action");
    db.claim_oxpilot_action("oxpa-cd-run", NOW).await.unwrap();
    let outcome = db
        .disconnect_oxpilot_target_tx(&t2.id, NOW)
        .await
        .expect("disconnect")
        .expect("outcome");
    assert!(!outcome.grants.is_empty());
    assert!(!outcome.cancelled_actions.is_empty());
    // Dead channel — NO cancel_requested survives disconnect.
    assert!(outcome.cancel_requested_actions.is_empty());
    assert!(db
        .oxpilot_cancel_requested_ids(&t2.id)
        .await
        .expect("set")
        .is_empty());
    let gone = db.find_oxpilot_target(&t2.id).await.expect("find").unwrap();
    assert_eq!(gone.status, "disconnected");
    assert!(gone.verifier_hash.is_none());
    let a = db
        .find_oxpilot_action("oxpa-cd-run")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(a.state, "cancelled");
    let g = db
        .find_oxpilot_grant("oxpg-cd-pend")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(g.state, "cancelled");
}

/// 0088.6 — C8 hook store leg: the bulk-cancel tx closes session grants
/// + actions; the session cascade then removes them; connect grants
/// (session_id NULL) survive both (BR6.5/BR6.6).
#[tokio::test]
async fn connector_session_delete_cancel_then_cascade() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "sessdel").await;
    let t = claimed_target(&db, "sessdel", &uid, "hash-sd", "vh-sd").await;
    let sid = "oxps-sessdel-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("session");
    db.insert_oxpilot_grant(&grant(
        "oxpg-sd-a1",
        &t.id,
        &uid,
        Some(sid),
        "action",
        Some("exec_shell"),
        Some("{}"),
        Some("x"),
        FAR_FUTURE,
    ))
    .await
    .expect("grant");
    db.insert_oxpilot_action(&action(
        "oxpa-sd-1",
        &t.id,
        &uid,
        sid,
        "oxpg-sd-a1",
        "exec_shell",
        "{}",
        "x",
    ))
    .await
    .expect("action");

    // Hook tx: grant + action directly `cancelled` (no cancel_requested —
    // the rows cascade before delivery could ever land).
    let (grants, actions) = db
        .cancel_oxpilot_session_connector_rows_tx(sid, NOW)
        .await
        .expect("hook tx");
    assert_eq!(grants.len(), 1);
    assert_eq!(actions.len(), 1);
    let g = db
        .find_oxpilot_grant("oxpg-sd-a1")
        .await
        .expect("find")
        .unwrap();
    assert_eq!(g.state, "cancelled");
    // Idempotent re-run finds nothing live.
    let (g2, a2) = db
        .cancel_oxpilot_session_connector_rows_tx(sid, NOW)
        .await
        .expect("re-run");
    assert!(g2.is_empty() && a2.is_empty());

    // Cascade removes session-scoped rows; connect grant + target survive.
    db.delete_oxpilot_session_cascade(sid, &uid)
        .await
        .expect("cascade");
    assert!(db
        .find_oxpilot_grant("oxpg-sd-a1")
        .await
        .expect("find")
        .is_none());
    assert!(db
        .find_oxpilot_action("oxpa-sd-1")
        .await
        .expect("find")
        .is_none());
    let connect = db
        .find_oxpilot_grant("oxpg-sessdel-connect")
        .await
        .expect("find")
        .expect("connect grant survives (BR6.6)");
    assert_eq!(connect.state, "pending");
    assert!(db.find_oxpilot_target(&t.id).await.expect("find").is_some());
}

/// 0088.7 — table-shape proof: the four connector tables exist with the
/// pinned columns on the live dialect (SELECT-list assertion — a
/// missing column fails the query).
#[tokio::test]
async fn connector_schema_columns_exist() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let pool = RawPool::connect(&url).await;

    for sql in [
        "SELECT id, user_id, code_hash, target_name, requested_caps, state, expires_at, created_at FROM oxpilot_pairings LIMIT 0",
        "SELECT id, user_id, pairing_id, name, caps, status, verifier_hash, last_seen_at, created_at, disconnected_at, revoked_at FROM oxpilot_targets LIMIT 0",
        "SELECT id, target_id, user_id, session_id, kind, action_kind, action_args, action_summary, state, expires_at, resolved_at, created_at FROM oxpilot_grants LIMIT 0",
        "SELECT id, target_id, user_id, session_id, grant_id, kind, args, args_summary, state, result_summary, created_at, claimed_at, finished_at FROM oxpilot_actions LIMIT 0",
    ] {
        pool.exec(sql).await.expect("column probe");
    }
    // Hot-path indexes exist by name where the dialect exposes them —
    // the FIFO dequeue index drives BR5.1's scan.
    match pool {
        RawPool::Postgres(_) => {
            let n = pool
                .count("SELECT COUNT(*) FROM pg_indexes WHERE indexname = 'idx_oxpilot_actions_dequeue'")
                .await;
            assert_eq!(n, 1, "dequeue index");
        }
        RawPool::MySql(_) => {
            let n = pool
                .count("SELECT COUNT(*) FROM information_schema.statistics WHERE index_name = 'idx_oxpilot_actions_dequeue'")
                .await;
            assert_eq!(n, 4, "dequeue composite index");
        }
        RawPool::Sqlite(_) => {
            let n = pool
                .count(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_oxpilot_actions_dequeue'",
                )
                .await;
            assert_eq!(n, 1, "dequeue index");
        }
    }
}

// ---- 0089 oxpilot_proposals_ops (U5) — ops_json + find-by-session ----

/// The additive `ops_json` column round-trips verbatim on insert/select
/// in every dialect, and a row written pre-column shape (raw INSERT
/// omitting it) reads back the `'[]'` backfill default — never NULL.
#[tokio::test]
async fn proposal_ops_json_round_trip_and_backfill_default() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let pool = RawPool::connect(&url).await;
    let (uid, repo) = seed(&db, "opsjson").await;
    let sid = "oxps-opsjson-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "ops"))
        .await
        .expect("session");

    // Round-trip: the serialized op set comes back byte-identical.
    let ops = r#"[{"path":"src/a.rs","op":"create","new_path":null,"content_b64":"aGk="},{"path":"old.rs","op":"rename","new_path":"new.rs","content_b64":null}]"#;
    db.insert_oxpilot_proposal(&oxidean_db::NewOxpilotProposal {
        id: "oxpp-opsjson-1",
        session_id: sid,
        title: "ops diff",
        base: "main",
        diff_body: "diff --git a/src/a.rs b/src/a.rs\n",
        ops_json: ops,
    })
    .await
    .expect("proposal");
    let rows = db
        .find_oxpilot_proposals_by_session(sid)
        .await
        .expect("find");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ops_json, ops, "ops_json must round-trip verbatim");

    // Backfill: a pre-column-shaped INSERT (no ops_json in the column
    // list) reads back the `'[]'` default on every dialect.
    pool.exec(
        "INSERT INTO oxpilot_proposals (id, session_id, title, base, diff_body, status, created_at) \
         VALUES ('oxpp-opsjson-legacy', 'oxps-opsjson-1', 'legacy', 'main', 'diff --git a/x b/x', 'pending', '2026-01-01T00:00:00Z')",
    )
    .await
    .expect("raw insert");
    let rows = db
        .find_oxpilot_proposals_by_session(sid)
        .await
        .expect("find legacy");
    let legacy = rows
        .iter()
        .find(|r| r.id == "oxpp-opsjson-legacy")
        .expect("legacy row");
    assert_eq!(legacy.ops_json, "[]", "pre-column row must backfill '[]'");
}

/// `find_oxpilot_proposals_by_session` is newest-first by
/// `(created_at DESC, id DESC)` and session-scoped — a foreign
/// session's proposals never appear in the result.
#[tokio::test]
async fn find_proposals_by_session_newest_first_and_scoped() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "propsess").await;
    let sid = "oxps-propsess-1";
    let other_sid = "oxps-propsess-2";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "mine"))
        .await
        .expect("session");
    db.insert_oxpilot_session(&session(other_sid, &uid, &repo, "other"))
        .await
        .expect("session");

    for (id, s) in [
        ("oxpp-propsess-a", sid),
        ("oxpp-propsess-b", sid),
        ("oxpp-propsess-c", sid),
        ("oxpp-propsess-foreign", other_sid),
    ] {
        db.insert_oxpilot_proposal(&oxidean_db::NewOxpilotProposal {
            id,
            session_id: s,
            title: "t",
            base: "main",
            diff_body: "d",
            ops_json: "[]",
        })
        .await
        .expect("proposal");
    }

    let rows = db
        .find_oxpilot_proposals_by_session(sid)
        .await
        .expect("find");
    assert_eq!(rows.len(), 3, "foreign session row must be excluded");
    assert!(rows.iter().all(|r| r.session_id == sid));
    // Newest-first = descending (created_at, id) — verified against the
    // rows themselves, not an assumed insert clock granularity.
    let mut expected = rows.clone();
    expected.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    let got: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    let want: Vec<&str> = expected.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(got, want, "must be (created_at DESC, id DESC) ordered");
}

/// The id-keyed read shares the same `PROPOSAL_SELECT_*` column list —
/// `ops_json` must surface there too, `pr_ref` starts NULL and the
/// `mark_oxpilot_proposal_applied` verb lands `status='applied'` + the
/// pull reference on every dialect.
#[tokio::test]
async fn proposal_id_keyed_read_and_apply_transition() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "idkey").await;
    let sid = "oxps-idkey-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("session");

    let ops = r#"[{"path":"a.rs","op":"delete","new_path":null,"content_b64":null}]"#;
    db.insert_oxpilot_proposal(&oxidean_db::NewOxpilotProposal {
        id: "oxpp-idkey-1",
        session_id: sid,
        title: "t",
        base: "main",
        diff_body: "diff --git a/a.rs b/a.rs\n",
        ops_json: ops,
    })
    .await
    .expect("insert");

    let row = db
        .find_oxpilot_proposal("oxpp-idkey-1")
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.ops_json, ops, "id-keyed read must carry ops_json");
    assert_eq!(row.status, "pending");
    assert_eq!(row.pr_ref, None);

    let n = db
        .mark_oxpilot_proposal_applied("oxpp-idkey-1", "oxpilot/repo-idkey#7")
        .await
        .expect("mark applied");
    assert_eq!(n, 1);
    let row = db
        .find_oxpilot_proposal("oxpp-idkey-1")
        .await
        .expect("find applied")
        .expect("row");
    assert_eq!(row.status, "applied");
    assert_eq!(row.pr_ref.as_deref(), Some("oxpilot/repo-idkey#7"));

    // Unknown id reads resolve None (the service's not_found source).
    assert!(db
        .find_oxpilot_proposal("oxpp-idkey-missing")
        .await
        .expect("find missing")
        .is_none());
}

/// `find_oxpilot_proposals_by_session` on a session that exists but has
/// no proposals — and on a dead id — is an empty list, never an error:
/// the ArtifactScope rebuild treats both identically.
#[tokio::test]
async fn find_proposals_by_session_empty_is_not_an_error() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "emptysess").await;
    db.insert_oxpilot_session(&session("oxps-empty-1", &uid, &repo, "s"))
        .await
        .expect("session");

    assert!(db
        .find_oxpilot_proposals_by_session("oxps-empty-1")
        .await
        .expect("empty session")
        .is_empty());
    assert!(db
        .find_oxpilot_proposals_by_session("oxps-no-such")
        .await
        .expect("dead session")
        .is_empty());
}

/// `ops_json` is stored verbatim at the db layer — the redaction
/// boundary is the service above it, not the store. Payloads carrying
/// unicode and nested quotes must round-trip untouched so apply
/// executes the exact op list the reviewer saw.
#[tokio::test]
async fn proposal_ops_json_stored_verbatim_no_redaction() {
    let Some(url) = database_url() else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    let _guard = SERIAL.lock().await;
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    let (uid, repo) = seed(&db, "verbatim").await;
    let sid = "oxps-verbatim-1";
    db.insert_oxpilot_session(&session(sid, &uid, &repo, "s"))
        .await
        .expect("session");

    let ops = r#"[{"path":"src/üñí.rs","op":"create","new_path":null,"content_b64":"eyJ0b2...zIn0="},{"path":"a.rs","op":"update","new_path":null,"content_b64":"Yj0="}]"#;
    db.insert_oxpilot_proposal(&oxidean_db::NewOxpilotProposal {
        id: "oxpp-verbatim-1",
        session_id: sid,
        title: "t",
        base: "main",
        diff_body: "diff --git a/src/x.rs b/src/x.rs\n",
        ops_json: ops,
    })
    .await
    .expect("insert");
    let row = db
        .find_oxpilot_proposal("oxpp-verbatim-1")
        .await
        .expect("find")
        .expect("row");
    assert_eq!(
        row.ops_json, ops,
        "store must not redact or mangle ops_json"
    );
}
