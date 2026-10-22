//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

use crate::handlers;

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
        "vcs.repo_enable" => arm!(handlers::repo_enable(ctx, input)),
        "vcs.status" => arm!(handlers::status(ctx, input)),
        "vcs.changeset_create" => arm!(handlers::changeset_create(ctx, input)),
        "vcs.changeset_get" => arm!(handlers::changeset_get(ctx, input)),
        "vcs.log" => arm!(handlers::log(ctx, input)),
        "vcs.diff" => arm!(handlers::diff(ctx, input)),
        "vcs.tree" => arm!(handlers::tree(ctx, input)),
        "vcs.file_get" => arm!(handlers::file_get(ctx, input)),
        "vcs.blame" => arm!(handlers::blame(ctx, input)),
        "vcs.path_history" => arm!(handlers::path_history(ctx, input)),
        "vcs.merge_base" => arm!(handlers::merge_base(ctx, input)),
        "vcs.merge" => arm!(handlers::merge(ctx, input)),
        "vcs.rebase" => arm!(handlers::rebase(ctx, input)),
        "vcs.ref_list" => arm!(handlers::ref_list(ctx, input)),
        "vcs.ref_update" => arm!(handlers::ref_update(ctx, input)),
        "vcs.git_import" => arm!(handlers::git_import(ctx, input)),
        "vcs.git_export" => arm!(handlers::git_export(ctx, input)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// vcs.* registration completeness (BR1.2/BR1.8, SD1): every pinned name
    /// is in `PROCEDURES` with a dispatch arm and a non-`SessionOnly` token
    /// class; the two object-route aliases are classified yet absent from
    /// `PROCEDURES` and dispatch.
    #[test]
    fn vcs_procedures_registered_and_classified() {
        use oxidean_core::{classify_pat_proc, PatProc};

        assert_eq!(oxidean_core::VCS_PROCEDURES.len(), 17);
        let listed: std::collections::BTreeSet<&str> = crate::PROCEDURES.iter().copied().collect();

        let src = include_str!("dispatch.rs");
        let arms: std::collections::BTreeSet<&str> = src
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                if !line.starts_with('"') {
                    return None;
                }
                let end = line[1..].find('"')?;
                let rest = line[2 + end..].trim_start();
                rest.starts_with("=>").then(|| &line[1..1 + end])
            })
            .collect();

        for name in oxidean_core::VCS_PROCEDURES {
            assert!(listed.contains(name), "{name} missing from PROCEDURES");
            assert!(arms.contains(name), "{name} has no dispatch arm");
            assert_ne!(
                classify_pat_proc(name),
                PatProc::SessionOnly,
                "{name} must classify non-SessionOnly — only unlisted names fail closed"
            );
        }
        for alias in oxidean_core::VCS_OBJECT_ROUTE_ALIASES {
            assert!(
                !listed.contains(alias),
                "route alias {alias} must never appear in PROCEDURES"
            );
            assert!(
                !arms.contains(alias),
                "route alias {alias} must never gain a dispatch arm"
            );
            assert_ne!(
                classify_pat_proc(alias),
                PatProc::SessionOnly,
                "route alias {alias} must classify for route authorization"
            );
        }

        // Reverse closed-set (BR1.2): nothing `vcs.*` may appear in
        // PROCEDURES or gain a dispatch arm without being in the pinned set —
        // an unlisted name would classify SessionOnly yet stay invocable by
        // cookie sessions, silently widening the surface.
        let pinned: std::collections::BTreeSet<&str> =
            oxidean_core::VCS_PROCEDURES.iter().copied().collect();
        for name in listed.iter().chain(arms.iter()) {
            if name.starts_with("vcs.") {
                assert!(
                    pinned.contains(name),
                    "unlisted vcs.* name {name} reached the wire surface"
                );
            }
        }
    }

    /// Each `vcs.*` arm must call its real `handlers::<proc>` handler —
    /// no `vcs.disabled` stub may remain (u7's `dispatch_real_arms` row; a
    /// handler's own `vcs.disabled` errors are returned by the domain layer,
    /// never by the arm itself).
    #[test]
    fn vcs_dispatch_arms_call_real_handlers() {
        let src = include_str!("dispatch.rs");
        let lines: Vec<&str> = src.lines().collect();
        for name in oxidean_core::VCS_PROCEDURES {
            let needle = format!("\"{name}\" =>");
            let start = lines
                .iter()
                .position(|l| l.trim_start().starts_with(&needle))
                .unwrap_or_else(|| panic!("no dispatch arm for {name}"));
            let arm: String = lines[start..(start + 6).min(lines.len())].join("\n");
            let handler = format!("handlers::{}(ctx, input)", &name["vcs.".len()..]);
            assert!(
                arm.contains(&handler),
                "{name} arm must call the real handler ({handler}):\n{arm}"
            );
            assert!(
                !arm.contains("VcsErrorCode::Disabled"),
                "{name} arm still carries a disabled stub:\n{arm}"
            );
        }
    }
}
