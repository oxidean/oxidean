//! `user.getContributions` — GitHub-style contribution graph payload for
//! `/{username}` (COL-12). Anonymous-safe: non-self viewers only ever receive
//! public-repo data; the owner's own view includes private repos.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, GetContributionsRequest, UserContributionByType, UserContributionDay,
    UserContributionItem, UserContributionMonth, UserContributionRepo, UserContributionSection,
    UserContributions,
};

/// Item cap per monthly section (insights-page `truncated` convention).
const SECTION_ITEM_CAP: usize = 30;
const DEFAULT_RANGE_DAYS: i64 = 365;
const MAX_RANGE_DAYS: i64 = 366 * 5;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("contributions db error: {e}");
        AppError::new("auth.internal", "contribution lookup failed")
    }
}

fn parse_day(s: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").map_err(|_| {
        AppError::new(
            "rpc.bad_input",
            "from/to must be ISO-8601 dates (YYYY-MM-DD)",
        )
    })
}

fn slug_href(kind: &str, slug: &str, number: i64) -> String {
    match kind {
        "pull_request" | "review" => format!("/{slug}/pull/{number}"),
        _ => format!("/{slug}/issues/{number}"),
    }
}

struct SectionAcc {
    total: i64,
    repos: BTreeMap<String, i64>,
    items: Vec<UserContributionItem>,
}

impl SectionAcc {
    fn new() -> Self {
        Self {
            total: 0,
            repos: BTreeMap::new(),
            items: Vec::new(),
        }
    }

    fn finish(self, kind: &str) -> UserContributionSection {
        let itemized = matches!(kind, "pull_request" | "issue" | "review");
        let truncated = itemized && self.items.len() > SECTION_ITEM_CAP;
        let mut items = self.items;
        items.truncate(SECTION_ITEM_CAP);
        let repos: Vec<UserContributionRepo> = self
            .repos
            .into_iter()
            .map(|(slug, count)| UserContributionRepo { slug, count })
            .collect();
        UserContributionSection {
            kind: section_kind(kind).to_string(),
            total: self.total,
            repo_count: repos_len(&repos),
            repos,
            items,
            truncated,
        }
    }
}

fn repos_len(repos: &[UserContributionRepo]) -> i64 {
    repos.len() as i64
}

/// Public section kind names (`pull_requests`, `issues`, `reviews`).
fn section_kind(kind: &str) -> &'static str {
    match kind {
        "commits" => "commits",
        "pull_request" => "pull_requests",
        "issue" => "issues",
        "review" => "reviews",
        _ => "repositories",
    }
}

/// `user.getContributions` — contribution days, type totals, monthly
/// sections, and the selectable year list for `/{username}`.
pub async fn get_contributions(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<UserContributions, AppError> {
    let req: GetContributionsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid user.getContributions input: {e}"),
        )
    })?;
    let username = req.username.trim();
    if username.is_empty() {
        return Err(AppError::new("user.not_found", "User not found"));
    }
    let user = ctx
        .db
        .find_user_by_username(username)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("user.not_found", "User not found"))?;
    if user.banned_at.is_some() {
        return Err(AppError::new("user.not_found", "User not found"));
    }

    let today = Utc::now().date_naive();
    let to = match req.to.as_deref() {
        Some(s) => parse_day(s)?.min(today),
        None => today,
    };
    let from = match req.from.as_deref() {
        Some(s) => parse_day(s)?,
        None => to - Duration::days(DEFAULT_RANGE_DAYS - 1),
    };
    if to < from || to - from > Duration::days(MAX_RANGE_DAYS) {
        return Err(AppError::new(
            "rpc.bad_input",
            "invalid contribution range (max 5 years, to must be on or after from)",
        ));
    }

    // GitHub's rule: the owner sees private contributions only while their
    // "Include private contributions on my profile" setting is on; every
    // other viewer always gets the public-only view. Bearer callers (PAT,
    // job tokens) carry `ctx.session.user_id` for the token owner but their
    // scopes don't cover every private repo the response can name — restrict
    // the private view to cookie sessions.
    let self_view = ctx.pat.is_none()
        && ctx.session.as_ref().map(|s| s.user_id.as_str()) == Some(user.id.as_str());
    let public_only = !(self_view && user.show_private_contributions);

    // Materialized rollups (COL-12): one-time per-user rebuild from source
    // tables under the `user:<id>` claim, kept fresh afterwards by bumps in
    // the facade insert paths.
    let user_scope = format!("user:{}", user.id);
    if let Ok(true) = ctx.db.claim_contribution_backfill(&user_scope).await {
        if let Err(e) = ctx.db.rebuild_user_contribution_days(&user.id).await {
            tracing::warn!(error = %e, "contribution rollup rebuild failed");
            let _ = ctx.db.release_contribution_backfill(&user_scope).await;
        }
    }

    // Authored-commit backfill: repos the user plausibly committed to get a
    // bounded default-branch history walk, spawned async (claimed per repo
    // so duplicate views never double-walk).
    if let Ok(repos) = ctx.db.list_actor_contribution_repos(&user.id).await {
        let repo_ids: Vec<String> = repos.iter().map(|r| r.id.clone()).collect();
        if let Ok(claimed) = ctx.db.claimed_repo_backfill_scopes(&repo_ids).await {
            let mut spawned = 0;
            for repo in repos {
                if spawned >= 4 {
                    break;
                }
                if claimed.contains(&format!("repo:{}", repo.id)) {
                    continue;
                }
                if let Ok(bare) = oxidean_api_core::git::bare_repo_path(
                    &ctx.repos_dir,
                    &repo.owner_slug,
                    &repo.name,
                ) {
                    let branch = repo
                        .default_branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&repo.default_branch);
                    oxidean_api_core::repo::spawn_contribution_backfill(
                        ctx.db.clone(),
                        ctx.git.clone(),
                        bare,
                        repo.id,
                        format!("refs/heads/{branch}"),
                    );
                    spawned += 1;
                }
            }
        }
    }

    // `to` is inclusive in the payload; the queries take the next day's bound.
    let to_excl = to + Duration::days(1);
    let from_s = from.format("%Y-%m-%d").to_string();
    let to_excl_s = to_excl.format("%Y-%m-%d").to_string();
    let events = ctx
        .db
        .list_user_contribution_events(&user.id, &from_s, &to_excl_s, public_only)
        .await
        .map_err(db_err)?;
    let day_counts = ctx
        .db
        .list_user_contribution_days(&user.id, &from_s, &to_excl_s, public_only)
        .await
        .map_err(db_err)?;

    let mut days: BTreeMap<String, i64> = BTreeMap::new();
    {
        let mut d = from;
        while d <= to {
            days.insert(d.format("%Y-%m-%d").to_string(), 0);
            d += Duration::days(1);
        }
    }
    // month (YYYY-MM) → kind → section accumulator
    let mut months: BTreeMap<String, BTreeMap<String, SectionAcc>> = BTreeMap::new();

    // Newest-first so capped sections keep recent items regardless of query order.
    let mut events = events;
    events.sort_by(|a, b| b.day.cmp(&a.day));
    for ev in events {
        let month = ev.day.get(..7).unwrap_or(&ev.day).to_string();
        let sec = months
            .entry(month)
            .or_default()
            .entry(ev.kind.clone())
            .or_insert_with(SectionAcc::new);
        sec.total += ev.count;
        *sec.repos.entry(ev.repo_slug.clone()).or_insert(0) += ev.count;
        if matches!(ev.kind.as_str(), "pull_request" | "issue" | "review") {
            if let (Some(number), Some(title)) = (ev.ref_number, ev.title) {
                sec.items.push(UserContributionItem {
                    href: slug_href(&ev.kind, &ev.repo_slug, number),
                    slug: ev.repo_slug,
                    number,
                    title,
                });
            }
        }
    }

    // Stable section order within a month: commits, pull_requests, issues,
    // reviews, repositories (GitHub's ordering).
    const KIND_ORDER: &[&str] = &[
        "commits",
        "pull_requests",
        "issues",
        "reviews",
        "repositories",
    ];
    let mut months_out: Vec<UserContributionMonth> = months
        .into_iter()
        .rev()
        .map(|(month, secs)| {
            let mut sections: Vec<UserContributionSection> =
                secs.into_iter().map(|(k, s)| s.finish(&k)).collect();
            sections.sort_by_key(|s| {
                KIND_ORDER
                    .iter()
                    .position(|k| *k == s.kind)
                    .unwrap_or(KIND_ORDER.len())
            });
            UserContributionMonth { month, sections }
        })
        .collect();
    months_out.sort_by(|a, b| b.month.cmp(&a.month));

    // Counts and streaks come from the materialized rollup — the UNION
    // above exists only for the itemized monthly sections.
    let mut total = 0_i64;
    let mut by_type = UserContributionByType {
        commits: 0,
        pull_requests: 0,
        issues: 0,
        reviews: 0,
    };
    for c in &day_counts {
        total += c.count;
        match c.kind.as_str() {
            "commits" => by_type.commits += c.count,
            "pull_request" => by_type.pull_requests += c.count,
            "issue" => by_type.issues += c.count,
            "review" => by_type.reviews += c.count,
            _ => {}
        }
        if let Some(d) = days.get_mut(&c.day) {
            *d += c.count;
        }
    }
    let (current_streak, longest_streak) = streaks(&days, to, today);

    let join_year = user
        .created_at
        .get(..4)
        .and_then(|y| y.parse::<i64>().ok())
        .unwrap_or_else(|| i64::from(from.year()));
    let years: Vec<i64> = (join_year..=i64::from(today.year())).rev().collect();

    Ok(UserContributions {
        total,
        from: from.format("%Y-%m-%d").to_string(),
        to: to.format("%Y-%m-%d").to_string(),
        includes_private: !public_only,
        days: days
            .into_iter()
            .map(|(date, count)| UserContributionDay { date, count })
            .collect(),
        by_type,
        months: months_out,
        years,
        current_streak,
        longest_streak,
    })
}

/// `(current, longest)` consecutive contributed-day runs over the zero-filled
/// day map. `current` tolerates one trailing empty `to` day only while `to`
/// is still in progress — for a past range an empty last day ends the streak.
fn streaks(days: &BTreeMap<String, i64>, to: NaiveDate, today: NaiveDate) -> (i64, i64) {
    let key = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let mut d = to;
    if to >= today && days.get(&key(d)).copied().unwrap_or(0) == 0 {
        d -= Duration::days(1);
    }
    let mut current = 0_i64;
    while days.get(&key(d)).copied().unwrap_or(0) > 0 {
        current += 1;
        d -= Duration::days(1);
    }
    let mut longest = 0_i64;
    let mut run = 0_i64;
    for c in days.values() {
        if *c > 0 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    (current, longest)
}
