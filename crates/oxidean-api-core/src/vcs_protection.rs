//! Host `ProtectionChecker` impl for `oxidean-vcs`'s git export — funnels
//! `vcs.git_export`'s proposed ref move through the same
//! [`crate::protection::check_ref_update`] evaluation a git receive-pack runs, so a
//! VCS write can never bypass branch/tag protection, signed-commit, or
//! capability rules. Denials surface as `vcs.forbidden` with the SAME
//! `ProtectionBlockReasons` verdict block a git push produces (AC7.2.2).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use oxidean_core::ProtectionBlockReasons;
use oxidean_db::Database;
use oxidean_vcs::git_interop::ProtectionChecker;

use crate::repo::Capability;

/// Zero-oid the protection layer uses for ref creation (`old: None`).
const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

/// Capability word → ACL tier; an unknown/absent capability reads as `Read`
/// (the conservative floor — `check_ref_update` then applies its own rules).
fn parse_capability(word: &str) -> Capability {
    match word {
        "admin" => Capability::Admin,
        "write" => Capability::Write,
        _ => Capability::Read,
    }
}

/// Evaluates export ref moves through the shared protection checker.
/// `repos_dir` is needed to map `git_dir` back to `{owner}/{name}` — the
/// checker resolves rules by repo.
pub struct HostProtectionChecker {
    db: Database,
    repos_dir: PathBuf,
}

impl HostProtectionChecker {
    pub fn new(db: Database, repos_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { db, repos_dir })
    }
}

#[async_trait]
impl ProtectionChecker for HostProtectionChecker {
    async fn check(
        &self,
        repo: &Path,
        refname: &str,
        old: Option<&str>,
        new: &str,
        actor_capability: &str,
    ) -> Result<ProtectionBlockReasons, String> {
        match crate::protection::check_ref_update(
            &self.db,
            &self.repos_dir,
            repo,
            refname,
            old.unwrap_or(ZERO_SHA),
            new,
            parse_capability(actor_capability),
        )
        .await
        {
            Ok(()) => Ok(ProtectionBlockReasons::default()),
            Err(e) => {
                // A protection denial carries the verdict block in `data`;
                // an internal failure carries nothing — Err hides the detail
                // from the wire (the port contract).
                match e
                    .data
                    .as_ref()
                    .and_then(|d| serde_json::from_value::<ProtectionBlockReasons>(d.clone()).ok())
                {
                    Some(verdicts) if !verdicts.reasons.is_empty() => Ok(verdicts),
                    _ if e.code.starts_with("repo.") || e.code.starts_with("admin.") => {
                        // Non-protection repo errors (pull_refs_read_only,
                        // ref_protection resolution) still deny — surface as a
                        // generic verdict block rather than leaking internals.
                        Ok(ProtectionBlockReasons {
                            reasons: vec!["protected".to_string()],
                            ..Default::default()
                        })
                    }
                    _ => Err("protection check failed".to_string()),
                }
            }
        }
    }
}
