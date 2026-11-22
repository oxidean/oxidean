//! Organization teams: team rows, team membership (`member` | `maintainer`),
//! and per-repo team access grants (`read` | `write` | `admin`) — COL-05.
//! Dialect SQL only; ACL semantics live in `oxidean-api-core::repo::acl`.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct OrgTeamRow {
    pub id: String,
    pub org_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct OrgTeamMemberRow {
    pub team_id: String,
    pub user_id: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct OrgTeamRepoRow {
    pub team_id: String,
    pub repo_id: String,
    pub permission: String,
    pub created_at: String,
}

/// Team + aggregate counts for `org.teams.list` / `org.teams.get`.
#[derive(Debug, Clone)]
pub struct OrgTeamListRow {
    pub id: String,
    pub org_id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub member_count: i64,
    pub repo_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Team member + username for `org.teams.members.list` (no email).
#[derive(Debug, Clone)]
pub struct OrgTeamMemberListRow {
    pub team_id: String,
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub created_at: String,
}

/// Team repo grant + repo name for `org.teams.repos.list`.
#[derive(Debug, Clone)]
pub struct OrgTeamRepoListRow {
    pub team_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub permission: String,
    pub created_at: String,
}

/// Team grant on one repo + team identity for `repo.teams.list`.
#[derive(Debug, Clone)]
pub struct RepoTeamListRow {
    pub team_id: String,
    pub slug: String,
    pub name: String,
    pub permission: String,
    pub created_at: String,
}

macro_rules! map_team {
    ($row:expr) => {{
        let row = $row;
        OrgTeamRow {
            id: row.try_get("id").map_err(|e| format!("team row: {e}"))?,
            org_id: row
                .try_get("org_id")
                .map_err(|e| format!("team row: {e}"))?,
            slug: row
                .try_get("slug")
                .map_err(|e| format!("team row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("team row: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("team row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("team row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("team row: {e}"))?,
        }
    }};
}

macro_rules! map_team_member {
    ($row:expr) => {{
        let row = $row;
        OrgTeamMemberRow {
            team_id: row
                .try_get("team_id")
                .map_err(|e| format!("team member row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("team member row: {e}"))?,
            role: row
                .try_get("role")
                .map_err(|e| format!("team member row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("team member row: {e}"))?,
        }
    }};
}

macro_rules! map_team_repo {
    ($row:expr) => {{
        let row = $row;
        OrgTeamRepoRow {
            team_id: row
                .try_get("team_id")
                .map_err(|e| format!("team repo row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("team repo row: {e}"))?,
            permission: row
                .try_get("permission")
                .map_err(|e| format!("team repo row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("team repo row: {e}"))?,
        }
    }};
}

const TEAM_SELECT_PG: &str = "SELECT id, org_id, slug, name, description,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM organization_teams";

const TEAM_SELECT_MYSQL: &str = "SELECT id, org_id, slug, name, description,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM organization_teams";

const TEAM_SELECT_SQLITE: &str = "SELECT id, org_id, slug, name, description,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM organization_teams";

const TEAM_MEMBER_SELECT_PG: &str = "SELECT team_id, user_id, role,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_team_members";

const TEAM_MEMBER_SELECT_MYSQL: &str = "SELECT team_id, user_id, role,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_team_members";

const TEAM_MEMBER_SELECT_SQLITE: &str = "SELECT team_id, user_id, role,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM organization_team_members";

const TEAM_REPO_SELECT_PG: &str = "SELECT team_id, repo_id, permission,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_team_repos";

const TEAM_REPO_SELECT_MYSQL: &str = "SELECT team_id, repo_id, permission,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_team_repos";

const TEAM_REPO_SELECT_SQLITE: &str = "SELECT team_id, repo_id, permission,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM organization_team_repos";

/// Insert a team row (id is caller-generated uuid; slug pre-validated).
pub async fn insert_team(
    pool: &DbPool,
    id: &str,
    org_id: &str,
    slug: &str,
    name: &str,
    description: &str,
) -> Result<OrgTeamRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO organization_teams (id, org_id, slug, name, description)
VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(org_id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("insert org team failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO organization_teams (id, org_id, slug, name, description)
VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(org_id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("insert org team failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO organization_teams (id, org_id, slug, name, description)
VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(org_id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("insert org team failed: {e}"))?;
        }
    }
    find_team_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert org team failed: row missing after insert".into())
}

/// Find a team inside an org by slug (case-insensitive).
pub async fn find_team_by_slug(
    pool: &DbPool,
    org_id: &str,
    slug: &str,
) -> Result<Option<OrgTeamRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_SELECT_PG} WHERE org_id = $1 AND lower(slug) = lower($2)"
            )))
            .bind(org_id)
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org team failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team!(&r))),
            None => Ok(None),
        }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_SELECT_MYSQL} WHERE org_id = ? AND lower(slug) = lower(?)"
            )))
            .bind(org_id)
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org team failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team!(&r))),
            None => Ok(None),
        }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_SELECT_SQLITE} WHERE org_id = ?1 AND lower(slug) = lower(?2)"
            )))
            .bind(org_id)
            .bind(slug)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org team failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team!(&r))),
            None => Ok(None),
        }
        }
    }
}

/// Find a team by primary key.
pub async fn find_team_by_id(pool: &DbPool, team_id: &str) -> Result<Option<OrgTeamRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_SELECT_PG} WHERE id = $1"
            )))
            .bind(team_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org team failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team!(&r))),
            None => Ok(None),
        }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_SELECT_MYSQL} WHERE id = ?"
            )))
            .bind(team_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org team failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team!(&r))),
            None => Ok(None),
        }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_SELECT_SQLITE} WHERE id = ?1"
            )))
            .bind(team_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find org team failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team!(&r))),
            None => Ok(None),
        }
        }
    }
}

/// All teams of an org with member/repo counts, ordered by slug.
pub async fn list_teams_for_org(
    pool: &DbPool,
    org_id: &str,
) -> Result<Vec<OrgTeamListRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT t.id, t.org_id, t.slug, t.name, t.description,
       (SELECT COUNT(*) FROM organization_team_members tm WHERE tm.team_id = t.id)::bigint AS member_count,
       (SELECT COUNT(*) FROM organization_team_repos tr WHERE tr.team_id = t.id)::bigint AS repo_count,
       to_char(t.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(t.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM organization_teams t
WHERE t.org_id = $1
ORDER BY lower(t.slug) ASC",
            )
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org teams failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamListRow {
                        id: row.try_get("id").map_err(|e| format!("team list: {e}"))?,
                        org_id: row
                            .try_get("org_id")
                            .map_err(|e| format!("team list: {e}"))?,
                        slug: row
                            .try_get("slug")
                            .map_err(|e| format!("team list: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("team list: {e}"))?,
                        description: row
                            .try_get("description")
                            .map_err(|e| format!("team list: {e}"))?,
                        member_count: row
                            .try_get("member_count")
                            .map_err(|e| format!("team list: {e}"))?,
                        repo_count: row
                            .try_get("repo_count")
                            .map_err(|e| format!("team list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("team list: {e}"))?,
                        updated_at: row
                            .try_get("updated_at")
                            .map_err(|e| format!("team list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT t.id, t.org_id, t.slug, t.name, t.description,
       (SELECT COUNT(*) FROM organization_team_members tm WHERE tm.team_id = t.id) AS member_count,
       (SELECT COUNT(*) FROM organization_team_repos tr WHERE tr.team_id = t.id) AS repo_count,
       DATE_FORMAT(t.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(t.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM organization_teams t
WHERE t.org_id = ?
ORDER BY lower(t.slug) ASC",
            )
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org teams failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamListRow {
                        id: row.try_get("id").map_err(|e| format!("team list: {e}"))?,
                        org_id: row
                            .try_get("org_id")
                            .map_err(|e| format!("team list: {e}"))?,
                        slug: row
                            .try_get("slug")
                            .map_err(|e| format!("team list: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("team list: {e}"))?,
                        description: row
                            .try_get("description")
                            .map_err(|e| format!("team list: {e}"))?,
                        member_count: row
                            .try_get("member_count")
                            .map_err(|e| format!("team list: {e}"))?,
                        repo_count: row
                            .try_get("repo_count")
                            .map_err(|e| format!("team list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("team list: {e}"))?,
                        updated_at: row
                            .try_get("updated_at")
                            .map_err(|e| format!("team list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT t.id, t.org_id, t.slug, t.name, t.description,
       (SELECT COUNT(*) FROM organization_team_members tm WHERE tm.team_id = t.id) AS member_count,
       (SELECT COUNT(*) FROM organization_team_repos tr WHERE tr.team_id = t.id) AS repo_count,
       strftime('%Y-%m-%dT%H:%M:%SZ', t.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', t.updated_at) AS updated_at
FROM organization_teams t
WHERE t.org_id = ?1
ORDER BY lower(t.slug) ASC",
            )
            .bind(org_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list org teams failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamListRow {
                        id: row.try_get("id").map_err(|e| format!("team list: {e}"))?,
                        org_id: row
                            .try_get("org_id")
                            .map_err(|e| format!("team list: {e}"))?,
                        slug: row
                            .try_get("slug")
                            .map_err(|e| format!("team list: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("team list: {e}"))?,
                        description: row
                            .try_get("description")
                            .map_err(|e| format!("team list: {e}"))?,
                        member_count: row
                            .try_get("member_count")
                            .map_err(|e| format!("team list: {e}"))?,
                        repo_count: row
                            .try_get("repo_count")
                            .map_err(|e| format!("team list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("team list: {e}"))?,
                        updated_at: row
                            .try_get("updated_at")
                            .map_err(|e| format!("team list: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Update a team's name/description/slug (callers pass resolved values).
pub async fn update_team(
    pool: &DbPool,
    team_id: &str,
    slug: &str,
    name: &str,
    description: &str,
) -> Result<OrgTeamRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let n = sqlx::query(
                "UPDATE organization_teams SET slug = $2, name = $3, description = $4, updated_at = now()
WHERE id = $1",
            )
            .bind(team_id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("update org team failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("org team not found".into());
            }
        }
        DbPool::MySql(p) => {
            let n = sqlx::query(
                "UPDATE organization_teams SET slug = ?, name = ?, description = ?
WHERE id = ?",
            )
            .bind(slug)
            .bind(name)
            .bind(description)
            .bind(team_id)
            .execute(p)
            .await
            .map_err(|e| format!("update org team failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("org team not found".into());
            }
        }
        DbPool::Sqlite(p) => {
            let n = sqlx::query(
                "UPDATE organization_teams SET slug = ?2, name = ?3, description = ?4, updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
WHERE id = ?1",
            )
            .bind(team_id)
            .bind(slug)
            .bind(name)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| format!("update org team failed: {e}"))?
            .rows_affected();
            if n == 0 {
                return Err("org team not found".into());
            }
        }
    }
    find_team_by_id(pool, team_id)
        .await?
        .ok_or_else(|| "org team not found".into())
}

/// Delete a team (cascades members + repo grants).
pub async fn delete_team(pool: &DbPool, team_id: &str) -> Result<(), String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query("DELETE FROM organization_teams WHERE id = $1")
            .bind(team_id)
            .execute(p)
            .await
            .map_err(|e| format!("delete org team failed: {e}"))?
            .rows_affected(),
        DbPool::MySql(p) => sqlx::query("DELETE FROM organization_teams WHERE id = ?")
            .bind(team_id)
            .execute(p)
            .await
            .map_err(|e| format!("delete org team failed: {e}"))?
            .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM organization_teams WHERE id = ?1")
            .bind(team_id)
            .execute(p)
            .await
            .map_err(|e| format!("delete org team failed: {e}"))?
            .rows_affected(),
    };
    if n == 0 {
        return Err("org team not found".into());
    }
    Ok(())
}

/// Insert a team membership (`member` | `maintainer`).
pub async fn insert_team_member(
    pool: &DbPool,
    team_id: &str,
    user_id: &str,
    role: &str,
) -> Result<OrgTeamMemberRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO organization_team_members (team_id, user_id, role)
VALUES ($1, $2, $3)",
            )
            .bind(team_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("insert team member failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO organization_team_members (team_id, user_id, role)
VALUES (?, ?, ?)",
            )
            .bind(team_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("insert team member failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO organization_team_members (team_id, user_id, role)
VALUES (?1, ?2, ?3)",
            )
            .bind(team_id)
            .bind(user_id)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| format!("insert team member failed: {e}"))?;
        }
    }
    find_team_member(pool, team_id, user_id)
        .await?
        .ok_or_else(|| "insert team member failed: row missing after insert".into())
}

/// Look up a team membership.
pub async fn find_team_member(
    pool: &DbPool,
    team_id: &str,
    user_id: &str,
) -> Result<Option<OrgTeamMemberRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_MEMBER_SELECT_PG} WHERE team_id = $1 AND user_id = $2"
            )))
            .bind(team_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find team member failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team_member!(&r))),
            None => Ok(None),
        }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_MEMBER_SELECT_MYSQL} WHERE team_id = ? AND user_id = ?"
            )))
            .bind(team_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find team member failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team_member!(&r))),
            None => Ok(None),
        }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_MEMBER_SELECT_SQLITE} WHERE team_id = ?1 AND user_id = ?2"
            )))
            .bind(team_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find team member failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team_member!(&r))),
            None => Ok(None),
        }
        }
    }
}

/// Team members with username for `org.teams.members.list` (no email).
pub async fn list_team_members(
    pool: &DbPool,
    team_id: &str,
) -> Result<Vec<OrgTeamMemberListRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT m.team_id, m.user_id, u.username, m.role,
       to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_team_members m
JOIN users u ON u.id = m.user_id
WHERE m.team_id = $1
ORDER BY m.created_at ASC, m.user_id ASC",
            )
            .bind(team_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list team members failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamMemberListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("member list: {e}"))?,
                        role: row
                            .try_get("role")
                            .map_err(|e| format!("member list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("member list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT m.team_id, m.user_id, u.username, m.role,
       DATE_FORMAT(m.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_team_members m
JOIN users u ON u.id = m.user_id
WHERE m.team_id = ?
ORDER BY m.created_at ASC, m.user_id ASC",
            )
            .bind(team_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list team members failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamMemberListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("member list: {e}"))?,
                        role: row
                            .try_get("role")
                            .map_err(|e| format!("member list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("member list: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT m.team_id, m.user_id, u.username, m.role,
       strftime('%Y-%m-%dT%H:%M:%SZ', m.created_at) AS created_at
FROM organization_team_members m
JOIN users u ON u.id = m.user_id
WHERE m.team_id = ?1
ORDER BY m.created_at ASC, m.user_id ASC",
            )
            .bind(team_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list team members failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamMemberListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        user_id: row
                            .try_get("user_id")
                            .map_err(|e| format!("member list: {e}"))?,
                        username: row
                            .try_get("username")
                            .map_err(|e| format!("member list: {e}"))?,
                        role: row
                            .try_get("role")
                            .map_err(|e| format!("member list: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("member list: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Change a team member's role.
pub async fn update_team_member_role(
    pool: &DbPool,
    team_id: &str,
    user_id: &str,
    role: &str,
) -> Result<OrgTeamMemberRow, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE organization_team_members SET role = $3
WHERE team_id = $1 AND user_id = $2",
        )
        .bind(team_id)
        .bind(user_id)
        .bind(role)
        .execute(p)
        .await
        .map_err(|e| format!("update team member role failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE organization_team_members SET role = ?
WHERE team_id = ? AND user_id = ?",
        )
        .bind(role)
        .bind(team_id)
        .bind(user_id)
        .execute(p)
        .await
        .map_err(|e| format!("update team member role failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE organization_team_members SET role = ?3
WHERE team_id = ?1 AND user_id = ?2",
        )
        .bind(team_id)
        .bind(user_id)
        .bind(role)
        .execute(p)
        .await
        .map_err(|e| format!("update team member role failed: {e}"))?
        .rows_affected(),
    };
    if n == 0 {
        return Err("team member not found".into());
    }
    find_team_member(pool, team_id, user_id)
        .await?
        .ok_or_else(|| "team member not found".into())
}

/// Remove one member from one team.
pub async fn delete_team_member(
    pool: &DbPool,
    team_id: &str,
    user_id: &str,
) -> Result<(), String> {
    let n = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM organization_team_members WHERE team_id = $1 AND user_id = $2")
                .bind(team_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove team member failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM organization_team_members WHERE team_id = ? AND user_id = ?")
                .bind(team_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove team member failed: {e}"))?
                .rows_affected()
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM organization_team_members WHERE team_id = ?1 AND user_id = ?2")
                .bind(team_id)
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("remove team member failed: {e}"))?
                .rows_affected()
        }
    };
    if n == 0 {
        return Err("team member not found".into());
    }
    Ok(())
}

/// Remove a user from every team in an org (org-member removal path). Team
/// grants are member-scoped, so the rows must die with the membership.
pub async fn delete_team_memberships_for_org_member(
    pool: &DbPool,
    org_id: &str,
    user_id: &str,
) -> Result<u64, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM organization_team_members tm
USING organization_teams t
WHERE t.id = tm.team_id AND t.org_id = $1 AND tm.user_id = $2",
        )
        .bind(org_id)
        .bind(user_id)
        .execute(p)
        .await
        .map_err(|e| format!("remove org member team rows failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "DELETE tm FROM organization_team_members tm
JOIN organization_teams t ON t.id = tm.team_id
WHERE t.org_id = ? AND tm.user_id = ?",
        )
        .bind(org_id)
        .bind(user_id)
        .execute(p)
        .await
        .map_err(|e| format!("remove org member team rows failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM organization_team_members
WHERE user_id = ?1 AND team_id IN (SELECT id FROM organization_teams WHERE org_id = ?2)",
        )
        .bind(user_id)
        .bind(org_id)
        .execute(p)
        .await
        .map_err(|e| format!("remove org member team rows failed: {e}"))?
        .rows_affected(),
    };
    Ok(n)
}

/// Insert or update a team repo grant (`read` | `write` | `admin`).
pub async fn upsert_team_repo(
    pool: &DbPool,
    team_id: &str,
    repo_id: &str,
    permission: &str,
) -> Result<OrgTeamRepoRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO organization_team_repos (team_id, repo_id, permission)
VALUES ($1, $2, $3)
ON CONFLICT (team_id, repo_id) DO UPDATE SET permission = EXCLUDED.permission",
            )
            .bind(team_id)
            .bind(repo_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("upsert team repo failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO organization_team_repos (team_id, repo_id, permission)
VALUES (?, ?, ?)
ON DUPLICATE KEY UPDATE permission = VALUES(permission)",
            )
            .bind(team_id)
            .bind(repo_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("upsert team repo failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO organization_team_repos (team_id, repo_id, permission)
VALUES (?1, ?2, ?3)
ON CONFLICT (team_id, repo_id) DO UPDATE SET permission = excluded.permission",
            )
            .bind(team_id)
            .bind(repo_id)
            .bind(permission)
            .execute(p)
            .await
            .map_err(|e| format!("upsert team repo failed: {e}"))?;
        }
    }
    find_team_repo(pool, team_id, repo_id)
        .await?
        .ok_or_else(|| "upsert team repo failed: row missing after write".into())
}

/// Look up one team's grant on one repo.
pub async fn find_team_repo(
    pool: &DbPool,
    team_id: &str,
    repo_id: &str,
) -> Result<Option<OrgTeamRepoRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_REPO_SELECT_PG} WHERE team_id = $1 AND repo_id = $2"
            )))
            .bind(team_id)
            .bind(repo_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find team repo failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team_repo!(&r))),
            None => Ok(None),
        }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_REPO_SELECT_MYSQL} WHERE team_id = ? AND repo_id = ?"
            )))
            .bind(team_id)
            .bind(repo_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find team repo failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team_repo!(&r))),
            None => Ok(None),
        }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{TEAM_REPO_SELECT_SQLITE} WHERE team_id = ?1 AND repo_id = ?2"
            )))
            .bind(team_id)
            .bind(repo_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find team repo failed: {e}"))?;
            match row {
            Some(r) => Ok(Some(map_team_repo!(&r))),
            None => Ok(None),
        }
        }
    }
}

/// Change an existing team repo grant's permission (strict — missing errs).
pub async fn update_team_repo_permission(
    pool: &DbPool,
    team_id: &str,
    repo_id: &str,
    permission: &str,
) -> Result<OrgTeamRepoRow, String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE organization_team_repos SET permission = $3
WHERE team_id = $1 AND repo_id = $2",
        )
        .bind(team_id)
        .bind(repo_id)
        .bind(permission)
        .execute(p)
        .await
        .map_err(|e| format!("update team repo failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE organization_team_repos SET permission = ?
WHERE team_id = ? AND repo_id = ?",
        )
        .bind(permission)
        .bind(team_id)
        .bind(repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("update team repo failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE organization_team_repos SET permission = ?3
WHERE team_id = ?1 AND repo_id = ?2",
        )
        .bind(team_id)
        .bind(repo_id)
        .bind(permission)
        .execute(p)
        .await
        .map_err(|e| format!("update team repo failed: {e}"))?
        .rows_affected(),
    };
    if n == 0 {
        return Err("team repo grant not found".into());
    }
    find_team_repo(pool, team_id, repo_id)
        .await?
        .ok_or_else(|| "team repo grant not found".into())
}

/// Remove a team repo grant.
pub async fn delete_team_repo(
    pool: &DbPool,
    team_id: &str,
    repo_id: &str,
) -> Result<(), String> {
    let n = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM organization_team_repos WHERE team_id = $1 AND repo_id = $2",
        )
        .bind(team_id)
        .bind(repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("remove team repo failed: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM organization_team_repos WHERE team_id = ? AND repo_id = ?",
        )
        .bind(team_id)
        .bind(repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("remove team repo failed: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM organization_team_repos WHERE team_id = ?1 AND repo_id = ?2",
        )
        .bind(team_id)
        .bind(repo_id)
        .execute(p)
        .await
        .map_err(|e| format!("remove team repo failed: {e}"))?
        .rows_affected(),
    };
    if n == 0 {
        return Err("team repo grant not found".into());
    }
    Ok(())
}

/// Grants held by one team, with repo names, for `org.teams.repos.list`.
pub async fn list_team_repos(
    pool: &DbPool,
    team_id: &str,
) -> Result<Vec<OrgTeamRepoListRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT tr.team_id, tr.repo_id, r.name AS repo_name, tr.permission,
       to_char(tr.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_team_repos tr
JOIN repositories r ON r.id = tr.repo_id AND r.deleted_at IS NULL
WHERE tr.team_id = $1
ORDER BY lower(r.name) ASC",
            )
            .bind(team_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list team repos failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamRepoListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("team repos: {e}"))?,
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("team repos: {e}"))?,
                        repo_name: row
                            .try_get("repo_name")
                            .map_err(|e| format!("team repos: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("team repos: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("team repos: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT tr.team_id, tr.repo_id, r.name AS repo_name, tr.permission,
       DATE_FORMAT(tr.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_team_repos tr
JOIN repositories r ON r.id = tr.repo_id AND r.deleted_at IS NULL
WHERE tr.team_id = ?
ORDER BY lower(r.name) ASC",
            )
            .bind(team_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list team repos failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamRepoListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("team repos: {e}"))?,
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("team repos: {e}"))?,
                        repo_name: row
                            .try_get("repo_name")
                            .map_err(|e| format!("team repos: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("team repos: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("team repos: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT tr.team_id, tr.repo_id, r.name AS repo_name, tr.permission,
       strftime('%Y-%m-%dT%H:%M:%SZ', tr.created_at) AS created_at
FROM organization_team_repos tr
JOIN repositories r ON r.id = tr.repo_id AND r.deleted_at IS NULL
WHERE tr.team_id = ?1
ORDER BY lower(r.name) ASC",
            )
            .bind(team_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list team repos failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(OrgTeamRepoListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("team repos: {e}"))?,
                        repo_id: row
                            .try_get("repo_id")
                            .map_err(|e| format!("team repos: {e}"))?,
                        repo_name: row
                            .try_get("repo_name")
                            .map_err(|e| format!("team repos: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("team repos: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("team repos: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Teams holding a grant on one repo, for `repo.teams.list`.
pub async fn list_repos_teams(
    pool: &DbPool,
    repo_id: &str,
) -> Result<Vec<RepoTeamListRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT tr.team_id, t.slug, t.name, tr.permission,
       to_char(tr.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM organization_team_repos tr
JOIN organization_teams t ON t.id = tr.team_id
WHERE tr.repo_id = $1
ORDER BY lower(t.slug) ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo teams failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(RepoTeamListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        slug: row
                            .try_get("slug")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("repo teams: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT tr.team_id, t.slug, t.name, tr.permission,
       DATE_FORMAT(tr.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM organization_team_repos tr
JOIN organization_teams t ON t.id = tr.team_id
WHERE tr.repo_id = ?
ORDER BY lower(t.slug) ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo teams failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(RepoTeamListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        slug: row
                            .try_get("slug")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("repo teams: {e}"))?,
                    })
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT tr.team_id, t.slug, t.name, tr.permission,
       strftime('%Y-%m-%dT%H:%M:%SZ', tr.created_at) AS created_at
FROM organization_team_repos tr
JOIN organization_teams t ON t.id = tr.team_id
WHERE tr.repo_id = ?1
ORDER BY lower(t.slug) ASC",
            )
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repo teams failed: {e}"))?;
            rows.iter()
                .map(|row| {
                    Ok(RepoTeamListRow {
                        team_id: row
                            .try_get("team_id")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        slug: row
                            .try_get("slug")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        name: row
                            .try_get("name")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        permission: row
                            .try_get("permission")
                            .map_err(|e| format!("repo teams: {e}"))?,
                        created_at: row
                            .try_get("created_at")
                            .map_err(|e| format!("repo teams: {e}"))?,
                    })
                })
                .collect()
        }
    }
}

/// Highest team-derived grant for `user_id` on `repo_id` — the ACL source.
/// Returns `'admin' | 'write' | 'read'` or `None`. Team grants are
/// member-scoped: the EXISTS on `organization_members` keeps a stale team
/// row from outliving org membership even if cleanup missed it.
pub async fn max_team_permission_for_user_repo(
    pool: &DbPool,
    repo_id: &str,
    user_id: &str,
) -> Result<Option<String>, String> {
    const TIER: &str = "CASE LOWER(TRIM(tr.permission))
        WHEN 'admin' THEN 3 WHEN 'write' THEN 2 WHEN 'read' THEN 1 ELSE 0 END";
    let rank = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT MAX({TIER}) FROM organization_team_repos tr
                 JOIN organization_team_members tm ON tm.team_id = tr.team_id
                 JOIN organization_teams t ON t.id = tr.team_id
                 JOIN repositories r ON r.id = tr.repo_id AND r.deleted_at IS NULL
                 WHERE tr.repo_id = $1 AND tm.user_id = $2
                   AND EXISTS (SELECT 1 FROM organization_members m
                               WHERE m.org_id = t.org_id AND m.user_id = tm.user_id)"
            )))
            .bind(repo_id)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("max team permission failed: {e}"))?
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT MAX({TIER}) FROM organization_team_repos tr
                 JOIN organization_team_members tm ON tm.team_id = tr.team_id
                 JOIN organization_teams t ON t.id = tr.team_id
                 JOIN repositories r ON r.id = tr.repo_id AND r.deleted_at IS NULL
                 WHERE tr.repo_id = ? AND tm.user_id = ?
                   AND EXISTS (SELECT 1 FROM organization_members m
                               WHERE m.org_id = t.org_id AND m.user_id = tm.user_id)"
            )))
            .bind(repo_id)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("max team permission failed: {e}"))?
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!(
                "SELECT MAX({TIER}) FROM organization_team_repos tr
                 JOIN organization_team_members tm ON tm.team_id = tr.team_id
                 JOIN organization_teams t ON t.id = tr.team_id
                 JOIN repositories r ON r.id = tr.repo_id AND r.deleted_at IS NULL
                 WHERE tr.repo_id = ?1 AND tm.user_id = ?2
                   AND EXISTS (SELECT 1 FROM organization_members m
                               WHERE m.org_id = t.org_id AND m.user_id = tm.user_id)"
            )))
            .bind(repo_id)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| format!("max team permission failed: {e}"))?
        }
    };
    Ok(match rank {
        3 => Some("admin".into()),
        2 => Some("write".into()),
        1 => Some("read".into()),
        _ => None,
    })
}
