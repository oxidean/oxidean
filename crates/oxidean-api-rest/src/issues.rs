//! `/api/v1/repos/{owner}/{repo}/issues/**` — issues + comments.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{
    call, call_map, ctx_for, dispatch, merge_fields, repo_ref, respond, CommentPath,
    RepoNumberPath, RepoPath,
};
use oxidean_api_core::app::AppState;
use oxidean_core::{
    CreateIssueCommentRequest, CreateIssueRequest, DeleteIssueCommentResponse, IssueCommentPublic,
    IssueCommentsListResponse, IssueHistoryResponse, IssueListRequest, IssueListResponse,
    IssuePublic, RpcResponse, UpdateIssueCommentRequest,
};

/// `PATCH /issues/{number}` body — `title`/`body` go to `issue.update`;
/// `state` fans out to `issue.close`/`issue.reopen`.
fn issue_patch_body(_: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "body": { "type": "string" },
            "state": {
                "type": "string",
                "enum": ["open", "closed"],
                "description": "`closed` → issue.close; `open` → issue.reopen",
            },
        },
        "description": "At least one of `title`, `body`, `state` is required",
    })
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues",
        tags: &["issues"],
        operation_id: "listIssues",
        summary: "List issues (`issue.list`)",
        procedure: "issue.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<IssueListRequest>),
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueListResponse>,
        )),
        mount: || get(list_issues),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues",
        tags: &["issues"],
        operation_id: "createIssue",
        summary: "Create issue (`issue.create`)",
        procedure: "issue.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateIssueRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssuePublic>,
        )),
        mount: || post(create_issue),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}",
        tags: &["issues"],
        operation_id: "getIssue",
        summary: "Get issue (`issue.get`)",
        procedure: "issue.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssuePublic>,
        )),
        mount: || get(get_issue),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/issues/{number}",
        tags: &["issues"],
        operation_id: "updateIssue",
        summary:
            "Update issue (`issue.update` + `issue.close`/`issue.reopen` when `state` is given)",
        procedure: "issue.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Json(issue_patch_body)),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssuePublic>,
        )),
        mount: || patch(update_issue),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}/timeline",
        tags: &["issues"],
        operation_id: "listIssueTimeline",
        summary: "Issue timeline (`issue.history` — title/body snapshots; partial GitHub coverage, issue #160)",
        procedure: "issue.history",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueHistoryResponse>,
        )),
        mount: || get(issue_timeline),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}/comments",
        tags: &["issues"],
        operation_id: "listIssueComments",
        summary: "Issue comments (`issue.comments.list`)",
        procedure: "issue.comments.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueCommentsListResponse>,
        )),
        mount: || get(list_issue_comments),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/comments",
        tags: &["issues"],
        operation_id: "createIssueComment",
        summary: "Comment on issue (`issue.comments.create`)",
        procedure: "issue.comments.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "number"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<CreateIssueCommentRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueCommentPublic>,
        )),
        mount: || post(create_issue_comment),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}",
        tags: &["issues"],
        operation_id: "updateIssueComment",
        summary: "Edit comment (`issue.comments.update`; author only)",
        procedure: "issue.comments.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "commentId"],
        query: None,
        body: Some(BodySpec::Rpc(
            SchemaGenerator::into_root_schema_for::<UpdateIssueCommentRequest>,
        )),
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<IssueCommentPublic>,
        )),
        mount: || patch(update_issue_comment),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}",
        tags: &["issues"],
        operation_id: "deleteIssueComment",
        summary: "Delete comment (`issue.comments.delete`)",
        procedure: "issue.comments.delete",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "number", "commentId"],
        query: None,
        body: None,
        response: Some(RespSpec::Schema(
            SchemaGenerator::subschema_for::<DeleteIssueCommentResponse>,
        )),
        mount: || delete(delete_issue_comment),
    },
    // ---- milestones ---------------------------------------------------------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/milestones",
        tags: &["issues"],
        operation_id: "issues/listMilestones",
        summary: "List milestones (`milestone.list`) — `state` filter",
        procedure: "milestone.list",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo"],
        query: Some(SchemaGenerator::into_root_schema_for::<MilestoneListQuery>),
        body: None,
        response: Some(RespSpec::Json(milestone_list_schema)),
        mount: || get(list_milestones),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/milestones",
        tags: &["issues"],
        operation_id: "issues/createMilestone",
        summary: "Create a milestone (`milestone.create`)",
        procedure: "milestone.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(milestone_body_schema)),
        response: Some(RespSpec::Json(milestone_schema)),
        mount: || post(create_milestone),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/milestones/{number}",
        tags: &["issues"],
        operation_id: "issues/getMilestone",
        summary: "Get a milestone (`milestone.get`)",
        procedure: "milestone.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(milestone_schema)),
        mount: || get(get_milestone),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/milestones/{number}",
        tags: &["issues"],
        operation_id: "issues/updateMilestone",
        summary: "Update a milestone (`milestone.update`)",
        procedure: "milestone.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(milestone_update_body_schema)),
        response: Some(RespSpec::Json(milestone_schema)),
        mount: || patch(update_milestone),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/milestones/{number}",
        tags: &["issues"],
        operation_id: "issues/deleteMilestone",
        summary: "Delete a milestone (`milestone.delete`)",
        procedure: "milestone.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_milestone),
    },
    // ---- repo label CRUD (GET list lives in repos.rs) -----------------------
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/labels",
        tags: &["issues"],
        operation_id: "issues/createLabel",
        summary: "Create a repo label (`label.create`)",
        procedure: "label.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo"],
        query: None,
        body: Some(BodySpec::Json(label_body_schema)),
        response: Some(RespSpec::Json(label_schema)),
        mount: || post(create_label),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/labels/{label_name}",
        tags: &["issues"],
        operation_id: "issues/getLabel",
        summary: "Get one label by name (`label.listForRepo`, filtered)",
        procedure: "label.listForRepo",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "label_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(label_schema)),
        mount: || get(get_label),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/labels/{label_name}",
        tags: &["issues"],
        operation_id: "issues/updateLabel",
        summary: "Update a label (`label.update`)",
        procedure: "label.update",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "label_name"],
        query: None,
        body: Some(BodySpec::Json(label_update_body_schema)),
        response: Some(RespSpec::Json(label_schema)),
        mount: || patch(update_label),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/labels/{label_name}",
        tags: &["issues"],
        operation_id: "issues/deleteLabel",
        summary: "Delete a label (`label.delete`)",
        procedure: "label.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo", "label_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_label),
    },
    // ---- issue labels -------------------------------------------------------
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}/labels",
        tags: &["issues"],
        operation_id: "issues/listLabelsOnIssue",
        summary: "List an issue's labels (`issue.get`)",
        procedure: "issue.get",
        ok: StatusCode::OK,
        anonymous: true,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(label_list_schema)),
        mount: || get(list_issue_labels),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/labels",
        tags: &["issues"],
        operation_id: "issues/addLabels",
        summary: "Add labels by name (`label.listForRepo` + `issue.labels.set`)",
        procedure: "issue.labels.set",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(issue_labels_body_schema)),
        response: Some(RespSpec::Json(label_list_schema)),
        mount: || post(add_issue_labels),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/issues/{number}/labels",
        tags: &["issues"],
        operation_id: "issues/setLabels",
        summary: "Replace an issue's labels (`issue.labels.set`)",
        procedure: "issue.labels.set",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(issue_labels_body_schema)),
        response: Some(RespSpec::Json(label_list_schema)),
        mount: || put(set_issue_labels),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/labels",
        tags: &["issues"],
        operation_id: "issues/removeAllLabels",
        summary: "Remove every label from an issue (`issue.labels.set`)",
        procedure: "issue.labels.set",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(label_list_schema)),
        mount: || delete(clear_issue_labels),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/labels/{label_name}",
        tags: &["issues"],
        operation_id: "issues/removeLabel",
        summary: "Remove one label by name (`issue.labels.set`)",
        procedure: "issue.labels.set",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number", "label_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(label_list_schema)),
        mount: || delete(remove_issue_label),
    },
    // ---- issue assignees -----------------------------------------------------
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/assignees",
        tags: &["issues"],
        operation_id: "issues/addAssignees",
        summary: "Add assignees by login (`issue.assigneeCandidates` + `issue.assignees.set`)",
        procedure: "issue.assignees.set",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(assignees_body_schema)),
        response: Some(RespSpec::Json(issue_schema)),
        mount: || post(add_assignees),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/assignees",
        tags: &["issues"],
        operation_id: "issues/removeAssignees",
        summary: "Remove assignees by login (`issue.assigneeCandidates` + `issue.labels.set`)",
        procedure: "issue.assignees.set",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(assignees_body_schema)),
        response: Some(RespSpec::Json(issue_schema)),
        mount: || delete(remove_assignees),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}",
        tags: &["issues"],
        operation_id: "issues/delete",
        summary: "Hard-delete an issue (`issue.delete`); URL is the typed confirm (Gitea)",
        procedure: "issue.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_issue),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}/links",
        tags: &["issues"],
        operation_id: "issues/listLinks",
        summary: "Linked issues/PR stubs (`issue.links.list`) — bare array",
        procedure: "issue.links.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(links_schema)),
        mount: || get(list_issue_links),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/links",
        tags: &["issues"],
        operation_id: "issues/addLink",
        summary: "Link an issue/PR stub (`issue.links.add`) — `{kind, target_number?, title?}`",
        procedure: "issue.links.add",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(add_link_body_schema)),
        response: Some(RespSpec::Json(links_schema)),
        mount: || post(add_issue_link),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/links/{link_id}",
        tags: &["issues"],
        operation_id: "issues/removeLink",
        summary: "Remove one link by id (`issue.links.remove`) → 204",
        procedure: "issue.links.remove",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo", "number", "link_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(remove_issue_link),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/issues/{number}/reactions",
        tags: &["issues"],
        operation_id: "issues/listReactions",
        summary: "Aggregated reactions on the issue (`issue.get` → `reactions`)",
        procedure: "issue.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(reactions_schema)),
        mount: || get(list_issue_reactions),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/reactions",
        tags: &["issues"],
        operation_id: "issues/createReaction",
        summary: "Toggle a reaction on the issue (`issue.reactions.toggle`) — `{content}`",
        procedure: "issue.reactions.toggle",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(reaction_body_schema)),
        response: Some(RespSpec::Json(reactions_schema)),
        mount: || post(toggle_issue_reaction),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/reactions",
        tags: &["issues"],
        operation_id: "issues/deleteReaction",
        summary: "Remove a reaction (`issue.reactions.toggle` → off) — `{content}` → 204",
        procedure: "issue.reactions.toggle",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo", "number"],
        query: None,
        body: Some(BodySpec::Json(reaction_body_schema)),
        response: None,
        mount: || delete(remove_issue_reaction),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}/reactions",
        tags: &["issues"],
        operation_id: "issues/createCommentReaction",
        summary: "Toggle a reaction on a comment (`issue.reactions.toggle`) — `{content}`",
        procedure: "issue.reactions.toggle",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "repo", "number", "comment_id"],
        query: None,
        body: Some(BodySpec::Json(reaction_body_schema)),
        response: Some(RespSpec::Json(reactions_schema)),
        mount: || post(toggle_comment_reaction),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}/reactions",
        tags: &["issues"],
        operation_id: "issues/deleteCommentReaction",
        summary: "Remove a comment reaction (`issue.reactions.toggle` → off) → 204",
        procedure: "issue.reactions.toggle",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "repo", "number", "comment_id"],
        query: None,
        body: Some(BodySpec::Json(reaction_body_schema)),
        response: None,
        mount: || delete(remove_comment_reaction),
    },
];

#[derive(serde::Serialize, Deserialize)]
pub struct IssueListQuery {
    /// `open` (default) | `closed` | `all`.
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /api/v1/repos/{owner}/{repo}/issues` (`issue.list`).
async fn list_issues(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<IssueListQuery>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        serde_json::to_value(&q).unwrap_or_else(|_| json!({})),
    );
    call(&state, &headers, "issue.list", input, StatusCode::OK).await
}

/// Resolve `assignees: ["login"]` → user ids via `issue.assigneeCandidates`.
async fn assignee_ids_for_logins(
    ctx: &mut oxidean_api_core::rpc::RpcCtx,
    owner: &str,
    repo: &str,
    logins: &[String],
) -> Result<Vec<String>, Box<Response>> {
    let resp = dispatch(ctx, "issue.assigneeCandidates", repo_ref(owner, repo)).await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (super::status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    let all = data["users"].as_array().cloned().unwrap_or_default();
    let mut ids = Vec::with_capacity(logins.len());
    for l in logins {
        match all.iter().find(|u| {
            u["username"]
                .as_str()
                .map(|n| n.eq_ignore_ascii_case(l))
                .unwrap_or(false)
        }) {
            Some(u) => ids.push(u["user_id"].as_str().unwrap_or_default().to_string()),
            None => {
                return Err(Box::new(validation_failed(&format!(
                    "assignee '{l}' is not eligible on this repository"
                ))))
            }
        }
    }
    Ok(ids)
}

/// `POST /api/v1/repos/{owner}/{repo}/issues` (`issue.create`) —
/// GitHub body: `title` (required), `body`, `milestone`, `labels`,
/// `assignees`. Labels/assignees fan out to `issue.labels.set` /
/// `issue.assignees.set` after create; the response re-reads the issue.
async fn create_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let labels: Vec<String> = body
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|l| {
                    l.as_str()
                        .map(str::to_string)
                        .or_else(|| l["name"].as_str().map(str::to_string))
                })
                .collect()
        })
        .unwrap_or_default();
    let assignees: Vec<String> = body
        .get("assignees")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|u| u.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut create_body = body.clone();
    if let Some(o) = create_body.as_object_mut() {
        o.remove("labels");
        o.remove("assignees");
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "issue.create",
        merge_fields(repo_ref(&p.owner, &p.repo), create_body),
    )
    .await;
    let number = match &resp {
        RpcResponse::Ok { data, .. } => match data["number"].as_i64() {
            Some(n) => n,
            None => return respond(resp, StatusCode::CREATED),
        },
        RpcResponse::Err { .. } => return respond(resp, StatusCode::CREATED),
    };
    if !labels.is_empty() {
        let ids = match label_ids_for_names(&state, &headers, &p.owner, &p.repo, &labels).await {
            Ok(ids) => ids,
            Err(res) => return *res,
        };
        let resp = dispatch(
            &mut ctx,
            "issue.labels.set",
            merge_fields(
                repo_ref(&p.owner, &p.repo),
                json!({ "number": number, "label_ids": ids }),
            ),
        )
        .await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::CREATED);
        }
    }
    if !assignees.is_empty() {
        let ids = match assignee_ids_for_logins(&mut ctx, &p.owner, &p.repo, &assignees).await {
            Ok(ids) => ids,
            Err(res) => return *res,
        };
        let resp = dispatch(
            &mut ctx,
            "issue.assignees.set",
            merge_fields(
                repo_ref(&p.owner, &p.repo),
                json!({ "number": number, "user_ids": ids }),
            ),
        )
        .await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::CREATED);
        }
    }
    call(
        &state,
        &headers,
        "issue.get",
        json!({ "owner": p.owner, "name": p.repo, "number": number }),
        StatusCode::CREATED,
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/issues/{number}` (`issue.get`).
async fn get_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(&state, &headers, "issue.get", p.ref_input(), StatusCode::OK).await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/issues/{number}` — `issue.update`
/// for `title`/`body`/`milestone`; `state: "closed"|"open"` maps to
/// `issue.close`/`issue.reopen`; `labels`/`assignees` fan out to
/// `issue.labels.set`/`issue.assignees.set`. Any may combine in one request.
async fn update_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let labels: Option<Vec<String>> = body.get("labels").and_then(|v| v.as_array()).map(|a| {
        a.iter()
            .filter_map(|l| {
                l.as_str()
                    .map(str::to_string)
                    .or_else(|| l["name"].as_str().map(str::to_string))
            })
            .collect()
    });
    let assignees: Option<Vec<String>> =
        body.get("assignees").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|u| u.as_str().map(str::to_string))
                .collect()
        });

    let mut update_body = body.clone();
    if let Some(o) = update_body.as_object_mut() {
        o.remove("labels");
        o.remove("assignees");
    }
    let has_fields = update_body.get("title").is_some()
        || update_body.get("body").is_some()
        || update_body.get("milestone").is_some();
    let state_change = body.get("state").and_then(|v| v.as_str());
    if !has_fields && state_change.is_none() && labels.is_none() && assignees.is_none() {
        return bad_input(
            "PATCH requires at least one of: title, body, milestone, state, labels, assignees",
        );
    }
    if let Some(other) = state_change {
        if !matches!(other, "open" | "closed") {
            return bad_input(&format!("invalid state: {other} (expected open|closed)"));
        }
    }

    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let mut last: Option<RpcResponse> = None;
    if has_fields {
        let input = merge_fields(p.ref_input(), update_body);
        let resp = dispatch(&mut ctx, "issue.update", input).await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::OK);
        }
        last = Some(resp);
    }
    match state_change {
        Some("closed") => {
            let resp = dispatch(&mut ctx, "issue.close", p.ref_input()).await;
            if matches!(resp, RpcResponse::Err { .. }) {
                return respond(resp, StatusCode::OK);
            }
            last = Some(resp);
        }
        Some("open") => {
            let resp = dispatch(&mut ctx, "issue.reopen", p.ref_input()).await;
            if matches!(resp, RpcResponse::Err { .. }) {
                return respond(resp, StatusCode::OK);
            }
            last = Some(resp);
        }
        _ => {}
    }
    if let Some(names) = labels {
        let ids = match label_ids_for_names(&state, &headers, &p.owner, &p.repo, &names).await {
            Ok(ids) => ids,
            Err(res) => return *res,
        };
        let resp = dispatch(
            &mut ctx,
            "issue.labels.set",
            merge_fields(p.ref_input(), json!({ "label_ids": ids })),
        )
        .await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::OK);
        }
        last = None;
    }
    if let Some(logins) = assignees {
        let ids = match assignee_ids_for_logins(&mut ctx, &p.owner, &p.repo, &logins).await {
            Ok(ids) => ids,
            Err(res) => return *res,
        };
        let resp = dispatch(
            &mut ctx,
            "issue.assignees.set",
            merge_fields(p.ref_input(), json!({ "user_ids": ids })),
        )
        .await;
        if matches!(resp, RpcResponse::Err { .. }) {
            return respond(resp, StatusCode::OK);
        }
        last = None;
    }
    // Labels/assignees set procs answer `{ok}` — re-read for the full DTO.
    match last {
        Some(resp) => respond(resp, StatusCode::OK),
        None => {
            let resp = dispatch(&mut ctx, "issue.get", p.ref_input()).await;
            respond(resp, StatusCode::OK)
        }
    }
}

fn bad_input(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "code": "rpc.bad_input", "message": message })),
    )
        .into_response()
}

/// `GET /api/v1/repos/{owner}/{repo}/issues/{number}/timeline`
/// (`issue.history`) — GitHub timeline backed by Oxidean's title/body
/// revision snapshots; partial coverage vs GitHub's labelled-event feed
/// (issue #160).
async fn issue_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "issue.history",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `GET /api/v1/repos/{owner}/{repo}/issues/{number}/comments`
/// (`issue.comments.list`).
async fn list_issue_comments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "issue.comments.list",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

/// `POST /api/v1/repos/{owner}/{repo}/issues/{number}/comments`
/// (`issue.comments.create`) — body: `body` (required).
async fn create_issue_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(p.ref_input(), body);
    call(
        &state,
        &headers,
        "issue.comments.create",
        input,
        StatusCode::CREATED,
    )
    .await
}

/// `PATCH /api/v1/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}`
/// (`issue.comments.update`) — body: `body` (required).
async fn update_issue_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CommentPath>,
    Json(body): Json<Value>,
) -> Response {
    let fields = json!({
        "owner": p.owner,
        "name": p.repo,
        "number": p.number,
        "commentId": p.comment_id,
    });
    let input = merge_fields(fields, body);
    call(
        &state,
        &headers,
        "issue.comments.update",
        input,
        StatusCode::OK,
    )
    .await
}

/// `DELETE /api/v1/repos/{owner}/{repo}/issues/{number}/comments/{comment_id}`
/// (`issue.comments.delete`).
async fn delete_issue_comment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CommentPath>,
) -> Response {
    let input = json!({
        "owner": p.owner,
        "name": p.repo,
        "number": p.number,
        "commentId": p.comment_id,
    });
    call(
        &state,
        &headers,
        "issue.comments.delete",
        input,
        StatusCode::OK,
    )
    .await
}

// ---------------------------------------------------------------------------
// Milestones / labels / issue label+assignee sub-ops (GitHub Issues REST)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LabelNamePath {
    owner: String,
    repo: String,
    label_name: String,
}

#[derive(Deserialize)]
pub struct IssueLabelPath {
    owner: String,
    repo: String,
    number: i64,
    label_name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct MilestoneListQuery {
    /// `open` | `closed` | `all`.
    #[serde(default)]
    state: Option<String>,
}

fn milestone_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn milestone_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn milestone_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["title"],
        "properties": {
            "title": {"type": "string"},
            "description": {"type": "string"},
            "due_on": {"type": "string", "description": "YYYY-MM-DD"},
        },
    })
}

fn milestone_update_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": {"type": "string"},
            "description": {"type": "string"},
            "due_on": {"type": ["string", "null"]},
            "state": {"type": "string", "enum": ["open", "closed"]},
        },
    })
}

fn label_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn label_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn label_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["name"],
        "properties": {
            "name": {"type": "string"},
            "color": {"type": "string"},
            "description": {"type": "string"},
        },
    })
}

fn label_update_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "new_name": {"type": "string"},
            "name": {"type": "string"},
            "color": {"type": "string"},
            "description": {"type": "string"},
        },
    })
}

fn issue_labels_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["labels"],
        "properties": {
            "labels": {"type": "array", "items": {"type": "string"}},
        },
    })
}

fn assignees_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["assignees"],
        "properties": {
            "assignees": {"type": "array", "items": {"type": "string"}},
        },
    })
}

fn issue_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

/// `LabelPublic` → GitHub's label object (id stays a string).
fn gh_label(l: &Value) -> Value {
    json!({
        "id": l["id"],
        "name": l["name"],
        "color": l["color"],
        "description": l["description"],
        "default": false,
        "url": l["url"],
    })
}

fn validation_failed(msg: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"message": "Validation Failed", "errors": [msg]})),
    )
        .into_response()
}

/// Resolve the repo's label name → row (the RPC `label.*` mutations key by
/// id; GitHub's REST keys labels by name). Returns Err(response) on failure.
async fn find_label(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    label_name: &str,
) -> Result<Value, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    let resp = dispatch(&mut ctx, "label.listForRepo", repo_ref(owner, repo)).await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (super::status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    data["labels"]
        .as_array()
        .and_then(|ls| {
            ls.iter()
                .find(|l| l["name"].as_str() == Some(label_name))
                .cloned()
        })
        .ok_or_else(|| {
            Box::new(
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "label.not_found", "message": "label not found"})),
                )
                    .into_response(),
            )
        })
}

/// Resolve GitHub `labels: [names]` to label row ids — unknown names 422
/// like GitHub's validation failure.
async fn label_ids_for_names(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    names: &[String],
) -> Result<Vec<String>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    let resp = dispatch(&mut ctx, "label.listForRepo", repo_ref(owner, repo)).await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (super::status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    let all = data["labels"].as_array().cloned().unwrap_or_default();
    let mut ids = Vec::with_capacity(names.len());
    for n in names {
        match all.iter().find(|l| l["name"].as_str() == Some(n.as_str())) {
            Some(l) => ids.push(l["id"].as_str().unwrap_or_default().to_string()),
            None => {
                return Err(Box::new(validation_failed(&format!(
                    "label '{n}' does not exist in this repository"
                ))))
            }
        }
    }
    Ok(ids)
}

/// Re-read the issue's labels after a set — the response is GitHub's label
/// array (the set proc answers `{ok}`, not the new state).
async fn issue_labels_response(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    number: i64,
) -> Result<Vec<Value>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    let resp = dispatch(
        &mut ctx,
        "issue.get",
        json!({"owner": owner, "name": repo, "number": number}),
    )
    .await;
    match resp {
        RpcResponse::Ok { data, .. } => Ok(data["labels"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(gh_label)
            .collect()),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

/// Current issue label ids + assignee ids for union/minus composition.
async fn issue_substate(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    number: i64,
) -> Result<Value, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    let resp = dispatch(
        &mut ctx,
        "issue.get",
        json!({"owner": owner, "name": repo, "number": number}),
    )
    .await;
    match resp {
        RpcResponse::Ok { data, .. } => Ok(data),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (super::status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

// ---- milestone handlers -----------------------------------------------------

async fn list_milestones(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Query(q): Query<MilestoneListQuery>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    if let Some(s) = q.state {
        input["state"] = json!(s);
    }
    call_map(
        &state,
        &headers,
        "milestone.list",
        input,
        StatusCode::OK,
        |d| d["milestones"].clone(),
    )
    .await
}

async fn create_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    call(
        &state,
        &headers,
        "milestone.create",
        merge_fields(repo_ref(&p.owner, &p.repo), body),
        StatusCode::CREATED,
    )
    .await
}

async fn get_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "milestone.get",
        p.ref_input(),
        StatusCode::OK,
    )
    .await
}

async fn update_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    call(
        &state,
        &headers,
        "milestone.update",
        merge_fields(p.ref_input(), body),
        StatusCode::OK,
    )
    .await
}

async fn delete_milestone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "milestone.delete",
        p.ref_input(),
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- repo label handlers ----------------------------------------------------

async fn create_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(
        json!({"scope": "repo", "owner": p.owner, "repo": p.repo}),
        body,
    );
    call_map(
        &state,
        &headers,
        "label.create",
        input,
        StatusCode::CREATED,
        |d| gh_label(&d),
    )
    .await
}

async fn get_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<LabelNamePath>,
) -> Response {
    match find_label(&state, &headers, &p.owner, &p.repo, &p.label_name).await {
        Ok(l) => (StatusCode::OK, Json(gh_label(&l))).into_response(),
        Err(r) => *r,
    }
}

async fn update_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<LabelNamePath>,
    Json(body): Json<Value>,
) -> Response {
    let label = match find_label(&state, &headers, &p.owner, &p.repo, &p.label_name).await {
        Ok(l) => l,
        Err(r) => return *r,
    };
    let mut body = body;
    // GitHub's rename field is `new_name`; the RPC rename field is `name`.
    if let Some(n) = body.get("new_name").and_then(Value::as_str) {
        body["name"] = json!(n);
        if let Some(m) = body.as_object_mut() {
            m.remove("new_name");
        }
    }
    let input = merge_fields(
        json!({"id": label["id"], "owner": p.owner, "repo": p.repo}),
        body,
    );
    call_map(
        &state,
        &headers,
        "label.update",
        input,
        StatusCode::OK,
        |d| gh_label(&d),
    )
    .await
}

async fn delete_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<LabelNamePath>,
) -> Response {
    let label = match find_label(&state, &headers, &p.owner, &p.repo, &p.label_name).await {
        Ok(l) => l,
        Err(r) => return *r,
    };
    call(
        &state,
        &headers,
        "label.delete",
        json!({"id": label["id"], "owner": p.owner, "repo": p.repo}),
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- issue label handlers ---------------------------------------------------

async fn list_issue_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    match issue_labels_response(&state, &headers, &p.owner, &p.repo, p.number).await {
        Ok(labels) => (StatusCode::OK, Json(json!(labels))).into_response(),
        Err(r) => *r,
    }
}

/// Shared set — `ids` is the resulting label-id set; the response is the
/// issue's post-write labels (GitHub answers the label array).
async fn set_issue_labels_inner(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    number: i64,
    ids: Vec<String>,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let resp = dispatch(
        &mut ctx,
        "issue.labels.set",
        json!({"owner": owner, "name": repo, "number": number, "labelIds": ids}),
    )
    .await;
    match resp {
        RpcResponse::Ok { .. } => {}
        RpcResponse::Err { error, .. } => {
            return (super::status_for_error(&error), Json(error)).into_response()
        }
    }
    match issue_labels_response(state, headers, owner, repo, number).await {
        Ok(labels) => (StatusCode::OK, Json(json!(labels))).into_response(),
        Err(r) => *r,
    }
}

fn requested_label_names(body: &Value) -> Result<Vec<String>, Box<Response>> {
    let names = body["labels"].as_array().map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    });
    match names {
        Some(n) => Ok(n),
        None => Err(Box::new(validation_failed(
            "`labels` must be an array of names",
        ))),
    }
}

async fn add_issue_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let names = match requested_label_names(&body) {
        Ok(n) => n,
        Err(r) => return *r,
    };
    let new_ids = match label_ids_for_names(&state, &headers, &p.owner, &p.repo, &names).await {
        Ok(ids) => ids,
        Err(r) => return *r,
    };
    let issue = match issue_substate(&state, &headers, &p.owner, &p.repo, p.number).await {
        Ok(i) => i,
        Err(r) => return *r,
    };
    let mut ids: Vec<String> = issue["labels"]
        .as_array()
        .map(|ls| {
            ls.iter()
                .filter_map(|l| l["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for id in new_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    set_issue_labels_inner(&state, &headers, &p.owner, &p.repo, p.number, ids).await
}

async fn set_issue_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let names = match requested_label_names(&body) {
        Ok(n) => n,
        Err(r) => return *r,
    };
    let ids = match label_ids_for_names(&state, &headers, &p.owner, &p.repo, &names).await {
        Ok(ids) => ids,
        Err(r) => return *r,
    };
    set_issue_labels_inner(&state, &headers, &p.owner, &p.repo, p.number, ids).await
}

async fn clear_issue_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    set_issue_labels_inner(&state, &headers, &p.owner, &p.repo, p.number, vec![]).await
}

async fn remove_issue_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<IssueLabelPath>,
) -> Response {
    let issue = match issue_substate(&state, &headers, &p.owner, &p.repo, p.number).await {
        Ok(i) => i,
        Err(r) => return *r,
    };
    let labels = issue["labels"].as_array().cloned().unwrap_or_default();
    let target = labels
        .iter()
        .find(|l| l["name"].as_str() == Some(p.label_name.as_str()));
    let Some(t) = target else {
        // GitHub 404s when the label isn't on the issue.
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "label.not_found", "message": "label is not on this issue"})),
        )
            .into_response();
    };
    let ids: Vec<String> = labels
        .iter()
        .filter(|l| l["id"] != t["id"])
        .filter_map(|l| l["id"].as_str().map(str::to_string))
        .collect();
    set_issue_labels_inner(&state, &headers, &p.owner, &p.repo, p.number, ids).await
}

// ---- issue assignee handlers ------------------------------------------------

/// Login → user id via `issue.assigneeCandidates`; unknown logins 422.
async fn ids_for_assignees(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    logins: &[String],
) -> Result<Vec<String>, Box<Response>> {
    let mut ctx = ctx_for(state, headers).await?;
    let resp = dispatch(&mut ctx, "issue.assigneeCandidates", repo_ref(owner, repo)).await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return Err(Box::new(
                (super::status_for_error(&error), Json(error)).into_response(),
            ))
        }
    };
    let users = data["users"].as_array().cloned().unwrap_or_default();
    let mut ids = Vec::with_capacity(logins.len());
    for l in logins {
        match users
            .iter()
            .find(|u| u["username"].as_str() == Some(l.as_str()))
        {
            Some(u) => ids.push(u["user_id"].as_str().unwrap_or_default().to_string()),
            None => {
                return Err(Box::new(validation_failed(&format!(
                    "assignee '{l}' cannot be assigned to this repository's issues"
                ))))
            }
        }
    }
    Ok(ids)
}

async fn set_assignees_inner(
    state: &AppState,
    headers: &HeaderMap,
    owner: &str,
    repo: &str,
    number: i64,
    user_ids: Vec<String>,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let resp = dispatch(
        &mut ctx,
        "issue.assignees.set",
        json!({"owner": owner, "name": repo, "number": number, "userIds": user_ids}),
    )
    .await;
    match resp {
        RpcResponse::Ok { .. } => {}
        RpcResponse::Err { error, .. } => {
            return (super::status_for_error(&error), Json(error)).into_response()
        }
    }
    // GitHub answers the updated issue.
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(
        &mut ctx,
        "issue.get",
        json!({"owner": owner, "name": repo, "number": number}),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => (StatusCode::OK, Json(data)).into_response(),
        RpcResponse::Err { error, .. } => {
            (super::status_for_error(&error), Json(error)).into_response()
        }
    }
}

fn requested_assignees(body: &Value) -> Result<Vec<String>, Box<Response>> {
    match body["assignees"].as_array().map(|a| {
        a.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    }) {
        Some(a) => Ok(a),
        None => Err(Box::new(validation_failed(
            "`assignees` must be an array of logins",
        ))),
    }
}

async fn add_assignees(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let logins = match requested_assignees(&body) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let new_ids = match ids_for_assignees(&state, &headers, &p.owner, &p.repo, &logins).await {
        Ok(ids) => ids,
        Err(r) => return *r,
    };
    let issue = match issue_substate(&state, &headers, &p.owner, &p.repo, p.number).await {
        Ok(i) => i,
        Err(r) => return *r,
    };
    let mut ids: Vec<String> = issue["assignees"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|u| u["user_id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    for id in new_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let res = set_assignees_inner(&state, &headers, &p.owner, &p.repo, p.number, ids).await;
    // GitHub answers 201 for add.
    if res.status() == StatusCode::OK {
        let (parts, body) = res.into_parts();
        let bytes = axum::body::to_bytes(body, 1 << 22)
            .await
            .unwrap_or_default();
        (StatusCode::CREATED, parts.headers, bytes).into_response()
    } else {
        res
    }
}

async fn remove_assignees(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    let logins = match requested_assignees(&body) {
        Ok(a) => a,
        Err(r) => return *r,
    };
    let drop_ids = match ids_for_assignees(&state, &headers, &p.owner, &p.repo, &logins).await {
        Ok(ids) => ids,
        Err(r) => return *r,
    };
    let issue = match issue_substate(&state, &headers, &p.owner, &p.repo, p.number).await {
        Ok(i) => i,
        Err(r) => return *r,
    };
    let ids: Vec<String> = issue["assignees"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|u| u["user_id"].as_str().map(str::to_string))
                .filter(|id| !drop_ids.contains(id))
                .collect()
        })
        .unwrap_or_default();
    set_assignees_inner(&state, &headers, &p.owner, &p.repo, p.number, ids).await
}

#[derive(Deserialize)]
struct LinkPath {
    owner: String,
    repo: String,
    number: i64,
    link_id: String,
}

/// `DELETE /repos/{o}/{r}/issues/{n}` — Gitea issue delete; the path
/// `number` is the typed confirmation.
async fn delete_issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call(
        &state,
        &headers,
        "issue.delete",
        merge_fields(p.ref_input(), json!({"confirm_number": p.number})),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `GET /repos/{o}/{r}/issues/{n}/links` — bare array.
async fn list_issue_links(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "issue.links.list",
        p.ref_input(),
        StatusCode::OK,
        |d| d["links"].clone(),
    )
    .await
}

/// `POST /repos/{o}/{r}/issues/{n}/links` — `{kind, target_number?,
/// target_repo_id?, title?}` → created link row.
async fn add_issue_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    call_map(
        &state,
        &headers,
        "issue.links.add",
        merge_fields(p.ref_input(), body),
        StatusCode::CREATED,
        |d| d,
    )
    .await
}

/// `DELETE /repos/{o}/{r}/issues/{n}/links/{link_id}` → 204.
async fn remove_issue_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<LinkPath>,
) -> Response {
    call(
        &state,
        &headers,
        "issue.links.remove",
        merge_fields(
            repo_ref(&p.owner, &p.repo),
            json!({"number": p.number, "link_id": p.link_id}),
        ),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// `GET /repos/{o}/{r}/issues/{n}/reactions` — aggregated groups off
/// `issue.get` (the issue embeds them).
async fn list_issue_reactions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
) -> Response {
    call_map(
        &state,
        &headers,
        "issue.get",
        p.ref_input(),
        StatusCode::OK,
        |d| d["reactions"].clone(),
    )
    .await
}

/// Reaction-toggle input builder shared by the issue/comment POST+DELETE.
fn reaction_input(
    p: &RepoNumberPath,
    target: &str,
    comment_id: Option<&str>,
    body: &Value,
) -> Value {
    let mut input = merge_fields(
        p.ref_input(),
        json!({"target": target, "content": body["content"]}),
    );
    if let Some(id) = comment_id {
        input["comment_id"] = json!(id);
    }
    input
}

/// Force-off semantics for DELETE: toggle once; when the result reports
/// `reacted == true` (the caller had not reacted — toggle added it), toggle
/// again so the end state is always "reaction absent".
async fn remove_reaction_inner(state: &AppState, headers: &HeaderMap, input: Value) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let first = match dispatch(&mut ctx, "issue.reactions.toggle", input.clone()).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (super::status_for_error(&error), Json(error)).into_response();
        }
    };
    if first["reacted"].as_bool().unwrap_or(false) {
        match dispatch(&mut ctx, "issue.reactions.toggle", input).await {
            RpcResponse::Ok { .. } => {}
            RpcResponse::Err { error, .. } => {
                return (super::status_for_error(&error), Json(error)).into_response();
            }
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// `POST /repos/{o}/{r}/issues/{n}/reactions` — toggle `{content}` → 201
/// with the aggregate result.
async fn toggle_issue_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    call_map(
        &state,
        &headers,
        "issue.reactions.toggle",
        reaction_input(&p, "issue", None, &body),
        StatusCode::CREATED,
        |d| d,
    )
    .await
}

/// `DELETE /repos/{o}/{r}/issues/{n}/reactions` — `{content}` → force-off → 204.
async fn remove_issue_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<RepoNumberPath>,
    Json(body): Json<Value>,
) -> Response {
    remove_reaction_inner(&state, &headers, reaction_input(&p, "issue", None, &body)).await
}

/// `POST /repos/{o}/{r}/issues/{n}/comments/{id}/reactions` — toggle → 201.
async fn toggle_comment_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CommentPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"number": p.number, "target": "comment", "comment_id": p.comment_id, "content": body["content"]}),
    );
    call_map(
        &state,
        &headers,
        "issue.reactions.toggle",
        input,
        StatusCode::CREATED,
        |d| d,
    )
    .await
}

/// `DELETE /repos/{o}/{r}/issues/{n}/comments/{id}/reactions` → 204.
async fn remove_comment_reaction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(p): Path<CommentPath>,
    Json(body): Json<Value>,
) -> Response {
    let input = merge_fields(
        repo_ref(&p.owner, &p.repo),
        json!({"number": p.number, "target": "comment", "comment_id": p.comment_id, "content": body["content"]}),
    );
    remove_reaction_inner(&state, &headers, input).await
}

fn links_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {"type": "object", "properties": {
            "id": {"type": "string"},
            "kind": {"type": "string", "enum": ["issue", "pr_stub", "pr"]},
            "target_number": {"type": ["integer", "null"]},
            "target_repo_id": {"type": ["string", "null"]},
            "title": {"type": ["string", "null"]},
            "created_at": {"type": "string"}
        }}
    })
}

fn add_link_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["kind"],
        "properties": {
            "kind": {"type": "string", "enum": ["issue", "pr_stub", "pr"]},
            "target_number": {"type": "integer"},
            "target_repo_id": {"type": "string"},
            "title": {"type": "string"}
        }
    })
}

fn reactions_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "array",
        "items": {"type": "object", "properties": {
            "content": {"type": "string"},
            "count": {"type": "integer"},
            "viewerHasReacted": {"type": "boolean"}
        }}
    })
}

fn reaction_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["content"],
        "properties": {"content": {"type": "string"}}
    })
}
