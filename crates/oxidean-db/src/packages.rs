//! Package metadata / blob refcount persistence (Phase 20).

use crate::pool::DbPool;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct PackageRow {
    pub id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub name: String,
    pub format: String,
    pub visibility: String,
    pub repository_id: Option<String>,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct PackageVersionRow {
    pub id: String,
    pub package_id: String,
    pub version: String,
    pub digest: Option<String>,
    pub metadata_json: String,
    pub published_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct PackageBlobRow {
    pub digest: String,
    pub size_bytes: i64,
    pub refcount: i64,
    pub created_at: String,
}

fn map_package_sqlite(row: &sqlx::sqlite::SqliteRow) -> PackageRow {
    PackageRow {
        id: row.get("id"),
        owner_type: row.get("owner_type"),
        owner_id: row.get("owner_id"),
        name: row.get("name"),
        format: row.get("format"),
        visibility: row.get("visibility"),
        repository_id: row.get("repository_id"),
        description: row.get("description"),
        created_at: row.get::<String, _>("created_at"),
        updated_at: row.get::<String, _>("updated_at"),
    }
}

/// Dialect-agnostic helpers via raw SQL with `?` / `$1` branching — keep simple inserts.
pub async fn upsert_blob(pool: &DbPool, digest: &str, size_bytes: i64) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO package_blobs (digest, size_bytes, refcount)
                   VALUES ($1, $2, 0)
                   ON CONFLICT (digest) DO NOTHING"#,
            )
            .bind(digest)
            .bind(size_bytes)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT IGNORE INTO package_blobs (digest, size_bytes, refcount)
                   VALUES (?, ?, 0)"#,
            )
            .bind(digest)
            .bind(size_bytes)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT OR IGNORE INTO package_blobs (digest, size_bytes, refcount)
                   VALUES (?, ?, 0)"#,
            )
            .bind(digest)
            .bind(size_bytes)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn adjust_blob_refcount(pool: &DbPool, digest: &str, delta: i64) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"UPDATE package_blobs SET refcount = refcount + $2
                   WHERE digest = $1 RETURNING refcount"#,
            )
            .bind(digest)
            .bind(delta)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            row.map(|r| i64::from(r.get::<i32, _>("refcount")))
                .ok_or_else(|| format!("blob not found: {digest}"))
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE package_blobs SET refcount = refcount + ? WHERE digest = ?")
                .bind(delta)
                .bind(digest)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            let v: Option<i64> = sqlx::query_scalar::<sqlx::MySql, i32>(
                "SELECT refcount FROM package_blobs WHERE digest = ?",
            )
            .bind(digest)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?
            .map(i64::from);
            v.ok_or_else(|| format!("blob not found: {digest}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE package_blobs SET refcount = refcount + ? WHERE digest = ?")
                .bind(delta)
                .bind(digest)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            let v: Option<i64> =
                sqlx::query_scalar("SELECT refcount FROM package_blobs WHERE digest = ?")
                    .bind(digest)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            v.ok_or_else(|| format!("blob not found: {digest}"))
        }
    }
}

pub async fn find_package(
    pool: &DbPool,
    owner_type: &str,
    owner_id: &str,
    name: &str,
    format: &str,
) -> Result<Option<PackageRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description, created_at, updated_at
                   FROM packages
                   WHERE owner_type = ? AND owner_id = ? AND name = ? AND format = ?"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .bind(name)
            .bind(format)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_package_sqlite(&r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          created_at::text AS created_at, updated_at::text AS updated_at
                   FROM packages
                   WHERE owner_type = $1 AND owner_id = $2 AND name = $3 AND format = $4"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .bind(name)
            .bind(format)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageRow {
                id: r.get("id"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                name: r.get("name"),
                format: r.get("format"),
                visibility: r.get("visibility"),
                repository_id: r.get("repository_id"),
                description: r.get("description"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          CAST(created_at AS CHAR) AS created_at,
                          CAST(updated_at AS CHAR) AS updated_at
                   FROM packages
                   WHERE owner_type = ? AND owner_id = ? AND name = ? AND format = ?"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .bind(name)
            .bind(format)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageRow {
                id: r.get("id"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                name: r.get("name"),
                format: r.get("format"),
                visibility: r.get("visibility"),
                repository_id: r.get("repository_id"),
                description: r.get("description"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_package(
    pool: &DbPool,
    id: &str,
    owner_type: &str,
    owner_id: &str,
    name: &str,
    format: &str,
    visibility: &str,
    repository_id: Option<&str>,
    description: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO packages
                   (id, owner_type, owner_id, name, format, visibility, repository_id, description)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(owner_type)
            .bind(owner_id)
            .bind(name)
            .bind(format)
            .bind(visibility)
            .bind(repository_id)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO packages
                   (id, owner_type, owner_id, name, format, visibility, repository_id, description)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
            )
            .bind(id)
            .bind(owner_type)
            .bind(owner_id)
            .bind(name)
            .bind(format)
            .bind(visibility)
            .bind(repository_id)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO packages
                   (id, owner_type, owner_id, name, format, visibility, repository_id, description)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(owner_type)
            .bind(owner_id)
            .bind(name)
            .bind(format)
            .bind(visibility)
            .bind(repository_id)
            .bind(description)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn find_version(
    pool: &DbPool,
    package_id: &str,
    version: &str,
) -> Result<Option<PackageVersionRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, package_id, version, digest, metadata_json, published_by, created_at
                   FROM package_versions WHERE package_id = ? AND version = ?"#,
            )
            .bind(package_id)
            .bind(version)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageVersionRow {
                id: r.get("id"),
                package_id: r.get("package_id"),
                version: r.get("version"),
                digest: r.get("digest"),
                metadata_json: r.get("metadata_json"),
                published_by: r.get("published_by"),
                created_at: r.get("created_at"),
            }))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, package_id, version, digest, metadata_json, published_by,
                          created_at::text AS created_at
                   FROM package_versions WHERE package_id = $1 AND version = $2"#,
            )
            .bind(package_id)
            .bind(version)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageVersionRow {
                id: r.get("id"),
                package_id: r.get("package_id"),
                version: r.get("version"),
                digest: r.get("digest"),
                metadata_json: r.get("metadata_json"),
                published_by: r.get("published_by"),
                created_at: r.get("created_at"),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, package_id, version, digest, metadata_json, published_by,
                          CAST(created_at AS CHAR) AS created_at
                   FROM package_versions WHERE package_id = ? AND version = ?"#,
            )
            .bind(package_id)
            .bind(version)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageVersionRow {
                id: r.get("id"),
                package_id: r.get("package_id"),
                version: r.get("version"),
                digest: r.get("digest"),
                metadata_json: r.get("metadata_json"),
                published_by: r.get("published_by"),
                created_at: r.get("created_at"),
            }))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_version(
    pool: &DbPool,
    id: &str,
    package_id: &str,
    version: &str,
    digest: Option<&str>,
    metadata_json: &str,
    published_by: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO package_versions
                   (id, package_id, version, digest, metadata_json, published_by)
                   VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(package_id)
            .bind(version)
            .bind(digest)
            .bind(metadata_json)
            .bind(published_by)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO package_versions
                   (id, package_id, version, digest, metadata_json, published_by)
                   VALUES (?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(package_id)
            .bind(version)
            .bind(digest)
            .bind(metadata_json)
            .bind(published_by)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO package_versions
                   (id, package_id, version, digest, metadata_json, published_by)
                   VALUES ($1,$2,$3,$4,$5,$6)"#,
            )
            .bind(id)
            .bind(package_id)
            .bind(version)
            .bind(digest)
            .bind(metadata_json)
            .bind(published_by)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn add_blob_ref(
    pool: &DbPool,
    version_id: &str,
    digest: &str,
    role: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO package_blob_refs (package_version_id, blob_digest, role)
                   VALUES (?, ?, ?)"#,
            )
            .bind(version_id)
            .bind(digest)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO package_blob_refs (package_version_id, blob_digest, role)
                   VALUES (?, ?, ?)"#,
            )
            .bind(version_id)
            .bind(digest)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO package_blob_refs (package_version_id, blob_digest, role)
                   VALUES ($1,$2,$3)"#,
            )
            .bind(version_id)
            .bind(digest)
            .bind(role)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_version_blob_digests(
    pool: &DbPool,
    version_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query("SELECT blob_digest FROM package_blob_refs WHERE package_version_id = ?")
                .bind(version_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.get("blob_digest")).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query("SELECT blob_digest FROM package_blob_refs WHERE package_version_id = ?")
                .bind(version_id)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.get("blob_digest")).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT blob_digest FROM package_blob_refs WHERE package_version_id = $1",
            )
            .bind(version_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.get("blob_digest")).collect())
        }
    }
}

pub async fn update_version_metadata(
    pool: &DbPool,
    version_id: &str,
    metadata_json: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE package_versions SET metadata_json = ? WHERE id = ?")
                .bind(metadata_json)
                .bind(version_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE package_versions SET metadata_json = ? WHERE id = ?")
                .bind(metadata_json)
                .bind(version_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE package_versions SET metadata_json = $1 WHERE id = $2")
                .bind(metadata_json)
                .bind(version_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn delete_version(pool: &DbPool, version_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM package_versions WHERE id = ?")
                .bind(version_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM package_versions WHERE id = ?")
                .bind(version_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM package_versions WHERE id = $1")
                .bind(version_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Delete a package row; versions + blob refs cascade (FK ON DELETE CASCADE).
pub async fn delete_package(pool: &DbPool, package_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM packages WHERE id = ?")
                .bind(package_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM packages WHERE id = ?")
                .bind(package_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM packages WHERE id = $1")
                .bind(package_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn list_versions_for_package(
    pool: &DbPool,
    package_id: &str,
) -> Result<Vec<PackageVersionRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, package_id, version, digest, metadata_json, published_by, created_at
                   FROM package_versions WHERE package_id = ? ORDER BY created_at"#,
            )
            .bind(package_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageVersionRow {
                    id: r.get("id"),
                    package_id: r.get("package_id"),
                    version: r.get("version"),
                    digest: r.get("digest"),
                    metadata_json: r.get("metadata_json"),
                    published_by: r.get("published_by"),
                    created_at: r.get("created_at"),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, package_id, version, digest, metadata_json, published_by,
                          created_at::text AS created_at
                   FROM package_versions WHERE package_id = $1 ORDER BY created_at"#,
            )
            .bind(package_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageVersionRow {
                    id: r.get("id"),
                    package_id: r.get("package_id"),
                    version: r.get("version"),
                    digest: r.get("digest"),
                    metadata_json: r.get("metadata_json"),
                    published_by: r.get("published_by"),
                    created_at: r.get("created_at"),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, package_id, version, digest, metadata_json, published_by,
                          CAST(created_at AS CHAR) AS created_at
                   FROM package_versions WHERE package_id = ? ORDER BY created_at"#,
            )
            .bind(package_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageVersionRow {
                    id: r.get("id"),
                    package_id: r.get("package_id"),
                    version: r.get("version"),
                    digest: r.get("digest"),
                    metadata_json: r.get("metadata_json"),
                    published_by: r.get("published_by"),
                    created_at: r.get("created_at"),
                })
                .collect())
        }
    }
}



pub async fn find_package_by_id(pool: &DbPool, id: &str) -> Result<Option<PackageRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description, created_at, updated_at
                   FROM packages WHERE id = ?"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| map_package_sqlite(&r)))
        }
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          created_at::text AS created_at, updated_at::text AS updated_at
                   FROM packages WHERE id = $1"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageRow {
                id: r.get("id"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                name: r.get("name"),
                format: r.get("format"),
                visibility: r.get("visibility"),
                repository_id: r.get("repository_id"),
                description: r.get("description"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }))
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          CAST(created_at AS CHAR) AS created_at,
                          CAST(updated_at AS CHAR) AS updated_at
                   FROM packages WHERE id = ?"#,
            )
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(row.map(|r| PackageRow {
                id: r.get("id"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                name: r.get("name"),
                format: r.get("format"),
                visibility: r.get("visibility"),
                repository_id: r.get("repository_id"),
                description: r.get("description"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }))
        }
    }
}

pub async fn list_packages_by_repository(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<PackageRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description, created_at, updated_at
                   FROM packages WHERE repository_id = ?
                   AND EXISTS (SELECT 1 FROM package_versions pv WHERE pv.package_id = packages.id)
                   ORDER BY name"#,
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_package_sqlite(&r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          created_at::text AS created_at, updated_at::text AS updated_at
                   FROM packages WHERE repository_id = $1
                   AND EXISTS (SELECT 1 FROM package_versions pv WHERE pv.package_id = packages.id)
                   ORDER BY name"#,
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| PackageRow {
                id: r.get("id"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                name: r.get("name"),
                format: r.get("format"),
                visibility: r.get("visibility"),
                repository_id: r.get("repository_id"),
                description: r.get("description"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          CAST(created_at AS CHAR) AS created_at,
                          CAST(updated_at AS CHAR) AS updated_at
                   FROM packages WHERE repository_id = ?
                   AND EXISTS (SELECT 1 FROM package_versions pv WHERE pv.package_id = packages.id)
                   ORDER BY name"#,
            )
            .bind(repository_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| PackageRow {
                id: r.get("id"),
                owner_type: r.get("owner_type"),
                owner_id: r.get("owner_id"),
                name: r.get("name"),
                format: r.get("format"),
                visibility: r.get("visibility"),
                repository_id: r.get("repository_id"),
                description: r.get("description"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }).collect())
        }
    }
}

pub async fn list_packages_by_owner(
    pool: &DbPool,
    owner_type: &str,
    owner_id: &str,
) -> Result<Vec<PackageRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description, created_at, updated_at
                   FROM packages WHERE owner_type = ? AND owner_id = ?
                   AND EXISTS (SELECT 1 FROM package_versions pv WHERE pv.package_id = packages.id)
                   ORDER BY name"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| map_package_sqlite(&r)).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          created_at::text AS created_at, updated_at::text AS updated_at
                   FROM packages WHERE owner_type = $1 AND owner_id = $2
                   AND EXISTS (SELECT 1 FROM package_versions pv WHERE pv.package_id = packages.id)
                   ORDER BY name"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageRow {
                    id: r.get("id"),
                    owner_type: r.get("owner_type"),
                    owner_id: r.get("owner_id"),
                    name: r.get("name"),
                    format: r.get("format"),
                    visibility: r.get("visibility"),
                    repository_id: r.get("repository_id"),
                    description: r.get("description"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT id, owner_type, owner_id, name, format, visibility,
                          repository_id, description,
                          CAST(created_at AS CHAR) AS created_at,
                          CAST(updated_at AS CHAR) AS updated_at
                   FROM packages WHERE owner_type = ? AND owner_id = ?
                   AND EXISTS (SELECT 1 FROM package_versions pv WHERE pv.package_id = packages.id)
                   ORDER BY name"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageRow {
                    id: r.get("id"),
                    owner_type: r.get("owner_type"),
                    owner_id: r.get("owner_id"),
                    name: r.get("name"),
                    format: r.get("format"),
                    visibility: r.get("visibility"),
                    repository_id: r.get("repository_id"),
                    description: r.get("description"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                })
                .collect())
        }
    }
}

pub async fn update_package_description(
    pool: &DbPool,
    id: &str,
    description: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE packages SET description = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?")
                .bind(description)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE packages SET description = ?, updated_at = CURRENT_TIMESTAMP(3) WHERE id = ?")
                .bind(description)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE packages SET description = $1, updated_at = now() WHERE id = $2")
                .bind(description)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}


pub async fn find_package_quota_override(
    pool: &DbPool,
    owner_type: &str,
    owner_id: &str,
) -> Result<Option<i64>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let v: Option<i64> = sqlx::query_scalar(
                "SELECT max_bytes FROM package_quota_overrides WHERE owner_type = ? AND owner_id = ?",
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(v)
        }
        DbPool::Postgres(p) => {
            let v: Option<i64> = sqlx::query_scalar(
                "SELECT max_bytes FROM package_quota_overrides WHERE owner_type = $1 AND owner_id = $2",
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(v)
        }
        DbPool::MySql(p) => {
            let v: Option<i64> = sqlx::query_scalar(
                "SELECT max_bytes FROM package_quota_overrides WHERE owner_type = ? AND owner_id = ?",
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(v)
        }
    }
}

pub async fn upsert_package_quota_override(
    pool: &DbPool,
    owner_type: &str,
    owner_id: &str,
    max_bytes: i64,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO package_quota_overrides (owner_type, owner_id, max_bytes)
                   VALUES (?, ?, ?)
                   ON CONFLICT(owner_type, owner_id) DO UPDATE SET max_bytes = excluded.max_bytes"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .bind(max_bytes)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO package_quota_overrides (owner_type, owner_id, max_bytes)
                   VALUES ($1,$2,$3)
                   ON CONFLICT (owner_type, owner_id) DO UPDATE SET max_bytes = EXCLUDED.max_bytes"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .bind(max_bytes)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO package_quota_overrides (owner_type, owner_id, max_bytes)
                   VALUES (?, ?, ?)
                   ON DUPLICATE KEY UPDATE max_bytes = VALUES(max_bytes)"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .bind(max_bytes)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn sum_package_blob_bytes_for_owner(
    pool: &DbPool,
    owner_type: &str,
    owner_id: &str,
) -> Result<i64, String> {
    // Approximate: sum size of blobs referenced by this owner's package versions
    match pool {
        DbPool::Sqlite(p) => {
            let v: Option<i64> = sqlx::query_scalar(
                r#"SELECT COALESCE(SUM(DISTINCT b.size_bytes), 0)
                   FROM package_blobs b
                   JOIN package_blob_refs r ON r.blob_digest = b.digest
                   JOIN package_versions v ON v.id = r.package_version_id
                   JOIN packages p ON p.id = v.package_id
                   WHERE p.owner_type = ? AND p.owner_id = ?"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(v.unwrap_or(0))
        }
        DbPool::Postgres(p) => {
            let v: Option<i64> = sqlx::query_scalar(
                r#"SELECT COALESCE(SUM(DISTINCT b.size_bytes), 0)::bigint
                   FROM package_blobs b
                   JOIN package_blob_refs r ON r.blob_digest = b.digest
                   JOIN package_versions v ON v.id = r.package_version_id
                   JOIN packages p ON p.id = v.package_id
                   WHERE p.owner_type = $1 AND p.owner_id = $2"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(v.unwrap_or(0))
        }
        DbPool::MySql(p) => {
            let v: Option<i64> = sqlx::query_scalar(
                r#"SELECT COALESCE(SUM(b.size_bytes), 0)
                   FROM package_blobs b
                   JOIN package_blob_refs r ON r.blob_digest = b.digest
                   JOIN package_versions v ON v.id = r.package_version_id
                   JOIN packages p ON p.id = v.package_id
                   WHERE p.owner_type = ? AND p.owner_id = ?"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(v.unwrap_or(0))
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageUsageBreakdownRow {
    pub package_id: String,
    pub name: String,
    pub format: String,
    pub bytes: i64,
}

pub async fn list_package_usage_for_owner(
    pool: &DbPool,
    owner_type: &str,
    owner_id: &str,
) -> Result<Vec<PackageUsageBreakdownRow>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT p.id AS package_id, p.name, p.format,
                          COALESCE(SUM(DISTINCT b.size_bytes), 0) AS bytes
                   FROM packages p
                   LEFT JOIN package_versions v ON v.package_id = p.id
                   LEFT JOIN package_blob_refs r ON r.package_version_id = v.id
                   LEFT JOIN package_blobs b ON b.digest = r.blob_digest
                   WHERE p.owner_type = ? AND p.owner_id = ?
                   GROUP BY p.id, p.name, p.format
                   ORDER BY p.format, p.name"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageUsageBreakdownRow {
                    package_id: r.get("package_id"),
                    name: r.get("name"),
                    format: r.get("format"),
                    bytes: r.get("bytes"),
                })
                .collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT p.id AS package_id, p.name, p.format,
                          COALESCE(SUM(DISTINCT b.size_bytes), 0)::bigint AS bytes
                   FROM packages p
                   LEFT JOIN package_versions v ON v.package_id = p.id
                   LEFT JOIN package_blob_refs r ON r.package_version_id = v.id
                   LEFT JOIN package_blobs b ON b.digest = r.blob_digest
                   WHERE p.owner_type = $1 AND p.owner_id = $2
                   GROUP BY p.id, p.name, p.format
                   ORDER BY p.format, p.name"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageUsageBreakdownRow {
                    package_id: r.get("package_id"),
                    name: r.get("name"),
                    format: r.get("format"),
                    bytes: r.get("bytes"),
                })
                .collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT p.id AS package_id, p.name, p.format,
                          COALESCE(SUM(b.size_bytes), 0) AS bytes
                   FROM packages p
                   LEFT JOIN package_versions v ON v.package_id = p.id
                   LEFT JOIN package_blob_refs r ON r.package_version_id = v.id
                   LEFT JOIN package_blobs b ON b.digest = r.blob_digest
                   WHERE p.owner_type = ? AND p.owner_id = ?
                   GROUP BY p.id, p.name, p.format
                   ORDER BY p.format, p.name"#,
            )
            .bind(owner_type)
            .bind(owner_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| PackageUsageBreakdownRow {
                    package_id: r.get("package_id"),
                    name: r.get("name"),
                    format: r.get("format"),
                    bytes: r.get("bytes"),
                })
                .collect())
        }
    }
}

pub async fn list_unref_package_blobs(pool: &DbPool, grace_secs: i64) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT digest FROM package_blobs
                   WHERE refcount = 0
                     AND created_at <= datetime('now', '-' || ? || ' seconds')"#,
            )
            .bind(grace_secs)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.get("digest")).collect())
        }
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT digest FROM package_blobs
                   WHERE refcount = 0
                     AND created_at <= now() - ($1 || ' seconds')::interval"#,
            )
            .bind(grace_secs.to_string())
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.get("digest")).collect())
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT digest FROM package_blobs
                   WHERE refcount = 0
                     AND created_at <= DATE_SUB(UTC_TIMESTAMP(3), INTERVAL ? SECOND)"#,
            )
            .bind(grace_secs)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.get("digest")).collect())
        }
    }
}

pub async fn delete_package_blob(pool: &DbPool, digest: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM package_blobs WHERE digest = ? AND refcount = 0")
                .bind(digest)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM package_blobs WHERE digest = $1 AND refcount = 0")
                .bind(digest)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM package_blobs WHERE digest = ? AND refcount = 0")
                .bind(digest)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::Database;

    #[tokio::test]
    async fn packages_blob_refcount_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite:{}", dir.path().join("pkg.db").display());
        let db = Database::connect(&url).await.unwrap();
        db.migrate().await.unwrap();
        let digest = format!("sha256:{}", "ab".repeat(32));
        db.upsert_package_blob(&digest, 12).await.unwrap();
        let n = db.adjust_package_blob_refcount(&digest, 1).await.unwrap();
        assert_eq!(n, 1);
        let n = db.adjust_package_blob_refcount(&digest, -1).await.unwrap();
        assert_eq!(n, 0);
    }
}
