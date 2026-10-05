//! Build git `allowedSignersFile` + temp GNUPGHOME for commit signature verification.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use oxidean_db::Database;
use oxidean_git::FORGE_NOREPLY_EMAIL;
use tokio::io::AsyncWriteExt;

use super::author_resolve;

/// Materials kept alive for the duration of a verify request.
pub struct VerifyKeyring {
    _signers_dir: Option<tempfile::TempDir>,
    pub allowed_signers: Option<PathBuf>,
    _gpg_dir: Option<tempfile::TempDir>,
    pub gpg_home: Option<PathBuf>,
}

impl VerifyKeyring {
    pub fn empty() -> Self {
        Self {
            _signers_dir: None,
            allowed_signers: None,
            _gpg_dir: None,
            gpg_home: None,
        }
    }

    pub fn has_any(&self) -> bool {
        self.allowed_signers.is_some() || self.gpg_home.is_some()
    }
}

/// Collect allowed_signers + GNUPGHOME for a page of committer/author emails.
/// `resolved` must be the [`author_resolve::resolve_authors_for_emails`] map
/// for `emails` — callers resolve once and reuse the map for output rows,
/// avoiding a second lookup pass.
pub async fn keyring_for_resolved(
    db: &Database,
    emails: &[String],
    resolved: &std::collections::HashMap<String, author_resolve::ResolvedAuthor>,
) -> VerifyKeyring {
    let mut out = VerifyKeyring::empty();

    if let Some(named) = author_resolve::build_allowed_signers_file(db, emails, resolved).await {
        if let Ok(dir) = tempfile::TempDir::new() {
            let path = dir.path().join("allowed_signers");
            if tokio::fs::copy(named.path(), &path).await.is_ok() {
                out.allowed_signers = Some(path);
                out._signers_dir = Some(dir);
            }
        }
    }

    if let Some(home) = build_gpg_home(db, resolved).await {
        out.gpg_home = Some(home.path().to_path_buf());
        out._gpg_dir = Some(home);
    }

    out
}

/// Resolve emails then build a verify keyring — used by push-protection hooks
/// that do not already have an author map on hand.
pub async fn keyring_for_emails(db: &Database, emails: &[String]) -> VerifyKeyring {
    let resolved =
        author_resolve::resolve_authors_for_emails(db, emails.iter().map(String::as_str)).await;
    keyring_for_resolved(db, emails, &resolved).await
}

/// Backward-compatible helper used by older call sites.
#[allow(dead_code)]
pub async fn allowed_signers_for_emails(
    db: &Database,
    emails: &[String],
) -> Option<(tempfile::TempDir, PathBuf)> {
    let resolved =
        author_resolve::resolve_authors_for_emails(db, emails.iter().map(String::as_str)).await;
    let ring = keyring_for_resolved(db, emails, &resolved).await;
    match (ring._signers_dir, ring.allowed_signers) {
        (Some(dir), Some(path)) => {
            // path is inside dir; return dir + path
            Some((dir, path))
        }
        _ => None,
    }
}

async fn build_gpg_home(
    db: &Database,
    resolved: &std::collections::HashMap<String, author_resolve::ResolvedAuthor>,
) -> Option<tempfile::TempDir> {
    let mut seen = std::collections::HashSet::new();
    let mut uids: Vec<String> = Vec::new();
    for author in resolved.values() {
        if let Some(uid) = author.user_id.as_deref() {
            if seen.insert(uid.to_string()) {
                uids.push(uid.to_string());
            }
        }
    }
    // One batched query; on error no keys are imported, same as the old
    // per-user `continue` on `list_gpg_keys_for_user` failure.
    let armors: Vec<String> = db
        .list_gpg_keys_for_users(&uids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|k| k.armored_public_key)
        .collect();
    if armors.is_empty() {
        return None;
    }

    let dir = tempfile::TempDir::new().ok()?;
    // GnuPG refuses world-writable homes.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700));
    }

    for armor in &armors {
        let mut child = tokio::process::Command::new("gpg")
            .args(["--batch", "--yes", "--import"])
            .env("GNUPGHOME", dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(armor.as_bytes()).await;
        }
        let _ = child.wait().await;
    }
    Some(dir)
}

/// Prefetched inputs for [`VerifiedPolicy::status`] — verified addresses and
/// GPG UID emails for all resolved users on a commit page, each fetched with a
/// single batched query. `None` on a map means the lookup failed (the old
/// per-call error path returned `"unknown"`).
pub struct VerifiedPolicy {
    verified: Option<std::collections::HashMap<String, std::collections::HashSet<String>>>,
    gpg_uids: Option<std::collections::HashMap<String, std::collections::HashSet<String>>>,
}

impl VerifiedPolicy {
    /// No data — equivalent to `load` with an empty user set. Every `status`
    /// on an unsigned commit early-returns before touching these maps.
    pub fn none() -> Self {
        Self {
            verified: Some(std::collections::HashMap::new()),
            gpg_uids: Some(std::collections::HashMap::new()),
        }
    }

    /// Fetch policy inputs for the given resolved users. `needs_gpg` should be
    /// true when any commit in the page reports `signature_kind == "gpg"`.
    pub async fn load(db: &Database, user_ids: &[String], needs_gpg: bool) -> Self {
        let verified = match db.list_verified_emails_for_users(user_ids).await {
            Ok(pairs) => {
                let mut m: std::collections::HashMap<String, std::collections::HashSet<String>> =
                    std::collections::HashMap::new();
                for (uid, email) in pairs {
                    m.entry(uid).or_default().insert(email.to_ascii_lowercase());
                }
                Some(m)
            }
            Err(_) => None,
        };
        let gpg_uids = if needs_gpg {
            match db.list_gpg_keys_for_users(user_ids).await {
                Ok(keys) => {
                    let mut m: std::collections::HashMap<
                        String,
                        std::collections::HashSet<String>,
                    > = std::collections::HashMap::new();
                    for k in keys {
                        let entry = m.entry(k.user_id.clone()).or_default();
                        for u in
                            serde_json::from_str::<Vec<String>>(&k.uid_emails).unwrap_or_default()
                        {
                            entry.insert(u.to_ascii_lowercase());
                        }
                    }
                    Some(m)
                }
                Err(_) => None,
            }
        } else {
            Some(std::collections::HashMap::new())
        };
        Self { verified, gpg_uids }
    }

    /// Apply forge Verified policy to one commit, using prefetched data.
    /// `resolved` is the author-resolution map for the page.
    pub fn status(
        &self,
        resolved: &std::collections::HashMap<String, author_resolve::ResolvedAuthor>,
        committer_email: &str,
        author_email: &str,
        signature_status: &str,
        signature_kind: &str,
    ) -> String {
        if signature_status != "valid" {
            return signature_status.to_string();
        }
        let email = if !committer_email.trim().is_empty() {
            committer_email.trim()
        } else {
            author_email.trim()
        };
        if email.is_empty() {
            return "unknown".into();
        }
        if signature_kind == "ssh" && email.eq_ignore_ascii_case(FORGE_NOREPLY_EMAIL) {
            return "valid".into();
        }
        let Some(user_id) = resolved.get(email).and_then(|r| r.user_id.as_deref()) else {
            return "unknown".into();
        };

        if !author_resolve::is_forge_noreply_email(email) {
            let Some(verified) = self.verified.as_ref() else {
                return "unknown".into();
            };
            let email_l = email.to_ascii_lowercase();
            let addr_ok = verified.get(user_id).is_some_and(|v| v.contains(&email_l));
            if !addr_ok {
                return "unknown".into();
            }
        }

        if signature_kind == "gpg" {
            let Some(gpg_uids) = self.gpg_uids.as_ref() else {
                return "unknown".into();
            };
            let email_l = email.to_ascii_lowercase();
            let uid_ok = gpg_uids
                .get(user_id)
                .is_some_and(|uids| uids.contains(&email_l));
            if !uid_ok {
                return "invalid".into();
            }
        }

        "valid".into()
    }
}

/// User ids that need [`VerifiedPolicy`] data for a commit page — only commits
/// reporting crypto-valid signatures reach the policy checks.
pub fn needs_verified_policy<S, K>(commits: impl IntoIterator<Item = (S, K)>) -> (bool, bool)
where
    S: AsRef<str>,
    K: AsRef<str>,
{
    let mut any_valid = false;
    let mut any_gpg = false;
    for (status, kind) in commits {
        if status.as_ref() == "valid" {
            any_valid = true;
            if kind.as_ref() == "gpg" {
                any_gpg = true;
            }
        }
    }
    (any_valid, any_gpg)
}

/// Apply forge Verified policy after crypto `%G?` succeeded.
///
/// - Committer email must resolve to a user, and that exact address must be
///   verified on the account (or be a forge noreply address).
/// - For GPG signatures, committer email must appear in a registered key UID.
///
/// Single-commit convenience over [`VerifiedPolicy::status`] — list endpoints
/// build one [`VerifiedPolicy`] per page instead.
#[allow(dead_code)]
pub async fn apply_verified_policy(
    db: &Database,
    committer_email: &str,
    author_email: &str,
    signature_status: &str,
    signature_kind: &str,
) -> String {
    let email = if !committer_email.trim().is_empty() {
        committer_email.trim()
    } else {
        author_email.trim()
    };
    let mut resolved_map = std::collections::HashMap::new();
    let mut uids: Vec<String> = Vec::new();
    if !email.is_empty() {
        let resolved = author_resolve::resolve_author_email(db, email).await;
        if let Some(uid) = resolved.user_id.clone() {
            uids.push(uid);
        }
        resolved_map.insert(email.to_string(), resolved);
    }
    let policy = VerifiedPolicy::load(
        db,
        &uids,
        signature_kind == "gpg" && signature_status == "valid",
    )
    .await;
    policy.status(
        &resolved_map,
        committer_email,
        author_email,
        signature_status,
        signature_kind,
    )
}

#[allow(dead_code)]
pub fn allowed_signers_path(ring: &VerifyKeyring) -> Option<&Path> {
    ring.allowed_signers.as_deref()
}

#[allow(dead_code)]
pub fn gpg_home_path(ring: &VerifyKeyring) -> Option<&Path> {
    ring.gpg_home.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidean_core::Role;
    use oxidean_db::Database;

    #[tokio::test]
    async fn verified_policy_requires_that_address_verified() {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("policy.db").display());
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        std::mem::forget(dir);

        let user = db
            .create_user(
                "u-policy",
                "primary@ex.com",
                "policyuser",
                Some("hash"),
                "P",
                "",
                None,
                Role::User,
            )
            .await
            .unwrap();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        db.set_email_verified_at(&user.id, &now).await.unwrap();
        db.create_user_email("sec-unverified", &user.id, "sec@ex.com", false, None)
            .await
            .unwrap();

        let status =
            apply_verified_policy(&db, "sec@ex.com", "sec@ex.com", "valid", "ssh").await;
        assert_eq!(status, "unknown");

        db.set_user_email_verified_at("sec-unverified", Some(&now))
            .await
            .unwrap();
        let status =
            apply_verified_policy(&db, "sec@ex.com", "sec@ex.com", "valid", "ssh").await;
        assert_eq!(status, "valid");
    }

    #[tokio::test]
    async fn verified_policy_accepts_forge_web_flow_identity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("policy-forge.db").display());
        let db = Database::connect(&url).await.expect("connect");
        db.migrate().await.expect("migrate");
        std::mem::forget(dir);

        // Forge committer resolves to no user — a crypto-valid SSH signature
        // under the web-flow principal still reports verified.
        let status = apply_verified_policy(
            &db,
            FORGE_NOREPLY_EMAIL,
            "someone@ex.com",
            "valid",
            "ssh",
        )
        .await;
        assert_eq!(status, "valid");

        // Empty committer falls back to the author address — same forge case.
        let status =
            apply_verified_policy(&db, "", FORGE_NOREPLY_EMAIL, "valid", "ssh").await;
        assert_eq!(status, "valid");

        // GPG under the forge identity still requires user resolution.
        let status = apply_verified_policy(
            &db,
            FORGE_NOREPLY_EMAIL,
            FORGE_NOREPLY_EMAIL,
            "valid",
            "gpg",
        )
        .await;
        assert_eq!(status, "unknown");
    }
}
