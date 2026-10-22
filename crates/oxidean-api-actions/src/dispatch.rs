//! Per-domain dispatch — see `oxidean-api-issues` for the contract. `None`
//! means the name is not this crate's; the shell tries the next dispatcher.

use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::RpcResponse;
use serde_json::Value;

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
        // Repo-template handlers stay in core — `templates` shares its
        // embedded assets with `runner_proto` (#162).
        "repo.templates.getEnabled" => arm!(
            oxidean_api_core::templates::handlers::repo_get_enabled(ctx, input)
        ),
        "repo.templates.setEnabled" => arm!(
            oxidean_api_core::templates::handlers::repo_set_enabled(ctx, input)
        ),
        // Workflow-template handlers stay in core — `actions::templates` shares
        // the vendored catalog + `template_icon` with `runner_proto` (#162).
        "admin.workflowTemplates.create" => arm!(
            oxidean_api_core::actions::templates::admin_create(ctx, input)
        ),
        "admin.workflowTemplates.delete" => arm!(
            oxidean_api_core::actions::templates::admin_delete(ctx, input)
        ),
        "admin.workflowTemplates.list" => {
            arm!(oxidean_api_core::actions::templates::admin_list(ctx))
        }
        "admin.workflowTemplates.setEnabled" => arm!(
            oxidean_api_core::actions::templates::admin_set_enabled(ctx, input)
        ),
        "admin.workflowTemplates.update" => arm!(
            oxidean_api_core::actions::templates::admin_update(ctx, input)
        ),
        "repo.actions.workflowTemplates.get" => arm!(
            oxidean_api_core::actions::templates::get_template(ctx, input)
        ),
        "repo.actions.workflowTemplates.list" => arm!(
            oxidean_api_core::actions::templates::list_templates(ctx, input)
        ),
        "repo.actions.workflows.create" => arm!(
            oxidean_api_core::actions::templates::create_workflow(ctx, input)
        ),
        "admin.actions.createRegistrationToken" => {
            arm!(crate::rpc::admin_create_registration_token(ctx, input))
        }
        "admin.actions.listRunners" => arm!(crate::rpc::admin_list_runners(ctx, input)),
        "repo.actions.approveJob" => arm!(crate::rpc::approve_job(ctx, input)),
        "repo.actions.approveRun" => arm!(crate::rpc::approve_run(ctx, input)),
        "repo.actions.cancelRun" => arm!(crate::rpc::cancel_run(ctx, input)),
        "repo.actions.createRegistrationToken" => {
            arm!(crate::rpc::create_registration_token(ctx, input))
        }
        "repo.actions.createRemoveToken" => arm!(crate::rpc::create_remove_token(ctx, input)),
        "repo.actions.deleteArtifact" => arm!(crate::rpc::delete_artifact(ctx, input)),
        "repo.actions.deleteCache" => arm!(crate::rpc::delete_cache(ctx, input)),
        "repo.actions.deleteRun" => arm!(crate::rpc::delete_run(ctx, input)),
        "repo.actions.deleteRunLogs" => arm!(crate::rpc::delete_run_logs(ctx, input)),
        "repo.actions.deleteRunner" => arm!(crate::rpc::delete_runner(ctx, input)),
        "repo.actions.dispatchRepository" => arm!(crate::rpc::dispatch_repository(ctx, input)),
        "repo.actions.dispatchWorkflow" => arm!(crate::rpc::dispatch_workflow(ctx, input)),
        "repo.actions.environments.delete" => arm!(crate::rpc::delete_environment(ctx, input)),
        "repo.actions.environments.get" => arm!(crate::rpc::get_environment(ctx, input)),
        "repo.actions.environments.list" => arm!(crate::rpc::list_environments(ctx, input)),
        "repo.actions.environments.secretsDelete" => {
            arm!(crate::rpc::delete_environment_secret(ctx, input))
        }
        "repo.actions.environments.secretsList" => {
            arm!(crate::rpc::list_environment_secrets(ctx, input))
        }
        "repo.actions.environments.secretsPublicKey" => {
            arm!(crate::rpc::environment_secret_public_key(ctx, input))
        }
        "repo.actions.environments.secretsPut" => {
            arm!(crate::rpc::put_environment_secret(ctx, input))
        }
        "repo.actions.environments.upsert" => arm!(crate::rpc::upsert_environment(ctx, input)),
        "repo.actions.environments.variablesDelete" => {
            arm!(crate::rpc::delete_environment_variable(ctx, input))
        }
        "repo.actions.environments.variablesList" => {
            arm!(crate::rpc::list_environment_variables(ctx, input))
        }
        "repo.actions.environments.variablesPut" => {
            arm!(crate::rpc::put_environment_variable(ctx, input))
        }
        "repo.actions.getArtifact" => arm!(crate::rpc::get_artifact(ctx, input)),
        "repo.actions.getDefaultPermissions" => {
            arm!(crate::rpc::get_default_permissions(ctx, input))
        }
        "repo.actions.getEnabled" => arm!(crate::rpc::get_enabled(ctx, input)),
        "repo.actions.getForkPrApproval" => arm!(crate::rpc::get_fork_pr_approval(ctx, input)),
        "repo.actions.getJob" => arm!(crate::rpc::get_job(ctx, input)),
        "repo.actions.getJobLog" => arm!(crate::rpc::get_job_log(ctx, input)),
        "repo.actions.getPullRequestTarget" => {
            arm!(crate::rpc::get_pull_request_target(ctx, input))
        }
        "repo.actions.getRun" => arm!(crate::rpc::get_run(ctx, input)),
        "repo.actions.getRunner" => arm!(crate::rpc::get_runner(ctx, input)),
        "repo.actions.jobAnnotations" => arm!(crate::rpc::job_annotations(ctx, input)),
        "repo.actions.listAllArtifacts" => arm!(crate::rpc::list_all_artifacts(ctx, input)),
        "repo.actions.listArtifacts" => arm!(crate::rpc::list_artifacts(ctx, input)),
        "repo.actions.listCaches" => arm!(crate::rpc::list_caches(ctx, input)),
        "repo.actions.listDeployments" => arm!(crate::rpc::list_deployments(ctx, input)),
        "repo.actions.listRunApprovals" => arm!(crate::rpc::list_run_approvals(ctx, input)),
        "repo.actions.listRunners" => arm!(crate::rpc::list_runners(ctx, input)),
        "repo.actions.listRuns" => arm!(crate::rpc::list_runs(ctx, input)),
        "repo.actions.listWorkflows" => arm!(crate::rpc::list_workflows(ctx, input)),
        "repo.actions.organizationSecrets" => arm!(crate::rpc::organization_secrets(ctx, input)),
        "repo.actions.organizationVariables" => {
            arm!(crate::rpc::organization_variables(ctx, input))
        }
        "repo.actions.rerunRun" => arm!(crate::rpc::rerun_run(ctx, input)),
        "repo.actions.reviewDeployment" => arm!(crate::rpc::review_deployment(ctx, input)),
        "repo.actions.secrets.delete" => arm!(crate::rpc::delete_secret(ctx, input)),
        "repo.actions.secrets.list" => arm!(crate::rpc::list_secrets(ctx, input)),
        "repo.actions.secrets.publicKey" => arm!(crate::rpc::repo_secret_public_key(ctx, input)),
        "repo.actions.secrets.put" => arm!(crate::rpc::put_secret(ctx, input)),
        "repo.actions.setDefaultPermissions" => {
            arm!(crate::rpc::set_default_permissions(ctx, input))
        }
        "repo.actions.setEnabled" => arm!(crate::rpc::set_enabled(ctx, input)),
        "repo.actions.setForkPrApproval" => arm!(crate::rpc::set_fork_pr_approval(ctx, input)),
        "repo.actions.setPullRequestTarget" => {
            arm!(crate::rpc::set_pull_request_target(ctx, input))
        }
        "repo.actions.setWorkflowEnabled" => arm!(crate::rpc::set_workflow_enabled(ctx, input)),
        "repo.actions.variables.delete" => arm!(crate::rpc::delete_variable(ctx, input)),
        "repo.actions.variables.list" => arm!(crate::rpc::list_variables(ctx, input)),
        "repo.actions.variables.put" => arm!(crate::rpc::put_variable(ctx, input)),
        _ => None,
    }
}
