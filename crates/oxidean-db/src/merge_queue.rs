//! Merge queue — `merge_queue_entries` is the per-`base_ref` FIFO train
//! (`state` `queued` → `merged` | `dequeued`); `merge_groups` is the tested
//! batch materialized on `refs/heads/gh-readonly-queue/{base}/…` (a merge
//! commit per queued PR, in order). A checking group's `head_sha` fast-
//! forwards `base_ref` when its required checks pass — the tested chain
//! becomes history (GitHub parity).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct MergeQueueEntryRow {
    pub id: String,
    pub repo_id: String,
    pub pull_id: String,
    pub pull_number: i64,
    pub base_ref: String,
    pub position: i64,
    /// `queued` | `merged` | `dequeued`.
    pub state: String,
    pub group_id: Option<String>,
    pub enqueued_by: String,
    /// The merge-commit link this PR produced in its group's chain — recorded
    /// on the PR as `merge_commit_sha` when the group lands.
    pub chain_sha: String,
    pub dequeue_reason: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct MergeGroupRow {
    pub id: String,
    pub repo_id: String,
    /// Bare branch name (no `refs/heads/`).
    pub base_ref: String,
    /// Base tip the chain was built on.
    pub base_sha: String,
    /// `refs/heads/gh-readonly-queue/{base}/group-{id}`.
    pub head_ref: String,
    /// Chain tip — the sha Actions `on: merge_group` runs check out.
    pub head_sha: String,
    /// `checking` | `merged` | `destroyed`.
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_entry {
    ($row:expr) => {{
        let row = $row;
        MergeQueueEntryRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("mq entry row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("mq entry row: {e}"))?,
            pull_id: row
                .try_get("pull_id")
                .map_err(|e| format!("mq entry row: {e}"))?,
            pull_number: row
                .try_get::<i64, _>("pull_number")
                .or_else(|_| row.try_get::<i32, _>("pull_number").map(i64::from))
                .map_err(|e| format!("mq entry row: {e}"))?,
            base_ref: row
                .try_get("base_ref")
                .map_err(|e| format!("mq entry row: {e}"))?,
            position: row
                .try_get::<i64, _>("position")
                .or_else(|_| row.try_get::<i32, _>("position").map(i64::from))
                .map_err(|e| format!("mq entry row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("mq entry row: {e}"))?,
            group_id: row
                .try_get("group_id")
                .map_err(|e| format!("mq entry row: {e}"))?,
            enqueued_by: row
                .try_get("enqueued_by")
                .map_err(|e| format!("mq entry row: {e}"))?,
            chain_sha: row
                .try_get("chain_sha")
                .map_err(|e| format!("mq entry row: {e}"))?,
            dequeue_reason: row
                .try_get("dequeue_reason")
                .map_err(|e| format!("mq entry row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("mq entry row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("mq entry row: {e}"))?,
        }
    }};
}

macro_rules! map_group {
    ($row:expr) => {{
        let row = $row;
        MergeGroupRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("mq group row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("mq group row: {e}"))?,
            base_ref: row
                .try_get("base_ref")
                .map_err(|e| format!("mq group row: {e}"))?,
            base_sha: row
                .try_get("base_sha")
                .map_err(|e| format!("mq group row: {e}"))?,
            head_ref: row
                .try_get("head_ref")
                .map_err(|e| format!("mq group row: {e}"))?,
            head_sha: row
                .try_get("head_sha")
                .map_err(|e| format!("mq group row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("mq group row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("mq group row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("mq group row: {e}"))?,
        }
    }};
}

const ENTRY_SELECT_PG: &str = "SELECT e.id, e.repo_id, e.pull_id, e.pull_number, e.base_ref, \
       e.position, e.state, e.group_id, e.enqueued_by, e.chain_sha, e.dequeue_reason, \
       to_char(e.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
       to_char(e.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
FROM merge_queue_entries e";

const ENTRY_SELECT_MYSQL: &str = "SELECT e.id, e.repo_id, e.pull_id, e.pull_number, e.base_ref, \
       e.position, e.state, e.group_id, e.enqueued_by, e.chain_sha, e.dequeue_reason, \
       DATE_FORMAT(e.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, \
       DATE_FORMAT(e.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
FROM merge_queue_entries e";

const ENTRY_SELECT_SQLITE: &str = "SELECT e.id, e.repo_id, e.pull_id, e.pull_number, e.base_ref, \
       e.position, e.state, e.group_id, e.enqueued_by, e.chain_sha, e.dequeue_reason, \
       strftime('%Y-%m-%dT%H:%M:%SZ', e.created_at) AS created_at, \
       strftime('%Y-%m-%dT%H:%M:%SZ', e.updated_at) AS updated_at \
FROM merge_queue_entries e";

const GROUP_SELECT_PG: &str = "SELECT g.id, g.repo_id, g.base_ref, g.base_sha, g.head_ref, \
       g.head_sha, g.state, \
       to_char(g.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at, \
       to_char(g.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at \
FROM merge_groups g";

const GROUP_SELECT_MYSQL: &str = "SELECT g.id, g.repo_id, g.base_ref, g.base_sha, g.head_ref, \
       g.head_sha, g.state, \
       DATE_FORMAT(g.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, \
       DATE_FORMAT(g.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at \
FROM merge_groups g";

const GROUP_SELECT_SQLITE: &str = "SELECT g.id, g.repo_id, g.base_ref, g.base_sha, g.head_ref, \
       g.head_sha, g.state, \
       strftime('%Y-%m-%dT%H:%M:%SZ', g.created_at) AS created_at, \
       strftime('%Y-%m-%dT%H:%M:%SZ', g.updated_at) AS updated_at \
FROM merge_groups g";

/// `updated_at` stamp per dialect — a literal folded into the SET list.
fn now_expr(pool: &DbPool) -> &'static str {
    match pool {
        DbPool::Sqlite(_) => "strftime('%Y-%m-%d %H:%M:%S','now')",
        _ => "CURRENT_TIMESTAMP",
    }
}

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// Append a pull to the queue tail — `position` derives `MAX(position)+1`
/// over the repo+base_ref train inside the INSERT (single statement; the
/// `(repo_id, base_ref, position)` unique index fails loudly on a race).
pub async fn insert_merge_queue_entry(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    pull_id: &str,
    pull_number: i64,
    base_ref: &str,
    enqueued_by: &str,
) -> Result<MergeQueueEntryRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO merge_queue_entries
                   (id, repo_id, pull_id, pull_number, base_ref, position, state, enqueued_by)
                 SELECT $1, $2, $3, $4, $5,
                        COALESCE(MAX(q.position), 0) + 1, 'queued', $6
                 FROM merge_queue_entries q
                 WHERE q.repo_id = $2 AND q.base_ref = $5",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pull_id)
            .bind(pull_number)
            .bind(base_ref)
            .bind(enqueued_by)
            .execute(p)
            .await
            .map_err(|e| format!("insert merge_queue_entry failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            // MySQL bars the target table in the INSERT…SELECT source — the
            // derived table materializes it first.
            sqlx::query(
                "INSERT INTO merge_queue_entries
                   (id, repo_id, pull_id, pull_number, base_ref, position, state, enqueued_by)
                 SELECT ?, ?, ?, ?, ?, COALESCE(m.pos, 0) + 1, 'queued', ?
                 FROM (SELECT MAX(position) AS pos FROM merge_queue_entries
                       WHERE repo_id = ? AND base_ref = ?) m",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pull_id)
            .bind(pull_number)
            .bind(base_ref)
            .bind(enqueued_by)
            .bind(repo_id)
            .bind(base_ref)
            .execute(p)
            .await
            .map_err(|e| format!("insert merge_queue_entry failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO merge_queue_entries
                   (id, repo_id, pull_id, pull_number, base_ref, position, state, enqueued_by)
                 SELECT ?1, ?2, ?3, ?4, ?5,
                        COALESCE(MAX(q.position), 0) + 1, 'queued', ?6
                 FROM merge_queue_entries q
                 WHERE q.repo_id = ?2 AND q.base_ref = ?5",
            )
            .bind(id)
            .bind(repo_id)
            .bind(pull_id)
            .bind(pull_number)
            .bind(base_ref)
            .bind(enqueued_by)
            .execute(p)
            .await
            .map_err(|e| format!("insert merge_queue_entry failed: {e}"))?;
        }
    }
    find_merge_queue_entry(pool, id)
        .await?
        .ok_or_else(|| "merge_queue_entry row vanished after insert".to_string())
}

pub async fn find_merge_queue_entry(
    pool: &DbPool,
    id: &str,
) -> Result<Option<MergeQueueEntryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{ENTRY_SELECT_PG} WHERE e.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find merge_queue_entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{ENTRY_SELECT_MYSQL} WHERE e.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find merge_queue_entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{ENTRY_SELECT_SQLITE} WHERE e.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find merge_queue_entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The pull's live (`queued`) queue slot, if any — the partial unique index
/// keeps this a single row.
pub async fn find_queued_entry_for_pull(
    pool: &DbPool,
    pull_id: &str,
) -> Result<Option<MergeQueueEntryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{ENTRY_SELECT_PG} WHERE e.pull_id = $1 AND e.state = 'queued'");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(pull_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find queued mq entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{ENTRY_SELECT_MYSQL} WHERE e.pull_id = ? AND e.state = 'queued'");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(pull_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find queued mq entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{ENTRY_SELECT_SQLITE} WHERE e.pull_id = ?1 AND e.state = 'queued'");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(pull_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find queued mq entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The pull's queue slot by repo-visible number (dequeue resolves this way —
/// GitHub's "remove from queue" acts on the PR).
pub async fn find_merge_queue_entry_for_pull_number(
    pool: &DbPool,
    repo_id: &str,
    pull_number: i64,
    state: &str,
) -> Result<Option<MergeQueueEntryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{ENTRY_SELECT_PG} WHERE e.repo_id = $1 AND e.pull_number = $2 AND e.state = $3 \
                   ORDER BY e.position DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(pull_number)
                .bind(state)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find mq entry by number failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{ENTRY_SELECT_MYSQL} WHERE e.repo_id = ? AND e.pull_number = ? AND e.state = ? \
                   ORDER BY e.position DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(pull_number)
                .bind(state)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find mq entry by number failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{ENTRY_SELECT_SQLITE} WHERE e.repo_id = ?1 AND e.pull_number = ?2 AND e.state = ?3 \
                   ORDER BY e.position DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(pull_number)
                .bind(state)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find mq entry by number failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The pull's newest queue row in any state — `merge_queue.get` uses it to
/// surface *why* an unqueued PR left the train (`dequeue_reason`).
pub async fn find_latest_merge_queue_entry_for_pull_number(
    pool: &DbPool,
    repo_id: &str,
    pull_number: i64,
) -> Result<Option<MergeQueueEntryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{ENTRY_SELECT_PG} WHERE e.repo_id = $1 AND e.pull_number = $2 \
                   ORDER BY e.created_at DESC, e.position DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(pull_number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find latest mq entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{ENTRY_SELECT_MYSQL} WHERE e.repo_id = ? AND e.pull_number = ? \
                   ORDER BY e.created_at DESC, e.position DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(pull_number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find latest mq entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{ENTRY_SELECT_SQLITE} WHERE e.repo_id = ?1 AND e.pull_number = ?2 \
                   ORDER BY e.created_at DESC, e.position DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(pull_number)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find latest mq entry failed: {e}"))?
            {
                Some(r) => Ok(Some(map_entry!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Train slice in position order — `state` filters (`"queued"` lists the
/// live queue). `base_ref` empty-string lists every queued train in the repo
/// (head-move dequeue scans this).
pub async fn list_merge_queue_entries(
    pool: &DbPool,
    repo_id: &str,
    base_ref: &str,
    state: &str,
) -> Result<Vec<MergeQueueEntryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{ENTRY_SELECT_PG} WHERE e.repo_id = $1 AND ($2 = '' OR e.base_ref = $2) \
                   AND ($3 = '' OR e.state = $3) ORDER BY e.position ASC"
            );
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .bind(state)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list merge_queue_entries failed: {e}"))?
            {
                out.push(map_entry!(&row));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{ENTRY_SELECT_MYSQL} WHERE e.repo_id = ? AND (? = '' OR e.base_ref = ?) \
                   AND (? = '' OR e.state = ?) ORDER BY e.position ASC"
            );
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .bind(state)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list merge_queue_entries failed: {e}"))?
            {
                out.push(map_entry!(&row));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{ENTRY_SELECT_SQLITE} WHERE e.repo_id = ?1 AND (?2 = '' OR e.base_ref = ?2) \
                   AND (?3 = '' OR e.state = ?3) ORDER BY e.position ASC"
            );
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .bind(state)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list merge_queue_entries failed: {e}"))?
            {
                out.push(map_entry!(&row));
            }
            Ok(out)
        }
    }
}

/// Entries riding a specific group, in position order.
pub async fn list_merge_queue_entries_for_group(
    pool: &DbPool,
    group_id: &str,
) -> Result<Vec<MergeQueueEntryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{ENTRY_SELECT_PG} WHERE e.group_id = $1 ORDER BY e.position ASC");
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(group_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list group mq entries failed: {e}"))?
            {
                out.push(map_entry!(&row));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!("{ENTRY_SELECT_MYSQL} WHERE e.group_id = ? ORDER BY e.position ASC");
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(group_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list group mq entries failed: {e}"))?
            {
                out.push(map_entry!(&row));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql =
                format!("{ENTRY_SELECT_SQLITE} WHERE e.group_id = ?1 ORDER BY e.position ASC");
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(group_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list group mq entries failed: {e}"))?
            {
                out.push(map_entry!(&row));
            }
            Ok(out)
        }
    }
}

/// Terminal-state an entry — `merged` | `dequeued` (reason records why:
/// `removed`, `failed_checks`, `head_moved`, `closed`).
pub async fn set_merge_queue_entry_state(
    pool: &DbPool,
    id: &str,
    state: &str,
    dequeue_reason: &str,
) -> Result<Option<MergeQueueEntryRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE merge_queue_entries SET state = $1, dequeue_reason = $2, \
                   updated_at = {now} WHERE id = $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(dequeue_reason)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set mq entry state failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE merge_queue_entries SET state = ?, dequeue_reason = ?, \
                   updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(dequeue_reason)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set mq entry state failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE merge_queue_entries SET state = ?, dequeue_reason = ?, \
                   updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(dequeue_reason)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set mq entry state failed: {e}"))?;
        }
    }
    find_merge_queue_entry(pool, id).await
}

/// Set (or clear) an entry's `group_id` + `chain_sha` — called once per
/// entry by group formation (chain link known only after its merge commit
/// materializes) and by group teardown (`None`, `""`).
pub async fn set_entry_group_and_chain(
    pool: &DbPool,
    entry_id: &str,
    group_id: Option<&str>,
    chain_sha: &str,
) -> Result<(), String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "UPDATE merge_queue_entries SET group_id = $1, chain_sha = $2, \
                   updated_at = {now} WHERE id = $3"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(group_id)
                .bind(chain_sha)
                .bind(entry_id)
                .execute(p)
                .await
                .map_err(|e| format!("set mq entry group/chain failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "UPDATE merge_queue_entries SET group_id = ?, chain_sha = ?, \
                   updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(group_id)
                .bind(chain_sha)
                .bind(entry_id)
                .execute(p)
                .await
                .map_err(|e| format!("set mq entry group/chain failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "UPDATE merge_queue_entries SET group_id = ?, chain_sha = ?, \
                   updated_at = {now} WHERE id = ?"
            );
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(group_id)
                .bind(chain_sha)
                .bind(entry_id)
                .execute(p)
                .await
                .map_err(|e| format!("set mq entry group/chain failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

/// Create a `checking` group row — the materialization step fills
/// Insert the group atomically once the chain is materialized —
/// `base_sha`/`head_ref`/`head_sha` land in one row so no `checking`
/// group ever exists with a stub head. The `checking`-per-base unique
/// index is the race breaker for concurrent enqueues: the loser's INSERT
/// fails and its caller re-checks for the winner's group.
pub async fn insert_merge_group(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    base_ref: &str,
    base_sha: &str,
    head_ref: &str,
    head_sha: &str,
) -> Result<MergeGroupRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO merge_groups \
                   (id, repo_id, base_ref, base_sha, head_ref, head_sha, state) \
                 VALUES ($1, $2, $3, $4, $5, $6, 'checking')",
            )
            .bind(id)
            .bind(repo_id)
            .bind(base_ref)
            .bind(base_sha)
            .bind(head_ref)
            .bind(head_sha)
            .execute(p)
            .await
            .map_err(|e| format!("insert merge_group failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO merge_groups \
                   (id, repo_id, base_ref, base_sha, head_ref, head_sha, state) \
                 VALUES (?, ?, ?, ?, ?, ?, 'checking')",
            )
            .bind(id)
            .bind(repo_id)
            .bind(base_ref)
            .bind(base_sha)
            .bind(head_ref)
            .bind(head_sha)
            .execute(p)
            .await
            .map_err(|e| format!("insert merge_group failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO merge_groups \
                   (id, repo_id, base_ref, base_sha, head_ref, head_sha, state) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'checking')",
            )
            .bind(id)
            .bind(repo_id)
            .bind(base_ref)
            .bind(base_sha)
            .bind(head_ref)
            .bind(head_sha)
            .execute(p)
            .await
            .map_err(|e| format!("insert merge_group failed: {e}"))?;
        }
    }
    find_merge_group(pool, id)
        .await?
        .ok_or_else(|| "merge_group row vanished after insert".to_string())
}

/// Terminal-state flip for a group — `checking` → `merged` | `destroyed`.
/// Also releases the `checking`-per-base unique slot so the next train
/// batch can form.
pub async fn set_merge_group_state(
    pool: &DbPool,
    id: &str,
    state: &str,
) -> Result<Option<MergeGroupRow>, String> {
    let now = now_expr(pool);
    match pool {
        DbPool::Postgres(p) => {
            let sql =
                format!("UPDATE merge_groups SET state = $1, updated_at = {now} WHERE id = $2");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set merge_group state failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let sql = format!("UPDATE merge_groups SET state = ?, updated_at = {now} WHERE id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set merge_group state failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let sql = format!("UPDATE merge_groups SET state = ?, updated_at = {now} WHERE id = ?");
            sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(state)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("set merge_group state failed: {e}"))?;
        }
    }
    find_merge_group(pool, id).await
}

pub async fn find_merge_group(pool: &DbPool, id: &str) -> Result<Option<MergeGroupRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{GROUP_SELECT_PG} WHERE g.id = $1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find merge_group failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{GROUP_SELECT_MYSQL} WHERE g.id = ?");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find merge_group failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{GROUP_SELECT_SQLITE} WHERE g.id = ?1");
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find merge_group failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The live (`checking`) group on a train — one at a time by construction.
pub async fn find_checking_merge_group(
    pool: &DbPool,
    repo_id: &str,
    base_ref: &str,
) -> Result<Option<MergeGroupRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{GROUP_SELECT_PG} WHERE g.repo_id = $1 AND g.base_ref = $2 \
                   AND g.state = 'checking' ORDER BY g.created_at DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find checking merge_group failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{GROUP_SELECT_MYSQL} WHERE g.repo_id = ? AND g.base_ref = ? \
                   AND g.state = 'checking' ORDER BY g.created_at DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find checking merge_group failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{GROUP_SELECT_SQLITE} WHERE g.repo_id = ?1 AND g.base_ref = ?2 \
                   AND g.state = 'checking' ORDER BY g.created_at DESC LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find checking merge_group failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Resolve a `checking` group by its materialized head sha — the check-run /
/// commit-status writers call this to drive queue reconcile (`None` = the
/// sha isn't a live queue tip).
pub async fn find_checking_merge_group_for_head(
    pool: &DbPool,
    repo_id: &str,
    head_sha: &str,
) -> Result<Option<MergeGroupRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{GROUP_SELECT_PG} WHERE g.repo_id = $1 AND g.head_sha = $2 \
                   AND g.state = 'checking' LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(head_sha)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find mq group by head failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{GROUP_SELECT_MYSQL} WHERE g.repo_id = ? AND g.head_sha = ? \
                   AND g.state = 'checking' LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(head_sha)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find mq group by head failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{GROUP_SELECT_SQLITE} WHERE g.repo_id = ?1 AND g.head_sha = ?2 \
                   AND g.state = 'checking' LIMIT 1"
            );
            match sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(head_sha)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find mq group by head failed: {e}"))?
            {
                Some(r) => Ok(Some(map_group!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Group history for a train, newest first.
pub async fn list_merge_groups(
    pool: &DbPool,
    repo_id: &str,
    base_ref: &str,
    limit: i64,
) -> Result<Vec<MergeGroupRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!(
                "{GROUP_SELECT_PG} WHERE g.repo_id = $1 AND ($2 = '' OR g.base_ref = $2) \
                   ORDER BY g.created_at DESC LIMIT $3"
            );
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list merge_groups failed: {e}"))?
            {
                out.push(map_group!(&row));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let sql = format!(
                "{GROUP_SELECT_MYSQL} WHERE g.repo_id = ? AND (? = '' OR g.base_ref = ?) \
                   ORDER BY g.created_at DESC LIMIT ?"
            );
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list merge_groups failed: {e}"))?
            {
                out.push(map_group!(&row));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let sql = format!(
                "{GROUP_SELECT_SQLITE} WHERE g.repo_id = ?1 AND (?2 = '' OR g.base_ref = ?2) \
                   ORDER BY g.created_at DESC LIMIT ?3"
            );
            let mut out = Vec::new();
            for row in sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(repo_id)
                .bind(base_ref)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list merge_groups failed: {e}"))?
            {
                out.push(map_group!(&row));
            }
            Ok(out)
        }
    }
}
