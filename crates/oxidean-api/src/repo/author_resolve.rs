//! Resolve commit author emails to Oxidean users (exact email + noreply forms).

use std::collections::{HashMap, HashSet};
use std::io::Write;

use oxidean_db::{Database, UserRow};
use oxidean_git::FORGE_NOREPLY_EMAIL;

use crate::public_origin::{origin_hostname, resolve_public_origin};

/// Resolved author fields attached to commit / blame DTOs.
#[derive(Debug, Clone, Default)]
pub struct ResolvedAuthor {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
}

/// Hostname used in `users.noreply.<host>` addresses.
pub fn noreply_host() -> String {
    origin_hostname(&resolve_public_origin())
        .unwrap_or("oxidean.local")
        .to_ascii_lowercase()
}

fn avatar_url_for(user: &UserRow) -> Option<String> {
    user.avatar_path
        .as_ref()
        .map(|_| format!("/uploads/avatars/{}.webp", user.id))
}

fn from_user(user: &UserRow) -> ResolvedAuthor {
    ResolvedAuthor {
        user_id: Some(user.id.clone()),
        username: Some(user.username.clone()),
        avatar_url: avatar_url_for(user),
    }
}

/// True when `email` is a forge noreply address for this instance host.
pub fn is_forge_noreply_email(email: &str) -> bool {
    let email = email.trim();
    let Some((_, domain)) = email.split_once('@') else {
        return false;
    };
    let expected = format!("users.noreply.{}", noreply_host());
    domain.eq_ignore_ascii_case(&expected)
}

/// Parse `{user_id}@users.noreply.<host>` or `{id}+{username}@users.noreply.<host>`.
pub fn parse_noreply_local_part(local: &str) -> Option<(String, Option<String>)> {
    let local = local.trim();
    if local.is_empty() {
        return None;
    }
    if let Some((id, username)) = local.split_once('+') {
        let id = id.trim();
        let username = username.trim();
        if id.is_empty() || username.is_empty() {
            return None;
        }
        return Some((id.to_string(), Some(username.to_string())));
    }
    Some((local.to_string(), None))
}

/// Resolve a single author email to a user (exact match, then noreply forms).
#[allow(dead_code)]
pub async fn resolve_author_email(db: &Database, email: &str) -> ResolvedAuthor {
    let email = email.trim();
    if email.is_empty() {
        return ResolvedAuthor::default();
    }

    if let Ok(Some(user)) = db.find_user_by_email(email).await {
        return from_user(&user);
    }

    let Some((local, domain)) = email.split_once('@') else {
        return ResolvedAuthor::default();
    };
    let domain = domain.to_ascii_lowercase();
    let expected = format!("users.noreply.{}", noreply_host());
    if domain != expected {
        return ResolvedAuthor::default();
    }

    let Some((user_id, maybe_username)) = parse_noreply_local_part(local) else {
        return ResolvedAuthor::default();
    };

    match db.find_user_by_id(&user_id).await {
        Ok(Some(user)) => {
            if let Some(expected_username) = maybe_username {
                if !user
                    .username
                    .eq_ignore_ascii_case(expected_username.trim())
                {
                    return ResolvedAuthor::default();
                }
            }
            from_user(&user)
        }
        _ => ResolvedAuthor::default(),
    }
}

/// Batch-resolve unique author emails for a page of commits / blame lines.
pub async fn resolve_author_emails(
    db: &Database,
    emails: impl IntoIterator<Item = impl AsRef<str>>,
) -> HashMap<String, ResolvedAuthor> {
    resolve_authors_for_emails(db, emails).await
}

/// Batch-resolve unique author emails for a page of commits / blame lines.
///
/// Same result as calling [`resolve_author_email`] per email, but bounded to a
/// handful of `IN`-list round trips instead of ~2 sequential queries per email.
pub async fn resolve_authors_for_emails(
    db: &Database,
    emails: impl IntoIterator<Item = impl AsRef<str>>,
) -> HashMap<String, ResolvedAuthor> {
    let mut unique: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for e in emails {
        let trimmed = e.as_ref().trim().to_string();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            unique.push(trimmed);
        }
    }
    let mut out: HashMap<String, ResolvedAuthor> = HashMap::with_capacity(unique.len());
    if unique.is_empty() {
        return out;
    }

    // Stage 1 — `user_emails` rows. An email with a row resolves to that row's
    // user (when the user row exists); without a row it falls through to the
    // legacy `users.email` fallback, exactly like `resolve_author_email`.
    let ue_rows = db
        .find_user_emails_by_addresses(&unique)
        .await
        .unwrap_or_default();
    let mut email_user_id: HashMap<String, String> = HashMap::new();
    let mut ue_hit: HashSet<String> = HashSet::new();
    let mut wanted_ids: Vec<String> = Vec::new();
    let mut wanted_seen = HashSet::new();
    for row in &ue_rows {
        let key = row.email.to_ascii_lowercase();
        ue_hit.insert(key.clone());
        email_user_id.insert(key, row.user_id.clone());
        if wanted_seen.insert(row.user_id.clone()) {
            wanted_ids.push(row.user_id.clone());
        }
    }
    let mut users_by_id: HashMap<String, UserRow> = db
        .find_users_by_ids(&wanted_ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| (u.id.clone(), u))
        .collect();

    let mut remaining: Vec<String> = Vec::new();
    let mut stage3: Vec<String> = Vec::new();
    for email in &unique {
        let key = email.to_ascii_lowercase();
        if ue_hit.contains(&key) {
            if let Some(user) = email_user_id.get(&key).and_then(|id| users_by_id.get(id)) {
                out.insert(email.clone(), from_user(user));
            } else {
                // user_emails row exists but user row missing →
                // find_user_by_email → None → noreply path only (no
                // users.email fallback), same as resolve_author_email.
                stage3.push(email.clone());
            }
        } else {
            remaining.push(email.clone());
        }
    }

    // Stage 2 — legacy `users.email` fallback for emails with no user_emails row.
    if !remaining.is_empty() {
        let fallback = db
            .find_users_by_emails(&remaining)
            .await
            .unwrap_or_default();
        let mut by_email: HashMap<String, &UserRow> = HashMap::new();
        for u in &fallback {
            by_email.entry(u.email.to_ascii_lowercase()).or_insert(u);
        }
        for email in &remaining {
            match by_email.get(&email.to_ascii_lowercase()) {
                Some(u) => {
                    out.insert(email.clone(), from_user(u));
                }
                None => stage3.push(email.clone()),
            }
        }
    }

    // Stage 3 — `{id}[+{username}]@users.noreply.<host>` resolution.
    let expected = format!("users.noreply.{}", noreply_host());
    let mut noreply: HashMap<String, (String, Option<String>)> = HashMap::new();
    for email in &stage3 {
        let Some((local, domain)) = email.split_once('@') else {
            continue;
        };
        if domain.to_ascii_lowercase() != expected {
            continue;
        }
        if let Some((user_id, maybe_username)) = parse_noreply_local_part(local) {
            noreply.insert(email.clone(), (user_id, maybe_username));
        }
    }
    if !noreply.is_empty() {
        let missing: Vec<String> = noreply
            .values()
            .map(|(id, _)| id.clone())
            .filter(|id| !users_by_id.contains_key(id))
            .collect();
        if !missing.is_empty() {
            users_by_id.extend(
                db.find_users_by_ids(&missing)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|u| (u.id.clone(), u)),
            );
        }
        for (email, (user_id, maybe_username)) in noreply {
            let Some(user) = users_by_id.get(&user_id) else {
                continue;
            };
            if let Some(expected_username) = maybe_username {
                if !user
                    .username
                    .eq_ignore_ascii_case(expected_username.trim())
                {
                    continue;
                }
            }
            out.insert(email, from_user(user));
        }
    }
    out
}

/// Path of a temp allowed_signers file, if any.
#[allow(dead_code)]
pub fn allowed_signers_path(file: &Option<tempfile::NamedTempFile>) -> Option<&std::path::Path> {
    file.as_ref().map(|f| f.path())
}

/// Look up a cached resolution (keyed by the email string git returned).
#[allow(dead_code)]
pub fn lookup_resolved<'a>(
    map: &'a HashMap<String, ResolvedAuthor>,
    email: &str,
) -> &'a ResolvedAuthor {
    static EMPTY: ResolvedAuthor = ResolvedAuthor {
        user_id: None,
        username: None,
        avatar_url: None,
    };
    map.get(email.trim()).unwrap_or(&EMPTY)
}

/// OpenSSH public-key line pieces: `key-type key [comment…]`.
fn openssh_key_material(line: &str) -> Option<(String, String)> {
    let mut parts = line.trim().split_whitespace();
    let key_type = parts.next()?.to_string();
    let key = parts.next()?.to_string();
    if key_type.is_empty() || key.is_empty() {
        return None;
    }
    Some((key_type, key))
}

/// Build a temporary `allowed_signers` file for SSH signature verification.
///
/// Format per line: `email namespaces="git" <key-type> <key>`
/// Includes the instance web-flow pubkey (if present) plus SSH keys for each
/// resolved author that appears on the page.
pub async fn build_allowed_signers_file(
    db: &Database,
    author_emails: &[String],
    resolved: &HashMap<String, ResolvedAuthor>,
) -> Option<tempfile::NamedTempFile> {
    let mut lines: Vec<String> = Vec::new();

    // Instance web-flow signing key (seed commits / forge-authored).
    if let Ok(Some(pub_line)) = crate::git::web_flow::public_key_line().await {
        if let Some((key_type, key)) = openssh_key_material(&pub_line) {
            lines.push(format!(
                "{FORGE_NOREPLY_EMAIL} namespaces=\"git\" {key_type} {key}"
            ));
            for email in author_emails {
                let e = email.trim();
                if !e.is_empty() && e != FORGE_NOREPLY_EMAIL {
                    lines.push(format!("{e} namespaces=\"git\" {key_type} {key}"));
                }
            }
        }
    }

    let mut uids: Vec<String> = Vec::new();
    {
        let mut seen = HashSet::new();
        for author in resolved.values() {
            if let Some(uid) = author.user_id.as_deref() {
                if seen.insert(uid.to_string()) {
                    uids.push(uid.to_string());
                }
            }
        }
    }
    // Batched lookups — one round trip each instead of two per user. On error
    // the maps stay empty, which mirrors the old per-user `continue` path.
    let all_keys: Vec<oxidean_db::ssh_keys::SshKeyRow> =
        db.list_ssh_keys_for_users(&uids).await.unwrap_or_default();
    let verified_pairs: Option<Vec<(String, String)>> =
        db.list_verified_emails_for_users(&uids).await.ok();
    let mut keys_by_user: HashMap<&str, Vec<&oxidean_db::ssh_keys::SshKeyRow>> = HashMap::new();
    for k in &all_keys {
        keys_by_user.entry(k.user_id.as_str()).or_default().push(k);
    }
    let mut verified_by_user: HashMap<&str, Vec<&str>> = HashMap::new();
    for (uid, email) in verified_pairs.iter().flatten() {
        verified_by_user
            .entry(uid.as_str())
            .or_default()
            .push(email.as_str());
    }

    for uid in &uids {
        let uid = uid.as_str();
        // A verified-emails lookup failure used to skip the user entirely.
        if verified_pairs.is_none() {
            continue;
        }
        // Always include the commit email that resolved this user, plus every
        // verified address on the account (forge multi-email principals).
        let mut principals: HashSet<String> = HashSet::new();
        for e in verified_by_user.get(uid).into_iter().flatten().copied() {
            let t = e.trim().to_string();
            if !t.is_empty() {
                principals.insert(t);
            }
        }
        for (page_email, page_author) in resolved {
            if page_author.user_id.as_deref() == Some(uid) {
                let t = page_email.trim();
                if !t.is_empty() {
                    principals.insert(t.to_string());
                }
            }
        }
        for key_row in keys_by_user.get(uid).into_iter().flatten().copied() {
            if !key_row.can_sign {
                continue;
            }
            let Some((key_type, key)) = openssh_key_material(&key_row.public_key) else {
                continue;
            };
            for e in &principals {
                lines.push(format!("{e} namespaces=\"git\" {key_type} {key}"));
            }
        }
    }

    if lines.is_empty() {
        return None;
    }
    lines.sort();
    lines.dedup();

    let mut tmp = tempfile::NamedTempFile::new().ok()?;
    for line in &lines {
        writeln!(tmp, "{line}").ok()?;
    }
    tmp.flush().ok()?;
    Some(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_noreply_id_only() {
        let (id, user) = parse_noreply_local_part("abc-123").expect("parse");
        assert_eq!(id, "abc-123");
        assert!(user.is_none());
    }

    #[test]
    fn parse_noreply_id_plus_username() {
        let (id, user) = parse_noreply_local_part("abc-123+jesse").expect("parse");
        assert_eq!(id, "abc-123");
        assert_eq!(user.as_deref(), Some("jesse"));
    }

    #[test]
    fn parse_noreply_rejects_empty_parts() {
        assert!(parse_noreply_local_part("+jesse").is_none());
        assert!(parse_noreply_local_part("abc+").is_none());
        assert!(parse_noreply_local_part("").is_none());
    }

    #[test]
    fn openssh_key_material_strips_comment() {
        let (t, k) = openssh_key_material("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI comment here")
            .expect("parse");
        assert_eq!(t, "ssh-ed25519");
        assert_eq!(k, "AAAAC3NzaC1lZDI1NTE5AAAAI");
    }

    #[tokio::test]
    async fn build_allowed_signers_excludes_can_sign_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!(
            "sqlite:{}",
            dir.path().join("allowed_signers.db").display()
        );
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        std::mem::forget(dir);

        let user_id = "user-sign-filter";
        db.create_user(
            user_id,
            "signer@example.com",
            "signer",
            Some("hash"),
            "Signer",
            "",
            None,
            oxidean_core::Role::User,
        )
        .await
        .expect("create user");

        const AUTH_ONLY_BLOB: &str =
            "AAAAC3NzaC1lZDI1NTE5AAAAIJqxgqAG6vw46mOJ8QZKNpHEoPuP5sW2YoBlT/24OycR";
        const SIGN_BLOB: &str =
            "AAAAC3NzaC1lZDI1NTE5AAAAIBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
        db.create_ssh_key(
            "k-auth-only",
            user_id,
            "auth-only",
            &format!("ssh-ed25519 {AUTH_ONLY_BLOB} auth"),
            "SHA256:auth-only-fp",
            "ssh-ed25519",
            true,
            false,
        )
        .await
        .expect("auth-only key");
        db.create_ssh_key(
            "k-sign",
            user_id,
            "sign",
            &format!("ssh-ed25519 {SIGN_BLOB} sign"),
            "SHA256:sign-fp",
            "ssh-ed25519",
            false,
            true,
        )
        .await
        .expect("sign key");

        let email = "signer@example.com".to_string();
        let mut resolved = HashMap::new();
        resolved.insert(
            email.clone(),
            ResolvedAuthor {
                user_id: Some(user_id.to_string()),
                username: Some("signer".into()),
                avatar_url: None,
            },
        );
        let file = build_allowed_signers_file(&db, &[email], &resolved)
            .await
            .expect("allowed_signers file");
        let contents = std::fs::read_to_string(file.path()).expect("read");
        assert!(
            contents.contains(SIGN_BLOB),
            "can_sign=true key must appear: {contents}"
        );
        assert!(
            !contents.contains(AUTH_ONLY_BLOB),
            "can_sign=false key must be excluded: {contents}"
        );
    }
}
