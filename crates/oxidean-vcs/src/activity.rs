//! Operation-granular `repo_activity` rows for mutating `vcs.*` ops (contract
//! C4). Ref-free kinds leave `ref_name`/`before_oid`/`after_oid` NULL
//! (migration 0053 made them nullable); ref-scoped kinds populate them from
//! the operation's own data — never fabricated.
//!
//! Inserts are best-effort, matching `repo::activity::record_ref_updates`: a
//! failed insert never fails the operation it annotates.

use oxidean_db::Database;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Columns for one activity row — `None` fields land NULL (C4 mapping).
#[derive(Debug, Clone, Default)]
pub struct ActivityRecord {
    /// One of `VCS_ACTIVITY_KINDS` — the `push_type` column value.
    pub push_type: &'static str,
    pub ref_name: Option<String>,
    pub before_oid: Option<String>,
    pub after_oid: Option<String>,
    pub commits_count: Option<i64>,
    pub commit_message: Option<String>,
    pub pr_number: Option<i64>,
}

/// Writes `vcs.*` activity rows through the u1 `insert_repo_activity` facade.
#[derive(Clone)]
pub struct ActivityRecorder {
    db: Database,
}

impl ActivityRecorder {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Insert one row; empty `actor_id` skips silently (defense — callers
    /// always carry a resolved session user). Failures warn, never fail the
    /// operation itself.
    pub async fn record(&self, repository_id: &str, actor_id: &str, rec: ActivityRecord) {
        if actor_id.trim().is_empty() || repository_id.is_empty() {
            return;
        }
        if self.db.is_skipped() {
            return;
        }
        let id = mint_activity_id(rec.push_type);
        if let Err(e) = self
            .db
            .insert_repo_activity(
                &id,
                repository_id,
                actor_id,
                rec.push_type,
                rec.ref_name.as_deref(),
                rec.before_oid.as_deref(),
                rec.after_oid.as_deref(),
                rec.commits_count,
                rec.commit_message.as_deref(),
                rec.pr_number,
            )
            .await
        {
            tracing::warn!(
                error = %e,
                repository_id,
                push_type = rec.push_type,
                "vcs activity insert failed"
            );
        }
    }
}

/// `vcs-act-<16-hex>` — time/pid/seq digest, same recipe as the import nonce
/// (no uuid dep in this crate).
fn mint_activity_id(kind: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(nanos.to_be_bytes());
    h.update(std::process::id().to_be_bytes());
    h.update(SEQ.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    h.update(kind.as_bytes());
    let digest = h.finalize();
    format!("vcs-act-{}", hex::encode(&digest[..8]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object_store::testutil::{seed_repo, test_db};
    use oxidean_core::{
        VCS_ACTIVITY_CHANGESET_CREATE, VCS_ACTIVITY_GIT_EXPORT, VCS_ACTIVITY_GIT_IMPORT,
        VCS_ACTIVITY_MERGE, VCS_ACTIVITY_REBASE, VCS_ACTIVITY_REF_UPDATE, VCS_ACTIVITY_REPO_ENABLE,
    };

    #[tokio::test]
    async fn record_writes_nullable_columns() {
        let (_tmp, db) = test_db().await;
        let rec = ActivityRecorder::new(db.clone());
        let repo = seed_repo(&db, "u-owner", "r1").await;

        // Ref-free kind — every ref-scoped column stays NULL.
        rec.record(
            &repo,
            "u-owner",
            ActivityRecord {
                push_type: VCS_ACTIVITY_REPO_ENABLE,
                ..Default::default()
            },
        )
        .await;
        let (rows, _) = db
            .list_repo_activity(&repo, None, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].push_type, VCS_ACTIVITY_REPO_ENABLE);
        assert_eq!(rows[0].actor_id, "u-owner");
        assert!(rows[0].ref_name.is_none());
        assert!(rows[0].before_oid.is_none());
        assert!(rows[0].after_oid.is_none());
        assert!(rows[0].commits_count.is_none());
        assert!(rows[0].commit_message.is_none());
        assert!(rows[0].pr_number.is_none());
        assert!(rows[0].id.starts_with("vcs-act-"));
    }

    #[tokio::test]
    async fn record_writes_ref_scoped_columns() {
        let (_tmp, db) = test_db().await;
        let rec = ActivityRecorder::new(db.clone());
        let repo = seed_repo(&db, "u-owner", "r1").await;
        rec.record(
            &repo,
            "u-owner",
            ActivityRecord {
                push_type: VCS_ACTIVITY_REF_UPDATE,
                ref_name: Some("main".into()),
                before_oid: None,
                after_oid: Some("cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR".into()),
                commits_count: Some(0),
                ..Default::default()
            },
        )
        .await;
        let (rows, _) = db
            .list_repo_activity(&repo, None, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].push_type, VCS_ACTIVITY_REF_UPDATE);
        assert_eq!(rows[0].ref_name.as_deref(), Some("main"));
        assert!(rows[0].before_oid.is_none());
        assert_eq!(
            rows[0].after_oid.as_deref(),
            Some("cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR")
        );
    }

    #[tokio::test]
    async fn record_skips_empty_actor_and_skipped_db() {
        let (_tmp, db) = test_db().await;
        let rec = ActivityRecorder::new(db.clone());
        let repo = seed_repo(&db, "u-owner", "r1").await;
        rec.record(
            &repo,
            "",
            ActivityRecord {
                push_type: VCS_ACTIVITY_MERGE,
                ..Default::default()
            },
        )
        .await;
        rec.record(
            &repo,
            "  ",
            ActivityRecord {
                push_type: VCS_ACTIVITY_REBASE,
                ..Default::default()
            },
        )
        .await;
        assert!(db
            .list_repo_activity(&repo, None, None, 0, 10)
            .await
            .unwrap()
            .0
            .is_empty());

        let rec2 = ActivityRecorder::new(Database::skipped());
        rec2.record(
            &repo,
            "u1",
            ActivityRecord {
                push_type: VCS_ACTIVITY_GIT_IMPORT,
                ..Default::default()
            },
        )
        .await;
        rec2.record(
            &repo,
            "u1",
            ActivityRecord {
                push_type: VCS_ACTIVITY_GIT_EXPORT,
                ..Default::default()
            },
        )
        .await;
        rec2.record(
            &repo,
            "u1",
            ActivityRecord {
                push_type: VCS_ACTIVITY_CHANGESET_CREATE,
                ..Default::default()
            },
        )
        .await;
    }

    #[tokio::test]
    async fn mint_ids_are_unique() {
        let a = mint_activity_id("k");
        let b = mint_activity_id("k");
        assert_ne!(a, b);
        assert!(a.starts_with("vcs-act-"));
    }
}
