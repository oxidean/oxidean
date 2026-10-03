//! DEBT-06: `repo_access` bulk read-ACL predicates — functional coverage on
//! the SQLite dialect (the same dialect the API integration suite runs on).

use oxidean_core::Role;
use oxidean_db::Database;

async fn user(db: &Database, id: &str, username: &str) -> oxidean_db::UserRow {
    db.create_user(
        id,
        &format!("{username}@ex.com"),
        username,
        Some("hash"),
        username,
        "",
        None,
        Role::User,
    )
    .await
    .expect("user")
}

#[tokio::test]
async fn readable_repo_ids_and_readers_of_repo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("access.db").display());
    let db = Database::connect(&url).await.expect("connect");
    db.migrate().await.expect("migrate");

    let owner = user(&db, "u-owner", "owner").await;
    let stranger = user(&db, "u-stranger", "stranger").await;
    let collab = user(&db, "u-collab", "collab").await;
    let orgowner = user(&db, "u-orgowner", "orgowner").await;
    let orgmember = user(&db, "u-orgmember", "orgmember").await;

    let public_repo = db
        .insert_repository("r-pub", &owner.id, "user", "pubrepo", "public", "", "main")
        .await
        .expect("public repo");
    let private_repo = db
        .insert_repository("r-priv", &owner.id, "user", "privrepo", "private", "", "main")
        .await
        .expect("private repo");

    let org = db
        .insert_organization("o1", "theorg", "The Org", "read")
        .await
        .expect("org");
    db.insert_org_member(&org.id, &orgowner.id, "owner")
        .await
        .expect("org owner member");
    db.insert_org_member(&org.id, &orgmember.id, "member")
        .await
        .expect("org member");
    let org_repo = db
        .insert_repository("r-org", &org.id, "org", "orgrepo", "private", "", "main")
        .await
        .expect("org repo");

    db.insert_repo_collaborator(&private_repo.id, &collab.id, "read")
        .await
        .expect("collaborator");
    db.insert_repo_collaborator(&org_repo.id, &collab.id, "write")
        .await
        .expect("org repo collaborator");

    let ids = vec![
        public_repo.id.clone(),
        private_repo.id.clone(),
        org_repo.id.clone(),
        "r-missing".to_string(),
    ];
    let has = |set: &std::collections::HashSet<String>, id: &str| set.contains(id);

    // Personal owner reads own private repo; public repo readable by all.
    let set = db
        .readable_repo_ids(&owner.id, &ids)
        .await
        .expect("owner readable");
    assert!(has(&set, &public_repo.id) && has(&set, &private_repo.id) && !has(&set, &org_repo.id));

    // Stranger: only the public repo.
    let set = db
        .readable_repo_ids(&stranger.id, &ids)
        .await
        .expect("stranger readable");
    assert!(has(&set, &public_repo.id) && !has(&set, &private_repo.id) && !has(&set, &org_repo.id));

    // Collaborator reads both granted repos.
    let set = db
        .readable_repo_ids(&collab.id, &ids)
        .await
        .expect("collab readable");
    assert!(has(&set, &private_repo.id) && has(&set, &org_repo.id));

    // Org member with base=read reads the org repo; owner reads it via role.
    let set = db
        .readable_repo_ids(&orgmember.id, &ids)
        .await
        .expect("member readable");
    assert!(has(&set, &org_repo.id) && !has(&set, &private_repo.id));
    let set = db
        .readable_repo_ids(&orgowner.id, &ids)
        .await
        .expect("org owner readable");
    assert!(has(&set, &org_repo.id));

    // member_base → none revokes the member's read.
    db.update_organization_settings(&org.id, Some("none"), None)
        .await
        .expect("member_base none");
    let set = db
        .readable_repo_ids(&orgmember.id, &ids)
        .await
        .expect("member readable none");
    assert!(!has(&set, &org_repo.id), "member loses read at base=none");
    let set = db
        .readable_repo_ids(&orgowner.id, &ids)
        .await
        .expect("org owner readable none");
    assert!(has(&set, &org_repo.id), "org owner keeps read at base=none");
    db.update_organization_settings(&org.id, Some("read"), None)
        .await
        .expect("member_base read");

    // Missing repo never reads.
    let set = db
        .readable_repo_ids(&owner.id, &["r-missing".to_string()])
        .await
        .expect("missing readable");
    assert!(set.is_empty());

    // readers_of_repo on the private org repo: org owner + member + collab.
    let readers = db
        .readers_of_repo(
            &org_repo.id,
            &[
                owner.id.clone(),
                stranger.id.clone(),
                collab.id.clone(),
                orgowner.id.clone(),
                orgmember.id.clone(),
            ],
        )
        .await
        .expect("readers of org repo");
    assert!(readers.contains(&collab.id));
    assert!(readers.contains(&orgowner.id));
    assert!(readers.contains(&orgmember.id));
    assert!(!readers.contains(&owner.id) && !readers.contains(&stranger.id));

    // Empty inputs short-circuit.
    assert!(
        db.readable_repo_ids(&owner.id, &[]).await.expect("empty").is_empty()
    );
    assert!(
        db.readers_of_repo(&org_repo.id, &[])
            .await
            .expect("empty")
            .is_empty()
    );
}
