//! Actions runners / runs / jobs / secrets persistence (Phase 19 / D-ACT-13 / D-ACT-17).

use crate::pool::DbPool;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct ActionRunnerRow {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub labels_json: String,
    pub owner_type: Option<String>,
    pub owner_id: Option<String>,
    pub repository_id: Option<String>,
    pub ephemeral: bool,
    pub last_online: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionRunRow {
    pub id: String,
    pub repository_id: String,
    pub workflow_path: String,
    pub workflow_name: String,
    pub event: String,
    pub head_sha: String,
    pub head_ref: String,
    pub status: String,
    pub title: String,
    pub triggered_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ActionJobRow {
    pub id: String,
    pub run_id: String,
    pub job_key: String,
    pub name: String,
    pub runs_on_json: String,
    pub status: String,
    pub runner_id: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionSecretMetaRow {
    pub id: String,
    pub repository_id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionSecretCipherRow {
    pub name: String,
    pub ciphertext: String,
}

pub async fn insert_runner(
    pool: &DbPool,
    id: &str,
    name: &str,
    token_hash: &str,
    labels_json: &str,
    repository_id: Option<&str>,
    ephemeral: bool,
) -> Result<ActionRunnerRow, String> {
    let eph_i = if ephemeral { 1i64 } else { 0 };
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO action_runners
                   (id, name, token_hash, labels_json, repository_id, ephemeral)
                   VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(id).bind(name).bind(token_hash).bind(labels_json).bind(repository_id).bind(ephemeral)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO action_runners
                   (id, name, token_hash, labels_json, repository_id, ephemeral)
                   VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id).bind(name).bind(token_hash).bind(labels_json).bind(repository_id).bind(eph_i as i8)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO action_runners
                   (id, name, token_hash, labels_json, repository_id, ephemeral)
                   VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id).bind(name).bind(token_hash).bind(labels_json).bind(repository_id).bind(eph_i)
            .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(ActionRunnerRow {
        id: id.into(), name: name.into(), token_hash: token_hash.into(),
        labels_json: labels_json.into(), owner_type: None, owner_id: None,
        repository_id: repository_id.map(str::to_string), ephemeral,
        last_online: None, created_at: String::new(), updated_at: String::new(),
    })
}

pub async fn find_runner_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<ActionRunnerRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          to_char(last_online AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_online,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                   FROM action_runners WHERE id = $1"#,
            ).bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"), name: r.get("name"), token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"), owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"), repository_id: r.get("repository_id"),
                ephemeral: r.get("ephemeral"), last_online: r.get("last_online"),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          DATE_FORMAT(last_online, '%Y-%m-%dT%H:%i:%sZ') AS last_online,
                          DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                          DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                   FROM action_runners WHERE id = ?"#,
            ).bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"), name: r.get("name"), token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"), owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"), repository_id: r.get("repository_id"),
                ephemeral: r.try_get::<i8,_>("ephemeral").map(|i| i!=0).unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
            }))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id,
                          ephemeral, last_online,
                          strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                          strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                   FROM action_runners WHERE id = ?"#,
            ).bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"), name: r.get("name"), token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"), owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"), repository_id: r.get("repository_id"),
                ephemeral: r.try_get::<i64,_>("ephemeral").map(|i| i!=0).unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: r.try_get("created_at").unwrap_or_default(),
                updated_at: r.try_get("updated_at").unwrap_or_default(),
            }))
        }
    }
}

pub async fn insert_run(
    pool: &DbPool, id: &str, repository_id: &str, workflow_path: &str, workflow_name: &str,
    event: &str, head_sha: &str, head_ref: &str, title: &str, triggered_by: Option<&str>,
) -> Result<ActionRunRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by)
                VALUES (?,?,?,?,?,?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_runs
                (id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, title, triggered_by)
                VALUES (?,?,?,?,?,?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(workflow_path).bind(workflow_name)
                .bind(event).bind(head_sha).bind(head_ref).bind(title).bind(triggered_by)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(ActionRunRow {
        id: id.into(), repository_id: repository_id.into(), workflow_path: workflow_path.into(),
        workflow_name: workflow_name.into(), event: event.into(), head_sha: head_sha.into(),
        head_ref: head_ref.into(), status: "queued".into(), title: title.into(),
        triggered_by: triggered_by.map(str::to_string), created_at: String::new(),
        updated_at: String::new(), finished_at: None,
    })
}

pub async fn find_run_by_id(pool: &DbPool, id: &str) -> Result<Option<ActionRunRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, finished_at::text AS finished_at FROM action_runs WHERE id = $1")
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunRow {
                id: r.get("id"),
                repository_id: r.get("repository_id"),
                workflow_path: r.get("workflow_path"),
                workflow_name: r.get("workflow_name"),
                event: r.get("event"),
                head_sha: r.get("head_sha"),
                head_ref: r.get("head_ref"),
                status: r.get("status"),
                title: r.get("title"),
                triggered_by: r.get("triggered_by"),
                created_at: String::new(),
                updated_at: String::new(),
                finished_at: r.get("finished_at"),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, CAST(finished_at AS CHAR) AS finished_at FROM action_runs WHERE id = ?")
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunRow {
                id: r.get("id"),
                repository_id: r.get("repository_id"),
                workflow_path: r.get("workflow_path"),
                workflow_name: r.get("workflow_name"),
                event: r.get("event"),
                head_sha: r.get("head_sha"),
                head_ref: r.get("head_ref"),
                status: r.get("status"),
                title: r.get("title"),
                triggered_by: r.get("triggered_by"),
                created_at: String::new(),
                updated_at: String::new(),
                finished_at: r.get("finished_at"),
            }))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, finished_at FROM action_runs WHERE id = ?")
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunRow {
                id: r.get("id"),
                repository_id: r.get("repository_id"),
                workflow_path: r.get("workflow_path"),
                workflow_name: r.get("workflow_name"),
                event: r.get("event"),
                head_sha: r.get("head_sha"),
                head_ref: r.get("head_ref"),
                status: r.get("status"),
                title: r.get("title"),
                triggered_by: r.get("triggered_by"),
                created_at: String::new(),
                updated_at: String::new(),
                finished_at: r.get("finished_at"),
            }))
        }
    }
}

pub async fn insert_job(
    pool: &DbPool, id: &str, run_id: &str, job_key: &str, name: &str, runs_on_json: &str,
) -> Result<ActionJobRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_jobs (id, run_id, job_key, name, runs_on_json) VALUES ($1,$2,$3,$4,$5)"#)
                .bind(id).bind(run_id).bind(job_key).bind(name).bind(runs_on_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_jobs (id, run_id, job_key, name, runs_on_json) VALUES (?,?,?,?,?)"#)
                .bind(id).bind(run_id).bind(job_key).bind(name).bind(runs_on_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_jobs (id, run_id, job_key, name, runs_on_json) VALUES (?,?,?,?,?)"#)
                .bind(id).bind(run_id).bind(job_key).bind(name).bind(runs_on_json)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(ActionJobRow {
        id: id.into(), run_id: run_id.into(), job_key: job_key.into(), name: name.into(),
        runs_on_json: runs_on_json.into(), status: "queued".into(), runner_id: None,
        started_at: None, finished_at: None, created_at: String::new(), updated_at: String::new(),
    })
}

pub async fn find_job_by_id(pool: &DbPool, id: &str) -> Result<Option<ActionJobRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT id, run_id, job_key, name, runs_on_json, status, runner_id, started_at::text AS started_at, finished_at::text AS finished_at FROM action_jobs WHERE id = $1")
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionJobRow {
                id: r.get("id"),
                run_id: r.get("run_id"),
                job_key: r.get("job_key"),
                name: r.get("name"),
                runs_on_json: r.get("runs_on_json"),
                status: r.get("status"),
                runner_id: r.get("runner_id"),
                started_at: r.get("started_at"),
                finished_at: r.get("finished_at"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT id, run_id, job_key, name, runs_on_json, status, runner_id, CAST(started_at AS CHAR) AS started_at, CAST(finished_at AS CHAR) AS finished_at FROM action_jobs WHERE id = ?")
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionJobRow {
                id: r.get("id"),
                run_id: r.get("run_id"),
                job_key: r.get("job_key"),
                name: r.get("name"),
                runs_on_json: r.get("runs_on_json"),
                status: r.get("status"),
                runner_id: r.get("runner_id"),
                started_at: r.get("started_at"),
                finished_at: r.get("finished_at"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT id, run_id, job_key, name, runs_on_json, status, runner_id, started_at, finished_at FROM action_jobs WHERE id = ?")
                .bind(id).fetch_optional(p).await.map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionJobRow {
                id: r.get("id"),
                run_id: r.get("run_id"),
                job_key: r.get("job_key"),
                name: r.get("name"),
                runs_on_json: r.get("runs_on_json"),
                status: r.get("status"),
                runner_id: r.get("runner_id"),
                started_at: r.get("started_at"),
                finished_at: r.get("finished_at"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
    }
}

pub async fn insert_secret(
    pool: &DbPool, id: &str, repository_id: &str, name: &str, ciphertext: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_secrets (id, repository_id, name, ciphertext) VALUES ($1,$2,$3,$4)"#)
                .bind(id).bind(repository_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_secrets (id, repository_id, name, ciphertext) VALUES (?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_secrets (id, repository_id, name, ciphertext) VALUES (?,?,?,?)"#)
                .bind(id).bind(repository_id).bind(name).bind(ciphertext)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_secret_ciphertexts(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<ActionSecretCipherRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT name, ciphertext FROM action_secrets WHERE repository_id = $1 ORDER BY name",
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretCipherRow {
                    name: row.get("name"),
                    ciphertext: row.get("ciphertext"),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT name, ciphertext FROM action_secrets WHERE repository_id = ? ORDER BY name",
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretCipherRow {
                    name: row.get("name"),
                    ciphertext: row.get("ciphertext"),
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT name, ciphertext FROM action_secrets WHERE repository_id = ? ORDER BY name",
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|row| ActionSecretCipherRow {
                    name: row.get("name"),
                    ciphertext: row.get("ciphertext"),
                })
                .collect())
        }
    }
}

pub async fn list_secret_names(
    pool: &DbPool, repository_id: &str,
) -> Result<Vec<ActionSecretMetaRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, name,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                   FROM action_secrets WHERE repository_id = $1 ORDER BY name"#,
            ).bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|row| ActionSecretMetaRow {
                id: row.get("id"), repository_id: row.get("repository_id"), name: row.get("name"),
                created_at: row.try_get("created_at").unwrap_or_default(),
                updated_at: row.try_get("updated_at").unwrap_or_default(),
            }).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, name,
                          DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                          DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                   FROM action_secrets WHERE repository_id = ? ORDER BY name"#,
            ).bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|row| ActionSecretMetaRow {
                id: row.get("id"), repository_id: row.get("repository_id"), name: row.get("name"),
                created_at: row.try_get("created_at").unwrap_or_default(),
                updated_at: row.try_get("updated_at").unwrap_or_default(),
            }).collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, repository_id, name,
                          strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                          strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                   FROM action_secrets WHERE repository_id = ? ORDER BY name"#,
            ).bind(repository_id).fetch_all(p).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(|row| ActionSecretMetaRow {
                id: row.get("id"), repository_id: row.get("repository_id"), name: row.get("name"),
                created_at: row.try_get("created_at").unwrap_or_default(),
                updated_at: row.try_get("updated_at").unwrap_or_default(),
            }).collect())
        }
    }
}

pub async fn delete_secret_by_name(
    pool: &DbPool,
    repository_id: &str,
    name: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let r = sqlx::query("DELETE FROM action_secrets WHERE repository_id = $1 AND name = $2")
                .bind(repository_id)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(r.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let r = sqlx::query("DELETE FROM action_secrets WHERE repository_id = ? AND name = ?")
                .bind(repository_id)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(r.rows_affected() > 0)
        }
        DbPool::Sqlite(p) => {
            let r = sqlx::query("DELETE FROM action_secrets WHERE repository_id = ? AND name = ?")
                .bind(repository_id)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(r.rows_affected() > 0)
        }
    }
}

pub async fn list_runners(pool: &DbPool) -> Result<Vec<ActionRunnerRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          to_char(last_online AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_online,
                          to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                          to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
                   FROM action_runners ORDER BY name"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|r| ActionRunnerRow {
                    id: r.get("id"),
                    name: r.get("name"),
                    token_hash: r.get("token_hash"),
                    labels_json: r.get("labels_json"),
                    owner_type: r.try_get("owner_type").ok(),
                    owner_id: r.try_get("owner_id").ok(),
                    repository_id: r.try_get("repository_id").ok(),
                    ephemeral: r.try_get::<bool, _>("ephemeral").unwrap_or(false),
                    last_online: r.try_get("last_online").ok(),
                    created_at: r.try_get("created_at").unwrap_or_default(),
                    updated_at: r.try_get("updated_at").unwrap_or_default(),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          DATE_FORMAT(last_online, '%Y-%m-%dT%H:%i:%sZ') AS last_online,
                          DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                          DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
                   FROM action_runners ORDER BY name"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|r| {
                    let eph: i8 = r.try_get("ephemeral").unwrap_or(0);
                    ActionRunnerRow {
                        id: r.get("id"),
                        name: r.get("name"),
                        token_hash: r.get("token_hash"),
                        labels_json: r.get("labels_json"),
                        owner_type: r.try_get("owner_type").ok(),
                        owner_id: r.try_get("owner_id").ok(),
                        repository_id: r.try_get("repository_id").ok(),
                        ephemeral: eph != 0,
                        last_online: r.try_get("last_online").ok(),
                        created_at: r.try_get("created_at").unwrap_or_default(),
                        updated_at: r.try_get("updated_at").unwrap_or_default(),
                    }
                })
                .collect())
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral,
                          strftime('%Y-%m-%dT%H:%M:%SZ', last_online) AS last_online,
                          strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
                          strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
                   FROM action_runners ORDER BY name"#,
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .iter()
                .map(|r| {
                    let eph: i64 = r.try_get("ephemeral").unwrap_or(0);
                    ActionRunnerRow {
                        id: r.get("id"),
                        name: r.get("name"),
                        token_hash: r.get("token_hash"),
                        labels_json: r.get("labels_json"),
                        owner_type: r.try_get("owner_type").ok(),
                        owner_id: r.try_get("owner_id").ok(),
                        repository_id: r.try_get("repository_id").ok(),
                        ephemeral: eph != 0,
                        last_online: r.try_get("last_online").ok(),
                        created_at: r.try_get("created_at").unwrap_or_default(),
                        updated_at: r.try_get("updated_at").unwrap_or_default(),
                    }
                })
                .collect())
        }
    }
}

pub async fn insert_runner_token(
    pool: &DbPool, id: &str, token_hash: &str, scope_type: &str, scope_id: Option<&str>, active: bool,
) -> Result<(), String> {
    let active_i = if active { 1i64 } else { 0 };
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(r#"INSERT INTO action_runner_tokens (id, token_hash, scope_type, scope_id, active) VALUES ($1,$2,$3,$4,$5)"#)
                .bind(id).bind(token_hash).bind(scope_type).bind(scope_id).bind(active)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(r#"INSERT INTO action_runner_tokens (id, token_hash, scope_type, scope_id, active) VALUES (?,?,?,?,?)"#)
                .bind(id).bind(token_hash).bind(scope_type).bind(scope_id).bind(active_i as i8)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(r#"INSERT INTO action_runner_tokens (id, token_hash, scope_type, scope_id, active) VALUES (?,?,?,?,?)"#)
                .bind(id).bind(token_hash).bind(scope_type).bind(scope_id).bind(active_i)
                .execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn wipe_actions_domain(pool: &DbPool) -> Result<(), String> {
    for sql in [
        "DELETE FROM action_jobs",
        "DELETE FROM action_runs",
        "DELETE FROM action_secrets",
        "DELETE FROM action_runners",
        "DELETE FROM action_runner_tokens",
    ] {
        match pool {
            DbPool::Postgres(p) => { sqlx::query(sql).execute(p).await.map_err(|e| e.to_string())?; }
            DbPool::MySql(p) => { sqlx::query(sql).execute(p).await.map_err(|e| e.to_string())?; }
            DbPool::Sqlite(p) => { sqlx::query(sql).execute(p).await.map_err(|e| e.to_string())?; }
        }
    }
    Ok(())
}

pub async fn get_actions_enabled(pool: &DbPool, repo_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query("SELECT actions_enabled FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| r.try_get::<i64, _>("actions_enabled").unwrap_or(1) != 0)
                .unwrap_or(true))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query("SELECT actions_enabled FROM repositories WHERE id = $1")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| r.try_get::<bool, _>("actions_enabled").unwrap_or(true))
                .unwrap_or(true))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query("SELECT actions_enabled FROM repositories WHERE id = ?")
                .bind(repo_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| r.try_get::<i8, _>("actions_enabled").unwrap_or(1) != 0)
                .unwrap_or(true))
        }
    }
}

pub async fn set_actions_enabled(pool: &DbPool, repo_id: &str, enabled: bool) -> Result<(), String> {
    let v = if enabled { 1i64 } else { 0 };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET actions_enabled = ? WHERE id = ?")
                .bind(v)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET actions_enabled = $1 WHERE id = $2")
                .bind(enabled)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET actions_enabled = ? WHERE id = ?")
                .bind(v as i8)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn consume_registration_token(pool: &DbPool, token_hash: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_runner_tokens SET active = 0 WHERE token_hash = ? AND active = 1",
            )
            .bind(token_hash)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_runner_tokens SET active = false WHERE token_hash = $1 AND active = true",
            )
            .bind(token_hash)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_runner_tokens SET active = 0 WHERE token_hash = ? AND active = 1",
            )
            .bind(token_hash)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
    }
}

pub async fn find_runner_by_token_hash(
    pool: &DbPool,
    token_hash: &str,
) -> Result<Option<ActionRunnerRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral, strftime('%Y-%m-%dT%H:%M:%SZ', last_online) AS last_online FROM action_runners WHERE token_hash = ?",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r.try_get::<i64, _>("ephemeral").map(|i| i != 0).unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral, to_char(last_online AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS last_online FROM action_runners WHERE token_hash = $1",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r.get("ephemeral"),
                last_online: r.get("last_online"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT id, name, token_hash, labels_json, owner_type, owner_id, repository_id, ephemeral, DATE_FORMAT(last_online, '%Y-%m-%dT%H:%i:%sZ') AS last_online FROM action_runners WHERE token_hash = ?",
            )
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| ActionRunnerRow {
                id: r.get("id"),
                name: r.get("name"),
                token_hash: r.get("token_hash"),
                labels_json: r.get("labels_json"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                repository_id: r.get("repository_id"),
                ephemeral: r.try_get::<i8, _>("ephemeral").map(|i| i != 0).unwrap_or(false),
                last_online: r.get("last_online"),
                created_at: String::new(),
                updated_at: String::new(),
            }))
        }
    }
}

fn labels_match(job_runs_on: &[String], runner_labels: &[String]) -> bool {
    job_runs_on.iter().all(|need| {
        let base = need.split(':').next().unwrap_or(need);
        runner_labels.iter().any(|have| {
            let have_base = have.split(':').next().unwrap_or(have);
            have_base == base || have == need
        })
    })
}

pub async fn claim_queued_job_for_labels(
    pool: &DbPool,
    runner_id: &str,
    runner_labels: &[String],
) -> Result<Option<ActionJobRow>, String> {
    // Fetch queued jobs then claim first match (sqlite-friendly tracer).
    let queued = list_queued_jobs(pool).await?;
    for job in queued {
        let runs_on: Vec<String> = serde_json::from_str(&job.runs_on_json).unwrap_or_default();
        if !labels_match(&runs_on, runner_labels) {
            continue;
        }
        if assign_job_runner(pool, &job.id, runner_id).await? {
            return find_job_by_id(pool, &job.id).await;
        }
    }
    Ok(None)
}

async fn list_queued_jobs(pool: &DbPool) -> Result<Vec<ActionJobRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, name, runs_on_json, status, runner_id FROM action_jobs WHERE status = 'queued' ORDER BY created_at ASC",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionJobRow {
                    id: r.get("id"),
                    run_id: r.get("run_id"),
                    job_key: r.get("job_key"),
                    name: r.get("name"),
                    runs_on_json: r.get("runs_on_json"),
                    status: r.get("status"),
                    runner_id: r.get("runner_id"),
                    started_at: None,
                    finished_at: None,
                    created_at: String::new(),
                    updated_at: String::new(),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, name, runs_on_json, status, runner_id FROM action_jobs WHERE status = 'queued' ORDER BY created_at ASC",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionJobRow {
                    id: r.get("id"),
                    run_id: r.get("run_id"),
                    job_key: r.get("job_key"),
                    name: r.get("name"),
                    runs_on_json: r.get("runs_on_json"),
                    status: r.get("status"),
                    runner_id: r.get("runner_id"),
                    started_at: None,
                    finished_at: None,
                    created_at: String::new(),
                    updated_at: String::new(),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, name, runs_on_json, status, runner_id FROM action_jobs WHERE status = 'queued' ORDER BY created_at ASC",
            )
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionJobRow {
                    id: r.get("id"),
                    run_id: r.get("run_id"),
                    job_key: r.get("job_key"),
                    name: r.get("name"),
                    runs_on_json: r.get("runs_on_json"),
                    status: r.get("status"),
                    runner_id: r.get("runner_id"),
                    started_at: None,
                    finished_at: None,
                    created_at: String::new(),
                    updated_at: String::new(),
                })
                .collect())
        }
    }
}

async fn assign_job_runner(pool: &DbPool, job_id: &str, runner_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'queued'",
            )
            .bind(runner_id)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = $1, started_at = COALESCE(started_at, now()), updated_at = now() WHERE id = $2 AND status = 'queued'",
            )
            .bind(runner_id)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_jobs SET status = 'in_progress', runner_id = ?, started_at = COALESCE(started_at, CURRENT_TIMESTAMP), updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status = 'queued'",
            )
            .bind(runner_id)
            .bind(job_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(res.rows_affected() > 0)
        }
    }
}

pub async fn list_runs_for_repo(
    pool: &DbPool,
    repository_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ActionRunRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, created_at, updated_at, finished_at FROM action_runs WHERE repository_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(repository_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionRunRow {
                    id: r.get("id"),
                    repository_id: r.get("repository_id"),
                    workflow_path: r.get("workflow_path"),
                    workflow_name: r.get("workflow_name"),
                    event: r.get("event"),
                    head_sha: r.get("head_sha"),
                    head_ref: r.get("head_ref"),
                    status: r.get("status"),
                    title: r.get("title"),
                    triggered_by: r.get("triggered_by"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    finished_at: r.get("finished_at"),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, created_at::text AS created_at, updated_at::text AS updated_at, finished_at::text AS finished_at FROM action_runs WHERE repository_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(repository_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionRunRow {
                    id: r.get("id"),
                    repository_id: r.get("repository_id"),
                    workflow_path: r.get("workflow_path"),
                    workflow_name: r.get("workflow_name"),
                    event: r.get("event"),
                    head_sha: r.get("head_sha"),
                    head_ref: r.get("head_ref"),
                    status: r.get("status"),
                    title: r.get("title"),
                    triggered_by: r.get("triggered_by"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    finished_at: r.get("finished_at"),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT id, repository_id, workflow_path, workflow_name, event, head_sha, head_ref, status, title, triggered_by, CAST(created_at AS CHAR) AS created_at, CAST(updated_at AS CHAR) AS updated_at, CAST(finished_at AS CHAR) AS finished_at FROM action_runs WHERE repository_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(repository_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionRunRow {
                    id: r.get("id"),
                    repository_id: r.get("repository_id"),
                    workflow_path: r.get("workflow_path"),
                    workflow_name: r.get("workflow_name"),
                    event: r.get("event"),
                    head_sha: r.get("head_sha"),
                    head_ref: r.get("head_ref"),
                    status: r.get("status"),
                    title: r.get("title"),
                    triggered_by: r.get("triggered_by"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    finished_at: r.get("finished_at"),
                })
                .collect())
        }
    }
}

pub async fn list_jobs_for_run(pool: &DbPool, run_id: &str) -> Result<Vec<ActionJobRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, name, runs_on_json, status, runner_id, started_at, finished_at FROM action_jobs WHERE run_id = ?",
            )
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionJobRow {
                    id: r.get("id"),
                    run_id: r.get("run_id"),
                    job_key: r.get("job_key"),
                    name: r.get("name"),
                    runs_on_json: r.get("runs_on_json"),
                    status: r.get("status"),
                    runner_id: r.get("runner_id"),
                    started_at: r.get("started_at"),
                    finished_at: r.get("finished_at"),
                    created_at: String::new(),
                    updated_at: String::new(),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, name, runs_on_json, status, runner_id, started_at::text AS started_at, finished_at::text AS finished_at FROM action_jobs WHERE run_id = $1",
            )
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionJobRow {
                    id: r.get("id"),
                    run_id: r.get("run_id"),
                    job_key: r.get("job_key"),
                    name: r.get("name"),
                    runs_on_json: r.get("runs_on_json"),
                    status: r.get("status"),
                    runner_id: r.get("runner_id"),
                    started_at: r.get("started_at"),
                    finished_at: r.get("finished_at"),
                    created_at: String::new(),
                    updated_at: String::new(),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT id, run_id, job_key, name, runs_on_json, status, runner_id, CAST(started_at AS CHAR) AS started_at, CAST(finished_at AS CHAR) AS finished_at FROM action_jobs WHERE run_id = ?",
            )
            .bind(run_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| ActionJobRow {
                    id: r.get("id"),
                    run_id: r.get("run_id"),
                    job_key: r.get("job_key"),
                    name: r.get("name"),
                    runs_on_json: r.get("runs_on_json"),
                    status: r.get("status"),
                    runner_id: r.get("runner_id"),
                    started_at: r.get("started_at"),
                    finished_at: r.get("finished_at"),
                    created_at: String::new(),
                    updated_at: String::new(),
                })
                .collect())
        }
    }
}


pub async fn update_job_status(
    pool: &DbPool,
    job_id: &str,
    status: &str,
) -> Result<(), String> {
    let in_progress = status == "in_progress";
    let terminal = matches!(status, "success" | "failure" | "cancelled");
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_jobs SET status = ?, started_at = CASE WHEN ? AND started_at IS NULL THEN CURRENT_TIMESTAMP ELSE started_at END, finished_at = CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE finished_at END, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_jobs SET status = $1, started_at = CASE WHEN $2 AND started_at IS NULL THEN now() ELSE started_at END, finished_at = CASE WHEN $3 THEN now() ELSE finished_at END, updated_at = now() WHERE id = $4")
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_jobs SET status = ?, started_at = CASE WHEN ? AND started_at IS NULL THEN CURRENT_TIMESTAMP ELSE started_at END, finished_at = CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE finished_at END, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(status).bind(in_progress).bind(terminal).bind(job_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
/// Recompute a run's status from its jobs.
///
/// - any job `in_progress` → run `in_progress`
/// - all jobs terminal → `failure` if any job failed, else `cancelled` if
///   any job cancelled, else `success`
/// - otherwise → keep current status (jobs still queued/claimed)
/// `finished_at` is set exactly when the run first reaches a terminal status.
pub async fn recompute_run_status(pool: &DbPool, run_id: &str) -> Result<(), String> {
    const ROLLUP: &str = "UPDATE action_runs SET \
        status = CASE \
          WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'in_progress') THEN 'in_progress' \
          WHEN NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled')) THEN \
            CASE \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'failure') THEN 'failure' \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'cancelled') THEN 'cancelled' \
              ELSE 'success' \
            END \
          ELSE status END, \
        finished_at = CASE \
          WHEN finished_at IS NULL AND NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled')) THEN CURRENT_TIMESTAMP \
          ELSE finished_at END, \
        updated_at = CURRENT_TIMESTAMP \
        WHERE id = ?";
    const ROLLUP_PG: &str = "UPDATE action_runs SET \
        status = CASE \
          WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'in_progress') THEN 'in_progress' \
          WHEN NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled')) THEN \
            CASE \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'failure') THEN 'failure' \
              WHEN EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status = 'cancelled') THEN 'cancelled' \
              ELSE 'success' \
            END \
          ELSE status END, \
        finished_at = CASE \
          WHEN finished_at IS NULL AND NOT EXISTS (SELECT 1 FROM action_jobs j WHERE j.run_id = action_runs.id AND j.status NOT IN ('success','failure','cancelled')) THEN now() \
          ELSE finished_at END, \
        updated_at = now() \
        WHERE id = $1";
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(ROLLUP).bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(ROLLUP_PG).bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(ROLLUP).bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Runner liveness heartbeat — bumps `last_online`/`updated_at`. Best-effort;
/// callers should log, not fail, on error.
pub async fn touch_runner_online(pool: &DbPool, runner_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_runners SET last_online = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_runners SET last_online = now(), updated_at = now() WHERE id = $1")
                .bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_runners SET last_online = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}


pub async fn update_runner_labels(
    pool: &DbPool,
    runner_id: &str,
    labels_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_runners SET labels_json = ? WHERE id = ?")
                .bind(labels_json).bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_runners SET labels_json = $1 WHERE id = $2")
                .bind(labels_json).bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_runners SET labels_json = ? WHERE id = ?")
                .bind(labels_json).bind(runner_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Total run count for a repository (pagination companion to [`list_runs_for_repo`]).
pub async fn count_runs_for_repo(pool: &DbPool, repository_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM action_runs WHERE repository_id = ?",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(n)
        }
        DbPool::Postgres(p) => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM action_runs WHERE repository_id = $1",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(n)
        }
        DbPool::MySql(p) => {
            let n: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM action_runs WHERE repository_id = ?",
            )
            .bind(repository_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(n)
        }
    }
}

/// Requeue a run for re-run: reset run + all jobs to queued and clear
/// runner/timing fields so a runner can claim the jobs again.
pub async fn requeue_run(pool: &DbPool, run_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE action_jobs SET status = 'queued', runner_id = NULL, started_at = NULL, finished_at = NULL, updated_at = CURRENT_TIMESTAMP WHERE run_id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = false, updated_at = now() WHERE id = $1",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE action_jobs SET status = 'queued', runner_id = NULL, started_at = NULL, finished_at = NULL, updated_at = now() WHERE run_id = $1",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE action_runs SET status = 'queued', finished_at = NULL, completion_notified = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE action_jobs SET status = 'queued', runner_id = NULL, started_at = NULL, finished_at = NULL, updated_at = CURRENT_TIMESTAMP WHERE run_id = ?",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Atomically claim the right to emit the "run completed" in-app notification
/// for a finished run (DEBT-06). `finished_at` is set exactly once per run
/// lifetime (requeue clears it), so a successful claim means this caller is the
/// single emitter for that completion.
pub async fn claim_run_completion_notice(pool: &DbPool, run_id: &str) -> Result<bool, String> {
    let n = match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_runs SET completion_notified = 1
             WHERE id = ? AND finished_at IS NOT NULL AND completion_notified = 0",
        )
        .bind(run_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_runs SET completion_notified = true
             WHERE id = $1 AND finished_at IS NOT NULL AND completion_notified = false",
        )
        .bind(run_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_runs SET completion_notified = 1
             WHERE id = ? AND finished_at IS NOT NULL AND completion_notified = 0",
        )
        .bind(run_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    Ok(n > 0)
}

/// Cancel a run and any unfinished jobs (queued/in_progress). Finished jobs
/// keep their conclusion, matching per-job cancellation semantics.
/// Returns `true` when the run was actually cancelled — `false` when it had
/// already finished (`finished_at` guard keeps cancel from clobbering a
/// terminal status, DEBT-06 review).
pub async fn cancel_run(pool: &DbPool, run_id: &str) -> Result<bool, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let res = sqlx::query(
                "UPDATE action_runs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND finished_at IS NULL",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                return Ok(false);
            }
            sqlx::query(
                "UPDATE action_jobs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued', 'in_progress')",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            let res = sqlx::query(
                "UPDATE action_runs SET status = 'cancelled', finished_at = now(), updated_at = now() WHERE id = $1 AND finished_at IS NULL",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                return Ok(false);
            }
            sqlx::query(
                "UPDATE action_jobs SET status = 'cancelled', finished_at = now(), updated_at = now() WHERE run_id = $1 AND status IN ('queued', 'in_progress')",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            let res = sqlx::query(
                "UPDATE action_runs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND finished_at IS NULL",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
            if res.rows_affected() == 0 {
                return Ok(false);
            }
            sqlx::query(
                "UPDATE action_jobs SET status = 'cancelled', finished_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE run_id = ? AND status IN ('queued', 'in_progress')",
            )
            .bind(run_id).execute(p).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(true)
}
