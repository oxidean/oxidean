//! Per-domain dispatch — the shell tries each domain's `dispatch` before
//! falling through to `oxidean-api-core`'s residual match. Returns `None`
//! when `procedure` is not one of this crate's names so the next dispatcher
//! can try it.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{issue, label, milestone};

macro_rules! arm {
    ($handler:expr) => {
        match $handler.await {
            Ok(v) => Some(RpcResponse::ok(v)),
            Err(e) => Some(RpcResponse::err(e)),
        }
    };
}

pub async fn dispatch(ctx: &mut RpcCtx, procedure: &str, input: Value) -> Option<RpcResponse> {
    match procedure {
        "issue.create" => arm!(issue::create(ctx, input)),
        "issue.get" => arm!(issue::get(ctx, input)),
        "issue.list" => arm!(issue::list(ctx, input)),
        "issue.update" => arm!(issue::update(ctx, input)),
        "issue.close" => arm!(issue::close(ctx, input)),
        "issue.reopen" => arm!(issue::reopen(ctx, input)),
        "issue.history" => arm!(issue::history(ctx, input)),
        "issue.delete" => arm!(issue::delete(ctx, input)),
        "issue.comments.list" => arm!(issue::comments_list(ctx, input)),
        "issue.comments.create" => arm!(issue::comments_create(ctx, input)),
        "issue.comments.update" => arm!(issue::comments_update(ctx, input)),
        "issue.comments.delete" => arm!(issue::comments_delete(ctx, input)),
        "issue.comments.history" => arm!(issue::comments_history(ctx, input)),
        "issue.labels.set" => arm!(issue::labels_set(ctx, input)),
        "issue.assignees.set" => arm!(issue::assignees_set(ctx, input)),
        "issue.milestone.set" => arm!(milestone::set_issue_milestone(ctx, input)),
        "issue.assigneeCandidates" => arm!(issue::assignee_candidates(ctx, input)),
        "issue.reactions.toggle" => arm!(issue::reactions_toggle(ctx, input)),
        "issue.links.list" => arm!(issue::links_list(ctx, input)),
        "issue.links.add" => arm!(issue::links_add(ctx, input)),
        "issue.links.remove" => arm!(issue::links_remove(ctx, input)),
        "milestone.list" => arm!(milestone::list(ctx, input)),
        "milestone.get" => arm!(milestone::get(ctx, input)),
        "milestone.create" => arm!(milestone::create(ctx, input)),
        "milestone.update" => arm!(milestone::update(ctx, input)),
        "milestone.delete" => arm!(milestone::delete(ctx, input)),
        "label.listForRepo" => arm!(label::list_for_repo(ctx, input)),
        "label.listForOrg" => arm!(label::list_for_org(ctx, input)),
        "label.create" => arm!(label::create(ctx, input)),
        "label.update" => arm!(label::update(ctx, input)),
        "label.delete" => arm!(label::delete(ctx, input)),
        _ => None,
    }
}
