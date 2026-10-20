//! `upload-artifact@v4`/`download-artifact@v4` Twirp surface
//! (`/twirp/github.actions.results.api.v1.ArtifactService/*`) + the
//! `/_apis/artifactblobs/{id}` signed-URL data plane. Covers the
//! create→upload→finalize→list→download roundtrip (PutBlob and staged-block
//! forms), duplicate-name `already_exists`, size/digest finalize checks,
//! pending-vs-committed visibility, `?t=` query auth (the real client sends
//! no `Authorization` on the data plane), and the runtime token's `scp` JWT
//! claim the toolkit decodes for backend ids.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use http_body_util::BodyExt;
use oxidean_api::actions::dispatch::enqueue_run;
use oxidean_api::actions::{mint_registration_token, parse_workflow_yaml};
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_core::Role;
use oxidean_db::Database;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const SVC: &str = "github.actions.results.api.v1.ArtifactService";

async fn app_db() -> (axum::Router, Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
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
    .with_actions_artifacts_dir(dir.path().join("actions-artifacts"))
    .with_actions_log_dir(dir.path().join("actions-logs"))
    .with_repos_dir(dir.path().join("repos"));
    let app = router_with_state(state, build_cors("development", None).unwrap());
    (app, db, dir)
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| serde_json::json!({}))
}

async fn twirp(
    app: &axum::Router,
    method: &str,
    bearer: Option<&str>,
    body: serde_json::Value,
) -> axum::response::Response {
    let mut b = Request::builder()
        .method("POST")
        .uri(format!("/twirp/{SVC}/{method}"))
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

/// Strip `{origin}` off a signed/blob URL — oneshot wants path+query only.
fn rel(url: &str) -> &str {
    url.find("/api/actions").map(|i| &url[i..]).unwrap_or(url)
}

/// Decode the runtime JWT's `scp` claim — exactly what
/// `getBackendIdsFromToken` does client-side (`jwtDecode`, no verify).
/// Returns (run_id, job_id) from `Actions.Results:<run>:<job>`.
fn backend_ids(token: &str) -> (String, String) {
    let payload = token.split('.').nth(1).expect("jwt payload segment");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("jwt payload base64");
    let claims: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let scp = claims["scp"].as_str().expect("scp claim");
    let results = scp
        .split(' ')
        .find_map(|s| s.strip_prefix("Actions.Results:"))
        .expect("Actions.Results scope");
    let mut parts = results.split(':');
    (
        parts.next().unwrap().to_string(),
        parts.next().unwrap().to_string(),
    )
}

/// Claim a job on `main` for a fresh repo; returns (runtime token, run_id,
/// job_id) — the last two decoded out of the token itself, like the client.
async fn claim(app: &axum::Router, db: &Database, key: &str) -> (String, String, String) {
    let (tok, run, job, _job_token) = claim_full(app, db, key).await;
    (tok, run, job)
}

/// `claim` plus the minted `oxidean_job_…` plaintext — the `GITHUB_TOKEN`
/// a step would use against the REST facade.
async fn claim_full(
    app: &axum::Router,
    db: &Database,
    key: &str,
) -> (String, String, String, String) {
    claim_full_yaml(
        app,
        db,
        key,
        b"name: CI\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .await
}

/// `claim_full` with a caller-supplied workflow — e.g. to pin `permissions:`
/// so the minted `GITHUB_TOKEN` carries `actions` scopes.
async fn claim_full_yaml(
    app: &axum::Router,
    db: &Database,
    key: &str,
    workflow: &[u8],
) -> (String, String, String, String) {
    let owner = db
        .create_user(
            &format!("u-{key}"),
            &format!("{key}@example.com"),
            key,
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository(
            &format!("r-{key}"),
            &owner.id,
            "user",
            key,
            "private",
            "",
            "main",
        )
        .await
        .unwrap();
    let doc = parse_workflow_yaml(workflow).unwrap();
    enqueue_run(
        db,
        &repo.id,
        ".github/workflows/ci.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let reg = mint_registration_token(db).await.unwrap();
    let reg_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/actions/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"name":"r1","labels":["ubuntu-latest"],"token":reg})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let runner_token = body_json(reg_res).await["runner_token"]
        .as_str()
        .unwrap()
        .to_string();
    let v = body_json(
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/actions/fetch_task")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {runner_token}"))
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    let token = v["cache_token"].as_str().unwrap().to_string();
    let job_token = v["job_token"]
        .as_str()
        .expect("fetch_task mints a job token")
        .to_string();
    assert!(!job_token.is_empty(), "job token must be minted at claim");
    let (run_id, job_id) = backend_ids(&token);
    (token, run_id, job_id, job_token)
}

fn ids(run_id: &str, job_id: &str) -> serde_json::Value {
    serde_json::json!({
        "workflow_run_backend_id": run_id,
        "workflow_job_run_backend_id": job_id,
    })
}

/// Raw request helper for the blob plane (PUT/GET with query, no JSON sugar).
async fn blob_req(
    app: &axum::Router,
    method: &str,
    path_query: &str,
    bytes: &[u8],
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path_query)
                .body(Body::from(bytes.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Create → upload → finalize one artifact; returns its blob bytes + id.
async fn upload_one(
    app: &axum::Router,
    token: &str,
    run_id: &str,
    job_id: &str,
    name: &str,
    payload: &[u8],
) -> String {
    let mut req = ids(run_id, job_id);
    req["name"] = name.into();
    req["version"] = 7.into();
    req["mime_type"] = "application/zip".into();
    let res = twirp(app, "CreateArtifact", Some(token), req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let created = body_json(res).await;
    assert_eq!(created["ok"], true);
    let upload_url = rel(created["signed_upload_url"].as_str().unwrap()).to_string();
    let put = blob_req(app, "PUT", &upload_url, payload).await;
    assert_eq!(put.status(), StatusCode::CREATED);

    let mut fin = ids(run_id, job_id);
    fin["name"] = name.into();
    fin["size"] = payload.len().to_string().into();
    fin["hash"] = format!("sha256:{}", hex::encode(Sha256::digest(payload))).into();
    let res = twirp(app, "FinalizeArtifact", Some(token), fin).await;
    assert_eq!(res.status(), StatusCode::OK);
    let done = body_json(res).await;
    assert_eq!(done["ok"], true, "{done}");
    done["artifact_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn v4_roundtrip_putblob_then_download() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "rt").await;

    let payload = b"PK\x03\x04 fake zip bytes";
    let artifact_id = upload_one(&app, &token, &run_id, &job_id, "dist", payload).await;
    assert!(artifact_id.parse::<i64>().is_ok(), "id must be int64-safe");

    // GetSignedArtifactURL → signed_url, fetched with no Authorization —
    // the real client follows the SAS as-is.
    let mut req = ids(&run_id, &job_id);
    req["name"] = "dist".into();
    let res = twirp(&app, "GetSignedArtifactURL", Some(&token), req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let signed = body_json(res).await;
    let get_url = rel(signed["signed_url"].as_str().unwrap()).to_string();

    let res = blob_req(&app, "GET", &get_url, b"").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "application/zip"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], payload);
}

#[tokio::test]
async fn v4_staged_block_upload() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "blocks").await;

    let mut req = ids(&run_id, &job_id);
    req["name"] = "chunked".into();
    req["version"] = 7.into();
    let res = twirp(&app, "CreateArtifact", Some(&token), req).await;
    let created = body_json(res).await;
    let upload_url = rel(created["signed_upload_url"].as_str().unwrap()).to_string();

    let (b1, b2) = (b"first-block-", b"second-block!");
    for (i, data) in [b1.as_ref(), b2.as_ref()].iter().enumerate() {
        let bid = base64::engine::general_purpose::STANDARD.encode(format!("blk-{i}"));
        let res = blob_req(
            &app,
            "PUT",
            &format!("{upload_url}&comp=block&blockid={bid}"),
            data,
        )
        .await;
        assert_eq!(res.status(), StatusCode::CREATED);
    }
    let blocklist = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><BlockList><Latest>{}</Latest><Latest>{}</Latest></BlockList>",
        base64::engine::general_purpose::STANDARD.encode("blk-0"),
        base64::engine::general_purpose::STANDARD.encode("blk-1"),
    );
    let res = blob_req(
        &app,
        "PUT",
        &format!("{upload_url}&comp=blocklist"),
        blocklist.as_bytes(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);

    let payload = [b1.as_slice(), b2.as_slice()].concat();
    let mut fin = ids(&run_id, &job_id);
    fin["name"] = "chunked".into();
    fin["size"] = payload.len().to_string().into();
    fin["hash"] = format!("sha256:{}", hex::encode(Sha256::digest(&payload))).into();
    let res = twirp(&app, "FinalizeArtifact", Some(&token), fin).await;
    assert_eq!(body_json(res).await["ok"], true);
}

#[tokio::test]
async fn v4_duplicate_name_is_already_exists() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "dup").await;
    upload_one(&app, &token, &run_id, &job_id, "dup", b"x").await;

    let mut req = ids(&run_id, &job_id);
    req["name"] = "dup".into();
    req["version"] = 7.into();
    let res = twirp(&app, "CreateArtifact", Some(&token), req).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert_eq!(body_json(res).await["code"], "already_exists");
}

#[tokio::test]
async fn v4_pending_is_invisible_until_finalize() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "pend").await;

    let mut req = ids(&run_id, &job_id);
    req["name"] = "staged".into();
    req["version"] = 7.into();
    let res = twirp(&app, "CreateArtifact", Some(&token), req).await;
    let created = body_json(res).await;
    assert_eq!(created["ok"], true);

    // Pending artifacts neither list nor sign download URLs.
    let listed =
        body_json(twirp(&app, "ListArtifacts", Some(&token), ids(&run_id, &job_id)).await).await;
    assert_eq!(listed["artifacts"].as_array().unwrap().len(), 0);
    let res = twirp(&app, "GetSignedArtifactURL", Some(&token), {
        let mut r = ids(&run_id, &job_id);
        r["name"] = "staged".into();
        r
    })
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn v4_finalize_size_mismatch_fails_soft() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "size").await;

    let mut req = ids(&run_id, &job_id);
    req["name"] = "sized".into();
    req["version"] = 7.into();
    let res = twirp(&app, "CreateArtifact", Some(&token), req).await;
    let created = body_json(res).await;
    let upload_url = rel(created["signed_upload_url"].as_str().unwrap()).to_string();
    blob_req(&app, "PUT", &upload_url, b"ten bytes!").await;

    let mut fin = ids(&run_id, &job_id);
    fin["name"] = "sized".into();
    fin["size"] = "9999".into();
    let res = twirp(&app, "FinalizeArtifact", Some(&token), fin).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["ok"], false);
}

#[tokio::test]
async fn v4_finalize_hash_mismatch_fails_soft() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "hash").await;

    let payload = b"payload bytes";
    let mut req = ids(&run_id, &job_id);
    req["name"] = "hashed".into();
    req["version"] = 7.into();
    let res = twirp(&app, "CreateArtifact", Some(&token), req).await;
    let created = body_json(res).await;
    let upload_url = rel(created["signed_upload_url"].as_str().unwrap()).to_string();
    blob_req(&app, "PUT", &upload_url, payload).await;

    let mut fin = ids(&run_id, &job_id);
    fin["name"] = "hashed".into();
    fin["size"] = payload.len().to_string().into();
    fin["hash"] = "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
    let res = twirp(&app, "FinalizeArtifact", Some(&token), fin).await;
    assert_eq!(body_json(res).await["ok"], false);
}

#[tokio::test]
async fn v4_list_artifacts_filters() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "list").await;
    upload_one(&app, &token, &run_id, &job_id, "alpha", b"aaa").await;
    let beta_id = upload_one(&app, &token, &run_id, &job_id, "beta", b"bbb").await;

    let listed =
        body_json(twirp(&app, "ListArtifacts", Some(&token), ids(&run_id, &job_id)).await).await;
    let names: Vec<&str> = listed["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["name"].as_str())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);

    let mut by_name = ids(&run_id, &job_id);
    by_name["name_filter"] = "alpha".into();
    let listed = body_json(twirp(&app, "ListArtifacts", Some(&token), by_name).await).await;
    assert_eq!(listed["artifacts"].as_array().unwrap().len(), 1);

    let mut by_id = ids(&run_id, &job_id);
    by_id["id_filter"] = beta_id.clone().into();
    let listed = body_json(twirp(&app, "ListArtifacts", Some(&token), by_id).await).await;
    let arts = listed["artifacts"].as_array().unwrap();
    assert_eq!(arts.len(), 1);
    assert_eq!(arts[0]["database_id"], beta_id);
}

#[tokio::test]
async fn v4_delete_artifact() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "del").await;
    upload_one(&app, &token, &run_id, &job_id, "gone", b"zzz").await;

    let mut req = ids(&run_id, &job_id);
    req["name"] = "gone".into();
    let res = twirp(&app, "DeleteArtifact", Some(&token), req).await;
    assert_eq!(body_json(res).await["ok"], true);

    let listed =
        body_json(twirp(&app, "ListArtifacts", Some(&token), ids(&run_id, &job_id)).await).await;
    assert_eq!(listed["artifacts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn v4_requires_runtime_token() {
    let (app, _db, _dir) = app_db().await;
    let res = twirp(
        &app,
        "CreateArtifact",
        None,
        serde_json::json!({"name": "x"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(body_json(res).await["code"], "unauthenticated");
}

#[tokio::test]
async fn v4_wrong_backend_ids_forbidden() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, _job_id) = claim(&app, &db, "forbid").await;
    let res = twirp(
        &app,
        "CreateArtifact",
        Some(&token),
        serde_json::json!({
            "workflow_run_backend_id": run_id,
            "workflow_job_run_backend_id": "someone-elses-job",
            "name": "x",
            "version": 7,
        }),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_json(res).await["code"], "permission_denied");
}

#[tokio::test]
async fn v4_runtime_token_is_decodable_jwt() {
    // With a secrets key configured the mint signs RS256 via the instance
    // OIDC key — the signature segment must be non-empty.
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "artifact-v2-test-key");
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "jwt").await;
    // backend_ids() already proves `scp` decodes; pin exp>iat sanity too.
    let payload = token.split('.').nth(1).unwrap();
    let claims: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .unwrap(),
    )
    .unwrap();
    assert!(claims["exp"].as_i64().unwrap() > claims["iat"].as_i64().unwrap());
    assert_eq!(
        claims["scp"].as_str().unwrap(),
        format!("Actions.Results:{run_id}:{job_id}")
    );
    assert!(!token.split('.').nth(2).unwrap_or_default().is_empty());
    let header: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token.split('.').next().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(header["alg"], "RS256");
}

#[tokio::test]
async fn v4_sweep_reaps_stale_pending_and_expired_artifacts() {
    let (app, db, dir) = app_db().await;
    let (token, run_id, job_id) = claim(&app, &db, "sweep").await;
    let artifacts_dir = dir.path().join("actions-artifacts");

    // Abandoned reservation — job died between CreateArtifact and
    // FinalizeArtifact: a `.part` payload plus staged `.blocks` residue.
    let mut req = ids(&run_id, &job_id);
    req["name"] = "stale".into();
    req["version"] = 7.into();
    let created = body_json(twirp(&app, "CreateArtifact", Some(&token), req).await).await;
    assert_eq!(created["ok"], true, "{created}");
    let upload_url = rel(created["signed_upload_url"].as_str().unwrap()).to_string();
    let put = blob_req(&app, "PUT", &upload_url, b"partial bytes").await;
    assert_eq!(put.status(), StatusCode::CREATED);

    let stale = db
        .find_action_artifact(&run_id, "stale")
        .await
        .unwrap()
        .expect("pending row exists");
    assert_eq!(stale.state, "pending");
    let dest = artifacts_dir.join(&stale.storage_path);
    assert!(dest.with_extension("part").exists());
    let blocks = dest.with_extension("blocks");
    std::fs::create_dir_all(&blocks).unwrap();
    std::fs::write(blocks.join("0"), b"blk").unwrap();

    // A committed artifact already past retention — GitHub deletes it.
    let mut req = ids(&run_id, &job_id);
    req["name"] = "expired".into();
    req["version"] = 7.into();
    req["expires_at"] = "2000-01-01T00:00:00Z".into();
    let created = body_json(twirp(&app, "CreateArtifact", Some(&token), req).await).await;
    assert_eq!(created["ok"], true, "{created}");
    let payload = b"old zip";
    let upload_url = rel(created["signed_upload_url"].as_str().unwrap()).to_string();
    let put = blob_req(&app, "PUT", &upload_url, payload).await;
    assert_eq!(put.status(), StatusCode::CREATED);
    let mut fin = ids(&run_id, &job_id);
    fin["name"] = "expired".into();
    fin["size"] = payload.len().to_string().into();
    fin["hash"] = format!("sha256:{}", hex::encode(Sha256::digest(payload))).into();
    let res = twirp(&app, "FinalizeArtifact", Some(&token), fin).await;
    assert_eq!(res.status(), StatusCode::OK);
    let expired = db
        .find_action_artifact(&run_id, "expired")
        .await
        .unwrap()
        .expect("committed row exists");
    assert!(artifacts_dir.join(&expired.storage_path).exists());

    // A committed artifact inside its retention window must survive.
    let kept_id = upload_one(&app, &token, &run_id, &job_id, "kept", b"ok").await;

    // A far-future pending cutoff reaps the abandoned reservation; a real
    // `now` reaps the expired row while `kept`'s default 90-day retention
    // still protects it — no SQL backdating.
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let n = oxidean_api::actions::artifacts_v2::sweep_artifacts_before(
        &db,
        &artifacts_dir,
        "2999-01-01T00:00:00Z",
        &now,
    )
    .await
    .unwrap();
    assert_eq!(n, 2, "pending reservation + expired artifact reap");
    assert!(db
        .find_action_artifact(&run_id, "stale")
        .await
        .unwrap()
        .is_none());
    assert!(db
        .find_action_artifact(&run_id, "expired")
        .await
        .unwrap()
        .is_none());
    assert!(!dest.with_extension("part").exists());
    assert!(!blocks.exists());
    assert!(!artifacts_dir.join(&expired.storage_path).exists());
    let kept = db
        .find_action_artifact_by_id(&kept_id)
        .await
        .unwrap()
        .expect("committed row survives");
    assert!(artifacts_dir.join(&kept.storage_path).exists());

    // The composed sweep on real cutoffs leaves fresh pending rows alone.
    let mut req = ids(&run_id, &job_id);
    req["name"] = "fresh".into();
    req["version"] = 7.into();
    let res = twirp(&app, "CreateArtifact", Some(&token), req).await;
    assert_eq!(res.status(), StatusCode::OK);
    let n = oxidean_api::actions::artifacts_v2::sweep_artifacts(&db, &artifacts_dir)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert!(db
        .find_action_artifact(&run_id, "fresh")
        .await
        .unwrap()
        .is_some());
}

/// `/api/v1` GitHub-shaped artifact routes — the cross-run path
/// `download-artifact` + `gh` take through `api_url`, authenticated with the
/// job token a step would see as `GITHUB_TOKEN`.
async fn rest(
    app: &axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
) -> axum::response::Response {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn rest_json(
    app: &axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    app.clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn v4_rest_artifact_facade_list_get_zip_delete() {
    let (app, db, dir) = app_db().await;
    // `permissions: actions: write` — the REST facade's ActionsRead/Write
    // classes need an `actions` scope the default `read` token omits, the
    // same `actions: read` requirement GitHub documents for this path.
    let (token, run_id, job_id, job_token) = claim_full_yaml(
        &app,
        &db,
        "rest",
        b"name: CI\non: [push]\npermissions:\n  contents: read\n  actions: write\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .await;
    let artifacts_dir = dir.path().join("actions-artifacts");

    let payload = b"PK\x03\x04 zip bytes";
    let artifact_id = upload_one(&app, &token, &run_id, &job_id, "dist", payload).await;
    let base = "/api/v1/repos/rest/rest";

    // The cross-run list — GitHub shape, job-token auth.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/artifacts"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let list = body_json(res).await;
    assert_eq!(list["total_count"], 1, "{list}");
    let art = &list["artifacts"][0];
    assert_eq!(art["name"], "dist");
    assert_eq!(art["size_in_bytes"], payload.len() as i64);
    assert_eq!(art["workflow_run"]["id"], run_id);
    assert_eq!(art["expired"], false);
    assert!(art["archive_download_url"]
        .as_str()
        .unwrap()
        .ends_with(&format!("/{artifact_id}/zip")));

    // Repo-wide list + `name` filter.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 1);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts?name=nope"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 0);

    // Single-artifact metadata.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts/{artifact_id}"),
        Some(&job_token),
    )
    .await;
    let meta = body_json(res).await;
    assert_eq!(meta["name"], "dist");

    // The zip Octokit download — already-zip payloads pass through.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts/{artifact_id}/zip"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap(),
        "application/zip"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], payload);

    // Builtin-uploaded tar.gz payloads repack to zip — `unzip` clients
    // see GitHub's wire format.
    let mut tgz = Vec::new();
    {
        let enc = flate2::write::GzEncoder::new(&mut tgz, flate2::Compression::default());
        let mut tb = tar::Builder::new(enc);
        let mut h = tar::Header::new_gnu();
        h.set_size(3);
        h.set_mode(0o644);
        h.set_cksum();
        tb.append_data(&mut h, "out.txt", &b"hi\n"[..]).unwrap();
        tb.into_inner().unwrap().finish().unwrap();
    }
    let tgz_id = "tgz-rest-1";
    let storage = format!("{run_id}/{tgz_id}");
    db.upsert_action_artifact(&oxidean_db::actions::ActionArtifactRow {
        id: tgz_id.into(),
        repository_id: "r-rest".into(),
        run_id: run_id.clone(),
        job_id: Some(job_id.clone()),
        name: "logs".into(),
        storage_path: storage.clone(),
        size_bytes: tgz.len() as i64,
        digest_sha256: String::new(),
        expires_at: None,
        state: "committed".into(),
        mime_type: "application/gzip".into(),
        created_at: String::new(),
    })
    .await
    .unwrap();
    let dest = artifacts_dir.join(&storage);
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(&dest, &tgz).unwrap();

    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts/{tgz_id}/zip"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let zipped = res.into_body().collect().await.unwrap().to_bytes();
    let mut zr = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
    let mut entry = zr.by_name("out.txt").unwrap();
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut entry, &mut contents).unwrap();
    assert_eq!(contents, "hi\n");

    // No credentials on a private repo → 404 (anti-enumeration, GitHub's
    // convention) — then delete → 204 → gone.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts/{artifact_id}/zip"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // deleteArtifact requires a verified user — mirror the signup path.
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at("u-rest", &now).await.unwrap();
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/artifacts/{artifact_id}"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/artifacts/{artifact_id}"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

/// The `gh`-facing Actions REST core — runs/jobs/workflows reads and the
/// rerun/cancel/delete mutations, authenticated with the job token.
#[tokio::test]
async fn rest_actions_runs_jobs_workflows_surface() {
    let (app, db, dir) = app_db().await;
    let (_token, run_id, job_id, job_token) = claim_full_yaml(
        &app,
        &db,
        "ghcli",
        b"name: CI\non: [push, workflow_dispatch]\npermissions:\n  contents: read\n  actions: write\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .await;
    // dispatchWorkflow requires a verified caller — mirror the signup path.
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at("u-ghcli", &now).await.unwrap();
    // Mutations past a cancel ride the owner's PAT — cancelling revokes the
    // run's job tokens.
    let pat = "oxidean_pat_ghcli_roundtrip";
    db.create_pat(
        "pat-ghcli",
        "u-ghcli",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();
    // listWorkflows/dispatchWorkflow read the bare repo's tree — seed one.
    let bare = dir.path().join("repos").join("ghcli").join("ghcli.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = oxidean_git::CliGitBackend::new();
    oxidean_git::GitBackend::init_bare(&git, &bare, "main")
        .await
        .unwrap();
    oxidean_git::GitBackend::seed_commit(
        &git,
        &bare,
        "main",
        "workflows",
        &[(
            ".github/workflows/ci.yml".into(),
            b"name: CI\non: [push, workflow_dispatch]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n".to_vec(),
        )],
    )
    .await
    .unwrap();
    let base = "/api/v1/repos/ghcli/ghcli";

    // gh run list — GitHub shape with workflow_runs.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let list = body_json(res).await;
    assert_eq!(list["total_count"], 1, "{list}");
    let run = &list["workflow_runs"][0];
    assert_eq!(run["name"], "CI");
    assert_eq!(run["event"], "push");
    assert_eq!(run["id"], run_id);
    assert!(run["conclusion"].is_null() || run["status"] == "completed");

    // status=completed is the GitHub umbrella for terminal rows — a still
    // running/queued run must not match.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs?status=completed"),
        Some(&job_token),
    )
    .await;
    let filtered = body_json(res).await;
    let all_terminal = filtered["workflow_runs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "completed");
    assert!(all_terminal);

    // gh run view.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let run = body_json(res).await;
    assert_eq!(run["head_branch"], "main");
    assert_eq!(run["path"], ".github/workflows/ci.yml");

    // gh run view's jobs table.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/jobs"),
        Some(&job_token),
    )
    .await;
    let jobs = body_json(res).await;
    assert_eq!(jobs["total_count"], 1, "{jobs}");
    assert_eq!(jobs["jobs"][0]["id"], job_id);

    // gh run view --job.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/jobs/{job_id}"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let job = body_json(res).await;
    assert_eq!(job["run_id"], run_id);

    // gh workflow list + view (by filename and by name).
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows"),
        Some(&job_token),
    )
    .await;
    let wfs = body_json(res).await;
    assert_eq!(wfs["total_count"], 1, "{wfs}");
    assert_eq!(wfs["workflows"][0]["path"], ".github/workflows/ci.yml");
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows/ci.yml"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows/CI"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows/nope.yml"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // gh run list --workflow.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows/ci.yml/runs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 1);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows/nope/runs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Workflow disable → the workflow reports disabled state. The mutation
    // RPCs gate on repo write capability — a `contents: read` job token is
    // read-only even with `actions: write` (a GitHub divergence), so these
    // calls ride the owner PAT.
    let res = rest(
        &app,
        "PUT",
        &format!("{base}/actions/workflows/ci.yml/disable"),
        Some(pat),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "disable: {}",
        res.status()
    );
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/workflows/ci.yml"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["state"], "disabled_manually");
    let res = rest(
        &app,
        "PUT",
        &format!("{base}/actions/workflows/ci.yml/enable"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Dispatch → 204 + a second run appears.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("{base}/actions/workflows/ci.yml/dispatches"))
                .header("authorization", format!("Bearer {pat}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"ref":"main","inputs":{}}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs?event=workflow_dispatch"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 1);

    // Cancel the queued run → 202; the job token is revoked with it, so the
    // rerun + delete ride the PAT like a real `gh` session would.
    let res = rest(
        &app,
        "POST",
        &format!("{base}/actions/runs/{run_id}/cancel"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = rest(
        &app,
        "POST",
        &format!("{base}/actions/runs/{run_id}/rerun"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED, "rerun: {}", res.status());

    // The rerun re-queued the run — GitHub only deletes completed runs, so
    // cancel once more before deleting.
    let res = rest(
        &app,
        "POST",
        &format!("{base}/actions/runs/{run_id}/cancel"),
        Some(pat),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::ACCEPTED,
        "recancel: {}",
        res.status()
    );

    // Delete the run → 204.
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/runs/{run_id}"),
        Some(pat),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "delete: {}",
        res.status()
    );
}

/// `/api/v1` secrets / variables / environments — the `gh secret set` /
/// `gh variable` surface: sealed-box `crypto_box_seal` uploads through the
/// `public-key` endpoints, GitHub create/update status codes, and the
/// org-scope twins.
#[tokio::test]
async fn rest_actions_secrets_variables_environments_surface() {
    std::env::set_var("OXIDEAN_ACTIONS_SECRETS_KEY", "rest-secrets-test-key");
    let (app, db, _dir) = app_db().await;
    let owner = db
        .create_user(
            "u-sec",
            "sec@example.com",
            "sec",
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-sec", &owner.id, "user", "sec", "private", "", "main")
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at("u-sec", &now).await.unwrap();
    let pat = "oxidean_pat_sec_roundtrip";
    db.create_pat(
        "pat-sec",
        "u-sec",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();

    let base = "/api/v1/repos/sec/sec";

    // public-key → libsodium sealed box → PUT — exactly `gh secret set`'s flow.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/secrets/public-key"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "public-key: {}", res.status());
    let key = body_json(res).await;
    let pk_raw = base64::engine::general_purpose::STANDARD
        .decode(key["key"].as_str().unwrap())
        .unwrap();
    assert_eq!(pk_raw.len(), 32);
    assert!(!key["key_id"].as_str().unwrap_or_default().is_empty());
    let pk = crypto_box::PublicKey::try_from(pk_raw.as_slice()).unwrap();
    let sealed = pk.seal(&mut crypto_box::aead::OsRng, b"hunter2").unwrap();
    let sealed_b64 = base64::engine::general_purpose::STANDARD.encode(&sealed);

    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/secrets/API_TOKEN"),
        Some(pat),
        &format!(
            r#"{{"encrypted_value":"{sealed_b64}","key_id":"{}"}}"#,
            key["key_id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "sealed put: {}",
        res.status()
    );
    // The stored ciphertext decrypts back to the sealed plaintext.
    let rows = db.list_action_secret_ciphertexts(&repo.id).await.unwrap();
    let row = rows.iter().find(|r| r.name == "API_TOKEN").unwrap();
    assert_eq!(
        oxidean_api::actions::secrets::decrypt_secret(&row.ciphertext).unwrap(),
        "hunter2"
    );

    // Plaintext `value` (first-party shape) → update path answers 204.
    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/secrets/API_TOKEN"),
        Some(pat),
        r#"{"value":"rotated"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = rest(&app, "GET", &format!("{base}/actions/secrets"), Some(pat)).await;
    let list = body_json(res).await;
    assert_eq!(list["total_count"], 1, "{list}");
    assert_eq!(list["secrets"][0]["name"], "API_TOKEN");
    assert!(!list["secrets"][0]["created_at"]
        .as_str()
        .unwrap_or_default()
        .is_empty());

    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/secrets/API_TOKEN"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/secrets/NOPE"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // A garbage sealed blob answers 400, not 500.
    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/secrets/BROKEN"),
        Some(pat),
        r#"{"encrypted_value":"aGVsbG8="}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/secrets/API_TOKEN"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // ---- repo variables ----
    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/actions/variables"),
        Some(pat),
        r#"{"name":"REGION","value":"us-east"}"#,
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "var create: {}",
        res.status()
    );
    // duplicate create → 409 like GitHub
    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/actions/variables"),
        Some(pat),
        r#"{"name":"REGION","value":"x"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);

    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/variables/REGION"),
        Some(pat),
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["value"], "us-east", "{v}");
    assert!(!v["created_at"].as_str().unwrap_or_default().is_empty());

    let res = rest_json(
        &app,
        "PATCH",
        &format!("{base}/actions/variables/REGION"),
        Some(pat),
        r#"{"value":"eu-west"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/variables/REGION"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["value"], "eu-west");
    // PATCH on a missing name → 404 (no upsert)
    let res = rest_json(
        &app,
        "PATCH",
        &format!("{base}/actions/variables/NOPE"),
        Some(pat),
        r#"{"value":"x"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/variables/REGION"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // ---- environments ----
    let res = rest_json(&app, "PUT", &format!("{base}/environments/prod"), Some(pat), &format!(
                    r#"{{"wait_timer":5,"prevent_self_review":true,"reviewers":[{{"type":"User","id":"{}"}}]}}"#,
                    owner.id
                )).await;
    assert_eq!(res.status(), StatusCode::OK, "env upsert: {}", res.status());
    let env = body_json(res).await;
    assert_eq!(env["name"], "prod");
    assert!(env["protection_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["type"] == "wait_timer" && r["wait_timer"] == 5));
    assert!(env["protection_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["type"] == "required_reviewers"));

    let res = rest(&app, "GET", &format!("{base}/environments"), Some(pat)).await;
    assert_eq!(body_json(res).await["total_count"], 1);
    let res = rest(&app, "GET", &format!("{base}/environments/prod"), Some(pat)).await;
    assert_eq!(body_json(res).await["name"], "prod");
    let res = rest(&app, "GET", &format!("{base}/environments/nope"), Some(pat)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // environment-scoped sealed secret + variable
    let res = rest(
        &app,
        "GET",
        &format!("{base}/environments/prod/secrets/public-key"),
        Some(pat),
    )
    .await;
    let ekey = body_json(res).await;
    let epk_raw = base64::engine::general_purpose::STANDARD
        .decode(ekey["key"].as_str().unwrap())
        .unwrap();
    // the env keypair differs from the repo one — scope isolation
    assert_ne!(epk_raw, pk_raw);
    let epk = crypto_box::PublicKey::try_from(epk_raw.as_slice()).unwrap();
    let esecret = epk.seal(&mut crypto_box::aead::OsRng, b"env-only").unwrap();
    let esecret_b64 = base64::engine::general_purpose::STANDARD.encode(&esecret);
    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/environments/prod/secrets/ENV_KEY"),
        Some(pat),
        &format!(r#"{{"encrypted_value":"{esecret_b64}"}}"#),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/environments/prod/secrets"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 1);

    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/environments/prod/variables"),
        Some(pat),
        r#"{"name":"EURL","value":"https://prod"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/environments/prod/variables/EURL"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["value"], "https://prod");

    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/environments/prod"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(&app, "GET", &format!("{base}/environments/prod"), Some(pat)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // ---- org scope ----
    let org = db
        .insert_organization("o-sec", "sec-org", "Sec Org", "read")
        .await
        .unwrap();
    db.insert_org_member(&org.id, &owner.id, "owner")
        .await
        .unwrap();
    let orgbase = "/api/v1/orgs/sec-org";

    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/secrets/public-key"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let okey = body_json(res).await;
    let opk_raw = base64::engine::general_purpose::STANDARD
        .decode(okey["key"].as_str().unwrap())
        .unwrap();
    assert_ne!(opk_raw, pk_raw);
    let opk = crypto_box::PublicKey::try_from(opk_raw.as_slice()).unwrap();
    let osecret = opk
        .seal(&mut crypto_box::aead::OsRng, b"org-secret")
        .unwrap();
    let osecret_b64 = base64::engine::general_purpose::STANDARD.encode(&osecret);
    let res = rest_json(
        &app,
        "PUT",
        &format!("{orgbase}/actions/secrets/ORG_KEY"),
        Some(pat),
        &format!(r#"{{"encrypted_value":"{osecret_b64}","visibility":"all"}}"#),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/secrets"),
        Some(pat),
    )
    .await;
    let olist = body_json(res).await;
    assert_eq!(olist["total_count"], 1, "{olist}");
    assert_eq!(olist["secrets"][0]["visibility"], "all");
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/secrets/ORG_KEY/selected-repositories"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["total_count"], 0);

    // selected-repositories set-management: visibility=selected via the PUT
    // body, then the granular replace/add/remove routes.
    let repo2 = db
        .insert_repository("r-sec2", &owner.id, "user", "sec2", "private", "", "main")
        .await
        .unwrap();
    let res = rest_json(
        &app,
        "PUT",
        &format!("{orgbase}/actions/secrets/ORG_KEY/selected-repositories"),
        Some(pat),
        &format!(r#"{{"selected_repository_ids":["{}"]}}"#, repo.id),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/secrets/ORG_KEY/selected-repositories"),
        Some(pat),
    )
    .await;
    let sel = body_json(res).await;
    assert_eq!(sel["total_count"], 1, "{sel}");
    assert_eq!(sel["repositories"][0]["name"], "sec");
    let res = rest(
        &app,
        "PUT",
        &format!(
            "{orgbase}/actions/secrets/ORG_KEY/selected-repositories/{}",
            repo2.id
        ),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/secrets/ORG_KEY/selected-repositories"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 2);
    let res = rest(
        &app,
        "DELETE",
        &format!(
            "{orgbase}/actions/secrets/ORG_KEY/selected-repositories/{}",
            repo.id
        ),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/secrets/ORG_KEY/selected-repositories"),
        Some(pat),
    )
    .await;
    let sel = body_json(res).await;
    assert_eq!(sel["total_count"], 1, "{sel}");
    assert_eq!(sel["repositories"][0]["name"], "sec2");

    let res = rest_json(
        &app,
        "POST",
        &format!("{orgbase}/actions/variables"),
        Some(pat),
        r#"{"name":"TIER","value":"gold","visibility":"all"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/variables/TIER"),
        Some(pat),
    )
    .await;
    let ov = body_json(res).await;
    assert_eq!(ov["value"], "gold", "{ov}");
    assert_eq!(ov["visibility"], "all");
    // Variable selected-set mutations ride the same three routes.
    let res = rest(
        &app,
        "PUT",
        &format!(
            "{orgbase}/actions/variables/TIER/selected-repositories/{}",
            repo.id
        ),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{orgbase}/actions/variables/TIER/selected-repositories"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 1);
    let res = rest(
        &app,
        "DELETE",
        &format!(
            "{orgbase}/actions/variables/TIER/selected-repositories/{}",
            repo.id
        ),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "DELETE",
        &format!("{orgbase}/actions/variables/TIER"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "DELETE",
        &format!("{orgbase}/actions/secrets/ORG_KEY"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // ---- repo-visible org secrets/variables (organization-secrets /
    // organization-variables) — an org-owned private repo sees `all`,
    // `private`, and `selected` rows whose allow-set contains it.
    let org_repo = db
        .insert_repository("r-osec", &org.id, "org", "osec", "private", "", "main")
        .await
        .unwrap();
    db.upsert_action_org_secret("os-all", &org.id, "ORG_ALL", "ct", "all")
        .await
        .unwrap();
    db.upsert_action_org_secret("os-priv", &org.id, "ORG_PRIV", "ct", "private")
        .await
        .unwrap();
    db.upsert_action_org_secret("os-sel", &org.id, "ORG_SEL", "ct", "selected")
        .await
        .unwrap();
    db.set_action_org_secret_repos("os-sel", std::slice::from_ref(&org_repo.id))
        .await
        .unwrap();
    db.upsert_action_org_secret("os-selno", &org.id, "ORG_SELNO", "ct", "selected")
        .await
        .unwrap();
    db.upsert_action_org_variable("ov-all", &org.id, "VAR_ALL", "gold", "all")
        .await
        .unwrap();
    db.upsert_action_org_variable("ov-sel", &org.id, "VAR_SEL", "silver", "selected")
        .await
        .unwrap();
    db.set_action_org_variable_repos("ov-sel", std::slice::from_ref(&org_repo.id))
        .await
        .unwrap();
    db.upsert_action_org_variable("ov-selno", &org.id, "VAR_SELNO", "no", "selected")
        .await
        .unwrap();

    let repobase = "/api/v1/repos/sec-org/osec";
    let res = rest(
        &app,
        "GET",
        &format!("{repobase}/actions/organization-secrets"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let vis = body_json(res).await;
    let names: Vec<&str> = vis["secrets"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names, ["ORG_ALL", "ORG_PRIV", "ORG_SEL"], "{vis}");
    // Metadata only — never a value or ciphertext field.
    assert!(vis["secrets"][0].get("value").is_none());
    assert!(vis["secrets"][0].get("encrypted_value").is_none());
    assert_eq!(
        vis["secrets"][2]["visibility"], "selected",
        "selected visibility is surfaced"
    );
    assert!(vis["secrets"][2]["selected_repositories_url"]
        .as_str()
        .unwrap_or_default()
        .contains("/orgs/sec-org/actions/secrets/ORG_SEL/selected-repositories"));

    let res = rest(
        &app,
        "GET",
        &format!("{repobase}/actions/organization-variables"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let vlist = body_json(res).await;
    let vnames: Vec<&str> = vlist["variables"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["name"].as_str())
        .collect();
    assert_eq!(vnames, ["VAR_ALL", "VAR_SEL"], "{vlist}");
    // Variables carry their plaintext value on this route (as on GitHub).
    assert_eq!(vlist["variables"][0]["value"], "gold");

    // A public repo does NOT see `private`-visibility rows.
    let pub_repo = db
        .insert_repository("r-opub", &org.id, "org", "opub", "public", "", "main")
        .await
        .unwrap();
    let metas = db
        .list_action_org_secret_metas_for_repo(&pub_repo.id)
        .await
        .unwrap();
    assert_eq!(
        metas.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["ORG_ALL"]
    );
}

/// `/api/v1` deployments + statuses + `repository_dispatch` + the
/// actions/permissions + workflow-permissions settings, and the
/// `pending_deployments` review roundtrip — the surface `gh api` / deployment
/// tooling targets.
#[tokio::test]
async fn rest_actions_deployments_permissions_dispatch_surface() {
    let (app, db, dir) = app_db().await;
    let owner = db
        .create_user(
            "u-dep",
            "dep@example.com",
            "dep",
            Some("h"),
            "F",
            "",
            None,
            Role::User,
        )
        .await
        .unwrap();
    let repo = db
        .insert_repository("r-dep", &owner.id, "user", "dep", "private", "", "main")
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at("u-dep", &now).await.unwrap();
    let pat = "oxidean_pat_dep_roundtrip";
    db.create_pat(
        "pat-dep",
        "u-dep",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();
    // dispatch + workflow discovery need the bare repo's tree.
    let bare = dir.path().join("repos").join("dep").join("dep.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    let git = oxidean_git::CliGitBackend::new();
    oxidean_git::GitBackend::init_bare(&git, &bare, "main")
        .await
        .unwrap();
    oxidean_git::GitBackend::seed_commit(
        &git,
        &bare,
        "main",
        "workflows",
        &[(
            ".github/workflows/deploy.yml".into(),
            b"name: D\non: [repository_dispatch]\njobs:\n  ship:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ship\n".to_vec(),
        )],
    )
    .await
    .unwrap();
    let base = "/api/v1/repos/dep/dep";

    // ---- deployments ----
    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/deployments"),
        Some(pat),
        r#"{"ref":"main","environment":"production","description":"first","task":"deploy"}"#,
    )
    .await;
    let status = res.status();
    let dep = body_json(res).await;
    assert_eq!(status, StatusCode::CREATED, "create: {dep}");
    let dep_id = dep["id"].as_str().unwrap().to_string();
    assert_eq!(dep["ref"], "refs/heads/main");
    assert_eq!(dep["environment"], "production");
    assert_eq!(dep["task"], "deploy");
    assert!(dep["url"].as_str().unwrap().contains("/deployments/"));

    let res = rest(&app, "GET", &format!("{base}/deployments"), Some(pat)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let list = body_json(res).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], dep_id);

    // query filters GitHub passes through
    let res = rest(
        &app,
        "GET",
        &format!("{base}/deployments?environment=staging"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await.as_array().unwrap().len(), 0);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/deployments?environment=production&ref=main&task=deploy"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await.as_array().unwrap().len(), 1);

    let res = rest(
        &app,
        "GET",
        &format!("{base}/deployments/{dep_id}"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(body_json(res).await["id"], dep_id);

    // ---- deployment statuses ----
    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/deployments/{dep_id}/statuses"),
        Some(pat),
        r#"{"state":"in_progress","description":"rolling out","target_url":"https://example.com/logs"}"#,
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::CREATED,
        "status create: {}",
        res.status()
    );
    let st = body_json(res).await;
    assert_eq!(st["state"], "in_progress");
    let res = rest(
        &app,
        "GET",
        &format!("{base}/deployments/{dep_id}/statuses"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let statuses = body_json(res).await;
    assert_eq!(statuses.as_array().unwrap().len(), 1);
    assert_eq!(statuses[0]["state"], "in_progress");
    // latest status surfaces on the deployment projection
    let res = rest(
        &app,
        "GET",
        &format!("{base}/deployments/{dep_id}"),
        Some(pat),
    )
    .await;
    let dep = body_json(res).await;
    assert!(
        dep["statuses_url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/repos/dep/dep/deployments/{dep_id}/statuses")),
        "statuses_url: {}",
        dep["statuses_url"]
    );

    // GitHub only deletes inactive deployments — the live one must 4xx.
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/deployments/{dep_id}"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/deployments/{dep_id}/statuses"),
        Some(pat),
        r#"{"state":"inactive","auto_inactive":false}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/deployments/{dep_id}"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(&app, "GET", &format!("{base}/deployments"), Some(pat)).await;
    assert_eq!(body_json(res).await.as_array().unwrap().len(), 0);

    // ---- actions/permissions (enabled toggle) ----
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/permissions"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let perms = body_json(res).await;
    assert_eq!(perms["enabled"], true);
    assert_eq!(perms["allowed_actions"], "all");

    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/permissions"),
        Some(pat),
        r#"{"enabled":false,"allowed_actions":"all"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/permissions"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await["enabled"], false);
    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/permissions"),
        Some(pat),
        r#"{"enabled":true}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // ---- actions/permissions/workflow ----
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/permissions/workflow"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let wp = body_json(res).await;
    assert_eq!(wp["default_workflow_permissions"], "read");
    assert_eq!(wp["can_approve_pull_request_reviews"], false);

    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/permissions/workflow"),
        Some(pat),
        r#"{"default_workflow_permissions":"write","can_approve_pull_request_reviews":true}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/permissions/workflow"),
        Some(pat),
    )
    .await;
    let wp = body_json(res).await;
    assert_eq!(wp["default_workflow_permissions"], "write");
    assert_eq!(wp["can_approve_pull_request_reviews"], true);
    // restore read so later mints stay restricted
    let res = rest_json(
        &app,
        "PUT",
        &format!("{base}/actions/permissions/workflow"),
        Some(pat),
        r#"{"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // ---- repository_dispatch ----
    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/dispatches"),
        Some(pat),
        r#"{"event_type":"deploy-preview","client_payload":{"env":"staging","n":2}}"#,
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "dispatch: {}",
        res.status()
    );
    let res = rest(&app, "GET", &format!("{base}/actions/runs"), Some(pat)).await;
    let runs = body_json(res).await;
    let dispatched = runs["workflow_runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["event"] == "repository_dispatch")
        .expect("repository_dispatch run enqueued");
    let run_id = dispatched["id"].as_str().unwrap().to_string();

    // ---- pending_deployments: env-gated job roundtrip ----
    db.upsert_action_environment(
        "env-dep",
        &repo.id,
        "production",
        &format!(r#"{{"required_reviewers":["{}"]}}"#, owner.id),
    )
    .await
    .unwrap();
    let doc = parse_workflow_yaml(
        b"name: E\non: [push]\njobs:\n  deploy:\n    runs-on: ubuntu-latest\n    environment: production\n    steps:\n      - run: echo deploy\n",
    )
    .unwrap();
    let (held_run, job_ids) = enqueue_run(
        &db,
        &repo.id,
        ".github/workflows/deploy.yml",
        &doc,
        "push",
        "deadbeef",
        "refs/heads/main",
        Some(&owner.id),
        None,
        None,
        false,
        None,
    )
    .await
    .unwrap();
    let held_job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert!(held_job.approval_hold);

    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{held_run}/pending_deployments"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let pending = body_json(res).await;
    let pending = pending.as_array().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["name"], "production");
    assert_eq!(pending[0]["current_user_can_approve"], true);

    // a run with no held jobs → empty list
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/pending_deployments"),
        Some(pat),
    )
    .await;
    assert_eq!(body_json(res).await.as_array().unwrap().len(), 0);

    let res = rest_json(
        &app,
        "POST",
        &format!("{base}/actions/runs/{held_run}/pending_deployments"),
        Some(pat),
        r#"{"environment_ids":["production"],"state":"approved","comment":"ship"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "review: {}", res.status());
    let reviewed = body_json(res).await;
    assert_eq!(reviewed[0]["name"], "production");
    let job = db
        .find_action_job_by_id(&job_ids[0])
        .await
        .unwrap()
        .unwrap();
    assert!(!job.approval_hold, "review releases the gate");
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{held_run}/pending_deployments"),
        Some(pat),
    )
    .await;
    assert_eq!(
        body_json(res).await.as_array().unwrap().len(),
        0,
        "no pending envs after approval"
    );

    // Review decisions surface on `GET …/runs/{id}/approvals` (GitHub shape).
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{held_run}/approvals"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let approvals = body_json(res).await;
    let approvals = approvals.as_array().unwrap();
    assert_eq!(approvals.len(), 1, "{approvals:?}");
    assert_eq!(approvals[0]["state"], "approved");
    assert_eq!(approvals[0]["comment"], "ship");
    assert_eq!(approvals[0]["environments"][0]["name"], "production");
    assert_eq!(approvals[0]["user"]["login"], "dep");
}

/// `/api/v1` caches list/delete + run-attempt routes — `gh cache` and
/// `gh run view --attempt` surface.
#[tokio::test]
async fn rest_actions_caches_attempts_surface() {
    let (app, db, _dir) = app_db().await;
    let (token, run_id, job_id, job_token) = claim_full_yaml(
        &app,
        &db,
        "cax",
        b"name: CI\non: [push]\npermissions:\n  contents: read\n  actions: write\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hi\n",
    )
    .await;
    let base = "/api/v1/repos/cax/cax";
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    // Deletes gate on repo Write capability + a verified email — the job
    // token's `actions: write` scope covers reads but repo capability stays
    // read; owner PAT it is.
    db.set_email_verified_at("u-cax", &now).await.unwrap();
    let pat = "oxidean_pat_cax_roundtrip";
    db.create_pat(
        "pat-cax",
        "u-cax",
        "classic",
        "test",
        "oxidean_pat_",
        &hex::encode(Sha256::digest(pat.as_bytes())),
        Some(r#"["repo","workflow"]"#),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();

    // Two committed cache rows — one on main, one scoped to a branch.
    for (id, key, scope, size) in [
        ("cache-1", "linux-cargo-main", "refs/heads/main", 100i64),
        ("cache-2", "linux-cargo-dev", "refs/heads/dev", 200i64),
        ("cache-3", "win-cargo-main", "refs/heads/main", 50i64),
    ] {
        db.insert_action_cache(&oxidean_db::actions_cache::ActionCacheRow {
            id: id.into(),
            repository_id: "r-cax".into(),
            lookup_hash: hex::encode(Sha256::digest(format!("{key}\nv\n{scope}").as_bytes())),
            cache_key: key.into(),
            version: "v".into(),
            scope: scope.into(),
            storage_path: format!("/tmp/{id}.tgz"),
            size_bytes: size,
            state: "committed".into(),
            created_at: now.clone(),
            last_accessed_at: now.clone(),
        })
        .await
        .unwrap();
        // insert lands `reserved`; commit marks it listable.
        db.commit_action_cache(id, size).await.unwrap();
    }

    // gh cache list → {total_count, actions_caches}
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches"),
        Some(&job_token),
    )
    .await;
    let status = res.status();
    let body = body_json(res).await;
    assert_eq!(body["total_count"], 3, "caches list ({status}): {body}");
    let caches = body["actions_caches"].as_array().unwrap();
    assert_eq!(caches.len(), 3);
    // GitHub shape: ref (from scope), key, size_in_bytes
    assert!(caches.iter().all(|c| c["ref"].as_str().is_some()
        && c["key"].as_str().is_some()
        && c["size_in_bytes"].as_i64().is_some()));

    // key prefix filter (GitHub semantics — prefix, not substring)
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches?key=linux-cargo"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 2);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches?key=cargo"),
        Some(&job_token),
    )
    .await;
    assert_eq!(
        body_json(res).await["total_count"],
        0,
        "substring must not match — prefix only"
    );

    // ref filter
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches?ref=refs/heads/dev"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 1);

    // sort=size_in_bytes&direction=asc
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches?sort=size_in_bytes&direction=asc"),
        Some(&job_token),
    )
    .await;
    let body = body_json(res).await;
    let sizes: Vec<i64> = body["actions_caches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["size_in_bytes"].as_i64().unwrap())
        .collect();
    assert_eq!(sizes, vec![50, 100, 200]);

    // DELETE by key+ref → only the matching row goes
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/caches?key=linux-cargo&ref=refs/heads/dev"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches"),
        Some(&job_token),
    )
    .await;
    assert_eq!(body_json(res).await["total_count"], 2);

    // DELETE by id
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/caches/cache-3"),
        Some(pat),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/caches"),
        Some(&job_token),
    )
    .await;
    let body = body_json(res).await;
    assert_eq!(body["total_count"], 1);
    assert_eq!(body["actions_caches"][0]["id"], "cache-1");

    // missing key+ref on bulk delete → 400
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/caches"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // ---- cache/usage rollup ----
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/cache/usage"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let usage = body_json(res).await;
    assert_eq!(usage["full_name"], "cax/cax");
    assert_eq!(usage["active_caches_count"], 1);
    assert_eq!(usage["active_caches_size_in_bytes"], 100);

    // ---- run attempts ----
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/attempts/1"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "attempt 1: {}", res.status());
    let att = body_json(res).await;
    assert_eq!(att["run_attempt"], 1);
    assert_eq!(att["id"], run_id);

    // stale attempt → 404 (reruns replace job rows — no attempt history)
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/attempts/2"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/attempts/1/jobs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let jobs = body_json(res).await;
    assert_eq!(jobs["total_count"], 1);
    assert_eq!(jobs["jobs"][0]["id"], job_id);

    // attempt logs — the same zip archive the unqualified route serves.
    // Seed a job log first: GitHub 404s runs with no log content, and so do we.
    let log_dir = _dir.path().join("actions-logs").join(&run_id);
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(
        log_dir.join(format!("{job_id}.log")),
        "Job leg — build / 1 step(s) [host]\n##[step]run\necho hi\n##[step-done]run\njob concluded success\n",
    )
    .unwrap();
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/attempts/1/logs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "attempt logs: {}",
        res.status()
    );
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/attempts/7/logs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // ---- DELETE runs/{id}/logs — files go, run row stays ----
    let res = rest(
        &app,
        "DELETE",
        &format!("{base}/actions/runs/{run_id}/logs"),
        Some(pat),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "delete logs: {}",
        res.status()
    );
    // run still reads; logs route now 404s.
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = rest(
        &app,
        "GET",
        &format!("{base}/actions/runs/{run_id}/logs"),
        Some(&job_token),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let _ = token;
}
