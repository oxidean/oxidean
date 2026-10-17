//! Phase 7 — full `permissions:` scope set in the durable payload, deployment
//! environments (auto-create, protection gates, wait timers, deployments),
//! per-job OIDC `id-token` credentials, and the OIDC HTTP surface.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::enqueue_run;
use oxidean_api::actions::parse::GITHUB_PERMISSION_SCOPES;
use oxidean_api::actions::parse_workflow_yaml;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use tower::ServiceExt;

async fn mk_owner_repo(
    db: &Database,
    suffix: &str,
) -> (oxidean_db::UserRow, oxidean_db::RepositoryRow) {
    let owner = db
        .create_user(
            &format!("u-p7-{suffix}"),
            &format!("p7{suffix}@example.com"),
            &format!("p7{suffix}"),
            Some("h"),
            "P7",
            "",
            None,
            Role::User,
        )
        .await
        .expect("user");
    let repo = db
        .insert_repository(
            &format!("r-p7-{suffix}"),
            &owner.id,
            "user",
            "p7-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");
    (owner, repo)
}

async fn enqueue(
    db: &Database,
    repo_id: &str,
    actor_id: &str,
    yaml: &[u8],
    hold: bool,
) -> (String, Vec<String>) {
    let doc = parse_workflow_yaml(yaml).expect("parse");
    enqueue_run(
        db,
        repo_id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(actor_id),
        None,
        None,
        hold,
    )
    .await
    .expect("enqueue")
}

async fn job_payload(db: &Database, job_id: &str) -> serde_json::Value {
    let job = db.find_action_job_by_id(job_id).await.unwrap().unwrap();
    let p = db
        .find_action_payload(job.payload_sha.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&p.payload_json).unwrap()
}

#[tokio::test]
async fn p7_permissions_full_map_in_payload() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "perm").await;

    // Job-level map wins over workflow-level; a declared map resets every
    // unspecified scope to `none` — the payload carries all 14 GH scopes.
    let (_, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: P
on: [push]
permissions:
  issues: write
jobs:
  a:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      deployments: read
    steps:
      - run: echo hi
"#,
        false,
    )
    .await;
    assert_eq!(job_ids.len(), 1);
    let perms = job_payload(&db, &job_ids[0])
        .await
        .get("permissions")
        .cloned()
        .unwrap();
    let obj = perms.as_object().expect("permissions object");
    for scope in GITHUB_PERMISSION_SCOPES {
        assert!(
            obj.contains_key(*scope),
            "scope {scope} missing from payload"
        );
    }
    assert_eq!(obj["contents"], "write", "job-level contents wins");
    assert_eq!(obj["deployments"], "read");
    assert_eq!(
        obj["issues"], "none",
        "workflow-level issues:write is shadowed by the job map"
    );
    assert_eq!(obj["id-token"], "none");
    assert_eq!(obj.len(), GITHUB_PERMISSION_SCOPES.len());
}

#[tokio::test]
async fn p7_default_permissions_floor() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "def").await;
    db.set_repo_actions_default_permissions(&repo.id, "write")
        .await
        .unwrap();

    // No `permissions:` anywhere → repo default `write` → every scope write.
    let (_, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: D
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        false,
    )
    .await;
    let perms = job_payload(&db, &job_ids[0])
        .await
        .get("permissions")
        .cloned()
        .unwrap();
    let obj = perms.as_object().unwrap();
    assert_eq!(obj["contents"], "write");
    assert_eq!(obj["id-token"], "write", "write-all grants id-token");

    // `read` default → contents+packages read, everything else none.
    db.set_repo_actions_default_permissions(&repo.id, "read")
        .await
        .unwrap();
    let (_, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: D2
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        false,
    )
    .await;
    let perms = job_payload(&db, &job_ids[0])
        .await
        .get("permissions")
        .cloned()
        .unwrap();
    let obj = perms.as_object().unwrap();
    assert_eq!(obj["contents"], "read");
    assert_eq!(obj["packages"], "read");
    assert_eq!(obj["id-token"], "none");
    assert_eq!(obj["issues"], "none");
}

#[tokio::test]
async fn p7_environment_auto_create_and_deployments() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "env").await;

    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: E
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment:
      name: staging
      url: https://staging.example.com
    steps:
      - run: echo deploy
"#,
        false,
    )
    .await;

    // The environment row auto-creates on first workflow reference.
    let env = db
        .find_action_environment(&repo.id, "staging")
        .await
        .unwrap()
        .expect("environment auto-created");
    assert_eq!(env.name, "staging");

    // One deployment record per leg, carrying the resolved URL.
    let deployments = db.list_action_deployments_for_run(&run_id).await.unwrap();
    assert_eq!(deployments.len(), job_ids.len());
    assert_eq!(deployments[0].environment, "staging");
    assert_eq!(
        deployments[0].url.as_deref(),
        Some("https://staging.example.com")
    );
    assert_eq!(deployments[0].git_ref, "refs/heads/main");

    // The frozen payload carries the resolved env for claim-time surfaces.
    let payload = job_payload(&db, &job_ids[0]).await;
    assert_eq!(payload["environment"], "staging");
    assert_eq!(payload["environment_url"], "https://staging.example.com");
}

#[tokio::test]
async fn p7_environment_required_reviewers_hold() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "rev").await;
    let reviewer = db
        .create_user(
            "u-p7-reviewer",
            "p7revr@example.com",
            "p7revr",
            Some("h"),
            "R",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();

    // Protection rule — the job holds for review before it can be claimed.
    db.upsert_action_environment(
        "env-1",
        &repo.id,
        "production",
        r#"{"required_reviewers":["u-p7-reviewer"]}"#,
    )
    .await
    .unwrap();
    let _ = reviewer;

    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: R
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: production
    steps:
      - run: echo deploy
"#,
        false,
    )
    .await;
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert!(
        job.approval_hold,
        "required_reviewers env holds the leg for approval"
    );
    assert_eq!(job.status, "waiting", "held legs stay unclaimable");

    // The hold releases per-job — the scheduler's recompute then queues it.
    assert!(db.approve_action_job(&job.id).await.unwrap());
    assert!(
        !db.approve_action_job(&job.id).await.unwrap(),
        "second approve is a no-op (already released)"
    );
    oxidean_api::actions::scheduler::recompute_ready_jobs(&db, &run_id)
        .await
        .unwrap();
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert!(!job.approval_hold);
    assert_eq!(job.status, "queued");
}

#[tokio::test]
async fn p7_environment_branch_allowlist_fails_loudly() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "br").await;

    db.upsert_action_environment(
        "env-2",
        &repo.id,
        "production",
        r#"{"deployment_branches":["release"]}"#,
    )
    .await
    .unwrap();

    // Push on `main` — the env only allows `release` → loud failed leg.
    let (_run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: B
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: production
    steps:
      - run: echo deploy
"#,
        false,
    )
    .await;
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "failure");
}

#[tokio::test]
async fn p7_environment_wait_timer() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "wait").await;

    db.upsert_action_environment("env-3", &repo.id, "staging", r#"{"wait_timer_minutes":30}"#)
        .await
        .unwrap();

    let (_run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: W
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: staging
    steps:
      - run: echo deploy
"#,
        false,
    )
    .await;
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "waiting", "timer-gated leg waits");
    assert!(
        job.deployment_not_before.is_some(),
        "deadline stamped at enqueue for a ready leg"
    );

    // Not yet due — the sweep promotes nothing.
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    assert_eq!(db.promote_due_deployment_jobs(&now).await.unwrap(), 0);
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "waiting");

    // Past the deadline — the sweep releases the leg to queued.
    let later = (chrono::Utc::now() + chrono::Duration::minutes(31))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    assert_eq!(db.promote_due_deployment_jobs(&later).await.unwrap(), 1);
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "queued");
    assert!(
        job.deployment_not_before.is_none(),
        "stamp cleared on promote"
    );
}

#[tokio::test]
async fn p7_id_token_request_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("t.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    let (owner, repo) = mk_owner_repo(&db, "oidc").await;

    // `id-token: write` + `contents: none` — GitHub injects the OIDC pair
    // without a GITHUB_TOKEN; the job token row carries the request hash.
    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: O
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    permissions:
      id-token: write
      contents: none
    steps:
      - run: echo oidc
"#,
        false,
    )
    .await;
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
    let owner_ref = oxidean_api::repo::owner_ref_for_repo(&db, &repo)
        .await
        .unwrap()
        .unwrap();
    let payload = job_payload(&db, &job_ids[0]).await;
    let requested = oxidean_api::actions::job_tokens::permissions_from_payload(&payload);
    let minted = oxidean_api::actions::job_tokens::mint_job_token(
        &db,
        &run,
        &repo,
        &owner_ref,
        &job_ids[0],
        &requested,
    )
    .await
    .unwrap()
    .expect("oidc-only mint still produces a row");
    assert!(
        minted.plaintext.is_none(),
        "contents:none → no OXIDEAN_TOKEN plaintext"
    );
    let request_token = minted
        .oidc_request_token
        .expect("id-token:write → request credential");
    assert!(request_token.starts_with("oxidean_oidc_"));

    // The hash binds back to exactly this job's token row.
    let hash = oxidean_api::auth::session::sha256_hex(request_token.as_bytes());
    let bound = db
        .find_job_token_by_oidc_hash(&hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bound.job_id, job_ids[0]);
    assert!(bound
        .perms_json
        .as_deref()
        .unwrap()
        .contains("\"id-token\":\"write\""));

    // Without `id-token: write` no request credential mints.
    let (run_id2, job_ids2) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: O2
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo no-oidc
"#,
        false,
    )
    .await;
    let run2 = db.find_action_run_by_id(&run_id2).await.unwrap().unwrap();
    let payload2 = job_payload(&db, &job_ids2[0]).await;
    let requested2 = oxidean_api::actions::job_tokens::permissions_from_payload(&payload2);
    let minted2 = oxidean_api::actions::job_tokens::mint_job_token(
        &db,
        &run2,
        &repo,
        &owner_ref,
        &job_ids2[0],
        &requested2,
    )
    .await
    .unwrap()
    .expect("default map mints a token");
    assert!(minted2.plaintext.is_some());
    assert!(minted2.oidc_request_token.is_none());
}

async fn app_db() -> (axum::Router, Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let log_dir = dir.path().join("actions-logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    let db = Database::connect(&format!("sqlite:{}", dir.path().join("r.db").display()))
        .await
        .unwrap();
    db.migrate().await.unwrap();
    support::unlock_signup(&db).await;
    let state = AppState::new(
        db.clone(),
        Arc::new(LogSink) as Arc<dyn EmailSender>,
        "development",
    )
    .with_actions_enabled(true)
    .with_actions_log_dir(log_dir);
    let app = router_with_state(state, build_cors("development", None).unwrap());
    (app, db, dir)
}

async fn get(app: &axum::Router, uri: &str, bearer: Option<&str>) -> (StatusCode, Vec<u8>) {
    let mut b = Request::builder().method("GET").uri(uri);
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, bytes.to_vec())
}

#[tokio::test]
async fn p7_oidc_token_endpoint_flow() {
    let (app, db, _dir) = app_db().await;
    let (owner, repo) = mk_owner_repo(&db, "http").await;

    // A live job with `id-token: write` + an environment (claim surfaces in
    // the JWT's `environment` claim).
    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: O3
on: [push]
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: production
    permissions:
      id-token: write
      contents: none
    steps:
      - run: echo oidc
"#,
        false,
    )
    .await;
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
    let owner_ref = oxidean_api::repo::owner_ref_for_repo(&db, &repo)
        .await
        .unwrap()
        .unwrap();
    let payload = job_payload(&db, &job_ids[0]).await;
    let requested = oxidean_api::actions::job_tokens::permissions_from_payload(&payload);
    let minted = oxidean_api::actions::job_tokens::mint_job_token(
        &db,
        &run,
        &repo,
        &owner_ref,
        &job_ids[0],
        &requested,
    )
    .await
    .unwrap()
    .unwrap();
    let request_token = minted.oidc_request_token.unwrap();

    // No bearer → 401; unknown token → 401.
    let (s, _) = get(&app, "/api/actions/oidc/token", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = get(&app, "/api/actions/oidc/token", Some("oxidean_oidc_bad")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Valid request token → {"value": jwt} bound to this job.
    let (s, body) = get(
        &app,
        "/api/actions/oidc/token?audience=sts.example.com",
        Some(&request_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{:?}", String::from_utf8_lossy(&body));
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let jwt = v["value"].as_str().expect("jwt");

    // JWKS serves the signing key's public JWK.
    let (s, body) = get(&app, "/api/actions/oidc/jwks", None).await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let keys = v["keys"].as_array().expect("jwks keys");
    assert_eq!(keys.len(), 1, "lazy key provisioning minted one key");
    let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(keys[0].clone()).expect("jwk shape");
    assert!(jwk.common.key_id.is_some());

    // Decode + verify the JWT against the advertised key.
    let (s, body) = get(&app, "/.well-known/actions-oidc/openid-configuration", None).await;
    assert_eq!(s, StatusCode::OK);
    let disc: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        disc["jwks_uri"].as_str().unwrap(),
        format!("{}/api/actions/oidc/jwks", disc["issuer"].as_str().unwrap())
    );

    let key = jsonwebtoken::DecodingKey::from_jwk(&jwk).expect("decoding key");
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&["sts.example.com"]);
    let data = jsonwebtoken::decode::<serde_json::Value>(jwt, &key, &validation)
        .expect("jwt verifies against JWKS");
    let claims = data.claims;
    assert_eq!(claims["iss"], disc["issuer"]);
    assert_eq!(claims["aud"], "sts.example.com");
    assert_eq!(claims["sha"], "deadbeef");
    assert_eq!(claims["ref"], "refs/heads/main");
    assert_eq!(claims["event_name"], "push");
    assert_eq!(claims["environment"], "production");
    assert_eq!(claims["run_id"], run_id);
    assert_eq!(claims["runner_environment"], "self-hosted");
    assert!(claims["sub"].as_str().unwrap().starts_with("repo:"));
    let exp = claims["exp"].as_i64().unwrap();
    let iat = claims["iat"].as_i64().unwrap();
    assert_eq!(exp - iat, 600, "10-minute token lifetime");

    // Terminal job → requests closed.
    db.update_action_job_status(&job_ids[0], "success")
        .await
        .unwrap();
    let (s, _) = get(
        &app,
        "/api/actions/oidc/token?audience=sts.example.com",
        Some(&request_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
