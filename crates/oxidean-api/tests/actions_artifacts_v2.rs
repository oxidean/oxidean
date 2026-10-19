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
    .with_actions_artifacts_dir(dir.path().join("actions-artifacts"));
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
