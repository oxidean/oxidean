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
    /// Job `env:` map captured at enqueue (expression-evaluated runner-side).
    pub env: Option<serde_json::Value>,
    /// Workflow `env:` map captured at enqueue.
    pub workflow_env: Option<serde_json::Value>,
    /// Evaluated `timeout-minutes` deadline (RFC3339) — job-wide kill time.
    pub timeout_at: Option<String>,
    /// Seconds until `timeout_at`, computed at claim.
    pub timeout_seconds: Option<i64>,
    /// Triggering event payload → `github.event` context + event.json file.
    pub event_payload: Option<serde_json::Value>,
    /// Matrix assignment → `matrix` context.
    pub matrix: Option<serde_json::Value>,
    /// `needs.<id>.{result,outputs}` for dependents — step expressions can
    /// read `needs.*.outputs.*`.
    pub needs: Option<serde_json::Value>,
    /// `jobs.<id>.container` spec (evaluated) — a long-lived job container
    /// the runner `docker exec`s steps into.
    pub container: Option<serde_json::Value>,
    /// `jobs.<id>.services` map — sibling containers on the job network.
    pub services: Option<serde_json::Value>,
    /// `jobs.<id>.environment` resolved at enqueue — deployment context
    /// (`jobs.<id>.outputs.url` / `environment.url` source).
    pub environment: Option<String>,
    /// `jobs.<id>.environment.url` resolved at enqueue — the `environment.url`
    /// expression context.
    pub environment_url: Option<String>,
    /// `ACTIONS_ID_TOKEN_REQUEST_URL` — OIDC mint endpoint (`id-token: write`).
    pub id_token_request_url: Option<String>,
    /// `ACTIONS_ID_TOKEN_REQUEST_TOKEN` — request credential bound to this job.
    pub id_token_request_token: Option<String>,
    /// `ACTIONS_CACHE_URL` — `_apis/artifactcache` service root for this job.
    pub cache_url: Option<String>,
    /// `ACTIONS_RUNTIME_TOKEN` — per-job bearer for the cache endpoints.
    pub cache_token: Option<String>,
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
    /// `steps.<id>.outputs` map — reported on terminal states so the API
    /// materializes `jobs.<id>.outputs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    outputs: Option<&'a serde_json::Value>,
    /// `$GITHUB_STEP_SUMMARY` markdown — reported once at terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<&'a str>,
}

/// `update_task`/`update_log` response — `cancelled` means the job was
/// cancelled server-side while running; `log_index` is the server's next
/// expected log-batch index (the runner resyncs its cursor to it, covering
/// accept, duplicate-drop, and the 409 gap response alike).
#[derive(Debug, Deserialize, Default)]
pub struct UpdateTaskAck {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub log_index: i64,
}

#[derive(Debug, Serialize)]
struct UpdateLogBody<'a> {
    run_id: &'a str,
    job_id: &'a str,
    chunk: &'a str,
    log_index: i64,
}

/// Artifact metadata returned by upload/list (server-side `ArtifactMeta`).
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactMeta {
    pub id: String,
    pub run_id: String,
    pub job_id: Option<String>,
    pub name: String,
    pub size_bytes: i64,
    pub digest_sha256: String,
    pub expires_at: Option<String>,
    #[allow(dead_code)]
    pub created_at: String,
}

/// Percent-encode a single path/query segment.
fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl Client {
    pub fn new(origin: &str, token: Option<String>) -> Self {
        Client {
            // 60s covers the server-side 30s `fetch_task` long-poll plus
            // margin; without it a blackholed API connection stalls a job's
            // log stream (and the claim loop) indefinitely.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
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

    /// Poll for work — `wait_ms` > 0 asks the server to hold an empty
    /// response (long-poll, capped 30s). Returns `Ok(None)` when no queued
    /// job matched before the hold lapsed.
    pub async fn fetch_task(&self, wait_ms: u64) -> Result<Option<Task>, String> {
        let res = self
            .authed(self.http.post(self.url("fetch_task")))
            .json(&serde_json::json!({ "wait_ms": wait_ms }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let task: Task = res.json().await.map_err(|e| e.to_string())?;
        Ok(if task.is_empty() { None } else { Some(task) })
    }

    /// Report a state transition. `outputs` carries `steps.<id>.outputs`
    /// on terminal calls; the ack's `cancelled` flag signals a server-side
    /// cancel (concurrency supersede, fail-fast, run cancel) — the runner
    /// should abort remaining steps rather than finish the queue.
    pub async fn update_task(
        &self,
        job_id: &str,
        state: &str,
        outputs: Option<&serde_json::Value>,
        summary: Option<&str>,
    ) -> Result<UpdateTaskAck, String> {
        let res = self
            .authed(self.http.post(self.url("update_task")))
            .json(&UpdateTaskBody {
                job_id,
                state,
                outputs,
                summary,
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<UpdateTaskAck>().await.map_err(|e| e.to_string())
    }

    /// Upload a raw tar.gz blob as a run artifact (GitHub v4 overwrite
    /// semantics — same (run, name) replaces).
    pub async fn upload_artifact(
        &self,
        job_id: &str,
        name: &str,
        retention_days: Option<u32>,
        bytes: Vec<u8>,
    ) -> Result<ArtifactMeta, String> {
        let mut url = format!(
            "{}?job_id={}&name={}",
            self.url("artifacts/upload"),
            urlencoding(job_id),
            urlencoding(name)
        );
        if let Some(d) = retention_days {
            url.push_str(&format!("&retention_days={d}"));
        }
        let res = self
            .authed(self.http.post(url))
            .header(reqwest::header::CONTENT_TYPE, "application/gzip")
            .body(bytes)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<ArtifactMeta>().await.map_err(|e| e.to_string())
    }

    /// Fetch a run artifact blob by name. `Err("artifact '…' not found")`
    /// on 404 so callers can emit GitHub-style messages.
    pub async fn download_artifact(&self, run_id: &str, name: &str) -> Result<Vec<u8>, String> {
        let res = self
            .authed(self.http.get(format!(
                "{}/{}/{}",
                self.url("artifacts"),
                urlencoding(run_id),
                urlencoding(name)
            )))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(format!("artifact '{name}' not found"));
        }
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| e.to_string())
    }

    /// Every artifact metadata row for a run.
    pub async fn list_artifacts(&self, run_id: &str) -> Result<Vec<ArtifactMeta>, String> {
        let res = self
            .authed(self.http.post(self.url("artifacts/list")))
            .json(&serde_json::json!({"run_id": run_id}))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<Vec<ArtifactMeta>>()
            .await
            .map_err(|e| e.to_string())
    }

    /// Append a log chunk at `log_index`. The ack's `cancelled` flag is the
    /// runner's mid-job cancel signal — checked between steps. A 409 still
    /// decodes the body: the server reports its next expected index there so
    /// the caller can resync its cursor.
    pub async fn update_log(
        &self,
        run_id: &str,
        job_id: &str,
        chunk: &str,
        log_index: i64,
    ) -> Result<UpdateTaskAck, String> {
        if chunk.is_empty() {
            return Ok(UpdateTaskAck::default());
        }
        let res = self
            .authed(self.http.post(self.url("update_log")))
            .json(&UpdateLogBody {
                run_id,
                job_id,
                chunk,
                log_index,
            })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status();
        if status == reqwest::StatusCode::CONFLICT {
            // Gap — body carries the server's expected index; surface it as
            // `ok:false` + `log_index` so the caller resyncs and resends.
            return res.json::<UpdateTaskAck>().await.map_err(|e| e.to_string());
        }
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<UpdateTaskAck>().await.map_err(|e| e.to_string())
    }
}
