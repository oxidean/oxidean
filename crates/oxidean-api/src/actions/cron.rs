//! `on.schedule` — POSIX 5-field cron matching (UTC, GitHub semantics) plus
//! the per-minute sweeper that enqueues scheduled workflow runs.
//!
//! GitHub Actions resolves `schedule[].cron` in UTC on the *default* branch.
//! Fire granularity is one minute; `action_schedule_state` dedupes
//! (repo, workflow, cron, minute) so concurrent sweepers and restarts inside
//! the same minute cannot double-fire.
//!
//! Cron syntax (POSIX, as accepted by GitHub):
//! `minute hour day-of-month month day-of-week`, fields support `*`, `*/n`,
//! `a`, `a-b`, `a-b/n`, `a/n`, comma lists and JAN..DEC / SUN..SAT names.
//! DOM/DOW: when *both* are restricted, a tick matches when either matches
//! (POSIX OR rule); when only one is restricted it acts as a plain AND.
//! `L`/`W`/`#`/`?` extensions are not POSIX — they never match and log a
//! warning instead of silently misexecuting.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Timelike, Utc};

use oxidean_db::Database;
use oxidean_git::GitBackend;

const MONTH_NAMES: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const DOW_NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

/// True when `expr` matches the UTC instant `t` (evaluated at minute
/// granularity — seconds/sub-second fields of `t` are ignored).
pub fn matches(expr: &str, t: &DateTime<Utc>) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }
    // GitHub/POSIX: both 0 and 7 mean Sunday — on Sundays try both
    // representations (`0`/`SUN` and `7`).
    let dow_val = t.weekday().num_days_from_sunday();
    let dow_f = field_matches(fields[4], dow_val, 0, 7, &DOW_NAMES, 0);
    let dow_f = if dow_val == 0 && dow_f == Some(false) {
        field_matches(fields[4], 7, 0, 7, &DOW_NAMES, 0)
    } else {
        dow_f
    };
    let (Some(mi), Some(h), Some(dom), Some(mo), Some(dow)) = (
        field_matches(fields[0], t.minute(), 0, 59, &[], 0),
        field_matches(fields[1], t.hour(), 0, 23, &[], 0),
        field_matches(fields[2], t.day(), 1, 31, &[], 0),
        field_matches(fields[3], t.month(), 1, 12, &MONTH_NAMES, 1),
        dow_f,
    ) else {
        return false;
    };
    let dom_restricted = !is_star(fields[2]);
    let dow_restricted = !is_star(fields[4]);
    if dom_restricted && dow_restricted {
        // POSIX OR: both restricted → match when either matches.
        return mi && h && mo && (dom || dow);
    }
    mi && h && dom && mo && dow
}

fn is_star(field: &str) -> bool {
    field.trim() == "*"
}

/// Match a single cron field against `value`. `None` = syntactically invalid
/// (caller treats the whole expression as non-matching); unsupported chars
/// land here too — they surface as a warning at parse, never silently fire.
fn field_matches(
    field: &str,
    value: u32,
    min: u32,
    max: u32,
    names: &[&str],
    name_base: u32,
) -> Option<bool> {
    let mut any = false;
    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        if item_matches(part, value, min, max, names, name_base)? {
            any = true;
        }
    }
    Some(any)
}

fn item_matches(
    part: &str,
    value: u32,
    min: u32,
    max: u32,
    names: &[&str],
    name_base: u32,
) -> Option<bool> {
    let (range_part, step) = match part.split_once('/') {
        Some((r, s)) => (r, s.parse::<u32>().ok().filter(|&n| n > 0)?),
        None => (part, 1),
    };
    let (lo, hi) = if range_part == "*" {
        (min, max)
    } else if let Some((a, b)) = range_part.split_once('-') {
        (
            parse_atom(a, names, name_base)?,
            parse_atom(b, names, name_base)?,
        )
    } else {
        let a = parse_atom(range_part, names, name_base)?;
        // `a/n` = a..max step n; bare `a` = only a.
        if part.contains('/') {
            (a, max)
        } else {
            (a, a)
        }
    };
    if lo > hi || lo < min || hi > max {
        return Some(false);
    }
    Some(value >= lo && value <= hi && (value - lo).is_multiple_of(step))
}

fn parse_atom(s: &str, names: &[&str], name_base: u32) -> Option<u32> {
    let s = s.trim();
    if let Ok(n) = s.parse::<u32>() {
        return Some(n);
    }
    names
        .iter()
        .position(|n| n.eq_ignore_ascii_case(s))
        .map(|i| i as u32 + name_base)
}

/// One sweep over every Actions-enabled repository: resolve the default
/// branch tip, discover workflows, and for each `schedule[].cron` matching
/// the current UTC minute claim the tick and enqueue a `schedule` run.
/// Returns the number of runs enqueued.
pub async fn sweep_scheduled_workflows(
    db: &Database,
    git: Arc<dyn GitBackend>,
    repos_dir: &Path,
) -> Result<usize, String> {
    let now = Utc::now();
    let minute_key = now.timestamp() / 60;
    let mut fired = 0usize;
    for repo_id in db.list_actions_enabled_repo_ids().await? {
        let Some(repo) = db.find_repository_by_id(&repo_id).await? else {
            continue;
        };
        let owner = match crate::repo::owner_ref_for_repo(db, &repo).await {
            Ok(Some(o)) => o.slug().to_string(),
            _ => continue,
        };
        let bare = crate::actions::dispatch::bare_repo_path(repos_dir, &owner, &repo.name);
        if !bare.is_dir() {
            continue;
        }
        let default_ref = format!("refs/heads/{}", repo.default_branch);
        let Ok(sha) = git.rev_parse(&bare, &default_ref).await else {
            continue;
        };
        let Ok(discovered) = crate::actions::discover_workflows(git.as_ref(), &bare, &sha).await
        else {
            continue;
        };
        if !discovered.errors.is_empty() {
            for fe in &discovered.errors {
                tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
            }
            if let Err(e) = crate::actions::dispatch::fail_discovery_runs(
                db,
                &repo.id,
                &discovered.errors,
                "schedule",
                &sha,
                &default_ref,
                None,
                None,
                None,
            )
            .await
            {
                tracing::warn!(error = %e, repo = %repo.name, "discovery validation runs failed");
            }
        }
        let repository =
            crate::actions::payload::repository_embed(&owner, &repo.name, &repo.default_branch);
        for wf in &discovered.workflows {
            for cron in &wf.document.triggers.schedule_crons {
                if !matches(cron, &now) {
                    continue;
                }
                match db
                    .claim_schedule_tick(&repo.id, &wf.path, cron, minute_key)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => continue, // another sweeper/minute already fired
                    Err(e) => {
                        tracing::warn!(error = %e, repo = %repo.name, "schedule tick claim failed");
                        continue;
                    }
                }
                let payload = crate::actions::payload::schedule_payload(cron, &repository);
                match crate::actions::dispatch::enqueue_run(
                    db,
                    &repo.id,
                    &wf.path,
                    &wf.document,
                    "schedule",
                    &sha,
                    &default_ref,
                    None,
                    None,
                    Some(&payload),
                    false,
                )
                .await
                {
                    Ok((run_id, _)) => {
                        fired += 1;
                        tracing::info!(
                            repo = %repo.name,
                            workflow = %wf.path,
                            cron,
                            run_id,
                            "scheduled run enqueued"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            repo = %repo.name,
                            workflow = %wf.path,
                            cron,
                            "scheduled enqueue failed"
                        );
                    }
                }
            }
        }
    }
    Ok(fired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn star_matches_everything() {
        assert!(matches("* * * * *", &at(2025, 6, 15, 3, 47)));
    }

    #[test]
    fn exact_and_ranges() {
        // Mon 2025-06-16 is a Monday (dow=1).
        assert!(matches("30 9 16 6 1", &at(2025, 6, 16, 9, 30)));
        assert!(!matches("30 9 16 6 1", &at(2025, 6, 16, 9, 31)));
        assert!(matches("*/15 * * * *", &at(2025, 6, 16, 9, 45)));
        assert!(!matches("*/15 * * * *", &at(2025, 6, 16, 9, 46)));
        assert!(matches("0 9-17 * * *", &at(2025, 6, 16, 12, 0)));
        assert!(matches("0 9-17/2 * * *", &at(2025, 6, 16, 13, 0)));
        assert!(!matches("0 9-17/2 * * *", &at(2025, 6, 16, 12, 0)));
    }

    #[test]
    fn dom_dow_or_semantics() {
        // 2025-06-16 is Monday (dow=1), dom=16: both restricted → OR.
        assert!(matches("0 0 1 * 1", &at(2025, 6, 16, 0, 0))); // dow matches
        assert!(matches("0 0 1 * 1", &at(2025, 6, 1, 0, 0))); // dom matches (2025-06-01 is Sunday, dow=0)
        assert!(!matches("0 0 1 * 1", &at(2025, 6, 17, 0, 0))); // neither
                                                                // Only dom restricted → AND.
        assert!(matches("0 0 1 * *", &at(2025, 6, 1, 0, 0)));
        assert!(!matches("0 0 1 * *", &at(2025, 6, 2, 0, 0)));
    }

    #[test]
    fn names_and_sunday_seven() {
        assert!(matches("0 0 * JAN SUN", &at(2025, 1, 5, 0, 0))); // 2025-01-05 = Sunday
        assert!(matches("0 0 * * 7", &at(2025, 1, 5, 0, 0))); // 7 = Sunday
        assert!(!matches("0 0 * * 7", &at(2025, 1, 6, 0, 0))); // Monday
    }

    #[test]
    fn invalid_never_matches() {
        assert!(!matches("* * * *", &at(2025, 6, 16, 0, 0)));
        assert!(!matches("0 0 L * *", &at(2025, 6, 16, 0, 0)));
        assert!(!matches("0 0 * * ?", &at(2025, 6, 16, 0, 0)));
        assert!(!matches("61 * * * *", &at(2025, 6, 16, 0, 0)));
        assert!(!matches("*/0 * * * *", &at(2025, 6, 16, 0, 0)));
    }

    #[test]
    fn lists() {
        assert!(matches("5,15,45 * * * *", &at(2025, 6, 16, 3, 15)));
        assert!(!matches("5,15,45 * * * *", &at(2025, 6, 16, 3, 25)));
    }
}
