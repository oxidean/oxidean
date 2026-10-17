//! GitHub Actions–compatible YAML subset parser (D-ACT-02 / ACT-01).

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Structured parse failure suitable for a failed-run log message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// `contents` permission level granted to the per-job `OXIDEAN_TOKEN`
/// (GitHub `permissions:` semantics — `none` revokes the token entirely).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentsPermission {
    None,
    Read,
    Write,
}

impl ContentsPermission {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

/// Parsed `permissions:` block — only `contents` is honoured today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WorkflowPermissions {
    pub contents: ContentsPermission,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDocument {
    pub name: String,
    pub triggers: WorkflowTriggers,
    pub jobs: Vec<JobSpec>,
    /// Raw `env` map if present (opaque JSON object).
    pub env: Option<JsonValue>,
    /// Workflow-level `permissions:` (`None` = not specified).
    pub permissions: Option<WorkflowPermissions>,
    /// Structural features parsed but not executable — dispatch must refuse
    /// the workflow rather than silently mis-execute it (D-parity: fail
    /// loudly). Sorted, dedup'd feature keys (`needs`, `strategy`, …).
    /// `expressions` flags malformed `${{ }}` syntax — evaluated keys no
    /// longer pass through literally, so a broken expression is a parse-level
    /// contract violation, not a runtime surprise.
    pub unsupported: Vec<String>,
    /// Workflow-level `concurrency:` — evaluated at enqueue onto the run row
    /// (enforcement lands with the scheduler phase).
    pub concurrency: Option<Concurrency>,
    /// Dispatch-time warnings: features that run but are partially ignored
    /// (`with:` inputs not applied, `concurrency` captured but not enforced).
    /// Surfaced in each job's log at enqueue — never silent.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowTriggers {
    pub push: bool,
    pub pull_request: bool,
    pub workflow_dispatch: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobSpec {
    pub id: String,
    pub name: Option<String>,
    pub runs_on: Vec<String>,
    pub steps: Vec<StepSpec>,
    pub env: Option<JsonValue>,
    /// Job-level `permissions:` — overrides the workflow-level block.
    pub permissions: Option<WorkflowPermissions>,
    /// `jobs.<id>.if` — expression source, evaluated at enqueue.
    pub if_expr: Option<String>,
    /// `jobs.<id>.timeout-minutes` — expression source (usually a bare
    /// number); evaluated at enqueue into `action_jobs.timeout_at`.
    pub timeout_minutes: Option<String>,
    /// `jobs.<id>.concurrency` — `{group, cancel-in-progress}` evaluated at
    /// enqueue (enforcement lands with the scheduler phase).
    pub concurrency: Option<Concurrency>,
    /// `jobs.<id>.outputs` — raw `{name: expression}` map, materialized for
    /// `needs.<id>.outputs.*` consumers when the scheduler lands.
    pub outputs: Option<JsonValue>,
}

/// `concurrency:` shape — a bare group string or `{group, cancel-in-progress}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Concurrency {
    /// Expression source for the group key.
    pub group: Option<String>,
    /// Expression source — usually a bare bool.
    pub cancel_in_progress: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepSpec {
    /// `id:` — the step's address in `steps.<id>.outputs/outcome` contexts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: Option<String>,
    pub uses: Option<String>,
    pub run: Option<String>,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    /// `with` / `env` / `if` retained as raw values/strings — the runner
    /// evaluates their `${{ }}` expressions at step time.
    pub with: Option<JsonValue>,
    pub env: Option<JsonValue>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_expr: Option<String>,
    /// `steps.*.timeout-minutes` — expression source; the runner enforces it.
    #[serde(rename = "timeout-minutes", skip_serializing_if = "Option::is_none")]
    pub timeout_minutes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawWorkflow {
    name: Option<String>,
    #[serde(rename = "on")]
    on: Option<RawOn>,
    jobs: Option<serde_yaml::Mapping>,
    env: Option<serde_yaml::Value>,
    defaults: Option<RawDefaults>,
    permissions: Option<serde_yaml::Value>,
    concurrency: Option<serde_yaml::Value>,
}
/// are honoured (workflow-level fallback for every step).
#[derive(Debug, Deserialize, Clone)]
struct RawDefaults {
    run: Option<RawRunDefaults>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawRunDefaults {
    shell: Option<String>,
    #[serde(rename = "working-directory")]
    working_directory: Option<String>,
}

fn run_defaults(d: Option<&RawDefaults>) -> (Option<String>, Option<String>) {
    let run = d.and_then(|d| d.run.as_ref());
    (
        run.and_then(|r| r.shell.clone()),
        run.and_then(|r| r.working_directory.clone()),
    )
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawOn {
    List(Vec<String>),
    Map(serde_yaml::Mapping),
    Single(String),
}

#[derive(Debug, Deserialize)]
struct RawJob {
    name: Option<String>,
    #[serde(rename = "runs-on")]
    runs_on: Option<RawRunsOn>,
    steps: Option<Vec<RawStep>>,
    env: Option<serde_yaml::Value>,
    defaults: Option<RawDefaults>,
    permissions: Option<serde_yaml::Value>,
    needs: Option<serde_yaml::Value>,
    strategy: Option<serde_yaml::Value>,
    container: Option<serde_yaml::Value>,
    services: Option<serde_yaml::Value>,
    environment: Option<serde_yaml::Value>,
    concurrency: Option<serde_yaml::Value>,
    outputs: Option<serde_yaml::Value>,
    #[serde(rename = "if")]
    if_expr: Option<serde_yaml::Value>,
    uses: Option<serde_yaml::Value>,
    with: Option<serde_yaml::Value>,
    secrets: Option<serde_yaml::Value>,
    #[serde(rename = "continue-on-error")]
    continue_on_error: Option<serde_yaml::Value>,
    #[serde(rename = "timeout-minutes")]
    timeout_minutes: Option<serde_yaml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawRunsOn {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct RawStep {
    id: Option<String>,
    name: Option<String>,
    uses: Option<String>,
    run: Option<String>,
    shell: Option<String>,
    #[serde(rename = "working-directory")]
    working_directory: Option<String>,
    with: Option<serde_yaml::Value>,
    env: Option<serde_yaml::Value>,
    /// `if:` accepts a bare scalar — `if: false` is valid YAML and GitHub
    /// treats it as the boolean expression, so keep the raw value.
    #[serde(rename = "if")]
    if_expr: Option<serde_yaml::Value>,
    #[serde(rename = "continue-on-error")]
    continue_on_error: Option<serde_yaml::Value>,
    #[serde(rename = "timeout-minutes")]
    timeout_minutes: Option<serde_yaml::Value>,
}

fn yaml_to_json(v: serde_yaml::Value) -> Result<JsonValue, ParseError> {
    serde_json::to_value(v).map_err(|e| ParseError {
        message: format!("failed to normalize workflow YAML value: {e}"),
    })
}

/// Expression-source strings can be bare scalars (`if: false`,
/// `timeout-minutes: 30`) — normalize YAML scalars to their source text.
fn expr_src(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// [`expr_src`] over an `Option` raw value.
fn expr_src_opt(v: &Option<serde_yaml::Value>) -> Option<String> {
    v.as_ref().and_then(expr_src)
}

/// `concurrency:` is either a bare group string or
/// `{group: …, cancel-in-progress: …}` — keep each field's expression source.
fn parse_concurrency(raw: Option<&serde_yaml::Value>) -> Option<Concurrency> {
    match raw? {
        s @ serde_yaml::Value::String(_) => Some(Concurrency {
            group: expr_src(s),
            cancel_in_progress: None,
        }),
        serde_yaml::Value::Mapping(map) => {
            let mut c = Concurrency {
                group: None,
                cancel_in_progress: None,
            };
            for (k, v) in map {
                match k.as_str() {
                    Some("group") => c.group = expr_src(v),
                    Some("cancel-in-progress") => c.cancel_in_progress = expr_src(v),
                    _ => {}
                }
            }
            Some(c)
        }
        _ => None,
    }
}

/// Check every `${{ }}` in `s` for syntax errors; detail strings go to
/// `warnings`, the presence of any error sets the `expressions` flag.
fn check_str(src: &str, unsupported: &mut Vec<String>, warnings: &mut Vec<String>) {
    for e in oxidean_actions_expr::check(src) {
        flag(unsupported, "expressions");
        warnings.push(format!("malformed expression in '{src}': {e}"));
    }
}

/// `if:` fields are whole expressions — bare `github.sha` or `${{ }}`-wrapped,
/// both must parse.
fn check_if(src: &str, unsupported: &mut Vec<String>, warnings: &mut Vec<String>) {
    for e in oxidean_actions_expr::check_expr(src) {
        flag(unsupported, "expressions");
        warnings.push(format!("malformed expression in '{src}': {e}"));
    }
}

/// Recursive walk for `env:`/`with:`/`outputs:` maps — only string leaves
/// carry expressions.
fn check_json(v: &JsonValue, unsupported: &mut Vec<String>, warnings: &mut Vec<String>) {
    match v {
        JsonValue::String(s) => check_str(s, unsupported, warnings),
        JsonValue::Array(items) => {
            for i in items {
                check_json(i, unsupported, warnings);
            }
        }
        JsonValue::Object(map) => {
            for (_, i) in map {
                check_json(i, unsupported, warnings);
            }
        }
        _ => {}
    }
}

fn parse_triggers(on: Option<RawOn>) -> (WorkflowTriggers, Vec<String>) {
    let mut t = WorkflowTriggers::default();
    let mut unsupported = Vec::new();
    match on {
        None => {}
        Some(RawOn::Single(s)) => apply_event(&mut t, &mut unsupported, &s),
        Some(RawOn::List(list)) => {
            for s in list {
                apply_event(&mut t, &mut unsupported, &s);
            }
        }
        Some(RawOn::Map(map)) => {
            for (k, _) in map {
                if let serde_yaml::Value::String(s) = k {
                    apply_event(&mut t, &mut unsupported, &s);
                }
            }
        }
    }
    (t, unsupported)
}

fn apply_event(t: &mut WorkflowTriggers, unsupported: &mut Vec<String>, name: &str) {
    match name.trim() {
        "push" => t.push = true,
        "pull_request" => t.pull_request = true,
        "workflow_dispatch" => t.workflow_dispatch = true,
        // Dangerous-on-forks event we never fire — restricted until designed in.
        "pull_request_target" => unsupported.push("pull_request_target".into()),
        _ => {
            // Other unimplemented events (schedule, release, issues, …) simply
            // never fire — harmless to ignore, not mis-executed.
        }
    }
}

/// Parse a `permissions:` value (GitHub subset).
///
/// - `read-all` / `write-all` scalars set `contents` to read/write.
/// - A map honours only the `contents` key (`read`/`write`/`none`); per GitHub
///   semantics, specifying a map resets unlisted scopes to `none`, so a map
///   without `contents` — including `{}` — yields `contents: none`.
/// - `None` (absent) or an unrecognized shape → `None` ("not specified").
fn parse_permissions(raw: Option<&serde_yaml::Value>) -> Option<WorkflowPermissions> {
    match raw {
        None => None,
        Some(serde_yaml::Value::String(s)) => match s.trim() {
            "read-all" => Some(WorkflowPermissions {
                contents: ContentsPermission::Read,
            }),
            "write-all" => Some(WorkflowPermissions {
                contents: ContentsPermission::Write,
            }),
            _ => None,
        },
        Some(serde_yaml::Value::Mapping(map)) => {
            let mut contents = ContentsPermission::None;
            for (k, v) in map {
                if k.as_str() == Some("contents") {
                    contents = match v.as_str().map(str::trim) {
                        Some("read") => ContentsPermission::Read,
                        Some("write") => ContentsPermission::Write,
                        _ => ContentsPermission::None,
                    };
                }
            }
            Some(WorkflowPermissions { contents })
        }
        Some(_) => None,
    }
}

/// Effective `contents` permission for a job: job-level block wins, then
/// workflow-level, else `read` (the instance default).
pub fn effective_contents_permission(doc: &WorkflowDocument, job: &JobSpec) -> ContentsPermission {
    job.permissions
        .map(|p| p.contents)
        .or_else(|| doc.permissions.map(|p| p.contents))
        .unwrap_or(ContentsPermission::Read)
}

/// Mark a feature key once per document (dedup'd, sorted at the end).
fn flag(set: &mut Vec<String>, key: &str) {
    if !set.iter().any(|k| k == key) {
        set.push(key.to_string());
    }
}

fn parse_runs_on(raw: Option<RawRunsOn>) -> Result<Vec<String>, ParseError> {
    match raw {
        None => Err(ParseError {
            message: "job missing required runs-on".into(),
        }),
        Some(RawRunsOn::One(s)) => Ok(vec![s]),
        Some(RawRunsOn::Many(v)) if v.is_empty() => Err(ParseError {
            message: "job runs-on must not be empty".into(),
        }),
        Some(RawRunsOn::Many(v)) => Ok(v),
    }
}

/// Parse a GitHub Actions–compatible workflow YAML subset.
///
/// Unsupported constructs that still fit the schema are retained as opaque JSON
/// where possible. Invalid YAML or missing required fields return [`ParseError`].
pub fn parse_workflow_yaml(bytes: &[u8]) -> Result<WorkflowDocument, ParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError {
        message: "workflow file is not valid UTF-8".into(),
    })?;
    let raw: RawWorkflow = serde_yaml::from_str(text).map_err(|e| ParseError {
        message: format!("invalid workflow YAML: {e}"),
    })?;

    let name = raw
        .name
        .unwrap_or_else(|| "Workflow".to_string())
        .trim()
        .to_string();
    if name.is_empty() {
        return Err(ParseError {
            message: "workflow name must not be empty".into(),
        });
    }

    let (triggers, mut unsupported) = parse_triggers(raw.on);
    let mut warnings = Vec::new();
    let wf_concurrency = parse_concurrency(raw.concurrency.as_ref());
    if wf_concurrency.is_some() {
        flag(&mut warnings, "concurrency (captured but not enforced yet)");
    }
    check_str(&name, &mut unsupported, &mut warnings);
    let wf_env = raw.env.map(yaml_to_json).transpose()?;
    if let Some(v) = &wf_env {
        check_json(v, &mut unsupported, &mut warnings);
    }
    if let Some(c) = &wf_concurrency {
        for s in [&c.group, &c.cancel_in_progress].into_iter().flatten() {
            check_str(s, &mut unsupported, &mut warnings);
        }
    }
    let jobs_map = raw.jobs.ok_or_else(|| ParseError {
        message: "workflow missing jobs".into(),
    })?;
    if jobs_map.is_empty() {
        return Err(ParseError {
            message: "workflow jobs must not be empty".into(),
        });
    }

    let (wf_shell, wf_workdir) = run_defaults(raw.defaults.as_ref());
    let mut jobs = Vec::new();
    for (key, value) in jobs_map {
        let id = match key {
            serde_yaml::Value::String(s) => s,
            other => {
                return Err(ParseError {
                    message: format!("invalid job id: {other:?}"),
                });
            }
        };
        if id.is_empty() || id.contains('/') || id.contains('\\') {
            return Err(ParseError {
                message: format!("invalid job id '{id}'"),
            });
        }
        let job: RawJob = serde_yaml::from_value(value).map_err(|e| ParseError {
            message: format!("invalid job '{id}': {e}"),
        })?;
        // Structural features that would mis-execute — fail loudly at dispatch.
        for (present, key) in [
            (job.needs.is_some(), "needs"),
            (job.strategy.is_some(), "strategy"),
            (job.container.is_some(), "container"),
            (job.services.is_some(), "services"),
            (job.environment.is_some(), "environment"),
            (job.continue_on_error.is_some(), "continue-on-error"),
        ] {
            if present {
                flag(&mut unsupported, key);
            }
        }
        // `uses:`/`with:`/`secrets:` on a job is the reusable-workflow call shape.
        if job.uses.is_some() || job.with.is_some() || job.secrets.is_some() {
            flag(&mut unsupported, "reusable-workflow");
        }
        let job_concurrency = parse_concurrency(job.concurrency.as_ref());
        if job_concurrency.is_some() {
            flag(&mut warnings, "concurrency (captured but not enforced yet)");
        }
        if job.outputs.is_some() {
            flag(
                &mut warnings,
                "outputs (materialize for needs-consumers with the scheduler)",
            );
        }
        // Reusable-call jobs (`uses:`) carry no `runs-on` — the unsupported
        // flag above already routes them to the validation-failure path.
        let runs_on = if job.uses.is_some() {
            Vec::new()
        } else {
            parse_runs_on(job.runs_on)?
        };
        // Expression-syntax checks — evaluated fields can't carry malformed
        // `${{ }}` through to silent mis-execution.
        if let Some(s) = &job.name {
            check_str(s, &mut unsupported, &mut warnings);
        }
        if let Some(s) = expr_src_opt(&job.if_expr) {
            check_if(&s, &mut unsupported, &mut warnings);
        }
        for s in &runs_on {
            check_str(s, &mut unsupported, &mut warnings);
        }
        if let Some(s) = expr_src_opt(&job.timeout_minutes) {
            check_str(&s, &mut unsupported, &mut warnings);
        }
        if let Some(c) = &job_concurrency {
            for s in [&c.group, &c.cancel_in_progress].into_iter().flatten() {
                check_str(s, &mut unsupported, &mut warnings);
            }
        }
        let job_env = job.env.map(yaml_to_json).transpose()?;
        if let Some(v) = &job_env {
            check_json(v, &mut unsupported, &mut warnings);
        }
        let job_outputs = job.outputs.map(yaml_to_json).transpose()?;
        if let Some(v) = &job_outputs {
            check_json(v, &mut unsupported, &mut warnings);
        }
        let (job_shell, job_workdir) = run_defaults(job.defaults.as_ref());
        let mut steps = Vec::new();
        for (i, step) in job.steps.unwrap_or_default().into_iter().enumerate() {
            if step.uses.is_none() && step.run.is_none() {
                return Err(ParseError {
                    message: format!("job '{id}' step {i} needs uses or run"),
                });
            }
            if step.continue_on_error.is_some() {
                flag(&mut unsupported, "continue-on-error");
            }
            if step.with.is_some()
                && !step
                    .uses
                    .as_deref()
                    .is_some_and(|u| u.starts_with("actions/checkout"))
            {
                flag(
                    &mut warnings,
                    "with (inputs applied for actions/checkout only)",
                );
            }
            if let Some(s) = &step.name {
                check_str(s, &mut unsupported, &mut warnings);
            }
            if let Some(s) = expr_src_opt(&step.if_expr) {
                check_if(&s, &mut unsupported, &mut warnings);
            }
            if let Some(s) = &step.run {
                check_str(s, &mut unsupported, &mut warnings);
            }
            if let Some(s) = &step.working_directory {
                check_str(s, &mut unsupported, &mut warnings);
            }
            if let Some(s) = expr_src_opt(&step.timeout_minutes) {
                check_str(&s, &mut unsupported, &mut warnings);
            }
            let step_with = step.with.map(yaml_to_json).transpose()?;
            if let Some(v) = &step_with {
                check_json(v, &mut unsupported, &mut warnings);
            }
            let step_env = step.env.map(yaml_to_json).transpose()?;
            if let Some(v) = &step_env {
                check_json(v, &mut unsupported, &mut warnings);
            }
            // Precedence: step > job defaults > workflow defaults.
            let shell = step
                .shell
                .or_else(|| job_shell.clone())
                .or_else(|| wf_shell.clone());
            let working_directory = step
                .working_directory
                .or_else(|| job_workdir.clone())
                .or_else(|| wf_workdir.clone());
            steps.push(StepSpec {
                id: step.id,
                name: step.name,
                uses: step.uses,
                run: step.run,
                shell,
                working_directory,
                with: step_with,
                env: step_env,
                if_expr: expr_src_opt(&step.if_expr),
                timeout_minutes: expr_src_opt(&step.timeout_minutes),
            });
        }
        if steps.is_empty() && job.uses.is_none() {
            // GitHub rejects the file: a job needs `steps:` or a `uses:` call.
            flag(&mut unsupported, "steps");
        }
        jobs.push(JobSpec {
            id,
            name: job.name,
            runs_on,
            steps,
            env: job_env,
            permissions: parse_permissions(job.permissions.as_ref()),
            if_expr: expr_src_opt(&job.if_expr),
            timeout_minutes: expr_src_opt(&job.timeout_minutes),
            concurrency: job_concurrency,
            outputs: job_outputs,
        });
    }

    unsupported.sort();
    warnings.sort();
    warnings.dedup();
    Ok(WorkflowDocument {
        name,
        triggers,
        jobs,
        env: wf_env,
        permissions: parse_permissions(raw.permissions.as_ref()),
        unsupported,
        concurrency: wf_concurrency,
        warnings,
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parses_minimal_push_workflow() {
        let yaml = br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo hi
"#;
        let doc = parse_workflow_yaml(yaml).expect("parse");
        assert_eq!(doc.name, "CI");
        assert!(doc.triggers.push);
        assert!(!doc.triggers.pull_request);
        assert_eq!(doc.jobs.len(), 1);
        assert_eq!(doc.jobs[0].id, "build");
        assert_eq!(doc.jobs[0].runs_on, vec!["ubuntu-latest"]);
        assert_eq!(doc.jobs[0].steps.len(), 2);
    }

    #[test]
    fn invalid_yaml_returns_parse_error() {
        let err = parse_workflow_yaml(b"name: [\n").expect_err("bad yaml");
        assert!(err.message.contains("invalid workflow YAML"));
    }

    fn perm_of(yaml: &[u8]) -> (Option<WorkflowPermissions>, Option<WorkflowPermissions>) {
        let doc = parse_workflow_yaml(yaml).expect("parse");
        (doc.permissions, doc.jobs[0].permissions)
    }

    #[test]
    fn permissions_read_all_and_write_all_scalars() {
        let (wf, _) = perm_of(
            br#"
name: CI
on: [push]
permissions: read-all
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        );
        assert_eq!(
            wf,
            Some(WorkflowPermissions {
                contents: ContentsPermission::Read
            })
        );

        let (wf, _) = perm_of(
            br#"
name: CI
on: [push]
permissions: write-all
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        );
        assert_eq!(
            wf,
            Some(WorkflowPermissions {
                contents: ContentsPermission::Write
            })
        );
    }

    #[test]
    fn permissions_map_contents_write() {
        let (wf, _) = perm_of(
            br#"
name: CI
on: [push]
permissions:
  contents: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        );
        assert_eq!(
            wf,
            Some(WorkflowPermissions {
                contents: ContentsPermission::Write
            })
        );
    }

    #[test]
    fn permissions_map_without_contents_defaults_to_none() {
        // GitHub semantics: a permissions map resets unlisted scopes to none.
        let (wf, _) = perm_of(
            br#"
name: CI
on: [push]
permissions:
  issues: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        );
        assert_eq!(
            wf,
            Some(WorkflowPermissions {
                contents: ContentsPermission::None
            })
        );
        let (wf, _) = perm_of(
            br#"
name: CI
on: [push]
permissions: {}
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        );
        assert_eq!(
            wf,
            Some(WorkflowPermissions {
                contents: ContentsPermission::None
            })
        );
    }

    #[test]
    fn effective_contents_permission_job_overrides_workflow() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
permissions: read-all
jobs:
  build:
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        assert_eq!(
            effective_contents_permission(&doc, &doc.jobs[0]),
            ContentsPermission::Write
        );
    }

    #[test]
    fn effective_contents_permission_defaults_to_read() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        assert_eq!(doc.permissions, None);
        assert_eq!(doc.jobs[0].permissions, None);
        assert_eq!(
            effective_contents_permission(&doc, &doc.jobs[0]),
            ContentsPermission::Read
        );
    }

    #[test]
    fn effective_contents_permission_workflow_level_applies() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
permissions: write-all
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        assert_eq!(
            effective_contents_permission(&doc, &doc.jobs[0]),
            ContentsPermission::Write
        );
    }

    fn unsupported_of(yaml: &[u8]) -> Vec<String> {
        parse_workflow_yaml(yaml).expect("parse").unsupported
    }

    #[test]
    fn clean_workflow_has_no_unsupported() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo hi
"#,
        )
        .expect("parse");
        assert!(doc.unsupported.is_empty());
        assert!(doc.warnings.is_empty());
    }

    #[test]
    fn structural_features_are_flagged_unsupported() {
        let cases: &[(&[u8], &[&str])] = &[
            (
                br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    needs: build
    steps:
      - run: echo hi
"#,
                &["needs"],
            ),
            (
                br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        os: [ubuntu, macos]
    steps:
      - run: echo hi
"#,
                &["strategy"],
            ),
            (
                br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    container: node:20
    services:
      pg:
        image: postgres:16
    steps:
      - run: echo hi
"#,
                &["container", "services"],
            ),
            (
                br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    environment: production
    steps:
      - run: echo hi
"#,
                &["environment"],
            ),
            (
                br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    continue-on-error: true
    steps:
      - run: echo hi
"#,
                &["continue-on-error"],
            ),
            (
                br#"
name: CI
on: [push]
jobs:
  call:
    uses: ./.github/workflows/reuse.yml
    with:
      x: 1
    secrets: inherit
"#,
                &["reusable-workflow"],
            ),
            (
                br#"
name: CI
on: pull_request_target
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
                &["pull_request_target"],
            ),
            (
                br#"
name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
        continue-on-error: true
"#,
                &["continue-on-error"],
            ),
        ];
        for (yaml, want) in cases {
            let got = unsupported_of(yaml);
            for key in *want {
                assert!(
                    got.iter().any(|k| k == key),
                    "expected unsupported '{key}' in {got:?}"
                );
            }
        }
    }

    #[test]
    fn captured_features_warn_not_unsupported() {
        // env blocks and if/timeout/concurrency/outputs are evaluated or
        // captured now — only not-yet-enforced pieces warn.
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
env:
  A: "1"
concurrency:
  group: ci-${{ github.ref }}
jobs:
  a:
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    timeout-minutes: 5
    concurrency: inner
    outputs:
      v: ${{ steps.x.outputs.v }}
    env:
      B: "2"
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - run: echo hi
        if: success()
        timeout-minutes: 2
"#,
        )
        .expect("parse");
        assert!(doc.unsupported.is_empty());
        assert_eq!(
            doc.warnings,
            vec![
                "concurrency (captured but not enforced yet)".to_string(),
                "outputs (materialize for needs-consumers with the scheduler)".to_string(),
                "with (inputs applied for actions/checkout only)".to_string(),
            ]
        );
        assert_eq!(
            doc.jobs[0].if_expr.as_deref(),
            Some("github.ref == 'refs/heads/main'")
        );
        assert_eq!(doc.jobs[0].timeout_minutes.as_deref(), Some("5"));
        assert_eq!(
            doc.concurrency.unwrap().group.as_deref(),
            Some("ci-${{ github.ref }}")
        );
        let jc = doc.jobs[0].concurrency.as_ref().unwrap();
        assert_eq!(jc.group.as_deref(), Some("inner"));
        assert_eq!(doc.jobs[0].steps[2].if_expr.as_deref(), Some("success()"));
    }

    #[test]
    fn malformed_expressions_flag_unsupported() {
        // A broken `${{ }}` anywhere an expression is evaluated is a
        // parse-level contract violation — it must never reach silent
        // mis-execution.
        for yaml in [
            // bad embedded expr in run
            &br#"name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo ${{ github.sha && }}
"#[..],
            // bad bare if expr
            &br#"name: CI
on: [push]
jobs:
  a:
    runs-on: ubuntu-latest
    if: ${{ github.ref ==
    steps:
      - run: echo hi
"#[..],
            // bad expr inside env value
            &br#"name: CI
on: [push]
env:
  A: ${{ f(, }}
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#[..],
        ] {
            let doc = parse_workflow_yaml(yaml).expect("parse");
            assert!(
                doc.unsupported.iter().any(|k| k == "expressions"),
                "expected 'expressions' flag: {:?}",
                doc.unsupported
            );
            assert!(
                doc.warnings
                    .iter()
                    .any(|w| w.contains("malformed expression")),
                "expected malformed-expression warning: {:?}",
                doc.warnings
            );
        }
        // Well-formed `${{ }}` everywhere is clean.
        let ok = parse_workflow_yaml(
            br#"name: CI
on: [push]
jobs:
  a:
    runs-on: ${{ 'ubuntu-latest' }}
    steps:
      - run: echo ${{ github.sha }}
"#,
        )
        .expect("parse");
        assert!(ok.unsupported.is_empty());
    }

    #[test]
    fn unimplemented_events_like_schedule_do_not_flag() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on:
  push:
  schedule:
    - cron: '0 0 * * *'
jobs:
  a:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        assert!(doc.unsupported.is_empty());
        assert!(doc.triggers.push);
    }
}
