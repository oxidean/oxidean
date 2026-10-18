//! CAS-only ref registry (u4).
//!
//! All updates run through [`Database::vcs_cas_ref_update`]; conflict
//! determination comes from the CAS outcome itself — no pre-read, no
//! `vcs_get_ref` point-read exists (R-05: an early-stop point read would be
//! collation-unsafe anyway; resolution scans `vcs_list_refs` until match or
//! exhaustion).
//!
//! Name grammar (BR4.9): `^[A-Za-z0-9][A-Za-z0-9._/-]{0,254}$` minus the
//! pinned reject list (`..`, `//`, leading/trailing `/` or `.`, `@{`, exact
//! `HEAD`, `refs/` prefix) minus id-shaped names — a `ChangesetId::parse` hit
//! or a pure 40/64-hex name would be creatable but unresolvable (the
//! id-precedence resolution order shadows it — a self-DoS).

use base64::Engine;
use oxidean_core::vcs_types::ChangesetId;
use oxidean_db::{CasOutcome, Database, VcsRefRow};

const LIST_PAGE: i64 = 500;
/// Opaque cursor payload bound — same budget as the read-index cursors.
const CURSOR_MAX: usize = 1024;

/// Errors surfaced by [`RefRegistry`]. u7 maps variants to wire codes
/// (LC4.6 shapes: `InvalidInput`/`Internal` carry strings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    /// Named ref does not exist (delete/get on absent, `Conflict{None}`).
    RefNotFound,
    /// CAS lost — `actual` is the loser-visible head the wire carries.
    RefConflict { actual: String },
    /// `new` changeset target does not exist (pre-CAS existence probe).
    TargetNotFound,
    /// Name grammar / id-shape / cursor rejection — `field: reason` text,
    /// never echoes the value.
    InvalidInput(String),
    /// Raw facade/io detail for host diagnostics — opaque to the wire.
    Internal(String),
    /// `Database::is_skipped()` — eager fail-closed check.
    DbNotConfigured,
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::RefNotFound => write!(f, "ref not found"),
            RefError::RefConflict { .. } => write!(f, "ref conflict"),
            RefError::TargetNotFound => write!(f, "target changeset not found"),
            RefError::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            RefError::Internal(detail) => write!(f, "internal error: {detail}"),
            RefError::DbNotConfigured => write!(f, "database not configured"),
        }
    }
}

impl std::error::Error for RefError {}

/// Every typed-error return warns once at construction — `error = %e`
/// carries the typed error only (gc.rs precedent; no path/name values).
fn warned(e: RefError) -> RefError {
    tracing::warn!(error = %e, "vcs ref typed error");
    e
}

fn internal(detail: String) -> RefError {
    warned(RefError::Internal(detail))
}

fn invalid(field: &str, reason: &str) -> RefError {
    warned(RefError::InvalidInput(format!("{field}: {reason}")))
}

/// A stored ref — `changeset_id` is `None` only for the assert-absent no-op
/// result (stored rows always carry a head; the facade never writes NULL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub name: String,
    pub changeset_id: Option<ChangesetId>,
    pub updated_at: String,
}

impl Ref {
    fn from_row(row: VcsRefRow) -> Result<Ref, RefError> {
        let head = row
            .changeset_id
            .as_deref()
            .map(ChangesetId::parse)
            .transpose()
            .map_err(internal)?;
        Ok(Ref {
            name: row.name,
            changeset_id: head,
            updated_at: row.updated_at,
        })
    }
}

/// Ref name grammar (BR4.9). All checks are local — failures never reach the
/// DB, and reasons never echo the rejected name.
fn validate_ref_name(name: &str) -> Result<(), RefError> {
    // Regex-equivalent shape, hand-rolled: first char alnum, the rest
    // [A-Za-z0-9._/-], total ≤255.
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 255 {
        return Err(invalid("name", "length must be 1..=255"));
    }
    let ok_first = bytes[0].is_ascii_alphanumeric();
    let ok_rest = bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'));
    if !ok_first || !ok_rest {
        return Err(invalid(
            "name",
            "must match [A-Za-z0-9][A-Za-z0-9._/-]{0,254}",
        ));
    }
    if name.contains("..") {
        return Err(invalid("name", "must not contain '..'"));
    }
    if name.contains("//") {
        return Err(invalid("name", "must not contain '//'"));
    }
    if name.ends_with('/') || name.ends_with('.') {
        return Err(invalid("name", "must not end with '/' or '.'"));
    }
    if name.contains("@{") {
        return Err(invalid("name", "must not contain '@{'"));
    }
    if name == "HEAD" {
        return Err(invalid("name", "the name HEAD is reserved"));
    }
    if name.starts_with("refs/") {
        return Err(invalid("name", "the 'refs/' prefix is reserved"));
    }
    // Id-shape rejection — a name that parses as a changeset id or reads as a
    // git oid would be creatable-but-unresolvable (BR4.9, SR4.3).
    if ChangesetId::parse(name).is_ok() {
        return Err(invalid("name", "changeset-id-shaped names are reserved"));
    }
    let pure_hex = name
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if pure_hex && (name.len() == 40 || name.len() == 64) {
        return Err(invalid("name", "git-oid-shaped names are reserved"));
    }
    Ok(())
}

/// Keyset cursor `{"k": last_ref_name}` — base64url-no-pad JSON like the
/// index cursors (SR4.7 uniformity).
#[derive(serde::Serialize, serde::Deserialize)]
struct RefCursor {
    k: String,
}

fn encode_cursor(name: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&RefCursor {
            k: name.to_string(),
        })
        .unwrap_or_default(),
    )
}

fn decode_cursor(raw: &str) -> Result<String, RefError> {
    if raw.len() > CURSOR_MAX {
        return Err(invalid("cursor", "malformed"));
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.as_bytes())
        .map_err(|_| invalid("cursor", "malformed"))?;
    if bytes.len() > CURSOR_MAX {
        return Err(invalid("cursor", "malformed"));
    }
    let cur: RefCursor =
        serde_json::from_slice(&bytes).map_err(|_| invalid("cursor", "malformed"))?;
    Ok(cur.k)
}

/// CAS-only ref registry.
pub struct RefRegistry {
    db: Database,
}

impl RefRegistry {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// CAS update — the strict expected-head contract (WF2/R-02):
    ///
    /// | expected_old | new      | semantics                              |
    /// |--------------|----------|----------------------------------------|
    /// | `None`       | `Some`   | create-only — fails if the ref exists  |
    /// | `Some`       | `Some`   | move — only if head == expected        |
    /// | `Some`       | `None`   | delete — only if head == expected      |
    /// | `None`       | `None`   | assert-absent no-op                    |
    ///
    /// `Conflict{actual:Some}` → `RefConflict{actual}`; `Conflict{actual:
    /// None}` → `RefNotFound` for the delete/move arms and `Ok` (empty-head
    /// `Ref`) for assert-absent — the outcome is the post-CAS observation,
    /// never a pre-read (SR4.4). A `new` target is existence-checked before
    /// the CAS so the absent-target class is honest (FK-violation strings
    /// would be dialect text, not a sentinel).
    pub async fn cas(
        &self,
        repo_id: &str,
        name: &str,
        expected_old: Option<&str>,
        new: Option<&str>,
    ) -> Result<Ref, RefError> {
        if self.db.is_skipped() {
            return Err(warned(RefError::DbNotConfigured));
        }
        validate_ref_name(name)?;
        if let Some(e) = expected_old {
            if ChangesetId::parse(e).is_err() {
                return Err(invalid("expected_old", "not a changeset id"));
            }
        }
        if let Some(n) = new {
            if ChangesetId::parse(n).is_err() {
                return Err(invalid("new", "not a changeset id"));
            }
            // Absent-target class is typed, not a dialect FK string.
            let exists = self
                .db
                .vcs_get_changeset(repo_id, n)
                .await
                .map_err(internal)?
                .is_some();
            if !exists {
                return Err(warned(RefError::TargetNotFound));
            }
        }

        let outcome = self
            .db
            .vcs_cas_ref_update(repo_id, name, expected_old, new)
            .await
            .map_err(internal)?;
        match outcome {
            CasOutcome::Applied(row) => Ref::from_row(row),
            CasOutcome::Conflict { actual } => {
                match actual {
                    Some(actual) => Err(warned(RefError::RefConflict { actual })),
                    None if expected_old.is_none() && new.is_none() => {
                        // Assert-absent on an absent ref — the no-op arm.
                        Ok(Ref {
                            name: name.to_string(),
                            changeset_id: None,
                            updated_at: String::new(),
                        })
                    }
                    None => Err(warned(RefError::RefNotFound)),
                }
            }
        }
    }

    /// Point resolution by full `vcs_list_refs` scan — match-or-exhaustion,
    /// no `vcs_get_ref` (collation-safe; SR4.4/R-05). `None`-head rows are
    /// unreachable through this facade and are skipped if ever seen.
    /// `pub(crate)` per LC4.3 — consumed by `ReadIndex::log`'s `from` arm
    /// and future u7 entry points, not a wire surface.
    pub(crate) async fn resolve(&self, repo_id: &str, name: &str) -> Result<ChangesetId, RefError> {
        if self.db.is_skipped() {
            return Err(warned(RefError::DbNotConfigured));
        }
        let mut cursor: Option<String> = None;
        loop {
            let page = self
                .db
                .vcs_list_refs(repo_id, cursor.as_deref(), LIST_PAGE)
                .await
                .map_err(internal)?;
            let fetched = page.len();
            for row in page {
                if row.name == name {
                    let head = row
                        .changeset_id
                        .as_deref()
                        .map(ChangesetId::parse)
                        .transpose()
                        .map_err(internal)?;
                    return head.ok_or_else(|| warned(RefError::RefNotFound));
                }
                cursor = Some(row.name);
            }
            if (fetched as i64) < LIST_PAGE {
                return Err(warned(RefError::RefNotFound));
            }
        }
    }

    /// Keyset page over `vcs_list_refs` — cursor is `{"k":last_name}`
    /// base64url. `limit` clamps into the facade's 1..=500 band.
    pub async fn list(
        &self,
        repo_id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Ref>, Option<String>), RefError> {
        if self.db.is_skipped() {
            return Err(warned(RefError::DbNotConfigured));
        }
        let after = match cursor {
            None => None,
            Some(raw) => Some(decode_cursor(raw)?),
        };
        let limit = limit.clamp(1, LIST_PAGE as usize) as i64;
        let rows = self
            .db
            .vcs_list_refs(repo_id, after.as_deref(), limit)
            .await
            .map_err(internal)?;
        let mut refs = Vec::with_capacity(rows.len());
        for row in rows {
            refs.push(Ref::from_row(row)?);
        }
        // Probe one row past the page so a full page only emits a cursor
        // when another row actually exists.
        let next = if (refs.len() as i64) == limit {
            let last_name = refs.last().map(|r| r.name.clone()).unwrap_or_default();
            let peek = self
                .db
                .vcs_list_refs(repo_id, Some(&last_name), 1)
                .await
                .map_err(internal)?;
            if peek.is_empty() {
                None
            } else {
                Some(encode_cursor(&last_name))
            }
        } else {
            None
        };
        Ok((refs, next))
    }

    /// Hard delete by name — `RefNotFound` when the row did not exist
    /// (WF5/R-04; non-atomic `vcs_delete_ref` return is the contract).
    pub async fn delete(&self, repo_id: &str, name: &str) -> Result<(), RefError> {
        if self.db.is_skipped() {
            return Err(warned(RefError::DbNotConfigured));
        }
        validate_ref_name(name)?;
        let removed = self
            .db
            .vcs_delete_ref(repo_id, name)
            .await
            .map_err(internal)?;
        if removed {
            Ok(())
        } else {
            Err(warned(RefError::RefNotFound))
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::changeset_store::{
        ChangeKind, ChangesetStore, CreateChangesetInput, FileChangeInput,
    };
    use crate::object_store::testutil::{seed_repo, test_db};

    const CS_A: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";

    fn blob(byte: u8) -> String {
        format!("{:02x}", byte).repeat(32)
    }

    /// Seed a real changeset so `new`-target probes find it.
    async fn seed_changeset(
        db: &Database,
        repo: &str,
        message: &str,
        blob_byte: u8,
    ) -> ChangesetId {
        let h = blob(blob_byte);
        db.vcs_insert_object_if_absent(&h, 4).await.unwrap();
        ChangesetStore::new(db.clone())
            .create(CreateChangesetInput {
                repo_id: repo.to_string(),
                parents: vec![],
                author_name: "T".into(),
                author_email: "t@example.com".into(),
                message: message.into(),
                committed_at: Some("2026-01-02T03:04:05Z".into()),
                changes: vec![FileChangeInput {
                    path: format!("f{blob_byte}"),
                    kind: ChangeKind::Add,
                    mode: 33188,
                    old_path: None,
                    blob_hash: Some(h),
                }],
            })
            .await
            .expect("seed changeset")
    }

    // ---- name grammar: every reject arm ----

    #[tokio::test]
    async fn name_rejection_arms() {
        let (_dir, db) = test_db().await;
        let registry = RefRegistry::new(db);
        let bad = [
            "",        // empty
            ".hidden", // leading '.'
            "-lead",   // first char not alnum
            "_lead",
            "/lead",
            "a..b", // double dot
            "a//b", // double slash
            "trail/",
            "trail.",
            "a@{1}",           // reflog syntax
            "HEAD",            // exact reserved
            "refs/heads/main", // refs/ prefix
            "has space",
            "a:b",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            &"n".repeat(256), // over 255
            CS_A,             // changeset-id-shaped
            &"ab".repeat(20), // 40-hex oid-shaped
            &"cd".repeat(32), // 64-hex oid-shaped
        ];
        for name in bad {
            match registry.cas("r", name, None, None).await {
                Err(RefError::InvalidInput(reason)) => {
                    // "the name HEAD is reserved" names the constant, not
                    // the user value — no-echo exempts fixed literals.
                    let fixed_literal = name == "HEAD";
                    assert!(
                        name.is_empty()
                            || name.len() > 255
                            || fixed_literal
                            || !reason.contains(name),
                        "reason echoed rejected name {name:?}: {reason}"
                    );
                }
                other => panic!("ref name {name:?} must reject, got {other:?}"),
            }
        }
        // Borderline-valid names pass validation (and fail at TargetNotFound
        // on the nonexistent new-target probe — proof the grammar accepted).
        for name in ["main", "feat/x", "v1.2.3", "a_b-c", "Head", "heads/main"] {
            match registry.cas("r", name, None, Some(CS_A)).await {
                Err(RefError::TargetNotFound) | Err(RefError::Internal(_)) => {}
                other => panic!("name {name:?} should pass grammar, got {other:?}"),
            }
        }
    }

    // ---- CAS arms (no preread; outcome-driven) ----

    #[tokio::test]
    async fn cas_create_move_delete_and_conflicts() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-rr", "r-rr").await;
        let a = seed_changeset(&db, &repo, "a", 0xa1).await;
        let b = seed_changeset(&db, &repo, "b", 0xa2).await;
        let registry = RefRegistry::new(db.clone());

        // create-only on absent
        let r = registry
            .cas(&repo, "main", None, Some(a.as_str()))
            .await
            .expect("create");
        assert_eq!(
            r.changeset_id.as_ref().map(|c| c.as_str()),
            Some(a.as_str())
        );
        // create-only on present → RefConflict carrying actual head.
        match registry.cas(&repo, "main", None, Some(b.as_str())).await {
            Err(RefError::RefConflict { actual }) => {
                assert_eq!(actual, a.as_str());
            }
            other => panic!("expected RefConflict, got {other:?}"),
        }
        // move with stale expectation → RefConflict{actual: head}.
        match registry
            .cas(&repo, "main", Some(b.as_str()), Some(b.as_str()))
            .await
        {
            Err(RefError::RefConflict { actual }) => assert_eq!(actual, a.as_str()),
            other => panic!("expected RefConflict, got {other:?}"),
        }
        // move with correct expectation → Applied.
        let r = registry
            .cas(&repo, "main", Some(a.as_str()), Some(b.as_str()))
            .await
            .expect("move");
        assert_eq!(
            r.changeset_id.as_ref().map(|c| c.as_str()),
            Some(b.as_str())
        );
        // retried (already-landed) CAS → RefConflict{actual} — non-idempotent
        // by design (clients re-read then re-CAS).
        match registry
            .cas(&repo, "main", Some(a.as_str()), Some(b.as_str()))
            .await
        {
            Err(RefError::RefConflict { actual }) => assert_eq!(actual, b.as_str()),
            other => panic!("retried CAS must conflict, got {other:?}"),
        }
        // delete with wrong expectation → conflict.
        match registry.cas(&repo, "main", Some(a.as_str()), None).await {
            Err(RefError::RefConflict { actual }) => assert_eq!(actual, b.as_str()),
            other => panic!("expected RefConflict, got {other:?}"),
        }
        // delete with right expectation → Applied(None-head row).
        let r = registry
            .cas(&repo, "main", Some(b.as_str()), None)
            .await
            .expect("delete");
        assert_eq!(r.changeset_id, None);
        // absent-delete → RefNotFound (Conflict{actual:None} post-state).
        assert_eq!(
            registry.cas(&repo, "main", Some(b.as_str()), None).await,
            Err(RefError::RefNotFound)
        );
        // assert-absent no-op on absent → Ok(empty head), NOT an error.
        let r = registry
            .cas(&repo, "gone", None, None)
            .await
            .expect("assert-absent noop");
        assert_eq!(r.changeset_id, None);
        assert_eq!(r.name, "gone");
        // assert-absent on present → RefConflict{actual}.
        registry
            .cas(&repo, "main", None, Some(a.as_str()))
            .await
            .unwrap();
        match registry.cas(&repo, "main", None, None).await {
            Err(RefError::RefConflict { actual }) => assert_eq!(actual, a.as_str()),
            other => panic!("assert-absent on present must conflict: {other:?}"),
        }
        // absent-move → RefNotFound.
        assert_eq!(
            registry
                .cas(&repo, "never", Some(a.as_str()), Some(a.as_str()))
                .await,
            Err(RefError::RefNotFound)
        );
    }

    #[tokio::test]
    async fn cas_target_not_found_and_id_validation() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-tn", "r-tn").await;
        let registry = RefRegistry::new(db.clone());
        // new target absent → TargetNotFound.
        assert_eq!(
            registry.cas(&repo, "r1", None, Some(CS_A)).await,
            Err(RefError::TargetNotFound)
        );
        // malformed expected_old / new → InvalidInput.
        assert!(matches!(
            registry.cas(&repo, "r1", Some("nope"), None).await,
            Err(RefError::InvalidInput(r)) if r.starts_with("expected_old:")
        ));
        assert!(matches!(
            registry.cas(&repo, "r1", None, Some("nope")).await,
            Err(RefError::InvalidInput(r)) if r.starts_with("new:")
        ));
    }

    #[tokio::test]
    async fn resolve_scans_and_list_pages() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-rl", "r-rl").await;
        let a = seed_changeset(&db, &repo, "a", 0xa1).await;
        let registry = RefRegistry::new(db.clone());
        // absent name → RefNotFound via full-scan exhaustion.
        assert_eq!(
            registry.resolve(&repo, "absent").await,
            Err(RefError::RefNotFound)
        );
        for name in ["main", "dev", "feature/deep"] {
            registry
                .cas(&repo, name, None, Some(a.as_str()))
                .await
                .unwrap();
        }
        assert_eq!(
            registry.resolve(&repo, "dev").await.unwrap().as_str(),
            a.as_str()
        );
        // list: keyset pages, bytewise order.
        let (page, next) = registry.list(&repo, None, 2).await.unwrap();
        assert_eq!(page.len(), 2);
        let cursor = next.expect("page must continue");
        let (page2, next2) = registry.list(&repo, Some(&cursor), 2).await.unwrap();
        assert_eq!(page2.len(), 1);
        assert!(next2.is_none());
        let all: Vec<String> = page
            .iter()
            .chain(page2.iter())
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(all, vec!["dev", "feature/deep", "main"]);
        // malformed cursor → InvalidInput (not panic, not oracle).
        assert!(matches!(
            registry.list(&repo, Some("!!!not-base64!!!"), 10).await,
            Err(RefError::InvalidInput(_))
        ));
        // wrong-shape decodable cursor → InvalidInput too.
        let wrong = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"n":5}"#);
        assert!(matches!(
            registry.list(&repo, Some(&wrong), 10).await,
            Err(RefError::InvalidInput(_))
        ));
        // delete removes; second delete → RefNotFound.
        registry.delete(&repo, "dev").await.unwrap();
        assert_eq!(
            registry.resolve(&repo, "dev").await,
            Err(RefError::RefNotFound)
        );
        assert_eq!(
            registry.delete(&repo, "dev").await,
            Err(RefError::RefNotFound)
        );
    }

    #[tokio::test]
    async fn skipped_db_fail_closed() {
        let registry = RefRegistry::new(Database::skipped());
        assert_eq!(
            registry.cas("r", "main", None, None).await,
            Err(RefError::DbNotConfigured)
        );
        assert_eq!(
            registry.resolve("r", "main").await,
            Err(RefError::DbNotConfigured)
        );
        assert_eq!(
            registry.list("r", None, 10).await,
            Err(RefError::DbNotConfigured)
        );
        assert_eq!(
            registry.delete("r", "main").await,
            Err(RefError::DbNotConfigured)
        );
    }
}
