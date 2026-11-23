//! 12-02: `0016_pull_requests` schema presence + migrate (PR domain).

use oxidean_core::Role;
use oxidean_db::Database;

#[test]
fn dialect_pull_review_submission_order_migrations_use_serialized_counters() {
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!(
            "{}/migrations/{dialect}/0107_pull_review_submission_order.sql",
            env!("CARGO_MANIFEST_DIR")
        );
        let sql =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{dialect}: missing {path}"));
        for needle in [
            "submission_order",
            "pull_review_submission_order",
            "last_order",
            "INSERT INTO pull_review_submission_order",
            "CREATE UNIQUE INDEX",
        ] {
            assert!(
                sql.contains(needle),
                "{dialect}: migration must contain {needle}"
            );
        }
    }
}

#[tokio::test]
async fn dialect_pulls_migrate_0016_schema_presence() {
    for (label, path) in [
        (
            "sqlite",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/migrations/sqlite/0016_pull_requests.sql"
            ),
        ),
        (
            "postgres",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/migrations/postgres/0016_pull_requests.sql"
            ),
        ),
        (
            "mysql",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/migrations/mysql/0016_pull_requests.sql"
            ),
        ),
    ] {
        let sql = std::fs::read_to_string(path).unwrap_or_default();
        assert!(!sql.is_empty(), "{label} 0016_pull_requests.sql must exist");
        for needle in [
            "pull_requests",
            "pull_comments",
            "pull_reviews",
            "pull_review_requests",
            "pull_labels",
            "pull_assignees",
            "allow_merge_commit",
            "allow_squash_merge",
            "allow_rebase_merge",
            "forked_from_repo_id",
        ] {
            assert!(sql.contains(needle), "{label} 0016 must define {needle}");
        }
        assert!(
            sql.contains("'pr'")
                || sql.contains("\"pr\"")
                || sql.contains(", 'pr'")
                || sql.contains("pr'"),
            "{label} 0016 must expand issue_links kind to include pr"
        );
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pulls.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate 0016_pull_requests");

    let author = db
        .create_user(
            "u-pull-author",
            "pullauthor@example.com",
            "pullauthor",
            Some("hash"),
            "Pull Author",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create author");

    let repo = db
        .insert_repository(
            "r-pulls-1",
            &author.id,
            "user",
            "pulls-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("insert repo");

    let settings = db
        .get_repo_merge_settings(&repo.id)
        .await
        .expect("merge settings defaults");
    assert!(settings.allow_merge_commit);
    assert!(settings.allow_squash_merge);
    assert!(settings.allow_rebase_merge);

    let n = db
        .allocate_next_issue_number(&repo.id)
        .await
        .expect("allocate shared #N");
    let pull = db
        .insert_pull(
            "p-1", &repo.id, n, "First PR", "body", &author.id, "main", "abc", &repo.id, "feature",
            "def", false,
        )
        .await
        .expect("insert pull");
    assert_eq!(pull.number, n);
    assert_eq!(pull.state, "open");

    let first_review = db
        .insert_pull_review("pr-order-1", &pull.id, &author.id, "approved", "", None)
        .await
        .expect("insert first review");
    let second_review = db
        .insert_pull_review(
            "pr-order-2",
            &pull.id,
            &author.id,
            "changes_requested",
            "",
            None,
        )
        .await
        .expect("insert second review");
    assert!(first_review.submission_order.is_some());
    assert!(
        first_review.submission_order < second_review.submission_order,
        "review submissions must receive increasing order values"
    );

    let found = db
        .find_pull_by_repo_number(&repo.id, n)
        .await
        .expect("find")
        .expect("present");
    assert_eq!(found.id, "p-1");

    let (list, total) = db
        .list_pulls_for_repo(&repo.id, Some("open"), 0, 20)
        .await
        .expect("list");
    assert_eq!(total, 1);
    assert_eq!(list.len(), 1);

    // Postgres maps TIMESTAMPTZ → String via PULL_COLS_PG; keep the cast list in sync.
    let pulls_src = include_str!("../src/pulls.rs");
    assert!(
        pulls_src.contains("PULL_COLS_PG"),
        "Postgres pull SELECTs must use PULL_COLS_PG (TIMESTAMPTZ → text)"
    );
    assert!(
        pulls_src.contains("to_char(created_at AT TIME ZONE 'UTC'")
            && pulls_src.contains("AS created_at"),
        "PULL_COLS_PG must cast created_at for sqlx String decode"
    );
}

/// Batch pull-list helpers: assignees, label filter, and reviews must cover a
/// whole page in one round trip each (N+1 regression guard).
#[tokio::test]
async fn dialect_pulls_batch_enrichment() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("pulls_batch.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let author = db
        .create_user(
            "u-pbatch-author",
            "pbatchauthor@example.com",
            "pbatchauthor",
            Some("hash"),
            "P Batch Author",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create author");
    let reviewer = db
        .create_user(
            "u-pbatch-rev",
            "pbatchrev@example.com",
            "pbatchrev",
            Some("hash"),
            "P Batch Rev",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create reviewer");

    let repo = db
        .insert_repository(
            "r-pbatch-1",
            &author.id,
            "user",
            "pbatch-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("insert repo");

    let p1 = db
        .insert_pull(
            "pb-1",
            &repo.id,
            db.allocate_next_issue_number(&repo.id).await.expect("n1"),
            "PR one",
            "b1",
            &author.id,
            "main",
            "abc",
            &repo.id,
            "f1",
            "def",
            false,
        )
        .await
        .expect("pull 1");
    let p2 = db
        .insert_pull(
            "pb-2",
            &repo.id,
            db.allocate_next_issue_number(&repo.id).await.expect("n2"),
            "PR two",
            "b2",
            &reviewer.id,
            "main",
            "abc",
            &repo.id,
            "f2",
            "ghi",
            false,
        )
        .await
        .expect("pull 2");

    // pull_assignees / pull_labels have no facade writers yet — insert directly.
    let pool = sqlx::SqlitePool::connect(&url).await.expect("raw pool");
    sqlx::query("INSERT INTO pull_assignees (pull_id, user_id) VALUES (?1, ?2)")
        .bind(&p1.id)
        .bind(&author.id)
        .execute(&pool)
        .await
        .expect("assignee p1");
    sqlx::query("INSERT INTO pull_assignees (pull_id, user_id) VALUES (?1, ?2)")
        .bind(&p2.id)
        .bind(&reviewer.id)
        .execute(&pool)
        .await
        .expect("assignee p2");

    let label = db
        .insert_label("plb-1", "bug", "d73a4a", "A bug", None, Some(&repo.id))
        .await
        .expect("insert label");
    sqlx::query("INSERT INTO pull_labels (pull_id, label_id) VALUES (?1, ?2)")
        .bind(&p1.id)
        .bind(&label.id)
        .execute(&pool)
        .await
        .expect("label p1");

    db.insert_pull_review("pr-1", &p1.id, &reviewer.id, "approved", "lgtm", None)
        .await
        .expect("review p1");
    pool.close().await;

    let pull_ids = vec![p1.id.clone(), p2.id.clone()];

    let assignee_pairs = db
        .list_pull_assignees_for_pulls(&pull_ids)
        .await
        .expect("batch assignees");
    assert_eq!(assignee_pairs.len(), 2);
    for (pull_id, want_user) in [(&p1.id, &author.id), (&p2.id, &reviewer.id)] {
        let row = assignee_pairs
            .iter()
            .find(|(pid, _)| pid == pull_id)
            .expect("pair present");
        assert_eq!(row.1.user_id, *want_user);
        assert!(!row.1.username.is_empty());
    }

    // Label filter — name or id, case-insensitive name.
    for needle in [label.id.as_str(), "BUG", "bug"] {
        let hit = db
            .pull_ids_with_label(&pull_ids, needle)
            .await
            .expect("label filter");
        assert_eq!(hit, vec![p1.id.clone()], "needle {needle}");
    }
    assert!(db
        .pull_ids_with_label(&pull_ids, "missing")
        .await
        .expect("no match")
        .is_empty());

    // Reviews batch.
    let reviews = db
        .list_reviews_for_pulls(&pull_ids)
        .await
        .expect("batch reviews");
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].pull_id, p1.id);
    assert_eq!(reviews[0].state, "approved");

    // comment_count is carried on every pull row via PULL_COLS subquery.
    assert_eq!(p1.comment_count, 0);
    db.insert_pull_comment(
        "pc-1",
        &p1.id,
        &reviewer.id,
        "looks good",
        None,
        None,
        None,
        None,
        None,
        false,
    )
    .await
    .expect("insert pull comment");
    let reloaded = db
        .find_pull_by_repo_number(&repo.id, p1.number)
        .await
        .expect("find")
        .expect("present");
    assert_eq!(reloaded.comment_count, 1);
    let (listed, _) = db
        .list_pulls_for_repo(&repo.id, Some("open"), 0, 20)
        .await
        .expect("list");
    assert_eq!(
        listed
            .iter()
            .find(|r| r.id == p1.id)
            .expect("row")
            .comment_count,
        1
    );
    assert_eq!(
        listed
            .iter()
            .find(|r| r.id == p2.id)
            .expect("row")
            .comment_count,
        0
    );

    // Batch repo/user lookups used by pull list enrichment.
    let repos = db
        .find_repositories_by_ids(std::slice::from_ref(&repo.id))
        .await
        .expect("batch repos");
    assert_eq!(repos.len(), 1);
    let users = db
        .find_users_by_ids(&[author.id.clone(), reviewer.id.clone()])
        .await
        .expect("batch users");
    assert_eq!(users.len(), 2);

    // Empty inputs must not error.
    assert!(db
        .list_pull_assignees_for_pulls(&[])
        .await
        .expect("empty assignees")
        .is_empty());
    assert!(db
        .list_reviews_for_pulls(&[])
        .await
        .expect("empty reviews")
        .is_empty());
    assert!(db
        .pull_ids_with_label(&[], "bug")
        .await
        .expect("empty labels")
        .is_empty());
}

#[tokio::test]
async fn postgres_review_order_follows_insert_order_not_transaction_start() {
    use sqlx::{postgres::PgPoolOptions, Row};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    if !url.starts_with("postgres://") && !url.starts_with("postgresql://") {
        eprintln!("skipping: DATABASE_URL is not PostgreSQL");
        return;
    }

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect PostgreSQL");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let schema = format!("pull_review_order_{}_{}", std::process::id(), nonce);
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&pool)
        .await
        .expect("create isolated schema");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {schema}.pull_review_submission_order (id SMALLINT PRIMARY KEY, last_order BIGINT NOT NULL)"
    )))
    .execute(&pool)
    .await
    .expect("create order table");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {schema}.pull_review_submission_order (id, last_order) VALUES (1, 0)"
    )))
    .execute(&pool)
    .await
    .expect("seed order table");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {schema}.pull_reviews (state TEXT NOT NULL, submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(), submission_order BIGINT NOT NULL)"
    )))
    .execute(&pool)
    .await
    .expect("create reviews table");

    let mut earlier = pool.begin().await.expect("begin earlier transaction");
    let earlier_started = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT now()")
        .fetch_one(&mut *earlier)
        .await
        .expect("read earlier transaction timestamp");
    tokio::time::sleep(Duration::from_secs(2)).await;

    let mut later = pool.begin().await.expect("begin later transaction");
    let later_started = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT now()")
        .fetch_one(&mut *later)
        .await
        .expect("read later transaction timestamp");
    let approval_order = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
        "UPDATE {schema}.pull_review_submission_order SET last_order = last_order + 1 WHERE id = 1 RETURNING last_order"
    )))
    .fetch_one(&mut *later)
    .await
    .expect("allocate approval order");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {schema}.pull_reviews (state, submission_order) VALUES ('approved', $1)"
    )))
    .bind(approval_order)
    .execute(&mut *later)
    .await
    .expect("insert approval");
    later.commit().await.expect("commit later transaction");

    let changes_order = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
        "UPDATE {schema}.pull_review_submission_order SET last_order = last_order + 1 WHERE id = 1 RETURNING last_order"
    )))
    .fetch_one(&mut *earlier)
    .await
    .expect("allocate changes-requested order");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {schema}.pull_reviews (state, submission_order) VALUES ('changes_requested', $1)"
    )))
    .bind(changes_order)
    .execute(&mut *earlier)
    .await
    .expect("insert changes-requested review");
    earlier.commit().await.expect("commit earlier transaction");

    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT state, submitted_at, submission_order FROM {schema}.pull_reviews ORDER BY submission_order"
    )))
    .fetch_all(&pool)
    .await
    .expect("read reviews");
    assert_eq!(approval_order, 1);
    assert_eq!(changes_order, 2);
    assert_eq!(rows[0].get::<String, _>("state"), "approved");
    assert_eq!(rows[1].get::<String, _>("state"), "changes_requested");
    let approval_at = rows[0].get::<chrono::DateTime<chrono::Utc>, _>("submitted_at");
    let changes_at = rows[1].get::<chrono::DateTime<chrono::Utc>, _>("submitted_at");
    assert!(earlier_started < later_started);
    assert!(changes_at < approval_at);

    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&pool)
        .await
        .expect("drop isolated schema");
    pool.close().await;
}
