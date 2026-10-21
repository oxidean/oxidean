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
        None,
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

/// The Actions secrets key is read process-wide — set it once before any
/// test in this binary can touch the DB (a mid-test `set_var` leaks into
/// siblings under plain `cargo test`).
fn ensure_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "p7-oidc-test-key");
    });
}

#[tokio::test]
async fn p7_permissions_full_map_in_payload() {
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    assert!(db
        .promote_due_deployment_jobs(&now)
        .await
        .unwrap()
        .is_empty());
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.status, "waiting");

    // Past the deadline — the sweep releases the leg to queued.
    let later = (chrono::Utc::now() + chrono::Duration::minutes(31))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    assert_eq!(
        db.promote_due_deployment_jobs(&later).await.unwrap(),
        vec![job_ids[0].clone()]
    );
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
    ensure_env();
    let dir = tempfile::tempdir().unwrap();
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("t.db").display()))
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
    let db = support::connect_test_db(&format!("sqlite:{}", dir.path().join("r.db").display()))
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

/// RS256 OIDC issuer interop — discovery at the issuer-relative
/// `/.well-known/openid-configuration`, RSA JWKS, GitHub's claim set, and the
/// private key stored as secrets ciphertext (never plaintext).
#[tokio::test]
async fn p7_oidc_token_endpoint_flow() {
    ensure_env();
    let (app, db, _dir) = app_db().await;
    let (owner, repo) = mk_owner_repo(&db, "http").await;

    // A live job with `id-token: write` + an environment (claim surfaces in
    // the JWT's `environment` claim and `sub`).
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

    // Lowercase `bearer` (GitHub's documented curl) must authenticate too.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/actions/oidc/token?api-version=2.0&audience=sts.example.com")
                .header("authorization", format!("bearer {request_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let jwt = v["value"].as_str().expect("jwt").to_string();

    // Discovery is served at exactly `{iss}/.well-known/openid-configuration`
    // — the location OIDC consumers resolve from the issuer claim. The legacy
    // root-level path is gone.
    let (s, _) = get(&app, "/.well-known/actions-oidc/openid-configuration", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "legacy discovery route removed");
    let (s, body) = get(
        &app,
        "/api/actions/oidc/.well-known/openid-configuration",
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let disc: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let iss = disc["issuer"].as_str().expect("issuer");
    assert!(
        iss.ends_with("/api/actions/oidc"),
        "issuer is a path issuer: {iss}"
    );
    assert_eq!(disc["jwks_uri"].as_str().unwrap(), format!("{iss}/jwks"));
    assert_eq!(
        disc["id_token_signing_alg_values_supported"],
        serde_json::json!(["RS256"])
    );
    assert_eq!(
        disc["subject_types_supported"],
        serde_json::json!(["public", "pairwise"])
    );
    assert_eq!(disc["scopes_supported"], serde_json::json!(["openid"]));
    assert!(
        disc["claims_supported"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "job_workflow_sha"),
        "claims_supported covers the emitted claims"
    );

    // JWKS serves only RSA keys.
    let (s, body) = get(&app, "/api/actions/oidc/jwks", None).await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let keys = v["keys"].as_array().expect("jwks keys");
    assert_eq!(keys.len(), 1, "lazy key provisioning minted one key");
    assert_eq!(keys[0]["kty"], "RSA");
    assert_eq!(keys[0]["alg"], "RS256");
    assert_eq!(keys[0]["use"], "sig");
    assert!(
        keys[0]["n"].as_str().unwrap().len() > 300,
        "RSA-2048 modulus"
    );
    let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(keys[0].clone()).expect("jwk shape");
    let jwk_kid = jwk.common.key_id.clone().expect("jwks kid");

    // The JWT verifies RS256 against the advertised key, matching `kid`.
    let header = jsonwebtoken::decode_header(&jwt).expect("jwt header");
    assert_eq!(header.alg, jsonwebtoken::Algorithm::RS256);
    assert_eq!(header.kid.as_deref(), Some(jwk_kid.as_str()));
    let key = jsonwebtoken::DecodingKey::from_jwk(&jwk).expect("decoding key");
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&["sts.example.com"]);
    let data = jsonwebtoken::decode::<serde_json::Value>(jwt, &key, &validation)
        .expect("jwt verifies RS256 against JWKS");
    let claims = data.claims;
    // `iss` byte-for-byte equals the discovery `issuer`.
    assert_eq!(claims["iss"], serde_json::Value::String(iss.to_string()));
    assert_eq!(claims["aud"], "sts.example.com");
    assert_eq!(claims["sha"], "deadbeef");
    assert_eq!(claims["ref"], "refs/heads/main");
    assert_eq!(claims["ref_type"], "branch");
    assert_eq!(claims["ref_protected"], false);
    assert_eq!(claims["event_name"], "push");
    assert_eq!(claims["environment"], "production");
    assert_eq!(
        claims["sub"],
        format!(
            "repo:{}/{}:environment:production",
            owner.username, repo.name
        )
    );
    assert_eq!(claims["run_id"], run_id);
    assert_eq!(claims["run_number"], 1);
    assert_eq!(claims["run_attempt"], 1);
    assert_eq!(claims["runner_environment"], "self-hosted");
    assert_eq!(claims["actor"], owner.username);
    assert_eq!(claims["actor_id"], owner.id);
    assert_eq!(claims["base_ref"], "");
    assert_eq!(claims["head_ref"], "");
    assert_eq!(
        claims["repository"],
        format!("{}/{}", owner.username, repo.name)
    );
    assert_eq!(claims["repository_id"], repo.id);
    assert_eq!(claims["repository_owner"], owner.username);
    assert_eq!(claims["repository_owner_id"], owner.id);
    assert_eq!(claims["repository_visibility"], "private");
    let workflow_ref = format!(
        "{}/{}/.github/workflows/ci.yml@refs/heads/main",
        owner.username, repo.name
    );
    assert_eq!(claims["workflow_ref"], workflow_ref);
    assert_eq!(claims["workflow_sha"], "deadbeef");
    assert_eq!(claims["job_workflow_ref"], workflow_ref);
    assert_eq!(claims["job_workflow_sha"], "deadbeef");
    let exp = claims["exp"].as_i64().unwrap();
    let iat = claims["iat"].as_i64().unwrap();
    assert_eq!(exp - iat, 600, "10-minute token lifetime");

    // The private key is ciphertext at rest — `private_enc` holds the
    // secrets-encrypted blob and decrypts to PKCS#8 hex; `private_der`
    // stays empty (plaintext column retired for RS256 keys).
    let stored = db
        .find_active_action_oidc_key("RS256")
        .await
        .unwrap()
        .expect("rs256 key row");
    assert_eq!(stored.alg, "RS256");
    assert!(stored.private_der.is_empty(), "no plaintext private key");
    let enc = stored.private_enc.as_deref().expect("private_enc present");
    let der_hex = oxidean_api::actions::secrets::decrypt_secret(enc)
        .expect("private key decrypts with secrets key");
    assert!(
        hex::decode(&der_hex).is_ok(),
        "decrypted material is der hex"
    );

    // Default `aud` is the repository owner's URL (GitHub parity).
    let origin = oxidean_api::public_origin::resolve_public_origin();
    let (s, body) = get(&app, "/api/actions/oidc/token", Some(&request_token)).await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let jwt2 = v["value"].as_str().expect("jwt");
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_audience(&[&format!("{origin}/{}", owner.username)]);
    let data = jsonwebtoken::decode::<serde_json::Value>(jwt2, &key, &validation)
        .expect("default-aud jwt verifies");
    assert_eq!(data.claims["aud"], format!("{origin}/{}", owner.username));

    // Without a secrets key the endpoint fails with a clear error instead of
    // minting — the stored key exists but cannot be decrypted.
    let saved: Vec<(&str, Option<String>)> =
        ["OXIDEAN_ACTIONS_SECRETS_KEY", "OXIDEAN_SESSION_SECRET"]
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect();
    for (k, _) in &saved {
        std::env::remove_var(k);
    }
    let (s, body) = get(&app, "/api/actions/oidc/token", Some(&request_token)).await;
    for (k, v) in &saved {
        if let Some(v) = v {
            std::env::set_var(k, v);
        }
    }
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v["message"]
            .as_str()
            .unwrap_or_default()
            .contains("OXIDEAN_ACTIONS_SECRETS_KEY"),
        "missing-key error names the fix: {v}"
    );

    // Terminal job → requests closed.
    db.update_action_job_status(&job_ids[0], "success")
        .await
        .unwrap();
    let (s, _) = get(
        &app,
        "/api/actions/oidc/token?api-version=2.0&audience=sts.example.com",
        Some(&request_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// RPC scope enforcement: the granted `perms_json` scope map rides the
/// resolved `PatIdentity`, so `issue.*` calls pass for `issues: write` jobs
/// and deny for ungranted scopes (GitHub `permissions:` parity). Before the
/// fix the map was dropped at bearer resolution and every domain procedure
/// scope-failed for job tokens.
#[tokio::test]
async fn p7_job_token_scope_map_gates_rpc() {
    ensure_env();
    let (app, db, _dir) = app_db().await;
    let (owner, repo) = mk_owner_repo(&db, "rpcscope").await;
    // Mutating procedures require a verified owner — the job token resolves
    // to the triggering user, so verify it like the signup-path tests do.
    db.set_email_verified_at(&owner.id, "2026-01-01T00:00:00Z")
        .await
        .unwrap();

    let rpc = |app: &axum::Router, token: &str, body: String| {
        let app = app.clone();
        let token = token.to_string();
        async move {
            let res = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/rpc")
                        .header("content-type", "application/json")
                        .header("Oxidean-RPC-Version", "1")
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        }
    };

    // Job grants `issues: write` + `contents: read` — nothing else.
    let (run_id, job_ids) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: S
on: [push]
jobs:
  work:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      issues: write
    steps:
      - run: echo hi
"#,
        false,
    )
    .await;
    let run = db.find_action_run_by_id(&run_id).await.unwrap().unwrap();
    let owner_ref = oxidean_api::repo::owner_ref_for_repo(&db, &repo)
        .await
        .unwrap()
        .unwrap();
    let requested = oxidean_api::actions::job_tokens::permissions_from_payload(
        &job_payload(&db, &job_ids[0]).await,
    );
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
    let token = minted.plaintext.expect("contents:read injects plaintext");

    let target = r#"{"owner":"p7rpcscope","name":"p7-demo"}"#;
    let list = rpc(
        &app,
        &token,
        format!(r#"{{"procedure":"issue.list","input":{target}}}"#),
    )
    .await;
    assert_eq!(
        list["ok"], true,
        "issues:write satisfies issue.list — {list}"
    );

    let create = rpc(
        &app,
        &token,
        r#"{"procedure":"issue.create","input":{"owner":"p7rpcscope","name":"p7-demo","title":"t","body":"b"}}"#.to_string(),
    )
    .await;
    assert_eq!(
        create["ok"], true,
        "issues:write allows issue.create — {create}"
    );

    let denied = rpc(
        &app,
        &token,
        format!(r#"{{"procedure":"pull.list","input":{target}}}"#),
    )
    .await;
    assert_eq!(denied["ok"], false, "{denied}");
    assert_eq!(denied["error"]["code"], "auth.pat_scope", "{denied}");

    let read = rpc(
        &app,
        &token,
        format!(r#"{{"procedure":"repo.get","input":{target}}}"#),
    )
    .await;
    assert_eq!(
        read["ok"], true,
        "contents:read satisfies repo.get — {read}"
    );

    // A second job with the repo default map grants no issues scope —
    // the same procedures now deny.
    let (run_id2, job_ids2) = enqueue(
        &db,
        &repo.id,
        &owner.id,
        br#"
name: S2
on: [push]
jobs:
  work:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        false,
    )
    .await;
    let run2 = db.find_action_run_by_id(&run_id2).await.unwrap().unwrap();
    let requested2 = oxidean_api::actions::job_tokens::permissions_from_payload(
        &job_payload(&db, &job_ids2[0]).await,
    );
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
    .unwrap();
    let token2 = minted2.plaintext.expect("default map injects plaintext");

    let denied2 = rpc(
        &app,
        &token2,
        format!(r#"{{"procedure":"issue.list","input":{target}}}"#),
    )
    .await;
    assert_eq!(denied2["ok"], false, "{denied2}");
    assert_eq!(denied2["error"]["code"], "auth.pat_scope", "{denied2}");

    let create2 = rpc(
        &app,
        &token2,
        r#"{"procedure":"issue.create","input":{"owner":"p7rpcscope","name":"p7-demo","title":"t2","body":"b"}}"#
            .to_string(),
    )
    .await;
    assert_eq!(create2["ok"], false, "{create2}");
    assert_eq!(create2["error"]["code"], "auth.pat_scope", "{create2}");
}
