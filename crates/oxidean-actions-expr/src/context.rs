//! Evaluation context — named context objects plus job status and the
//! runner-injected `hashFiles` callback.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

/// Job/run status driving `success()`/`failure()`/`cancelled()`.
/// `Pending` = not yet terminal (jobs being claimed) — `success()`/`failure()`
/// `cancelled()` are all false there, `always()` still true. GitHub's
/// default for a running job is "would-be success" for `success()` — runners
/// evaluating step `if` mid-job use [`JobStatus::Success`] by convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Success,
    Failure,
    Cancelled,
}

impl JobStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "success" => Self::Success,
            "failure" => Self::Failure,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }
}

/// `hashFiles(patterns…)` runs runner-side over the workspace — the runner
/// injects this callback; server-side contexts leave it unset ("" result).
pub type HashFilesFn = Arc<dyn Fn(&[String]) -> Result<String, String> + Send + Sync>;

#[derive(Clone)]
pub struct Context {
    values: HashMap<String, Value>,
    job_status: JobStatus,
    hash_files: Option<HashFilesFn>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            job_status: JobStatus::Pending,
            hash_files: None,
        }
    }

    /// Bind a named context object (`github`, `env`, `matrix`, `needs`,
    /// `steps`, `inputs`, `vars`, `secrets`, `runner`, `job`, `strategy`).
    pub fn with_context(mut self, name: &str, v: Value) -> Self {
        self.values.insert(name.to_string(), v);
        self
    }
    pub fn set_context(&mut self, name: &str, v: Value) {
        self.values.insert(name.to_string(), v);
    }
    pub fn get_context(&self, name: &str) -> Option<&Value> {
        self.values
            .get(&name.to_lowercase())
            .or_else(|| self.values.get(name))
    }

    pub fn with_job_status(mut self, status: &str) -> Self {
        self.job_status = JobStatus::parse(status);
        self
    }
    pub fn set_job_status(&mut self, status: JobStatus) {
        self.job_status = status;
    }
    pub fn job_status(&self) -> JobStatus {
        self.job_status
    }

    pub fn with_hash_files<F>(mut self, f: F) -> Self
    where
        F: Fn(&[String]) -> Result<String, String> + Send + Sync + 'static,
    {
        self.hash_files = Some(Arc::new(f));
        self
    }
    pub fn hash_files(&self) -> Option<&HashFilesFn> {
        self.hash_files.as_ref()
    }

    /// Merge `extra` into the `env` context (later keys win) — used to layer
    /// workflow → job → step env.
    pub fn overlay_env(&mut self, extra: &Value) {
        let merged = match (self.values.get("env").cloned(), extra) {
            (Some(Value::Object(mut base)), Value::Object(add)) => {
                for (k, v) in add {
                    base.insert(k.clone(), v.clone());
                }
                Value::Object(base)
            }
            (_, v @ Value::Object(_)) => v.clone(),
            (cur, _) => cur.unwrap_or(Value::Object(Default::default())),
        };
        self.values.insert("env".into(), merged);
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}
