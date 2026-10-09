//! Phase 11: factory_reset_instance must wipe issue domain tables via repository CASCADE.

use oxidean_core::Role;
use oxidean_db::Database;
use sqlx::sqlite::SqlitePoolOptions;

async fn count_table(db_path: &std::path::Path, table: &str) -> i64 {
    let url = format!("sqlite:{}", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("reconnect for count");
    let sql = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(&*sql))
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("count {table}: {e}"))
}

async fn seed_issue_domain(db: &Database) -> (String, String, String, String) {
    let author = db
        .create_user(
            "u-iss-author",
            "issauthor@example.com",
            "issauthor",
            Some("hash"),
            "Issue Author",
            "",
            None,
            Role::User,
        )
        .await
        .expect("author");
    let assignee = db
        .create_user(
            "u-iss-assignee",
            "issassignee@example.com",
            "issassignee",
            Some("hash"),
            "Assignee",
            "",
            None,
            Role::User,
        )
        .await
        .expect("assignee");

    let org = db
        .insert_organization("o-iss", "issorg", "Issue Org", "read")
        .await
        .expect("org");
    db.insert_org_owner_membership(&org.id, &author.id)
        .await
        .expect("membership");

    let repo = db
        .insert_repository(
            "r-iss-1",
            &author.id,
            "user",
            "issues-wipe",
            "private",
            "",
            "main",
        )
        .await
        .expect("repo");

    let issue = db
        .insert_issue("i-wipe-1", &repo.id, &author.id, "Wipe me", "body")
        .await
        .expect("issue");
    assert_eq!(issue.number, 1);

    db.insert_issue_revision(
        "rev-wipe-1",
        &issue.id,
        &author.id,
        "Wipe me",
        "body edited",
    )
    .await
    .expect("issue revision");

    let comment = db
        .insert_issue_comment("c-wipe-1", &issue.id, &author.id, "a comment")
        .await
        .expect("comment");
    db.insert_comment_revision("crev-wipe-1", &comment.id, &author.id, "edited comment")
        .await
        .expect("comment revision");

    let repo_label = db
        .insert_label(
            "lab-repo-1",
            "bug",
            "#ff0000",
            "repo bug",
            None,
            Some(&repo.id),
        )
        .await
        .expect("repo label");
    let org_label = db
        .insert_label(
            "lab-org-1",
            "priority",
            "#00ff00",
            "org priority",
            Some(&org.id),
            None,
        )
        .await
        .expect("org label");
    db.set_repo_label_hidden(&repo.id, &org_label.id, true)
        .await
        .expect("hide org label");
    db.set_issue_labels(&issue.id, &[repo_label.id.clone(), org_label.id.clone()])
        .await
        .expect("issue labels");
    db.set_issue_assignees(&issue.id, &[assignee.id.clone()])
        .await
        .expect("assignees");

    assert!(db
        .toggle_issue_reaction(&issue.id, &author.id, "+1")
        .await
        .expect("issue reaction"));
    assert!(db
        .toggle_comment_reaction(&comment.id, &author.id, "heart")
        .await
        .expect("comment reaction"));

    db.insert_issue_link(
        "link-wipe-1",
        &issue.id,
        "pr_stub",
        Some(&repo.id),
        Some(42),
        Some("opaque-pr-1"),
        Some("Linked PR stub"),
        &author.id,
    )
    .await
    .expect("issue link");

    // Sanity: seed landed.
    assert!(db.find_issue_by_id(&issue.id).await.unwrap().is_some());
    assert!(db
        .find_issue_comment_by_id(&comment.id)
        .await
        .unwrap()
        .is_some());
    assert!(db.find_label_by_id(&repo_label.id).await.unwrap().is_some());
    assert!(db.find_label_by_id(&org_label.id).await.unwrap().is_some());
    assert_eq!(db.list_issue_assignees(&issue.id).await.unwrap().len(), 1);
    assert!(!db
        .list_issue_reaction_groups(&issue.id, None)
        .await
        .unwrap()
        .is_empty());
    assert!(!db
        .list_comment_reaction_groups(&comment.id, None)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(db.list_issue_links(&issue.id).await.unwrap().len(), 1);
    assert_eq!(db.list_issue_revisions(&issue.id).await.unwrap().len(), 1);
    assert_eq!(
        db.list_comment_revisions(&comment.id).await.unwrap().len(),
        1
    );

    (repo.id, issue.id, comment.id, org.id)
}

/// After factory_reset_instance, issue domain tables are empty (cascade or ordered delete).
#[tokio::test]
async fn factory_reset_issues_wipes_issue_domain_tables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("factory_reset_issues.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let (repo_id, issue_id, comment_id, org_id) = seed_issue_domain(&db).await;

    db.factory_reset_instance()
        .await
        .expect("factory_reset_instance");

    assert_eq!(db.count_users().await.unwrap(), 0, "users wiped");
    assert!(db.find_repository_by_id(&repo_id).await.unwrap().is_none());
    assert!(db.find_organization_by_id(&org_id).await.unwrap().is_none());
    assert!(db.find_issue_by_id(&issue_id).await.unwrap().is_none());
    assert!(db
        .find_issue_comment_by_id(&comment_id)
        .await
        .unwrap()
        .is_none());
    assert!(db.find_label_by_id("lab-repo-1").await.unwrap().is_none());
    assert!(db.find_label_by_id("lab-org-1").await.unwrap().is_none());
    assert!(db
        .find_issue_link_by_id("link-wipe-1")
        .await
        .unwrap()
        .is_none());

    for table in [
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
        let n = count_table(&db_path, table).await;
        assert_eq!(n, 0, "{table} must be empty after factory_reset");
    }
}

/// Seeding issue rows then deleting the parent repository cascades issue children.
#[tokio::test]
async fn factory_reset_issues_repository_cascade() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("factory_reset_issues_cascade.db");
    let url = format!("sqlite:{}", db_path.display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let (repo_id, issue_id, comment_id, _org_id) = seed_issue_domain(&db).await;

    db.hard_delete_repository(&repo_id)
        .await
        .expect("hard_delete_repository");

    assert!(db.find_repository_by_id(&repo_id).await.unwrap().is_none());
    assert!(db.find_issue_by_id(&issue_id).await.unwrap().is_none());
    assert!(db
        .find_issue_comment_by_id(&comment_id)
        .await
        .unwrap()
        .is_none());
    assert!(db.find_label_by_id("lab-repo-1").await.unwrap().is_none());
    // Org-scoped label survives repo delete (owned by org, not repo).
    assert!(db.find_label_by_id("lab-org-1").await.unwrap().is_some());

    for table in [
        "issues",
        "issue_counters",
        "issue_comments",
        "issue_revisions",
        "comment_revisions",
        "issue_labels",
        "issue_assignees",
        "issue_reactions",
        "comment_reactions",
        "issue_links",
        "repo_hidden_labels",
    ] {
        let n = count_table(&db_path, table).await;
        assert_eq!(
            n, 0,
            "{table} must cascade-wipe when parent repository is deleted"
        );
    }
}
