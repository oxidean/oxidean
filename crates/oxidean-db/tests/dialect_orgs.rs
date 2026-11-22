//! 10-02: `0010_orgs_acl` + organizations / members / invites / collaborators.

use oxidean_core::Role;
use oxidean_db::Database;

/// Expect sqlite `0010_orgs_acl.sql` with org ACL tables + repositories.owner_type,
/// then insert organization + Owner membership round-trip.
#[tokio::test]
async fn dialect_orgs_migrate_0010_schema_presence() {
    let migration_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/sqlite/0010_orgs_acl.sql"
    );
    let sql = std::fs::read_to_string(migration_path).unwrap_or_default();
    assert!(
        !sql.is_empty(),
        "0010_orgs_acl.sql must exist (organizations + members + invites + collaborators)"
    );
    assert!(
        sql.contains("organizations"),
        "0010 must define organizations"
    );
    assert!(
        sql.contains("organization_members"),
        "0010 must define organization_members"
    );
    assert!(
        sql.contains("organization_invites"),
        "0010 must define organization_invites"
    );
    assert!(
        sql.contains("repository_collaborators"),
        "0010 must define repository_collaborators"
    );
    assert!(
        sql.contains("owner_type"),
        "0010 must add repositories.owner_type"
    );
    assert!(
        sql.contains("token_hash") || sql.contains("member_base_permission"),
        "0010 must include invite token_hash and/or member_base_permission"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("orgs.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate 0010_orgs_acl");

    let owner = db
        .create_user(
            "u-org-owner",
            "orgowner@example.com",
            "orgowner",
            Some("hash"),
            "Org Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner user");

    let org = db
        .insert_organization("o-acme", "acme", "Acme Corp", "none")
        .await
        .expect("insert organization");
    assert_eq!(org.id, "o-acme");
    assert_eq!(org.slug, "acme");
    assert_eq!(org.display_name, "Acme Corp");
    assert_eq!(org.member_base_permission, "none");

    let by_slug = db
        .find_organization_by_slug("ACME")
        .await
        .expect("find by slug")
        .expect("org present");
    assert_eq!(by_slug.id, "o-acme");

    let member = db
        .insert_org_owner_membership(&org.id, &owner.id)
        .await
        .expect("insert Owner membership");
    assert_eq!(member.org_id, org.id);
    assert_eq!(member.user_id, owner.id);
    assert_eq!(member.role, "owner");

    let found = db
        .find_org_member(&org.id, &owner.id)
        .await
        .expect("find member")
        .expect("membership present");
    assert_eq!(found.role, "owner");

    // Polymorphic owner_type column exists and defaults via insert argument.
    let repo = db
        .insert_repository("r-org-1", &owner.id, "user", "demo", "private", "", "main")
        .await
        .expect("insert user-owned repo after 0010");
    assert_eq!(repo.owner_id, owner.id);
    assert_eq!(repo.owner_type, "user");

    let org_repo = db
        .insert_repository("r-org-2", &org.id, "org", "org-demo", "public", "", "main")
        .await
        .expect("insert org-owned repo");
    assert_eq!(org_repo.owner_id, org.id);
    assert_eq!(org_repo.owner_type, "org");
}

/// Tri-dialect parity: postgres and mysql siblings must exist alongside sqlite.
#[test]
fn dialect_orgs_tri_dialect_files() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/migrations");
    for dialect in ["sqlite", "postgres", "mysql"] {
        let path = format!("{root}/{dialect}/0010_orgs_acl.sql");
        let sql = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !sql.is_empty(),
            "missing {path} — tri-dialect 0010_orgs_acl required"
        );
        assert!(
            sql.contains("organizations"),
            "{dialect} 0010_orgs_acl must mention organizations"
        );
        assert!(
            sql.contains("organization_members"),
            "{dialect} 0010_orgs_acl must mention organization_members"
        );
        assert!(
            sql.contains("organization_invites"),
            "{dialect} 0010_orgs_acl must mention organization_invites"
        );
        assert!(
            sql.contains("repository_collaborators"),
            "{dialect} 0010_orgs_acl must mention repository_collaborators"
        );
        assert!(
            sql.contains("owner_type"),
            "{dialect} 0010_orgs_acl must mention owner_type"
        );
        assert!(
            sql.contains("member_base_permission"),
            "{dialect} 0010_orgs_acl must mention member_base_permission"
        );
        assert!(
            sql.contains("token_hash"),
            "{dialect} 0010_orgs_acl must mention invite token_hash"
        );
    }
}

#[tokio::test]
async fn team_repo_grants_do_not_survive_repository_transfer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("team-transfer.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let member = db
        .create_user(
            "u-transfer-member",
            "transfer-member@example.com",
            "transfer_member",
            Some("hash"),
            "Transfer Member",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create member");
    let former_org = db
        .insert_organization("o-transfer-a", "transfer-a", "Transfer A", "none")
        .await
        .expect("insert former org");
    let new_org = db
        .insert_organization("o-transfer-b", "transfer-b", "Transfer B", "none")
        .await
        .expect("insert new org");
    db.insert_org_owner_membership(&former_org.id, &member.id)
        .await
        .expect("insert org member");
    let team = db
        .insert_org_team("t-transfer", &former_org.id, "devs", "Devs", "")
        .await
        .expect("insert team");
    db.insert_team_member(&team.id, &member.id, "member")
        .await
        .expect("insert team member");
    let repo = db
        .insert_repository(
            "r-transfer",
            &former_org.id,
            "org",
            "private-project",
            "private",
            "",
            "main",
        )
        .await
        .expect("insert repository");
    db.upsert_team_repo(&team.id, &repo.id, "admin")
        .await
        .expect("grant team access");

    assert_eq!(
        db.max_team_permission_for_user_repo(&repo.id, &member.id)
            .await
            .expect("resolve initial team access")
            .as_deref(),
        Some("admin")
    );

    db.update_repository_owner(&repo.id, &new_org.id, "org")
        .await
        .expect("transfer repository");

    assert_eq!(
        db.max_team_permission_for_user_repo(&repo.id, &member.id)
            .await
            .expect("resolve team access after transfer"),
        None
    );
    assert!(db
        .list_repo_teams(&repo.id)
        .await
        .expect("list repo teams")
        .is_empty());
    assert!(db
        .list_team_repos(&team.id)
        .await
        .expect("list team repos")
        .is_empty());
}

#[tokio::test]
async fn removing_org_member_also_removes_team_membership() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("team-member-removal.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let member = db
        .create_user(
            "u-team-removal",
            "team-removal@example.com",
            "team_removal",
            Some("hash"),
            "Team Removal",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create member");
    let org = db
        .insert_organization("o-team-removal", "team-removal", "Team Removal", "none")
        .await
        .expect("insert org");
    db.insert_org_owner_membership(&org.id, &member.id)
        .await
        .expect("insert org member");
    let team = db
        .insert_org_team("t-team-removal", &org.id, "devs", "Devs", "")
        .await
        .expect("insert team");
    db.insert_team_member(&team.id, &member.id, "member")
        .await
        .expect("insert team member");

    db.remove_org_member(&org.id, &member.id)
        .await
        .expect("remove org member");

    assert!(db
        .find_team_member(&team.id, &member.id)
        .await
        .expect("find team member")
        .is_none());
}
