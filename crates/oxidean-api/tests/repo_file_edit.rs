//! GIT-19: browser file editing RPCs — direct commits, protected-branch →
//! branch+PR fallback, ACL denial, traversal rejection, empty file/dir edges.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use oxidean_api::email::{EmailSender, LogSink};
use oxidean_api::{build_cors, router_with_state, AppState};
use oxidean_db::Database;
use tower::ServiceExt;

async fn test_app(db: Database, repos_dir: std::path::PathBuf) -> axum::Router {
    let state = AppState::new(db, Arc::new(LogSink) as Arc<dyn EmailSender>, "development")
        .with_repos_dir(repos_dir);
    let cors = build_cors("development", None).expect("cors");
    router_with_state(state, cors)
}

fn rpc_req(body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn rpc_req_with_cookie(body: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/rpc")
        .header("content-type", "application/json")
        .header("Oxidean-RPC-Version", "1")
        .header("cookie", cookie)
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn session_cookie_from_response(res: &axum::http::Response<Body>) -> String {
    let set_cookie = res
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().trim().to_string()
}

async fn signup_and_login(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, serde_json::Value) {
    let signup_body = format!(
        r#"{{"procedure":"auth.signup","input":{{"email":"{email}","username":"{username}","password":"password1"}}}}"#
    );
    let signup = app.clone().oneshot(rpc_req(&signup_body)).await.unwrap();
    assert_eq!(signup.status(), StatusCode::OK);
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body)).await.unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v)
}

async fn rpc_json(app: &axum::Router, body: &str, cookie: &str) -> serde_json::Value {
    let res = app
        .clone()
        .oneshot(rpc_req_with_cookie(body, cookie))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("rpc json body")
}

async fn seed_owner_repo(
    app: &axum::Router,
    db: &Database,
    email: &str,
    username: &str,
    repo: &str,
    visibility: &str,
) -> String {
    let (cookie, login_v) = signup_and_login(app, email, username).await;
    let user_id = login_v["data"]["id"].as_str().expect("id").to_string();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(&user_id, &now)
        .await
        .expect("verify");

    let create = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.create","input":{{"name":"{repo}","visibility":"{visibility}","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "seed create — {create}");
    cookie
}

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let mut n = (chunk[0] as u32) << 16;
        if chunk.len() > 1 {
            n |= (chunk[1] as u32) << 8;
        }
        if chunk.len() > 2 {
            n |= chunk[2] as u32;
        }
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

async fn blob_content(
    app: &axum::Router,
    cookie: &str,
    owner: &str,
    repo: &str,
    ref_name: &str,
    path: &str,
) -> serde_json::Value {
    rpc_json(
        app,
        &format!(
            r#"{{"procedure":"repo.blob","input":{{"owner":"{owner}","name":"{repo}","ref":"{ref_name}","path":"{path}"}}}}"#
        ),
        cookie,
    )
    .await
}

/// Direct-commit path: create, update, rename (blob-identity), upload batch,
/// mkdir (.gitkeep), empty file, delete file + directory — all on `main`.
#[tokio::test]
async fn repo_file_edit_direct_commit_crud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("file_edit.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "own@ex.com", "ownu", "editme", "public").await;

    // create
    let create = rpc_json(
        &app,
        r##"{"procedure":"repo.file.create","input":{"owner":"ownu","name":"editme","path":"docs/guide.md","content":"# Guide\n","message":"Add guide"}}"##,
        &cookie,
    )
    .await;
    assert_eq!(create["ok"], true, "create — {create}");
    assert_eq!(create["data"]["branch"], "main");
    assert_eq!(create["data"]["created_branch"], false);
    assert_eq!(create["data"]["commit_sha"].as_str().unwrap().len(), 40);

    let blob = blob_content(&app, &cookie, "ownu", "editme", "main", "docs/guide.md").await;
    assert_eq!(blob["ok"], true, "blob — {blob}");
    assert_eq!(blob["data"]["content"], "# Guide\n");

    // update
    let update = rpc_json(
        &app,
        r##"{"procedure":"repo.file.update","input":{"owner":"ownu","name":"editme","path":"docs/guide.md","content":"# Guide v2\n","message":"Update guide"}}"##,
        &cookie,
    )
    .await;
    assert_eq!(update["ok"], true, "update — {update}");
    let blob = blob_content(&app, &cookie, "ownu", "editme", "main", "docs/guide.md").await;
    assert_eq!(blob["data"]["content"], "# Guide v2\n");

    // update of a missing path → not found
    let miss = rpc_json(
        &app,
        r#"{"procedure":"repo.file.update","input":{"owner":"ownu","name":"editme","path":"missing.md","content":"x","message":"nope"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(miss["ok"], false);
    assert_eq!(miss["error"]["code"], "repo.path_not_found");

    // create on existing path → conflict
    let dup = rpc_json(
        &app,
        r#"{"procedure":"repo.file.create","input":{"owner":"ownu","name":"editme","path":"docs/guide.md","content":"x","message":"dup"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(dup["ok"], false);
    assert_eq!(dup["error"]["code"], "repo.path_conflict");

    // rename (pure move, preserves blob identity)
    let mv = rpc_json(
        &app,
        r#"{"procedure":"repo.file.rename","input":{"owner":"ownu","name":"editme","from_path":"docs/guide.md","to_path":"docs/handbook.md","message":"Rename guide"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(mv["ok"], true, "rename — {mv}");
    let moved = blob_content(&app, &cookie, "ownu", "editme", "main", "docs/handbook.md").await;
    assert_eq!(moved["data"]["content"], "# Guide v2\n");

    // mkdir → .gitkeep materialization (Git has no empty-dir object)
    let mk = rpc_json(
        &app,
        r#"{"procedure":"repo.file.mkdir","input":{"owner":"ownu","name":"editme","path":"assets/img","message":"add assets dir"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(mk["ok"], true, "mkdir — {mk}");
    let keep = blob_content(
        &app,
        &cookie,
        "ownu",
        "editme",
        "main",
        "assets/img/.gitkeep",
    )
    .await;
    assert_eq!(keep["ok"], true, ".gitkeep blob — {keep}");
    assert_eq!(keep["data"]["content"], "");

    // upload — multi-file atomic commit incl. empty + binary bytes
    let files = serde_json::json!([
        {"path": "assets/img/logo.bin", "content_base64": b64(&[0u8, 1, 2, 255])},
        {"path": "assets/img/EMPTY.txt", "content_base64": b64(b"")},
    ]);
    let up = rpc_json(
        &app,
        &serde_json::json!({
            "procedure": "repo.file.upload",
            "input": {
                "owner": "ownu", "name": "editme",
                "files": files, "message": "Upload assets"
            }
        })
        .to_string(),
        &cookie,
    )
    .await;
    assert_eq!(up["ok"], true, "upload — {up}");
    let bin = blob_content(
        &app,
        &cookie,
        "ownu",
        "editme",
        "main",
        "assets/img/logo.bin",
    )
    .await;
    assert_eq!(bin["ok"], true);
    assert_eq!(bin["data"]["is_binary"], true, "logo.bin is binary — {bin}");
    let empty = blob_content(
        &app,
        &cookie,
        "ownu",
        "editme",
        "main",
        "assets/img/EMPTY.txt",
    )
    .await;
    assert_eq!(empty["data"]["content"], "");

    // delete file
    let del = rpc_json(
        &app,
        r#"{"procedure":"repo.file.delete","input":{"owner":"ownu","name":"editme","path":"docs/handbook.md","message":"Drop handbook"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(del["ok"], true, "delete — {del}");
    let gone = blob_content(&app, &cookie, "ownu", "editme", "main", "docs/handbook.md").await;
    assert_eq!(gone["ok"], false, "deleted blob — {gone}");

    // delete a whole directory
    let deldir = rpc_json(
        &app,
        r#"{"procedure":"repo.file.delete","input":{"owner":"ownu","name":"editme","path":"assets/img","message":"Drop img dir"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(deldir["ok"], true, "delete dir — {deldir}");
    let gone2 = blob_content(
        &app,
        &cookie,
        "ownu",
        "editme",
        "main",
        "assets/img/logo.bin",
    )
    .await;
    assert_eq!(gone2["ok"], false, "dir child gone — {gone2}");

    // history grew on main (seed commit + 7 web commits)
    let commits = rpc_json(
        &app,
        r#"{"procedure":"repo.commits","input":{"owner":"ownu","name":"editme","ref":"main","limit":50}}"#,
        &cookie,
    )
    .await;
    assert_eq!(commits["ok"], true, "commits — {commits}");
    let list = commits["data"]["commits"].as_array().unwrap();
    assert_eq!(list.len(), 8, "expected 8 commits — {commits}");
    assert_eq!(list[0]["subject"], "Drop img dir");
}

/// Protected default branch: direct push blocked → change lands on a new
/// `web-edit/*` branch and a PR is opened; `main` stays untouched.
#[tokio::test]
async fn repo_file_edit_protected_branch_opens_pr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("file_edit_pr.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "own@ex.com", "ownu", "prot", "public").await;

    // Protect `main`: require_reviews blocks direct pushes.
    let rule = rpc_json(
        &app,
        r#"{"procedure":"repo.branchProtection.create","input":{"owner":"ownu","name":"prot","pattern":"main","require_reviews":true,"required_approving_review_count":1,"enforce_admins":true}}"#,
        &cookie,
    )
    .await;
    assert_eq!(rule["ok"], true, "rule — {rule}");

    // commitPolicy tells the UI the PR flow is required.
    let policy = rpc_json(
        &app,
        r#"{"procedure":"repo.file.commitPolicy","input":{"owner":"ownu","name":"prot","branch":"main"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(policy["ok"], true, "policy — {policy}");
    assert_eq!(policy["data"]["direct_commit_allowed"], false);
    assert_eq!(policy["data"]["requires_pr"], true);

    let main_tip_before = {
        let commits = rpc_json(
            &app,
            r#"{"procedure":"repo.commits","input":{"owner":"ownu","name":"prot","ref":"main","limit":1}}"#,
            &cookie,
        )
        .await;
        commits["data"]["commits"][0]["sha"]
            .as_str()
            .unwrap()
            .to_string()
    };

    // No new_branch supplied → automatic branch + PR.
    let edit = rpc_json(
        &app,
        r#"{"procedure":"repo.file.create","input":{"owner":"ownu","name":"prot","path":"docs/new.md","content":"hello\n","message":"Add new doc","pr_title":"Add new doc via web"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(edit["ok"], true, "protected create — {edit}");
    assert_eq!(edit["data"]["created_branch"], true);
    assert_ne!(edit["data"]["branch"], "main");
    assert!(
        edit["data"]["branch"]
            .as_str()
            .unwrap()
            .starts_with("web-edit/"),
        "generated branch — {edit}"
    );
    let pr_number = edit["data"]["pr_number"].as_i64().expect("pr_number");
    let branch = edit["data"]["branch"].as_str().unwrap().to_string();

    // main tip unchanged
    let commits = rpc_json(
        &app,
        r#"{"procedure":"repo.commits","input":{"owner":"ownu","name":"prot","ref":"main","limit":1}}"#,
        &cookie,
    )
    .await;
    assert_eq!(
        commits["data"]["commits"][0]["sha"], main_tip_before,
        "protected main must not move"
    );

    // the change exists on the generated branch
    let blob = blob_content(&app, &cookie, "ownu", "prot", &branch, "docs/new.md").await;
    assert_eq!(blob["ok"], true, "blob on branch — {blob}");
    assert_eq!(blob["data"]["content"], "hello\n");

    // PR opened targeting main from the generated branch
    let pr = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"pull.get","input":{{"owner":"ownu","name":"prot","number":{pr_number}}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(pr["ok"], true, "pull.get — {pr}");
    assert_eq!(pr["data"]["state"], "open");
    assert_eq!(pr["data"]["base_ref"], "main");
    assert_eq!(pr["data"]["head_ref"], branch);
    assert_eq!(pr["data"]["title"], "Add new doc via web");

    // Drop the rule; explicit new_branch on the (now unprotected) repo.
    let rules = rpc_json(
        &app,
        r#"{"procedure":"repo.branchProtection.list","input":{"owner":"ownu","name":"prot"}}"#,
        &cookie,
    )
    .await;
    let rule_id = rules["data"]["rules"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let del_rule = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"repo.branchProtection.delete","input":{{"owner":"ownu","name":"prot","id":"{rule_id}"}}}}"#
        ),
        &cookie,
    )
    .await;
    assert_eq!(del_rule["ok"], true, "delete rule — {del_rule}");
    let edit2 = rpc_json(
        &app,
        r#"{"procedure":"repo.file.create","input":{"owner":"ownu","name":"prot","path":"docs/second.md","content":"two\n","message":"Second doc","new_branch":"docs/second"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(edit2["ok"], true, "explicit new_branch — {edit2}");
    assert_eq!(edit2["data"]["branch"], "docs/second");
    assert_eq!(edit2["data"]["created_branch"], true);
    assert!(edit2["data"]["pr_number"].as_i64().is_some());
}

/// ACL: users without Write get soft `repo.not_found` (private) and are
/// denied on public repos too; `read` collaborators still can't write.
#[tokio::test]
async fn repo_file_edit_acl_denies_without_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("file_edit_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let _owner_cookie =
        seed_owner_repo(&app, &db, "own@ex.com", "ownu", "priv-edit", "private").await;
    let (stranger, stranger_v) = signup_and_login(&app, "stranger@ex.com", "stranger1").await;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(stranger_v["data"]["id"].as_str().unwrap(), &now)
        .await
        .expect("verify stranger");

    // Private repo → anti-enumeration soft 404.
    let denied = rpc_json(
        &app,
        r#"{"procedure":"repo.file.create","input":{"owner":"ownu","name":"priv-edit","path":"x.md","content":"x","message":"x"}}"#,
        &stranger,
    )
    .await;
    assert_eq!(denied["ok"], false, "stranger private — {denied}");
    assert_eq!(denied["error"]["code"], "repo.not_found");

    // Public repo, read-only stranger → still not_found (mutation gate).
    let owner_cookie =
        seed_owner_repo(&app, &db, "own2@ex.com", "ownu2", "pub-edit", "public").await;
    let denied_pub = rpc_json(
        &app,
        r#"{"procedure":"repo.file.create","input":{"owner":"ownu2","name":"pub-edit","path":"x.md","content":"x","message":"x"}}"#,
        &stranger,
    )
    .await;
    assert_eq!(denied_pub["ok"], false, "stranger public — {denied_pub}");
    assert_eq!(denied_pub["error"]["code"], "repo.not_found");

    // Granting `read` is still insufficient.
    let add = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"ownu2","name":"pub-edit","username":"stranger1","permission":"read"}}"#,
        &owner_cookie,
    )
    .await;
    assert_eq!(add["ok"], true, "add collab — {add}");
    let denied_read = rpc_json(
        &app,
        r#"{"procedure":"repo.file.delete","input":{"owner":"ownu2","name":"pub-edit","path":"README.md","message":"rm"}}"#,
        &stranger,
    )
    .await;
    assert_eq!(denied_read["ok"], false, "read collab — {denied_read}");
    assert_eq!(denied_read["error"]["code"], "repo.not_found");
}

/// Path traversal, NUL, `.git`, empty/dup-segment inputs all rejected before
/// any git work.
#[tokio::test]
async fn repo_file_edit_rejects_unsafe_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repos = dir.path().join("repos");
    let url = format!("sqlite:{}", dir.path().join("file_edit_paths.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), repos).await;

    let cookie = seed_owner_repo(&app, &db, "own@ex.com", "ownu", "paths", "public").await;

    let cases = [
        "../escape.md",
        "a/../../escape.md",
        ".git/hooks/x",
        "a//b.md",
        "x/./y.md",
        "dir/",
        "/abs.md",
        "",
    ];
    for bad in cases {
        let body = serde_json::json!({
            "procedure": "repo.file.create",
            "input": {
                "owner": "ownu", "name": "paths",
                "path": bad, "content": "x", "message": "bad"
            }
        });
        let res = rpc_json(&app, &body.to_string(), &cookie).await;
        assert_eq!(res["ok"], false, "{bad} must reject — {res}");
        assert_eq!(
            res["error"]["code"], "repo.invalid_input",
            "{bad} code — {res}"
        );
    }

    // NUL byte inside JSON string (\u0000 escapes the JSON layer).
    let nul = rpc_json(
        &app,
        &serde_json::json!({
            "procedure": "repo.file.create",
            "input": {
                "owner": "ownu", "name": "paths",
                "path": "a\u{0}b.md", "content": "x", "message": "bad"
            }
        })
        .to_string(),
        &cookie,
    )
    .await;
    assert_eq!(nul["ok"], false, "NUL — {nul}");

    // rename onto an existing directory → conflict.
    let to_dir = rpc_json(
        &app,
        r#"{"procedure":"repo.file.rename","input":{"owner":"ownu","name":"paths","from_path":"README.md","to_path":"src","message":"mv"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(to_dir["ok"], false, "rename onto dir — {to_dir}");

    // delete missing → not_found.
    let miss = rpc_json(
        &app,
        r#"{"procedure":"repo.file.delete","input":{"owner":"ownu","name":"paths","path":"nope.txt","message":"rm"}}"#,
        &cookie,
    )
    .await;
    assert_eq!(miss["error"]["code"], "repo.path_not_found", "{miss}");

    // text `content` refuses NUL payloads — binary must go through base64.
    let bin = rpc_json(
        &app,
        &serde_json::json!({
            "procedure": "repo.file.create",
            "input": {
                "owner": "ownu", "name": "paths",
                "path": "bin.txt", "content": "a\u{0}b", "message": "x"
            }
        })
        .to_string(),
        &cookie,
    )
    .await;
    assert_eq!(bin["ok"], false, "binary via text — {bin}");
    assert_eq!(bin["error"]["code"], "repo.binary_content");
}
