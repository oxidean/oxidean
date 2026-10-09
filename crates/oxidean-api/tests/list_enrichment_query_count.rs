//! N+1 regression guard: `issue.list` / `pull.list` must keep a constant query
//! count regardless of page size — enrichment is batched (`IN (...)`) rather
//! than per-row.
//!
//! sqlx emits one `sqlx::query` tracing event per statement (debug level, on by
//! default), so counting events around an RPC measures real DB round trips.

mod support;

use std::io::Write;
use std::sync::Mutex;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;

/// Serializes tests in this binary — the statement buffer is process-global.
static SERIAL: Mutex<()> = Mutex::new(());
static SQL_BUF: Mutex<String> = Mutex::new(String::new());

struct SqlWriter;

impl Write for SqlWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        SQL_BUF
            .lock()
            .unwrap()
            .push_str(&String::from_utf8_lossy(buf));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct SqlMakeWriter;

impl<'a> MakeWriter<'a> for SqlMakeWriter {
    type Writer = SqlWriter;

    fn make_writer(&'a self) -> Self::Writer {
        SqlWriter
    }
}

fn install_sql_capture() {
    // Ignore the error — another test in this binary may have installed first;
    // both write into the same buffer and serialize via `SERIAL`.
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(SqlMakeWriter)
        .with_ansi(false)
        .try_init();
}

fn sql_count() -> usize {
    SQL_BUF.lock().unwrap().matches("sqlx::query").count()
}

fn clear_sql() {
    SQL_BUF.lock().unwrap().clear();
}

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

use std::sync::Arc;

fn rpc_req(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn rpc_json(app: &axum::Router, cookie: &str, body: &str) -> serde_json::Value {
    let res = app.clone().oneshot(rpc_req(body, cookie)).await.unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn signup_verified_owner(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
) -> String {
    let signup = rpc_json(
        app,
        "",
        &format!(
            r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
        ),
    )
    .await;
    assert_eq!(signup["ok"], true, "signup — {signup}");
    let user_id = signup["data"]["id"].as_str().expect("user id");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");

    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rpc")
                .header("content-type", "application/json")
                .header("Oxidean-RPC-Version", "1")
                .body(Body::from(format!(
                    r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    login
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .trim()
        .to_string()
}

/// `issue.list` on a 1-row page and a 6-row page must run the same number of
/// statements — labels/assignees/reactions/comments enrich in batch.
#[tokio::test]
async fn issue_list_query_count_does_not_grow_with_rows() {
    let _serial = SERIAL.lock().unwrap();
    install_sql_capture();

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("issue_n1.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = signup_verified_owner(&app, &db, "qc@ex.com", "qcowner").await;

    for repo_name in ["solo", "crowd"] {
        let created = rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"repo.create","input":{{"name":"{repo_name}","visibility":"public","description":""}}}}"#
            ),
        )
        .await;
        assert_eq!(created["ok"], true, "repo.create {repo_name} — {created}");
    }

    // "solo": one issue. "crowd": six issues, each with label + assignee +
    // reaction + comment so every enrichment path is exercised.
    let owner = db
        .find_user_by_username("qcowner")
        .await
        .expect("user")
        .expect("present");
    for (repo_name, n) in [("solo", 1), ("crowd", 6)] {
        let label = rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"label.create","input":{{"scope":"repo","owner":"qcowner","repo":"{repo_name}","name":"bug","color":"d73a4a","description":"bugs"}}}}"#
            ),
        )
        .await;
        assert_eq!(label["ok"], true, "label.create — {label}");
        let label_id = label["data"]["id"].as_str().expect("label id");
        for i in 1..=n {
            let issue = rpc_json(
                &app,
                &cookie,
                &format!(
                    r#"{{"procedure":"issue.create","input":{{"owner":"qcowner","name":"{repo_name}","title":"Issue {i}","body":"b"}}}}"#
                ),
            )
            .await;
            assert_eq!(issue["ok"], true, "issue.create — {issue}");
            for body in [
                format!(
                    r#"{{"procedure":"issue.labels.set","input":{{"owner":"qcowner","name":"{repo_name}","number":{i},"labelIds":["{label_id}"]}}}}"#
                ),
                format!(
                    r#"{{"procedure":"issue.assignees.set","input":{{"owner":"qcowner","name":"{repo_name}","number":{i},"userIds":["{}"]}}}}"#,
                    owner.id
                ),
                format!(
                    r#"{{"procedure":"issue.comments.create","input":{{"owner":"qcowner","name":"{repo_name}","number":{i},"body":"note"}}}}"#
                ),
                format!(
                    r#"{{"procedure":"issue.reactions.toggle","input":{{"owner":"qcowner","name":"{repo_name}","number":{i},"target":"issue","content":"+1"}}}}"#
                ),
            ] {
                let v = rpc_json(&app, &cookie, &body).await;
                assert_eq!(v["ok"], true, "seed — {v}");
            }
        }
    }

    let mut counts = Vec::new();
    for repo_name in ["solo", "crowd"] {
        clear_sql();
        let list = rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"issue.list","input":{{"owner":"qcowner","name":"{repo_name}","state":"all","limit":50}}}}"#
            ),
        )
        .await;
        assert_eq!(list["ok"], true, "issue.list {repo_name} — {list}");
        counts.push((
            repo_name,
            sql_count(),
            list["data"]["issues"].as_array().unwrap().len(),
        ));
    }

    let [(solo_repo, solo_q, solo_rows), (crowd_repo, crowd_q, crowd_rows)] =
        [counts[0].clone(), counts[1].clone()];
    assert_eq!(solo_rows, 1);
    assert_eq!(crowd_rows, 6);
    assert!(
        crowd_q > 0,
        "sql statement capture recorded nothing — guard would pass vacuously"
    );
    assert_eq!(
        solo_q, crowd_q,
        "issue.list ran {crowd_q} statements for 6 rows vs {solo_q} for 1 ({solo_repo}/{crowd_repo}) — per-row enrichment regressed"
    );
}

/// `pull.list` must also stay row-count-independent: authors, head repos,
/// head owners, and assignees all enrich in batch.
#[tokio::test]
async fn pull_list_query_count_does_not_grow_with_rows() {
    let _serial = SERIAL.lock().unwrap();
    install_sql_capture();

    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("pull_n1.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = signup_verified_owner(&app, &db, "pq@ex.com", "pqowner").await;
    let owner = db
        .find_user_by_username("pqowner")
        .await
        .expect("user")
        .expect("present");

    let created = rpc_json(
        &app,
        &cookie,
        r#"{"procedure":"repo.create","input":{"name":"prbox","visibility":"public","description":""}}"#,
    )
    .await;
    assert_eq!(created["ok"], true, "repo.create — {created}");
    let repo = db
        .find_repository_by_owner_name(&owner.id, "prbox")
        .await
        .expect("repo")
        .expect("present");

    // Seed 6 pulls directly — pull.create needs git branches on disk. Reviews
    // seed via the facade; assignee/label joins stay empty (no writers yet),
    // which is fine — the batch queries still execute either way.
    for i in 0..6 {
        let n = db
            .allocate_next_issue_number(&repo.id)
            .await
            .expect("number");
        let pull = db
            .insert_pull(
                &format!("pqc-{i}"),
                &repo.id,
                n,
                &format!("PR {i}"),
                "b",
                &owner.id,
                "main",
                "abc",
                &repo.id,
                &format!("f{i}"),
                "def",
                false,
            )
            .await
            .expect("pull");
        db.insert_pull_review(
            &format!("pqc-r{i}"),
            &pull.id,
            &owner.id,
            "approved",
            "ok",
            None,
        )
        .await
        .expect("review");
    }

    // Full page (6 rows) vs a 1-row slice of the same list — enrichment query
    // count must not scale with page size.
    let mut counts = Vec::new();
    for limit in [1, 6] {
        clear_sql();
        let list = rpc_json(
            &app,
            &cookie,
            &format!(
                r#"{{"procedure":"pull.list","input":{{"owner":"pqowner","name":"prbox","state":"all","limit":{limit}}}}}"#
            ),
        )
        .await;
        assert_eq!(list["ok"], true, "pull.list — {list}");
        counts.push(sql_count());
    }
    assert!(
        counts[1] > 0,
        "sql statement capture recorded nothing — guard would pass vacuously"
    );
    assert_eq!(
        counts[0], counts[1],
        "pull.list ran {} statements for limit=6 vs {} for limit=1 — per-row enrichment regressed",
        counts[1], counts[0]
    );
}
