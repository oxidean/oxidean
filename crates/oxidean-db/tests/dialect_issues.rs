//! 11-02: `0011_issues` + issues / counters / comments / revisions / labels / assignees / reactions / links.

use oxidean_core::Role;
use oxidean_db::Database;

/// Expect sqlite `0011_issues.sql` with issue domain tables, migrate, and prove
/// monotonic `#N` allocation that does not reclaim after hard-delete (D-ISS-01).
#[tokio::test]
async fn dialect_issues_migrate_0011_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0011_issues.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0011_issues.sql must exist (issues + counters + comments + revisions + labels + assignees + reactions + links)"
    );
    for needle in [
        "issues",
        "issue_counters",
        "issue_comments",
        "issue_revisions",
        "comment_revisions",
        "labels",
        "repo_hidden_labels",
        "issue_labels",
        "issue_assignees",
        "issue_reactions",
        "comment_reactions",
        "issue_links",
    ] {
        assert!(sql.contains(needle), "0011 must define {needle}");
    }
    assert!(
        sql.contains("max_number"),
        "0011 issue_counters must track max_number"
    );
    assert!(
        sql.contains("pr_stub") || sql.contains("'pr_stub'"),
        "0011 issue_links must allow pr_stub kind"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("issues.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate 0011_issues");

    let author = db
        .create_user(
            "u-issue-author",
            "issueauthor@example.com",
            "issueauthor",
            Some("hash"),
            "Issue Author",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create author");

    let repo = db
        .insert_repository(
            "r-issues-1",
            &author.id,
            "user",
            "issues-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("insert repo");

    let first = db
        .insert_issue("i-1", &repo.id, &author.id, "First issue", "body one")
        .await
        .expect("insert first issue");
    assert_eq!(first.number, 1);
    assert_eq!(first.state, "open");
    assert_eq!(first.title, "First issue");

    let second = db
        .insert_issue("i-2", &repo.id, &author.id, "Second issue", "body two")
        .await
        .expect("insert second issue");
    assert_eq!(second.number, 2);

    db.delete_issue(&first.id)
        .await
        .expect("hard-delete first issue");

    let third = db
        .insert_issue("i-3", &repo.id, &author.id, "Third issue", "body three")
        .await
        .expect("insert third after delete");
    assert_eq!(
        third.number, 3,
        "D-ISS-01: hard-delete must not reclaim #N (expected 3, got {})",
        third.number
    );

    let label = db
        .insert_label("l-bug", "bug", "d73a4a", "A bug", None, Some(&repo.id))
        .await
        .expect("insert repo-local label");
    assert_eq!(label.name, "bug");
    assert_eq!(label.repo_id.as_deref(), Some(repo.id.as_str()));

    db.set_issue_labels(&third.id, &["l-bug".to_string()])
        .await
        .expect("assign label");
    db.set_issue_assignees(&third.id, std::slice::from_ref(&author.id))
        .await
        .expect("assign assignee");
}

/// Batch enrichment: the `*_for_issues` / `*_for_comments` / `find_many`
/// helpers must return the same rows as their single-item counterparts in one
/// round trip, and issue rows must carry `comment_count` (N+1 regression guard).
#[tokio::test]
async fn dialect_issues_batch_enrichment() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("issues_batch.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let author = db
        .create_user(
            "u-batch-author",
            "batchauthor@example.com",
            "batchauthor",
            Some("hash"),
            "Batch Author",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create author");
    let other = db
        .create_user(
            "u-batch-other",
            "batchother@example.com",
            "batchother",
            Some("hash"),
            "Batch Other",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create other user");

    let repo = db
        .insert_repository(
            "r-batch-1",
            &author.id,
            "user",
            "batch-demo",
            "private",
            "",
            "main",
        )
        .await
        .expect("insert repo");

    let i1 = db
        .insert_issue("ib-1", &repo.id, &author.id, "One", "b1")
        .await
        .expect("issue 1");
    let i2 = db
        .insert_issue("ib-2", &repo.id, &other.id, "Two", "b2")
        .await
        .expect("issue 2");
    let i3 = db
        .insert_issue("ib-3", &repo.id, &author.id, "Three", "b3")
        .await
        .expect("issue 3");

    let c1 = db
        .insert_issue_comment("ic-1", &i1.id, &author.id, "comment one")
        .await
        .expect("comment 1");
    let c2 = db
        .insert_issue_comment("ic-2", &i1.id, &other.id, "comment two")
        .await
        .expect("comment 2");
    let c3 = db
        .insert_issue_comment("ic-3", &i2.id, &author.id, "comment three")
        .await
        .expect("comment 3");

    let label = db
        .insert_label("lb-bug", "bug", "d73a4a", "A bug", None, Some(&repo.id))
        .await
        .expect("insert label");
    db.set_issue_labels(&i1.id, std::slice::from_ref(&label.id))
        .await
        .expect("labels i1");
    db.set_issue_labels(&i3.id, std::slice::from_ref(&label.id))
        .await
        .expect("labels i3");
    db.set_issue_assignees(&i1.id, &[author.id.clone(), other.id.clone()])
        .await
        .expect("assignees i1");
    db.set_issue_assignees(&i2.id, std::slice::from_ref(&other.id))
        .await
        .expect("assignees i2");

    db.toggle_issue_reaction(&i1.id, &author.id, "+1")
        .await
        .expect("issue reaction");
    db.toggle_issue_reaction(&i1.id, &other.id, "+1")
        .await
        .expect("issue reaction 2");
    db.toggle_comment_reaction(&c1.id, &other.id, "heart")
        .await
        .expect("comment reaction");

    // comment_count rides along on every issue row — no extra queries needed.
    let (rows, _total) = db
        .list_issues_for_repo(&repo.id, oxidean_db::issues::IssueListFilters {
            state: "all",
            author_id: None,
            label_id: None,
            assignee_id: None,
            q: None,
            offset: 0,
            limit: 50,
        })
        .await
        .expect("list issues");
    assert_eq!(rows.len(), 3);
    let count_of = |id: &str| rows.iter().find(|r| r.id == id).unwrap().comment_count;
    assert_eq!(count_of(&i1.id), 2, "i1 has two comments");
    assert_eq!(count_of(&i2.id), 1, "i2 has one comment");
    assert_eq!(count_of(&i3.id), 0, "i3 has none");

    let issue_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();

    // Labels — batch equals the per-issue lookup.
    let label_pairs = db
        .list_labels_for_issues(&issue_ids)
        .await
        .expect("batch labels");
    for id in &issue_ids {
        let single = db.list_labels_for_issue(id).await.expect("single labels");
        let batched: Vec<&oxidean_db::LabelRow> = label_pairs
            .iter()
            .filter(|(iid, _)| iid == id)
            .map(|(_, l)| l)
            .collect();
        assert_eq!(single.len(), batched.len(), "label count mismatch for {id}");
        for l in &single {
            assert!(batched.iter().any(|b| b.id == l.id));
        }
    }

    // Assignees.
    let assignee_pairs = db
        .list_assignees_for_issues(&issue_ids)
        .await
        .expect("batch assignees");
    assert_eq!(assignee_pairs.len(), 3, "2 on i1 + 1 on i2");

    // Issue reactions — viewer flag must survive the batch path.
    let reaction_pairs = db
        .list_issue_reaction_groups_for_issues(&issue_ids, Some(&author.id))
        .await
        .expect("batch reactions");
    let i1_groups: Vec<_> = reaction_pairs
        .iter()
        .filter(|(iid, _)| iid == &i1.id)
        .map(|(_, g)| g)
        .collect();
    assert_eq!(i1_groups.len(), 1);
    assert_eq!(i1_groups[0].content, "+1");
    assert_eq!(i1_groups[0].count, 2);
    assert!(i1_groups[0].viewer_has_reacted);

    // Comment reactions.
    let comment_ids = vec![c1.id.clone(), c2.id.clone(), c3.id.clone()];
    let cr_pairs = db
        .list_comment_reaction_groups_for_comments(&comment_ids, Some(&other.id))
        .await
        .expect("batch comment reactions");
    assert_eq!(cr_pairs.len(), 1);
    assert_eq!(cr_pairs[0].0, c1.id);
    assert_eq!(cr_pairs[0].1.content, "heart");
    assert!(cr_pairs[0].1.viewer_has_reacted);

    // find_many lookups return the same rows as find_by_id.
    let users = db
        .find_users_by_ids(&[author.id.clone(), other.id.clone()])
        .await
        .expect("batch users");
    assert_eq!(users.len(), 2);
    for u in [&author, &other] {
        assert!(users.iter().any(|x| x.id == u.id && x.username == u.username));
    }
    let repos = db
        .find_repositories_by_ids(std::slice::from_ref(&repo.id))
        .await
        .expect("batch repos");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "batch-demo");

    // Empty inputs must not error.
    assert!(db.find_users_by_ids(&[]).await.expect("empty users").is_empty());
    assert!(
        db.list_labels_for_issues(&[])
            .await
            .expect("empty labels")
            .is_empty()
    );
    assert!(
        db.list_issue_reaction_groups_for_issues(&[], None)
            .await
            .expect("empty reactions")
            .is_empty()
    );
}

/// Tri-dialect parity: postgres and mysql siblings must exist alongside sqlite.
#[test]
fn dialect_issues_tri_dialect_files() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!("{root}/{dialect}/0011_issues.sql");
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "missing {path} — tri-dialect 0011_issues required"
        );
        for needle in [
            "issues",
            "issue_counters",
            "issue_comments",
            "issue_revisions",
            "comment_revisions",
            "labels",
            "repo_hidden_labels",
            "issue_labels",
            "issue_assignees",
            "issue_reactions",
            "comment_reactions",
            "issue_links",
            "max_number",
        ] {
            assert!(
                sql.contains(needle),
                "{dialect} 0011_issues must mention {needle}"
            );
        }
        assert!(
            sql.contains("pr_stub") || sql.contains("'pr_stub'"),
            "{dialect} 0011_issues must mention pr_stub"
        );
    }
}
