use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oxidean_core::{PullCodeownersPathStatus, PullCodeownersStatus};
use oxidean_db::{Database, PullReviewRow, PullRow, RepositoryRow};
use oxidean_git::{GitBackend, GitError};

use crate::git::bare_repo_path;
use crate::repo::{self, Capability, OwnerRef};

use super::codeowners_rules::{Codeowners, Owner, MAX_CHANGED_PATHS};

const MAX_DECLARED_OWNERS: usize = 500;
const MAX_CODEOWNER_REVIEWERS: usize = 500;
const MAX_TEAM_REVIEWERS: usize = 200;

#[derive(Debug, Default)]
pub struct Evaluation {
    pub status: PullCodeownersStatus,
    pub requested_user_ids: BTreeSet<String>,
    pub approved: bool,
}

pub fn current_head_approved_user_ids(
    reviews: &[PullReviewRow],
    head_sha: &str,
) -> BTreeSet<String> {
    super::review_order::latest_unambiguous_reviews_by_author(reviews)
        .into_iter()
        .filter(|review| {
            review.state == "approved" && review.commit_sha.as_deref() == Some(head_sha)
        })
        .map(|review| review.author_id.clone())
        .collect()
}
pub struct EvaluationInput<'a> {
    pub repository: &'a RepositoryRow,
    pub base_owner: &'a str,
    pub pull: &'a PullRow,
    pub approved_user_ids: &'a BTreeSet<String>,
    pub required: bool,
}

pub async fn evaluate(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    input: EvaluationInput<'_>,
) -> Result<Evaluation, String> {
    let EvaluationInput {
        repository,
        base_owner,
        pull,
        approved_user_ids,
        required,
    } = input;
    let mut evaluation = Evaluation::default();
    evaluation.status.required = required;
    evaluation.approved = true;
    let bare = bare_repo_path(repos_dir, base_owner, &repository.name)
        .map_err(|_| "repository path could not be resolved".to_string())?;
    let bytes = match git
        .cat_blob(&bare, &pull.base_sha, ".github/CODEOWNERS")
        .await
    {
        Ok(bytes) => bytes,
        Err(GitError::NotFound(_)) => {
            if matches!(git.commit_exists(&bare, &pull.base_sha).await, Ok(true)) {
                return Ok(evaluation);
            }
            evaluation
                .status
                .errors
                .push("could not read .github/CODEOWNERS from the pull request base".into());
            evaluation.approved = !required;
            return Ok(evaluation);
        }
        Err(_) => {
            evaluation
                .status
                .errors
                .push("could not read .github/CODEOWNERS from the pull request base".into());
            evaluation.approved = !required;
            return Ok(evaluation);
        }
    };
    evaluation.status.file_present = true;
    let parsed = Codeowners::parse(&bytes);
    evaluation
        .status
        .errors
        .extend(parsed.errors.iter().map(|error| match error.line {
            Some(line) => format!("line {line}: {}", error.message),
            None => error.message.clone(),
        }));
    let changed_paths = match git
        .changed_paths(&bare, &pull.base_sha, &pull.head_sha, MAX_CHANGED_PATHS)
        .await
    {
        Ok(paths) => paths,
        Err(_) => {
            evaluation
                .status
                .errors
                .push("could not read changed paths for CODEOWNERS evaluation".into());
            evaluation.approved = !required;
            return Ok(evaluation);
        }
    };
    if changed_paths.truncated {
        evaluation
            .status
            .errors
            .push("changed path set exceeds configured CODEOWNERS limits".into());
        evaluation.approved = !required;
        return Ok(evaluation);
    }
    let Some(owner) = repo::owner_ref_for_repo(db, repository).await? else {
        evaluation
            .status
            .errors
            .push("repository owner could not be resolved".into());
        evaluation.approved = !required;
        return Ok(evaluation);
    };
    let mut resolved_owners: BTreeMap<Owner, Result<Vec<String>, String>> = BTreeMap::new();

    let paths = changed_paths.paths;
    let declared_by_path = match parsed.owners_for_paths(&paths) {
        Ok(owners) => owners,
        Err(error) => {
            evaluation.status.errors.push(error);
            evaluation.approved = !required;
            return Ok(evaluation);
        }
    };
    if distinct_owner_count(&declared_by_path) > MAX_DECLARED_OWNERS {
        evaluation.status.errors.push(format!(
            "CODEOWNERS evaluation exceeds {MAX_DECLARED_OWNERS} distinct owners"
        ));
        evaluation.approved = !required;
        return Ok(evaluation);
    }

    for (path, declared) in paths.into_iter().zip(declared_by_path) {
        let declared_handles = declared.iter().map(owner_handle).collect::<Vec<_>>();
        if declared.is_empty() {
            evaluation.status.paths.push(PullCodeownersPathStatus {
                path,
                owners: declared_handles,
                approved: true,
            });
            continue;
        }
        let mut owner_ids = BTreeSet::new();
        let mut owner_resolution_failed = false;
        for declared_owner in declared {
            let resolved = if let Some(cached) = resolved_owners.get(&declared_owner) {
                cached.clone()
            } else {
                let resolved =
                    resolve_owner(db, repository, &owner, base_owner, declared_owner.clone()).await;
                resolved_owners.insert(declared_owner, resolved.clone());
                resolved
            };
            match resolved {
                Ok(resolved) => {
                    owner_ids.extend(
                        resolved
                            .into_iter()
                            .filter(|user_id| user_id != &pull.author_id),
                    );
                }
                Err(error) => {
                    owner_resolution_failed = true;
                    evaluation.status.errors.push(format!("{}: {error}", path));
                }
            }
        }
        if owner_ids.is_empty() && !owner_resolution_failed {
            evaluation.status.errors.push(format!(
                "{}: no eligible code owner can approve this pull request",
                path
            ));
        }
        let is_approved = !owner_ids.is_empty()
            && owner_ids
                .iter()
                .any(|user_id| approved_user_ids.contains(user_id));
        if required && !is_approved {
            evaluation.approved = false;
        }
        evaluation
            .requested_user_ids
            .extend(owner_ids.iter().cloned());
        if evaluation.requested_user_ids.len() > MAX_CODEOWNER_REVIEWERS {
            evaluation.status.errors.push(format!(
                "CODEOWNERS evaluation exceeds {MAX_CODEOWNER_REVIEWERS} reviewer requests"
            ));
            evaluation.requested_user_ids.clear();
            evaluation.status.paths.clear();
            evaluation.approved = !required;
            return Ok(evaluation);
        }
        evaluation.status.paths.push(PullCodeownersPathStatus {
            path,
            owners: declared_handles,
            approved: is_approved,
        });
    }
    if required && !evaluation.status.errors.is_empty() {
        evaluation.approved = false;
    }
    Ok(evaluation)
}

fn distinct_owner_count(paths: &[Vec<Owner>]) -> usize {
    paths.iter().flatten().collect::<BTreeSet<_>>().len()
}

fn owner_handle(owner: &Owner) -> String {
    match owner {
        Owner::User(username) => format!("@{username}"),
        Owner::Team { organization, slug } => format!("@{organization}/{slug}"),
    }
}

async fn resolve_owner(
    db: &Database,
    repository: &RepositoryRow,
    owner: &OwnerRef,
    base_owner: &str,
    declared: Owner,
) -> Result<Vec<String>, String> {
    match declared {
        Owner::User(username) => {
            let user = db
                .find_user_by_username(&username)
                .await
                .map_err(|_| "owner access could not be verified".to_string())?
                .ok_or_else(|| format!("owner @{username} is unknown or lacks write access"))?;
            if user.banned_at.is_some() {
                return Err(format!(
                    "owner @{username} is unknown or lacks write access"
                ));
            }
            let capability = repo::effective_capability(db, Some(&user.id), repository, owner)
                .await
                .map_err(|_| "owner access could not be verified".to_string())?;
            if !repo::meets(capability, Capability::Write) {
                return Err(format!(
                    "owner @{username} is unknown or lacks write access"
                ));
            }
            Ok(vec![user.id])
        }
        Owner::Team { organization, slug } => {
            let OwnerRef::Org {
                id: org_id,
                slug: repo_org,
            } = owner
            else {
                return Err(format!(
                    "@{organization}/{slug} is not a team in this repository's organization"
                ));
            };
            if !organization.eq_ignore_ascii_case(repo_org)
                || !organization.eq_ignore_ascii_case(base_owner)
            {
                return Err(format!(
                    "@{organization}/{slug} is not a team in this repository's organization"
                ));
            }
            let team = db
                .find_org_team_by_slug(org_id, &slug)
                .await
                .map_err(|_| "team ownership could not be verified".to_string())?
                .ok_or_else(|| {
                    format!("team owner @{organization}/{slug} is unknown or lacks write access")
                })?;
            let grant = db
                .find_team_repo(&team.id, &repository.id)
                .await
                .map_err(|_| "team access could not be verified".to_string())?;
            if !grant.is_some_and(|grant| {
                grant.permission.eq_ignore_ascii_case("write")
                    || grant.permission.eq_ignore_ascii_case("admin")
            }) {
                return Err(format!(
                    "team owner @{organization}/{slug} is unknown or lacks write access"
                ));
            }
            let members = db
                .list_team_members_limited(&team.id, (MAX_TEAM_REVIEWERS + 1) as i64)
                .await
                .map_err(|_| "team membership could not be verified".to_string())?;
            if members.len() > MAX_TEAM_REVIEWERS {
                return Err(format!(
                    "team owner @{organization}/{slug} exceeds the {MAX_TEAM_REVIEWERS}-reviewer limit"
                ));
            }
            let mut eligible = Vec::new();
            for member in members {
                let Some(user) = db
                    .find_user_by_id(&member.user_id)
                    .await
                    .map_err(|_| "team membership could not be verified".to_string())?
                else {
                    continue;
                };
                if user.banned_at.is_some() {
                    continue;
                }
                let capability = repo::effective_capability(db, Some(&user.id), repository, owner)
                    .await
                    .map_err(|_| "team member access could not be verified".to_string())?;
                if repo::meets(capability, Capability::Write) {
                    eligible.push(user.id);
                }
            }
            if eligible.is_empty() {
                return Err(format!(
                    "team @{organization}/{slug} has no eligible write-capable members"
                ));
            }
            Ok(eligible)
        }
    }
}

pub struct ReviewerRequestBatch {
    pub added_user_ids: Vec<String>,
    pub error: Option<String>,
}

pub async fn request_reviewers(
    db: &Database,
    pull_id: &str,
    requested_by: &str,
    evaluation: &Evaluation,
) -> ReviewerRequestBatch {
    let mut result = ReviewerRequestBatch {
        added_user_ids: Vec::new(),
        error: None,
    };
    for user_id in &evaluation.requested_user_ids {
        let inserted = db
            .upsert_pull_review_request(pull_id, user_id, requested_by)
            .await;
        record_review_request(&mut result, user_id, inserted);
    }
    result
}

fn record_review_request(
    result: &mut ReviewerRequestBatch,
    user_id: &str,
    inserted: Result<bool, String>,
) {
    match inserted {
        Ok(true) => result.added_user_ids.push(user_id.to_string()),
        Ok(false) => {}
        Err(error) => {
            if result.error.is_none() {
                result.error = Some(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        current_head_approved_user_ids, distinct_owner_count, record_review_request,
        ReviewerRequestBatch, MAX_DECLARED_OWNERS,
    };
    use crate::pull::codeowners_rules::Owner;
    use oxidean_db::PullReviewRow;

    fn review(id: &str, state: &str, submission_order: Option<i64>) -> PullReviewRow {
        review_at(id, state, submission_order, "2026-11-22T12:00:00Z")
    }

    fn review_at(
        id: &str,
        state: &str,
        submission_order: Option<i64>,
        submitted_at: &str,
    ) -> PullReviewRow {
        PullReviewRow {
            id: id.into(),
            pull_id: "pull".into(),
            author_id: "reviewer".into(),
            state: state.into(),
            body: String::new(),
            commit_sha: Some("head".into()),
            submitted_at: submitted_at.into(),
            submission_order,
            dismissed_at: None,
            dismiss_reason: None,
        }
    }

    #[test]
    fn distinct_owner_budget_deduplicates_and_caps_owner_resolution() {
        let paths = (0..=MAX_DECLARED_OWNERS)
            .map(|index| vec![Owner::User(format!("owner-{index}"))])
            .collect::<Vec<_>>();
        assert_eq!(distinct_owner_count(&paths), MAX_DECLARED_OWNERS + 1);
        assert!(distinct_owner_count(&paths) > MAX_DECLARED_OWNERS);

        let repeated = vec![vec![Owner::User("same".into())]; MAX_DECLARED_OWNERS + 1];
        assert_eq!(distinct_owner_count(&repeated), 1);
    }

    #[test]
    fn failed_request_keeps_prior_insertions_for_notification() {
        let mut result = ReviewerRequestBatch {
            added_user_ids: Vec::new(),
            error: None,
        };
        record_review_request(&mut result, "first-owner", Ok(true));
        record_review_request(&mut result, "second-owner", Err("database failure".into()));

        assert_eq!(result.added_user_ids, ["first-owner"]);
        assert_eq!(result.error.as_deref(), Some("database failure"));
    }

    #[test]
    fn later_same_second_review_replaces_the_earlier_review() {
        let approved_then_changes = [
            review("b", "approved", Some(1)),
            review("a", "changes_requested", Some(2)),
        ];
        assert!(current_head_approved_user_ids(&approved_then_changes, "head").is_empty());

        let changes_then_approved = [
            review("b", "changes_requested", Some(1)),
            review("a", "approved", Some(2)),
        ];
        assert_eq!(
            current_head_approved_user_ids(&changes_then_approved, "head"),
            BTreeSet::from(["reviewer".to_string()])
        );
    }

    #[test]
    fn submission_order_precedes_transaction_start_timestamp() {
        let reviews = [
            review_at("approval", "approved", Some(1), "2026-11-22T12:00:02Z"),
            review_at(
                "changes",
                "changes_requested",
                Some(2),
                "2026-11-22T12:00:00Z",
            ),
        ];
        assert!(current_head_approved_user_ids(&reviews, "head").is_empty());
    }

    #[test]
    fn ambiguous_legacy_same_second_reviews_do_not_count_as_approval() {
        let reviews = [
            review("b", "approved", None),
            review("a", "changes_requested", None),
        ];
        assert!(current_head_approved_user_ids(&reviews, "head").is_empty());
    }
}
