//! COL-05 (WI-0001): org teams with per-repo access lists — integration suite.
//!
//! Covers spec §11 acceptance criteria: team CRUD + gates, membership rules
//! (org members only, member|maintainer roles), per-repo grants feeding
//! `effective_capability` (read/write/admin via team on private org repos),
//! org-2FA lockout suppression, access-loss sweeps, org-member-removal
//! cleanup, and the repo-side `repo.teams.*` surface.

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

async fn rpc_json(
    app: &axum::Router,
    body: &str,
    cookie: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let req = match cookie {
        Some(c) => rpc_req_with_cookie(body, c),
        None => rpc_req(body),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
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
    let _ = signup.into_body().collect().await;

    let login_body = format!(
        r#"{{"procedure":"auth.login","input":{{"identifier":"{email}","password":"password1","remember_me":false}}}}"#
    );
    let login = app.clone().oneshot(rpc_req(&login_body)).await.unwrap();
    let cookie = session_cookie_from_response(&login);
    let bytes = login.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (cookie, v)
}

async fn verify_user(db: &Database, user_id: &str) {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    db.set_email_verified_at(user_id, &now)
        .await
        .expect("verify");
}

/// Boot an app with an org owned by `owner`, plus `member` as an org
/// `member`, plus `outsider` with no org tie. Returns cookies and ids.
struct Fixture {
    owner_cookie: String,
    member_cookie: String,
    outsider_cookie: String,
    member_id: String,
    outsider_id: String,
}

async fn fixture(app: &axum::Router, db: &Database, tag: &str) -> Fixture {
    let (owner_cookie, owner_v) =
        signup_and_login(app, &format!("{tag}-owner@ex.com"), &format!("{tag}owner")).await;
    let owner_id = owner_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(db, &owner_id).await;

    let (member_cookie, member_v) =
        signup_and_login(app, &format!("{tag}-mem@ex.com"), &format!("{tag}mem")).await;
    let member_id = member_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(db, &member_id).await;

    let (outsider_cookie, outsider_v) =
        signup_and_login(app, &format!("{tag}-out@ex.com"), &format!("{tag}out")).await;
    let outsider_id = outsider_v["data"]["id"].as_str().expect("id").to_string();
    verify_user(db, &outsider_id).await;

    let org_slug = format!("{tag}-org");
    let (_, create_v) = rpc_json(
        app,
        &format!(r#"{{"procedure":"org.create","input":{{"slug":"{org_slug}"}}}}"#),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(create_v["ok"], true, "{create_v}");
    let org_id = create_v["data"]["id"].as_str().expect("org id").to_string();

    let (_, add_v) = rpc_json(
        app,
        &format!(
            r#"{{"procedure":"org.members.add","input":{{"slug":"{org_slug}","username":"{tag}mem","role":"member"}}}}"#
        ),
        Some(&owner_cookie),
    )
    .await;
    assert_eq!(add_v["ok"], true, "{add_v}");

    let _ = &org_id;
    Fixture {
        owner_cookie,
        member_cookie,
        outsider_cookie,
        member_id,
        outsider_id,
    }
}

/// AC1: org.teams.create/list/get/update/delete happy path + slug conflicts.
#[tokio::test]
async fn team_crud_and_slug_rules() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("team_crud.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "crud").await;

    // Create derives slug from name.
    let (status, create_v) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"crud-org","name":"Core Devs","description":"core team"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{create_v}");
    assert_eq!(create_v["ok"], true, "{create_v}");
    assert_eq!(create_v["data"]["slug"], "core-devs");
    assert_eq!(create_v["data"]["name"], "Core Devs");
    assert_eq!(create_v["data"]["member_count"], 0);

    // Explicit slug honored.
    let (_, create2) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"crud-org","name":"Ops","team_slug":"ops-team"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(create2["data"]["slug"], "ops-team", "{create2}");

    // Duplicate slug rejected (case-insensitive).
    let (_, dup) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"crud-org","name":"x","team_slug":"OPS-TEAM"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(dup["error"]["code"], "org.team_slug_taken", "{dup}");

    // Invalid slug rejected.
    let (_, bad) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"crud-org","name":"x","team_slug":"bad slug!"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(bad["ok"], false, "{bad}");

    // List returns both teams ordered by slug.
    let (_, list_v) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.list","input":{"slug":"crud-org"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(list_v["ok"], true, "{list_v}");
    let teams = list_v["data"]["teams"].as_array().expect("teams array");
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0]["slug"], "core-devs");
    assert_eq!(teams[1]["slug"], "ops-team");

    // Get by slug.
    let (_, get_v) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.get","input":{"slug":"crud-org","team_slug":"ops-team"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(get_v["data"]["name"], "Ops", "{get_v}");

    // Missing team.
    let (_, miss) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.get","input":{"slug":"crud-org","team_slug":"nope"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(miss["error"]["code"], "org.team_not_found", "{miss}");

    // Update name + description + reslug.
    let (_, upd) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.update","input":{"slug":"crud-org","team_slug":"ops-team","name":"Operations","description":"d","new_slug":"operations"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    assert_eq!(upd["data"]["slug"], "operations");
    assert_eq!(upd["data"]["name"], "Operations");

    // Non-admin member cannot create.
    let (_, forbidden) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"crud-org","name":"x"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(forbidden["error"]["code"], "org.forbidden", "{forbidden}");

    // Delete: admin only; then gone.
    let (_, del_forbidden) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.delete","input":{"slug":"crud-org","team_slug":"operations"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(
        del_forbidden["error"]["code"], "org.forbidden",
        "{del_forbidden}"
    );
    let (_, del) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.delete","input":{"slug":"crud-org","team_slug":"operations"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(del["ok"], true, "{del}");
    let (_, gone) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.get","input":{"slug":"crud-org","team_slug":"operations"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(gone["error"]["code"], "org.team_not_found", "{gone}");
}

/// AC2: team membership requires org membership; role member|maintainer.
#[tokio::test]
async fn team_membership_rules() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("team_mem.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "mem").await;

    let (_, team) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"mem-org","name":"team"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(team["ok"], true, "{team}");

    // Non-org-member cannot join a team.
    let (_, not_member) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"mem-org","team_slug":"team","username":"memout"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(
        not_member["error"]["code"], "org.member_not_found",
        "{not_member}"
    );

    // Org member joins (default role member).
    let (_, add) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"mem-org","team_slug":"team","username":"memmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");
    assert_eq!(add["data"]["role"], "member");

    // Duplicate rejected.
    let (_, dupe) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"mem-org","team_slug":"team","username":"memmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(dupe["error"]["code"], "org.team_member_exists", "{dupe}");

    // Promote to maintainer.
    let (_, role) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.teams.members.set_role","input":{{"slug":"mem-org","team_slug":"team","user_id":"{}","role":"maintainer"}}}}"#,
            f.member_id
        ),
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(role["ok"], true, "{role}");
    assert_eq!(role["data"]["role"], "maintainer");

    // List shows the maintainer.
    let (_, list) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.list","input":{"slug":"mem-org","team_slug":"team"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    let members = list["data"]["members"].as_array().expect("members");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["role"], "maintainer");
    assert_eq!(members[0]["username"], "memmem");

    // Maintainer (not org admin) can manage membership: add a second user.
    let (_, out_add) = rpc_json(
        &app,
        r#"{"procedure":"org.members.add","input":{"slug":"mem-org","username":"memout","role":"member"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(out_add["ok"], true, "{out_add}");
    let (_, maint_add) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"mem-org","team_slug":"team","username":"memout"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(
        maint_add["ok"], true,
        "maintainer adds member — {maint_add}"
    );

    // ...but cannot delete the team.
    let (_, maint_del) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.delete","input":{"slug":"mem-org","team_slug":"team"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(maint_del["error"]["code"], "org.forbidden", "{maint_del}");

    // Plain member cannot add members.
    let (_, plain) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"mem-org","team_slug":"team","username":"memout"}}"#,
        Some(&f.outsider_cookie),
    )
    .await;
    assert_eq!(plain["error"]["code"], "org.forbidden", "{plain}");

    // Remove membership.
    let (_, rm) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.teams.members.remove","input":{{"slug":"mem-org","team_slug":"team","user_id":"{}"}}}}"#,
            f.outsider_id
        ),
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(rm["ok"], true, "{rm}");
    let (_, rm2) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.teams.members.remove","input":{{"slug":"mem-org","team_slug":"team","user_id":"{}"}}}}"#,
            f.outsider_id
        ),
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(rm2["error"]["code"], "org.team_member_not_found", "{rm2}");
}

/// AC4+AC5: team grant on private org repo yields capability through
/// `effective_capability` — read/write/admin via repo.get flags.
#[tokio::test]
async fn team_repo_grant_feeds_capability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("team_acl.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "acl").await;

    // Private org repo.
    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"acl-org"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");

    // member_base=none (default) → member has no read.
    let (_, before) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(before["error"]["code"], "repo.not_found", "{before}");

    // Team + member + read grant.
    let (_, t) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"acl-org","name":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(t["ok"], true, "{t}");
    let (_, m) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"acl-org","team_slug":"devs","username":"aclmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(m["ok"], true, "{m}");
    let (_, g) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"acl-org","team_slug":"devs","repo":"secret","permission":"read"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(g["ok"], true, "{g}");
    assert_eq!(g["data"]["permission"], "read");

    // Member now reads, but cannot write/admin.
    let (_, read_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(read_get["ok"], true, "team read grants read — {read_get}");
    assert_eq!(read_get["data"]["can_write"], false, "{read_get}");
    assert_eq!(read_get["data"]["can_admin"], false, "{read_get}");

    // Outsider (not a team member) still cannot read.
    let (_, out_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"secret"}}"#,
        Some(&f.outsider_cookie),
    )
    .await;
    assert_eq!(out_get["error"]["code"], "repo.not_found", "{out_get}");

    // Escalate grant to admin → can_admin + can_write true.
    let (_, up) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.update","input":{"slug":"acl-org","team_slug":"devs","repo":"secret","permission":"admin"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(up["ok"], true, "{up}");
    let (_, admin_get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(admin_get["data"]["can_admin"], true, "{admin_get}");
    assert_eq!(admin_get["data"]["can_write"], true, "{admin_get}");

    // Grant list reflects it.
    let (_, rl) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.list","input":{"slug":"acl-org","team_slug":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    let repos = rl["data"]["repos"].as_array().expect("repos");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0]["repo_name"], "secret");
    assert_eq!(repos[0]["permission"], "admin");

    // Remove grant → access drops again (highest-wins: nothing else grants).
    let (_, rm) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.remove","input":{"slug":"acl-org","team_slug":"devs","repo":"secret"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(rm["ok"], true, "{rm}");
    let (_, after) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"acl-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(after["error"]["code"], "repo.not_found", "{after}");
}

/// AC5 highest-wins: collaborator write + team read → write; and granting a
/// repo the team doesn't own is rejected.
#[tokio::test]
async fn team_grant_highest_wins_and_org_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("team_wins.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "wins").await;

    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"code","visibility":"private","owner":"wins-org"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");

    // Personal repo of the outsider (user-owned) cannot be team-granted.
    let (_, personal) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"mine","visibility":"private"}}"#,
        Some(&f.outsider_cookie),
    )
    .await;
    assert_eq!(personal["ok"], true, "{personal}");

    let (_, t) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"wins-org","name":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(t["ok"], true, "{t}");
    let (_, bad_repo) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"wins-org","team_slug":"devs","repo":"mine","permission":"read"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(bad_repo["error"]["code"], "repo.not_found", "{bad_repo}");

    // Direct collaborator write on the member + team read → write wins.
    let (_, c) = rpc_json(
        &app,
        r#"{"procedure":"repo.collaborators.add","input":{"owner":"wins-org","name":"code","username":"winsmem","permission":"write"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(c["ok"], true, "{c}");
    let (_, m) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"wins-org","team_slug":"devs","username":"winsmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(m["ok"], true, "{m}");
    let (_, g) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"wins-org","team_slug":"devs","repo":"code","permission":"read"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(g["ok"], true, "{g}");
    let (_, get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"wins-org","name":"code"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(
        get["data"]["can_write"], true,
        "collab write > team read — {get}"
    );
    assert_eq!(get["data"]["can_admin"], false, "{get}");

    // A second team with admin raises to admin (teams union).
    let (_, t2) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"wins-org","name":"leads"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(t2["ok"], true, "{t2}");
    let (_, m2) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"wins-org","team_slug":"leads","username":"winsmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(m2["ok"], true, "{m2}");
    let (_, g2) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"wins-org","team_slug":"leads","repo":"code","permission":"admin"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(g2["ok"], true, "{g2}");
    let (_, get2) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"wins-org","name":"code"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(get2["data"]["can_admin"], true, "team admin wins — {get2}");
}

#[tokio::test]
async fn team_maintainer_cannot_grant_access_to_repo_without_admin() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("team_maintainer_grant.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "grantguard").await;

    let (_, repo) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"private","visibility":"private","owner":"grantguard-org"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(repo["ok"], true, "{repo}");

    let (_, team) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"grantguard-org","name":"maintainers"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(team["ok"], true, "{team}");

    let (_, member) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"grantguard-org","team_slug":"maintainers","username":"grantguardmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(member["ok"], true, "{member}");

    let (_, role) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.teams.members.set_role","input":{{"slug":"grantguard-org","team_slug":"maintainers","user_id":"{}","role":"maintainer"}}}}"#,
            f.member_id
        ),
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(role["ok"], true, "{role}");

    let (_, before) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"grantguard-org","name":"private"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(before["error"]["code"], "repo.not_found", "{before}");

    let (_, grant) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"grantguard-org","team_slug":"maintainers","repo":"private","permission":"admin"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(grant["error"]["code"], "repo.not_found", "{grant}");

    let (_, after) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"grantguard-org","name":"private"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(after["error"]["code"], "repo.not_found", "{after}");
}

/// AC7+AC8: removing a team member drops access; removing the org member
/// removes team rows (member_count drops).
#[tokio::test]
async fn member_removal_drops_access() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("team_rm.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "rm").await;

    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"rm-org"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");
    let (_, t) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"rm-org","name":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(t["ok"], true, "{t}");
    let (_, m) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"rm-org","team_slug":"devs","username":"rmmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(m["ok"], true, "{m}");
    let (_, g) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.repos.add","input":{"slug":"rm-org","team_slug":"devs","repo":"secret","permission":"read"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(g["ok"], true, "{g}");

    // Sanity: member reads via team.
    let (_, ok) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"rm-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(ok["ok"], true, "{ok}");

    // Remove from team → loses read.
    let (_, rm) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.teams.members.remove","input":{{"slug":"rm-org","team_slug":"devs","user_id":"{}"}}}}"#,
            f.member_id
        ),
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(rm["ok"], true, "{rm}");
    let (_, gone) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"rm-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(gone["error"]["code"], "repo.not_found", "{gone}");

    // Re-add, then remove the ORG member → team row gone too (count 0).
    let (_, m2) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"rm-org","team_slug":"devs","username":"rmmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(m2["ok"], true, "{m2}");
    let (_, rmorg) = rpc_json(
        &app,
        &format!(
            r#"{{"procedure":"org.members.remove","input":{{"slug":"rm-org","user_id":"{}"}}}}"#,
            f.member_id
        ),
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(rmorg["ok"], true, "{rmorg}");
    let (_, tl) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.get","input":{"slug":"rm-org","team_slug":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(tl["data"]["member_count"], 0, "{tl}");
}

/// AC9: repo.teams.* — repo-side grant management for org repos.
#[tokio::test]
async fn repo_teams_surface() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("repo_teams.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");
    support::unlock_signup(&db).await;
    let app = test_app(db.clone(), dir.path().join("repos")).await;
    let f = fixture(&app, &db, "rt").await;

    let (_, repo_v) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"secret","visibility":"private","owner":"rt-org"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(repo_v["ok"], true, "{repo_v}");
    let (_, personal) = rpc_json(
        &app,
        r#"{"procedure":"repo.create","input":{"name":"mine","visibility":"private"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(personal["ok"], true, "{personal}");

    let (_, t) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.create","input":{"slug":"rt-org","name":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(t["ok"], true, "{t}");

    // User-owned repo: list → empty; add → org_only error.
    let (_, empty) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.list","input":{"owner":"rtowner","name":"mine"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["data"]["teams"].as_array().expect("teams").len(), 0);
    let (_, org_only) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.add","input":{"owner":"rtowner","name":"mine","team_slug":"devs","permission":"read"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(
        org_only["error"]["code"], "repo.teams.org_only",
        "{org_only}"
    );

    // Grant from repo side → member reads.
    let (_, m) = rpc_json(
        &app,
        r#"{"procedure":"org.teams.members.add","input":{"slug":"rt-org","team_slug":"devs","username":"rtmem"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(m["ok"], true, "{m}");
    let (_, add) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.add","input":{"owner":"rt-org","name":"secret","team_slug":"devs","permission":"write"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(add["ok"], true, "{add}");
    let (_, list) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.list","input":{"owner":"rt-org","name":"secret"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    let teams = list["data"]["teams"].as_array().expect("teams");
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0]["slug"], "devs");
    assert_eq!(teams[0]["permission"], "write");
    let (_, get) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"rt-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(get["data"]["can_write"], true, "{get}");

    // Update + remove from repo side.
    let (_, upd) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.update","input":{"owner":"rt-org","name":"secret","team_slug":"devs","permission":"admin"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(upd["ok"], true, "{upd}");
    let (_, rm) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.remove","input":{"owner":"rt-org","name":"secret","team_slug":"devs"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(rm["ok"], true, "{rm}");
    let (_, gone) = rpc_json(
        &app,
        r#"{"procedure":"repo.get","input":{"owner":"rt-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(gone["error"]["code"], "repo.not_found", "{gone}");

    // Team-admin on the repo can manage repo-side grants (delegation).
    let (_, add2) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.add","input":{"owner":"rt-org","name":"secret","team_slug":"devs","permission":"admin"}}"#,
        Some(&f.owner_cookie),
    )
    .await;
    assert_eq!(add2["ok"], true, "{add2}");
    let (_, memlist) = rpc_json(
        &app,
        r#"{"procedure":"repo.teams.list","input":{"owner":"rt-org","name":"secret"}}"#,
        Some(&f.member_cookie),
    )
    .await;
    assert_eq!(memlist["ok"], true, "repo admin lists teams — {memlist}");
}
