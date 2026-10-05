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

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowDocument {
    pub name: String,
    pub triggers: WorkflowTriggers,
    pub jobs: Vec<JobSpec>,
    /// Raw `env` map if present (opaque JSON object).
    pub env: Option<JsonValue>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepSpec {
    pub name: Option<String>,
    pub uses: Option<String>,
    pub run: Option<String>,
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    /// Opaque `with` / `env` / `if` — retained for runner; not interpreted in-process.
    pub with: Option<JsonValue>,
    pub env: Option<JsonValue>,
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub if_expr: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawWorkflow {
    name: Option<String>,
    #[serde(rename = "on")]
    on: Option<RawOn>,
    jobs: Option<serde_yaml::Mapping>,
    env: Option<serde_yaml::Value>,
    defaults: Option<RawDefaults>,
}

/// `defaults:` — currently only `run.shell` / `run.working-directory`
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
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawRunsOn {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct RawStep {
    name: Option<String>,
    uses: Option<String>,
    run: Option<String>,
    shell: Option<String>,
    #[serde(rename = "working-directory")]
    working_directory: Option<String>,
    with: Option<serde_yaml::Value>,
    env: Option<serde_yaml::Value>,
    #[serde(rename = "if")]
    if_expr: Option<String>,
}

fn yaml_to_json(v: serde_yaml::Value) -> Result<JsonValue, ParseError> {
    serde_json::to_value(v).map_err(|e| ParseError {
        message: format!("failed to normalize workflow YAML value: {e}"),
    })
}

fn parse_triggers(on: Option<RawOn>) -> WorkflowTriggers {
    let mut t = WorkflowTriggers::default();
    match on {
        None => {}
        Some(RawOn::Single(s)) => apply_event(&mut t, &s),
        Some(RawOn::List(list)) => {
            for s in list {
                apply_event(&mut t, &s);
            }
        }
        Some(RawOn::Map(map)) => {
            for (k, _) in map {
                if let serde_yaml::Value::String(s) = k {
                    apply_event(&mut t, &s);
                }
            }
        }
    }
    t
}

fn apply_event(t: &mut WorkflowTriggers, name: &str) {
    match name.trim() {
        "push" => t.push = true,
        "pull_request" => t.pull_request = true,
        "workflow_dispatch" => t.workflow_dispatch = true,
        _ => {
            // Unsupported triggers ignored for matching; jobs still parse.
        }
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

    let triggers = parse_triggers(raw.on);
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
        let runs_on = parse_runs_on(job.runs_on)?;
        let (job_shell, job_workdir) = run_defaults(job.defaults.as_ref());
        let mut steps = Vec::new();
        for (i, step) in job.steps.unwrap_or_default().into_iter().enumerate() {
            if step.uses.is_none() && step.run.is_none() {
                return Err(ParseError {
                    message: format!("job '{id}' step {i} needs uses or run"),
                });
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
                name: step.name,
                uses: step.uses,
                run: step.run,
                shell,
                working_directory,
                with: step.with.map(yaml_to_json).transpose()?,
                env: step.env.map(yaml_to_json).transpose()?,
                if_expr: step.if_expr,
            });
        }
        jobs.push(JobSpec {
            id,
            name: job.name,
            runs_on,
            steps,
            env: job.env.map(yaml_to_json).transpose()?,
        });
    }

    Ok(WorkflowDocument {
        name,
        triggers,
        jobs,
        env: raw.env.map(yaml_to_json).transpose()?,
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
}
