//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::{comments, lifecycle, merge_ops, reviews, update_branch};

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
        // `pull.create` stays in core — `repo::files` create-PR flow and the
        // webhook path share the handler across domain boundaries (#162).
        "pull.create" => arm!(oxidean_api_core::pull::create(ctx, input)),
        "pull.get" => arm!(lifecycle::get(ctx, input)),
        "pull.list" => arm!(lifecycle::list(ctx, input)),
        "pull.update" => arm!(lifecycle::update(ctx, input)),
        "pull.close" => arm!(lifecycle::close(ctx, input)),
        "pull.reopen" => arm!(lifecycle::reopen(ctx, input)),
        "pull.files" => arm!(merge_ops::files(ctx, input)),
        "pull.commits" => arm!(merge_ops::commits(ctx, input)),
        "pull.comments.list" => arm!(comments::comments_list(ctx, input)),
        "pull.comments.create" => arm!(comments::comments_create(ctx, input)),
        "pull.comments.update" => arm!(comments::comments_update(ctx, input)),
        "pull.comments.delete" => arm!(comments::comments_delete(ctx, input)),
        "pull.comments.history" => arm!(comments::comments_history(ctx, input)),
        "pull.comments.resolve" => arm!(comments::comments_resolve(ctx, input)),
        "pull.reviews.list" => arm!(reviews::reviews_list(ctx, input)),
        "pull.reviews.submit" => arm!(reviews::reviews_submit(ctx, input)),
        "pull.reviews.dismiss" => arm!(reviews::reviews_dismiss(ctx, input)),
        "pull.reviewRequests.list" => arm!(reviews::review_requests_list(ctx, input)),
        "pull.reviewRequests.add" => arm!(reviews::review_requests_add(ctx, input)),
        "pull.reviewRequests.remove" => arm!(reviews::review_requests_remove(ctx, input)),
        "pull.merge" => arm!(merge_ops::merge(ctx, input)),
        "pull.branchStatus" => arm!(update_branch::branch_status(ctx, input)),
        "pull.updateBranch" => arm!(update_branch::update_branch(ctx, input)),
        "repo.mergeSettings.get" => arm!(merge_ops::merge_settings_get(ctx, input)),
        "repo.mergeSettings.update" => arm!(merge_ops::merge_settings_update(ctx, input)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// `PROCEDURES` must exactly mirror the `dispatch` match arms — same
    /// source-scan approach as `oxidean-api-core::rpc`'s parity test.
    #[test]
    fn procedures_list_matches_dispatch() {
        let src = include_str!("dispatch.rs");
        let mut arms: Vec<&str> = Vec::new();
        for line in src.lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            if let Some(end) = line[1..].find('"') {
                let name = &line[1..1 + end];
                let rest = line[2 + end..].trim_start();
                if rest.starts_with("=>") {
                    arms.push(name);
                }
            }
        }
        let listed: std::collections::BTreeSet<&str> = crate::PROCEDURES.iter().copied().collect();
        for arm in &arms {
            assert!(
                listed.contains(arm),
                "dispatch arm `{arm}` missing from PROCEDURES"
            );
        }
        for name in &listed {
            assert!(
                arms.contains(name),
                "PROCEDURES entry `{name}` has no dispatch arm"
            );
        }
    }
}
