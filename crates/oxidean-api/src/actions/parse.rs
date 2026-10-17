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

/// GitHub `permissions:` scope names — the full key set (parity).
/// `id-token: write` gates the OIDC `ACTIONS_ID_TOKEN_REQUEST_*` surface.
pub const GITHUB_PERMISSION_SCOPES: &[&str] = &[
    "actions",
    "attestations",
    "checks",
    "contents",
    "deployments",
    "discussions",
    "id-token",
    "issues",
    "models",
    "packages",
    "pages",
    "pull-requests",
    "repository-projects",
    "security-events",
    "statuses",
];

/// Parsed `permissions:` block — `contents` stays promoted (git/LFS token
/// path); `scopes` carries the remaining GitHub families with explicit
/// `none|read|write` levels. A map resets unlisted scopes to `none`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkflowPermissions {
    pub contents: ContentsPermission,
    pub scopes: std::collections::BTreeMap<String, ContentsPermission>,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorkflowTriggers {
    pub push: bool,
    pub pull_request: bool,
    /// `on.pull_request_target` — trusted-context PR runs; dispatch requires
    /// the repository `actions_pull_request_target` opt-in.
    pub pull_request_target: bool,
    pub workflow_dispatch: bool,
    /// `on.release` — fires from the release lifecycle hooks.
    pub release: bool,
    /// `on.issues` — fires from the issue lifecycle hooks.
    pub issues: bool,
    /// `on.schedule` — fires from the cron sweeper (default branch only).
    pub schedule: bool,
    /// Filter detail under `on.push` — empty = match-all.
    pub push_filters: EventFilters,
    /// Filter detail under `on.pull_request`.
    pub pull_request_filters: EventFilters,
    /// Filter detail under `on.pull_request_target`.
    pub pull_request_target_filters: EventFilters,
    /// `on.release.types` — empty = GitHub's default subset
    /// (published/created/edited/prereleased/released).
    pub release_types: Vec<String>,
    /// `on.issues.types` — empty = GitHub's default (all activity types).
    pub issues_types: Vec<String>,
    /// `on.schedule[].cron` entries.
    pub schedule_crons: Vec<String>,
    /// `on.workflow_dispatch.inputs` — `{name: {type, default, required,
    /// options, description}}` normalized to JSON for the dispatch RPC.
    pub dispatch_inputs: Option<JsonValue>,
}

/// `on.<event>` filter detail. GitHub's semantics per key:
/// - `branches`/`tags`/`paths`/`types` — at least one pattern must match;
///   empty = no constraint.
/// - `*-ignore` variants — fire only when NO pattern matches; GitHub forbids
///   mixing a positive list with its `-ignore` twin on the same event.
/// - `types` applies to `pull_request`/`pull_request_target` (activity types
///   like `opened`/`synchronize`/`reopened`); meaningless on `push`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventFilters {
    pub branches: Vec<String>,
    pub branches_ignore: Vec<String>,
    pub tags: Vec<String>,
    pub tags_ignore: Vec<String>,
    pub paths: Vec<String>,
    pub paths_ignore: Vec<String>,
    pub types: Vec<String>,
}

impl EventFilters {
    /// True when no constraint is set (bare `on: push` etc.).
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
            && self.branches_ignore.is_empty()
            && self.tags.is_empty()
            && self.tags_ignore.is_empty()
            && self.paths.is_empty()
            && self.paths_ignore.is_empty()
            && self.types.is_empty()
    }
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
    /// `jobs.<id>.needs` — upstream job keys; non-empty needs defer `if`
    /// evaluation to ready-time (the `needs.<id>.result/outputs` context
    /// only exists once the upstream legs finish).
    pub needs: Vec<String>,
    /// `jobs.<id>.strategy` — matrix expansion happens at enqueue into N
    /// job rows sharing `job_key` with distinct `matrix_key` legs.
    pub strategy: Option<StrategySpec>,
    /// `jobs.<id>.container` — string (`image`) or object form, normalized
    /// JSON; expressions evaluated at enqueue, executed by the runner.
    pub container: Option<JsonValue>,
    /// `jobs.<id>.services` — `{name: {image, env, ports, …}}` map.
    pub services: Option<JsonValue>,
    /// `jobs.<id>.environment` — scalar name or `{name, url}`; name/url may
    /// interpolate and are evaluated at enqueue.
    pub environment: Option<EnvironmentSpec>,
    /// The raw YAML subtree for this job, re-marshaled at parse. The Gitea
    /// actions-proto surface embeds it into the single-job workflow payload
    /// stock act_runner executes — fields we don't model still reach the
    /// runner verbatim.
    pub raw_yaml: String,
}

/// `environment:` shape — a bare name string or `{name, url}` mapping.
/// Fields stay expression-source strings; enqueue interpolates them.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentSpec {
    pub name: String,
    pub url: Option<String>,
}

/// `strategy:` shape — `matrix` is an evaluatable map of `key → [values]`
/// (embedded `${{ }}` against github/env/vars/inputs); `include`/`exclude`
/// ride inside the matrix map itself, per GitHub.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategySpec {
    pub matrix: Option<JsonValue>,
    /// `fail-fast` — bool literal or expression source; default true.
    pub fail_fast: Option<JsonValue>,
    /// `max-parallel` — int literal or expression source; default unbounded.
    pub max_parallel: Option<JsonValue>,
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
    /// `steps.*.continue-on-error` — bool or `${{ }}` source; the runner
    /// evaluates and applies failure-suppression semantics.
    #[serde(rename = "continue-on-error", skip_serializing_if = "Option::is_none")]
    pub continue_on_error: Option<JsonValue>,
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

/// `needs:` — a job-id string or a list of job-id strings. Job-ids are
/// literal (GitHub doesn't allow expressions here); anything else is a
/// parse error.
fn parse_needs(raw: Option<&serde_yaml::Value>, job_id: &str) -> Result<Vec<String>, ParseError> {
    let bad = |v: &serde_yaml::Value| ParseError {
        message: format!("job '{job_id}': needs must be a job id or a list of job ids, got {v:?}"),
    };
    match raw {
        None => Ok(Vec::new()),
        Some(serde_yaml::Value::String(s)) => Ok(vec![s.clone()]),
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .map(|v| match v {
                serde_yaml::Value::String(s) => Ok(s.clone()),
                other => Err(bad(other)),
            })
            .collect(),
        Some(other) => Err(bad(other)),
    }
}

/// `strategy:` — keep `matrix`/`fail-fast`/`max-parallel` as evaluatable
/// JSON values; `${{ }}` inside them is syntax-checked but resolved at
/// enqueue (`github`/`env`/`vars`/`inputs` contexts — `needs.*` matrices
/// need the scheduler, flagged separately there).
fn parse_strategy(
    raw: Option<&serde_yaml::Value>,
    job_id: &str,
) -> Result<Option<StrategySpec>, ParseError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let serde_yaml::Value::Mapping(map) = raw else {
        return Err(ParseError {
            message: format!("job '{job_id}': strategy must be a mapping"),
        });
    };
    let mut s = StrategySpec {
        matrix: None,
        fail_fast: None,
        max_parallel: None,
    };
    for (k, v) in map {
        match k.as_str() {
            Some("matrix") => s.matrix = Some(yaml_to_json(v.clone())?),
            Some("fail-fast") => s.fail_fast = Some(yaml_to_json(v.clone())?),
            Some("max-parallel") => s.max_parallel = Some(yaml_to_json(v.clone())?),
            _ => {}
        }
    }
    Ok(Some(s))
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

fn parse_triggers(on: Option<RawOn>) -> (WorkflowTriggers, Vec<String>, Vec<String>) {
    let mut t = WorkflowTriggers::default();
    let unsupported = Vec::new();
    let mut warnings = Vec::new();
    match on {
        None => {}
        Some(RawOn::Single(s)) => apply_event(&mut t, &s, None),
        Some(RawOn::List(list)) => {
            for s in list {
                apply_event(&mut t, &s, None);
            }
        }
        Some(RawOn::Map(map)) => {
            for (k, v) in &map {
                if let serde_yaml::Value::String(s) = k {
                    apply_event(&mut t, s, Some(v));
                }
            }
        }
    }
    // GitHub forbids mixing a positive filter list with its `-ignore` twin on
    // the same event — surface it as a validation failure rather than
    // silently honoring one side.
    for (event, f) in [
        ("push", &t.push_filters),
        ("pull_request", &t.pull_request_filters),
        ("pull_request_target", &t.pull_request_target_filters),
    ] {
        if !f.branches.is_empty() && !f.branches_ignore.is_empty() {
            warnings.push(format!(
                "on.{event}: 'branches' and 'branches-ignore' are mutually exclusive"
            ));
        }
        if !f.tags.is_empty() && !f.tags_ignore.is_empty() {
            warnings.push(format!(
                "on.{event}: 'tags' and 'tags-ignore' are mutually exclusive"
            ));
        }
        if !f.paths.is_empty() && !f.paths_ignore.is_empty() {
            warnings.push(format!(
                "on.{event}: 'paths' and 'paths-ignore' are mutually exclusive"
            ));
        }
    }
    (t, unsupported, warnings)
}

fn apply_event(t: &mut WorkflowTriggers, name: &str, value: Option<&serde_yaml::Value>) {
    match name.trim() {
        "push" => {
            t.push = true;
            t.push_filters = parse_event_filters(value);
        }
        "pull_request" => {
            t.pull_request = true;
            t.pull_request_filters = parse_event_filters(value);
        }
        "pull_request_target" => {
            t.pull_request_target = true;
            t.pull_request_target_filters = parse_event_filters(value);
        }
        "workflow_dispatch" => {
            t.workflow_dispatch = true;
            if let Some(serde_yaml::Value::Mapping(m)) = value {
                if let Some(inputs) = m.get(serde_yaml::Value::String("inputs".into())) {
                    t.dispatch_inputs = serde_json::to_value(inputs).ok();
                }
            }
        }
        "schedule" => {
            t.schedule = true;
            // `on.schedule` is a list of `{cron: "…"}` entries.
            if let Some(serde_yaml::Value::Sequence(items)) = value {
                for item in items {
                    if let Some(cron) = item
                        .get(serde_yaml::Value::String("cron".into()))
                        .and_then(|v| v.as_str())
                    {
                        t.schedule_crons.push(cron.trim().to_string());
                    }
                }
            }
        }
        "release" => {
            t.release = true;
            t.release_types = parse_event_filters(value).types;
        }
        "issues" => {
            t.issues = true;
            t.issues_types = parse_event_filters(value).types;
        }
        _ => {
            // Other unimplemented events (fork, watch, discussion, …) simply
            // never fire — harmless to ignore, not mis-executed.
        }
    }
}

/// Parse the `{branches, tags, paths, types, *-ignore}` mapping under an
/// `on.<event>` key. Bare `on: push:` (null value) → empty filters.
fn parse_event_filters(value: Option<&serde_yaml::Value>) -> EventFilters {
    let mut f = EventFilters::default();
    let Some(serde_yaml::Value::Mapping(m)) = value else {
        return f;
    };
    for (k, v) in m {
        let Some(key) = k.as_str() else { continue };
        let list = yaml_string_list(v);
        match key.trim() {
            "branches" => f.branches = list,
            "branches-ignore" => f.branches_ignore = list,
            "tags" => f.tags = list,
            "tags-ignore" => f.tags_ignore = list,
            "paths" => f.paths = list,
            "paths-ignore" => f.paths_ignore = list,
            "types" => f.types = list,
            _ => {}
        }
    }
    f
}

/// YAML scalar-or-sequence → Vec<String> (filter keys accept both shapes).
fn yaml_string_list(v: &serde_yaml::Value) -> Vec<String> {
    match v {
        serde_yaml::Value::String(s) => vec![s.clone()],
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Parse a `permissions:` value (full GitHub key set).
///
/// - `read-all` / `write-all` scalars grant every scope that level.
/// - A map sets each listed scope (`read`/`write`/`none`); per GitHub
///   semantics, specifying a map resets unlisted scopes to `none`, so a map
///   without `contents` — including `{}` — yields `contents: none`.
/// - Unknown scope keys fail loud: the caller flags them `unsupported`.
/// - `None` (absent) or an unrecognized shape → `None` ("not specified").
fn parse_permissions(
    raw: Option<&serde_yaml::Value>,
    unsupported: &mut Vec<String>,
) -> Option<WorkflowPermissions> {
    fn all(level: ContentsPermission) -> WorkflowPermissions {
        WorkflowPermissions {
            contents: level,
            scopes: GITHUB_PERMISSION_SCOPES
                .iter()
                .filter(|s| **s != "contents")
                .map(|s| (s.to_string(), level))
                .collect(),
        }
    }
    match raw {
        None => None,
        Some(serde_yaml::Value::String(s)) => match s.trim() {
            "read-all" => Some(all(ContentsPermission::Read)),
            "write-all" => Some(all(ContentsPermission::Write)),
            _ => None,
        },
        Some(serde_yaml::Value::Mapping(map)) => {
            let mut contents = ContentsPermission::None;
            let mut scopes = std::collections::BTreeMap::new();
            for (k, v) in map {
                let Some(key) = k.as_str() else { continue };
                let level = match v.as_str().map(str::trim) {
                    Some("read") => ContentsPermission::Read,
                    Some("write") => ContentsPermission::Write,
                    _ => ContentsPermission::None,
                };
                if key == "contents" {
                    contents = level;
                } else if GITHUB_PERMISSION_SCOPES.contains(&key) {
                    scopes.insert(key.to_string(), level);
                } else {
                    flag(unsupported, &format!("permissions:{key}"));
                }
            }
            Some(WorkflowPermissions { contents, scopes })
        }
        Some(_) => None,
    }
}

/// Effective `contents` permission for a job: job-level block wins, then
/// workflow-level, else `read` (the instance default).
pub fn effective_contents_permission(doc: &WorkflowDocument, job: &JobSpec) -> ContentsPermission {
    job.permissions
        .as_ref()
        .map(|p| p.contents)
        .or_else(|| doc.permissions.as_ref().map(|p| p.contents))
        .unwrap_or(ContentsPermission::Read)
}

/// The resolved GitHub scope set for a job — `{scope: none|read|write}` with
/// every declared scope present. `default_permissions` is the repo's
/// "Workflow permissions" setting (`read` restricted / `write` permissive)
/// applied when neither job nor workflow declares `permissions:`.
///
/// Resolution (GitHub parity): a job-level `permissions:` block replaces the
/// workflow-level one entirely — a map resets every unlisted scope to none.
/// Absent both → `read` grants contents+packages read, `write` grants every
/// scope write (GitHub's permissive default).
pub fn effective_permissions(
    doc: &WorkflowDocument,
    job: &JobSpec,
    default_permissions: &str,
) -> std::collections::BTreeMap<String, ContentsPermission> {
    let block = job.permissions.as_ref().or(doc.permissions.as_ref());
    let mut out: std::collections::BTreeMap<String, ContentsPermission> = match block {
        Some(p) => {
            let mut m = p.scopes.clone();
            m.insert("contents".to_string(), p.contents);
            m
        }
        None => {
            let mut m = std::collections::BTreeMap::new();
            if default_permissions == "write" {
                for s in GITHUB_PERMISSION_SCOPES {
                    m.insert(s.to_string(), ContentsPermission::Write);
                }
            } else {
                m.insert("contents".to_string(), ContentsPermission::Read);
                m.insert("packages".to_string(), ContentsPermission::Read);
            }
            m
        }
    };
    // A declared block resets unlisted scopes to none — fill the gaps so the
    // frozen payload always carries the complete scope set.
    for s in GITHUB_PERMISSION_SCOPES {
        out.entry(s.to_string()).or_insert(ContentsPermission::None);
    }
    out
}

/// `environment:` — scalar name or `{name, url}`. Unknown shapes warn (the
/// job still parses; the environment simply doesn't apply).
fn parse_environment(
    raw: Option<&serde_yaml::Value>,
    warnings: &mut Vec<String>,
) -> Option<EnvironmentSpec> {
    match raw {
        None => None,
        Some(serde_yaml::Value::String(s)) => {
            let name = s.trim().to_string();
            (!name.is_empty()).then_some(EnvironmentSpec { name, url: None })
        }
        Some(serde_yaml::Value::Mapping(map)) => {
            let name = map
                .get(serde_yaml::Value::String("name".into()))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            let url = map
                .get(serde_yaml::Value::String("url".into()))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if name.is_empty() {
                warnings.push("environment mapping lacks a name".to_string());
                None
            } else {
                Some(EnvironmentSpec { name, url })
            }
        }
        Some(_) => {
            warnings.push("unsupported environment shape".to_string());
            None
        }
    }
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

    let (triggers, mut unsupported, mut warnings) = parse_triggers(raw.on);
    let wf_concurrency = parse_concurrency(raw.concurrency.as_ref());
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
        let job_yaml = serde_yaml::to_string(&value).unwrap_or_default();
        let job: RawJob = serde_yaml::from_value(value).map_err(|e| ParseError {
            message: format!("invalid job '{id}': {e}"),
        })?;
        // Structural features that would mis-execute — fail loudly at dispatch.
        if job.continue_on_error.is_some() {
            flag(&mut unsupported, "continue-on-error");
        }
        // `uses:`/`with:`/`secrets:` on a job is the reusable-workflow call shape.
        if job.uses.is_some() || job.with.is_some() || job.secrets.is_some() {
            flag(&mut unsupported, "reusable-workflow");
        }
        let job_concurrency = parse_concurrency(job.concurrency.as_ref());
        let needs = parse_needs(job.needs.as_ref(), &id)?;
        let strategy = parse_strategy(job.strategy.as_ref(), &id)?;
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
        if let Some(s) = &strategy {
            for v in [&s.matrix, &s.fail_fast, &s.max_parallel]
                .into_iter()
                .flatten()
            {
                check_json(v, &mut unsupported, &mut warnings);
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
        let job_container = job.container.map(yaml_to_json).transpose()?;
        if let Some(v) = &job_container {
            check_json(v, &mut unsupported, &mut warnings);
        }
        let job_services = job.services.map(yaml_to_json).transpose()?;
        if let Some(v) = &job_services {
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
            if step.with.is_some() && step.uses.is_none() {
                flag(&mut warnings, "with on run: step has no effect");
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
                continue_on_error: step.continue_on_error.map(yaml_to_json).transpose()?,
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
            permissions: parse_permissions(job.permissions.as_ref(), &mut unsupported),
            if_expr: expr_src_opt(&job.if_expr),
            timeout_minutes: expr_src_opt(&job.timeout_minutes),
            concurrency: job_concurrency,
            outputs: job_outputs,
            needs,
            strategy,
            container: job_container,
            services: job_services,
            environment: parse_environment(job.environment.as_ref(), &mut warnings),
            raw_yaml: job_yaml,
        });
    }

    // `needs:` references must resolve to declared job ids — GitHub fails
    // validation; we flag it loud rather than wedge a `waiting` job forever.
    let job_ids: std::collections::BTreeSet<&str> = jobs.iter().map(|j| j.id.as_str()).collect();
    for j in &jobs {
        for n in &j.needs {
            if !job_ids.contains(n.as_str()) {
                flag(&mut unsupported, "needs");
                warnings.push(format!("job '{}' needs unknown job '{n}'", j.id));
            }
        }
    }
    // Cyclic `needs:` would wedge every member in `waiting` — GitHub
    // rejects the file; flag it loud.
    {
        let mut state: std::collections::HashMap<&str, u8> = std::collections::HashMap::new(); // 0=visiting,1=done
        fn visit<'a>(
            id: &'a str,
            graph: &std::collections::HashMap<&'a str, &'a [String]>,
            state: &mut std::collections::HashMap<&'a str, u8>,
        ) -> bool {
            match state.get(id) {
                Some(0) => return true,  // back edge — cycle
                Some(_) => return false, // already fully explored
                None => {}
            }
            state.insert(id, 0);
            let cyclic = graph
                .get(id)
                .into_iter()
                .flat_map(|ns| ns.iter().map(String::as_str))
                .filter(|n| graph.contains_key(n))
                .any(|n| visit(n, graph, state));
            state.insert(id, 1);
            cyclic
        }
        let graph: std::collections::HashMap<&str, &[String]> = jobs
            .iter()
            .map(|j| (j.id.as_str(), j.needs.as_slice()))
            .collect();
        if jobs
            .iter()
            .any(|j| visit(j.id.as_str(), &graph, &mut state))
        {
            flag(&mut unsupported, "needs");
            warnings.push("cyclic needs graph".to_string());
        }
    }

    unsupported.sort();
    warnings.sort();
    warnings.dedup();
    Ok(WorkflowDocument {
        name,
        triggers,
        jobs,
        env: wf_env,
        permissions: parse_permissions(raw.permissions.as_ref(), &mut unsupported),
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
        (doc.permissions, doc.jobs[0].permissions.clone())
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
        let wf = wf.unwrap();
        assert_eq!(wf.contents, ContentsPermission::Read);
        // read-all lifts every scope.
        assert_eq!(wf.scopes.get("id-token"), Some(&ContentsPermission::Read));

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
        let wf = wf.unwrap();
        assert_eq!(wf.contents, ContentsPermission::Write);
        assert_eq!(
            wf.scopes.get("pull-requests"),
            Some(&ContentsPermission::Write)
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
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        );
        let wf = wf.unwrap();
        assert_eq!(wf.contents, ContentsPermission::Write);
        assert_eq!(wf.scopes.get("id-token"), Some(&ContentsPermission::Write));
        // Unlisted scopes are not recorded on the block — resolution fills
        // them as `none` (GitHub's map-reset semantics).
        assert!(!wf.scopes.contains_key("issues"));
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
        let wf = wf.unwrap();
        assert_eq!(wf.contents, ContentsPermission::None);
        assert_eq!(wf.scopes.get("issues"), Some(&ContentsPermission::Write));
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
        assert_eq!(wf.unwrap().contents, ContentsPermission::None);
    }

    #[test]
    fn effective_permissions_resolution() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
permissions:
  contents: write
  id-token: write
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        let m = effective_permissions(&doc, &doc.jobs[0], "read");
        assert_eq!(m.get("contents"), Some(&ContentsPermission::Write));
        assert_eq!(m.get("id-token"), Some(&ContentsPermission::Write));
        // Declared map — unlisted scopes resolve to none.
        assert_eq!(m.get("issues"), Some(&ContentsPermission::None));

        // No `permissions:` anywhere — 'read' default: contents+packages.
        let doc2 = parse_workflow_yaml(
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
        let m = effective_permissions(&doc2, &doc2.jobs[0], "read");
        assert_eq!(m.get("contents"), Some(&ContentsPermission::Read));
        assert_eq!(m.get("packages"), Some(&ContentsPermission::Read));
        assert_eq!(m.get("id-token"), Some(&ContentsPermission::None));
        // 'write' (permissive) default — every scope write.
        let m = effective_permissions(&doc2, &doc2.jobs[0], "write");
        assert_eq!(m.get("statuses"), Some(&ContentsPermission::Write));
        assert_eq!(m.get("id-token"), Some(&ContentsPermission::Write));
    }

    #[test]
    fn environment_parses_scalar_and_mapping() {
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
jobs:
  build:
    environment: production
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        assert_eq!(
            doc.jobs[0].environment.as_ref().map(|e| e.name.as_str()),
            Some("production")
        );
        let doc = parse_workflow_yaml(
            br#"
name: CI
on: [push]
jobs:
  build:
    environment:
      name: staging
      url: https://staging.example.com
    runs-on: ubuntu-latest
    steps:
      - run: echo hi
"#,
        )
        .expect("parse");
        let env = doc.jobs[0].environment.as_ref().unwrap();
        assert_eq!(env.name, "staging");
        assert_eq!(env.url.as_deref(), Some("https://staging.example.com"));
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
    needs: b
    steps:
      - run: echo hi
  b:
    runs-on: ubuntu-latest
    needs: a
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
    container:
      image: node:20
      env:
        INNER: "yes"
    services:
      pg:
        image: postgres:16
        ports:
          - 5432:5432
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
        // `with:` applies to every uses: step now — nothing left to warn on.
        assert!(doc.warnings.is_empty());
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
        // container:/services: are captured for the runner, not unsupported.
        assert_eq!(
            doc.jobs[0].container.as_ref().unwrap()["image"],
            serde_json::json!("node:20")
        );
        assert_eq!(
            doc.jobs[0].services.as_ref().unwrap()["pg"]["ports"],
            serde_json::json!(["5432:5432"])
        );
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
