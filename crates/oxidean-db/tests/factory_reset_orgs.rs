//! 10-12: factory_reset_instance must wipe org ACL tables and all repository rows.

use oxidean_core::Role;
use oxidean_db::Database;

/// Seed org + members + invite + collab + user/org repos, then factory-reset.
/// After reset every org ACL table and repository row must be empty.
#[tokio::test]
async fn factory_reset_wipes_orgs_members_invites_collaborators_and_repos() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!(
        "sqlite:{}",
        dir.path().join("factory_reset_orgs.db").display()
    );
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = db
        .create_user(
            "u-owner",
            "owner@example.com",
            "owneruser",
            Some("hash"),
            "Owner",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create owner");
    let collab = db
        .create_user(
            "u-collab",
            "collab@example.com",
            "collabuser",
            Some("hash"),
            "Collab",
            "",
            None,
            Role::User,
        )
        .await
        .expect("create collab");

    let org = db
        .insert_organization("o-acme", "acme", "Acme Corp", "read")
        .await
        .expect("insert org");
    db.insert_org_owner_membership(&org.id, &owner.id)
        .await
        .expect("owner membership");
    db.insert_org_invite(
        "inv-1",
        &org.id,
        Some("new@example.com"),
        "member",
        "tokenhashabc",
        Some("2099-01-01 00:00:00"),
        &owner.id,
        Some(1),
    )
    .await
    .expect("invite");

    let user_repo = db
        .insert_repository(
            "r-user-1",
            &owner.id,
            "user",
            "personal",
            "private",
            "",
            "main",
        )
        .await
        .expect("user repo");
    let org_repo = db
        .insert_repository("r-org-1", &org.id, "org", "shared", "private", "", "main")
        .await
        .expect("org repo");
    db.insert_repo_collaborator(&user_repo.id, &collab.id, "write")
        .await
        .expect("collaborator");

    // Sanity: seed landed.
    assert!(db.find_organization_by_id(&org.id).await.unwrap().is_some());
    assert!(db
        .find_org_member(&org.id, &owner.id)
        .await
        .unwrap()
        .is_some());
    assert!(db.find_org_invite_by_id("inv-1").await.unwrap().is_some());
    assert!(db.find_repository_by_id(&user_repo.id).await.unwrap().is_some());
    assert!(db.find_repository_by_id(&org_repo.id).await.unwrap().is_some());
    assert!(db
        .find_repo_collaborator(&user_repo.id, &collab.id)
        .await
        .unwrap()
        .is_some());
    assert!(db.count_users().await.unwrap() >= 2);

    db.factory_reset_instance()
        .await
        .expect("factory_reset_instance");

    assert_eq!(db.count_users().await.unwrap(), 0, "users wiped");
    assert!(
        db.find_organization_by_id(&org.id).await.unwrap().is_none(),
        "organizations must be wiped"
    );
    assert!(
        db.find_org_member(&org.id, &owner.id)
            .await
            .unwrap()
            .is_none(),
        "organization_members must be wiped"
    );
    assert!(
        db.find_org_invite_by_id("inv-1").await.unwrap().is_none(),
        "organization_invites must be wiped"
    );
    assert!(
        db.find_repository_by_id(&user_repo.id)
            .await
            .unwrap()
            .is_none(),
        "user-owned repositories must be wiped"
    );
    assert!(
        db.find_repository_by_id(&org_repo.id)
            .await
            .unwrap()
            .is_none(),
        "org-owned repositories must be wiped"
    );
    assert!(
        db.find_repo_collaborator(&user_repo.id, &collab.id)
            .await
            .unwrap()
            .is_none(),
        "repository_collaborators must be wiped"
    );
}
