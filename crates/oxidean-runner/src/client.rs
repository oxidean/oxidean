//! Oxidean Actions runner protocol client (`/api/actions/*` JSON endpoints).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

#[derive(Debug, Serialize)]
struct RegisterBody<'a> {
    name: &'a str,
    labels: &'a [String],
    token: &'a str,
}

#[derive(Debug, Deserialize)]
struct RegisterResp {
    runner_id: String,
    runner_token: String,
}

#[derive(Debug, Serialize)]
struct DeclareBody<'a> {
    labels: &'a [String],
}

/// A claimed job from `fetch_task`.
#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub job_id: Option<String>,
    pub run_id: Option<String>,
    pub job_key: Option<String>,
    pub runs_on: Option<Vec<String>>,
    pub workflow_name: Option<String>,
    pub head_sha: Option<String>,
    pub head_ref: Option<String>,
    pub steps: Option<serde_json::Value>,
    pub secrets: Option<HashMap<String, String>>,
    pub repository_owner: Option<String>,
    pub repository_name: Option<String>,
    /// Triggering event name (push, pull_request, workflow_dispatch, …).
    pub event: Option<String>,
    pub run_number: Option<i64>,
    /// Re-run counter — bumped per `requeue_run` (GITHUB_RUN_ATTEMPT).
    pub run_attempt: Option<i64>,
    /// Triggering user's username / id ("" when system-triggered).
    pub actor: Option<String>,
    pub actor_id: Option<String>,
    pub repository_id: Option<String>,
    /// Owner user id or org id.
    pub repository_owner_id: Option<String>,
    /// Instance public origin.
    pub server_url: Option<String>,
    /// PR context for pull_request runs only ("" / 0 otherwise).
    pub base_ref: Option<String>,
    pub pull_number: Option<i64>,
    /// Per-job OXIDEAN_TOKEN bearer; None/"" when not minted. The API also
    /// sends `job_token_permission` (read|write) — informational, ignored.
    pub job_token: Option<String>,
    /// "{owner}/{repo}/{workflow_path}@{head_ref}".
    pub workflow_ref: Option<String>,
}

impl Task {
    pub fn is_empty(&self) -> bool {
        self.job_id.is_none()
    }
}

#[derive(Debug, Serialize)]
struct UpdateTaskBody<'a> {
    job_id: &'a str,
    state: &'a str,
}

#[derive(Debug, Serialize)]
struct UpdateLogBody<'a> {
    run_id: &'a str,
    job_id: &'a str,
    chunk: &'a str,
}

impl Client {
    pub fn new(origin: &str, token: Option<String>) -> Self {
        Client {
            http: reqwest::Client::new(),
            base: origin.trim_end_matches('/').to_string(),
            token,
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.token = Some(token);
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/actions/{}", self.base, path)
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.bearer_auth(t),
            None => req,
        }
    }

    /// Register with a one-shot registration token. Returns the persistent
    /// runner id + bearer token to store in state.
    pub async fn register(
        &self,
        name: &str,
        labels: &[String],
        registration_token: &str,
    ) -> Result<(String, String), String> {
        let res = self
            .http
            .post(self.url("register"))
            .json(&RegisterBody {
                name,
                labels,
                token: registration_token,
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err("registration token rejected".into());
        }
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let body: RegisterResp = res.json().await.map_err(|e| e.to_string())?;
        Ok((body.runner_id, body.runner_token))
    }

    pub async fn declare(&self, labels: &[String]) -> Result<(), String> {
        let res = self
            .authed(self.http.post(self.url("declare")))
            .json(&DeclareBody { labels })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        res.error_for_status().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Poll for work. Returns `Ok(None)` when no queued job matches.
    pub async fn fetch_task(&self) -> Result<Option<Task>, String> {
        let res = self
            .authed(self.http.post(self.url("fetch_task")))
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let task: Task = res.json().await.map_err(|e| e.to_string())?;
        Ok(if task.is_empty() { None } else { Some(task) })
    }

    pub async fn update_task(&self, job_id: &str, state: &str) -> Result<(), String> {
        let res = self
            .authed(self.http.post(self.url("update_task")))
            .json(&UpdateTaskBody { job_id, state })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        res.error_for_status().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn update_log(&self, run_id: &str, job_id: &str, chunk: &str) -> Result<(), String> {
        if chunk.is_empty() {
            return Ok(());
        }
        let res = self
            .authed(self.http.post(self.url("update_log")))
            .json(&UpdateLogBody {
                run_id,
                job_id,
                chunk,
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        res.error_for_status().map_err(|e| e.to_string())?;
        Ok(())
    }
}
