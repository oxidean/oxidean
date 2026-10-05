//! Personal access token DTOs and scope enums (D-03, D-06, D-08, D-15).
//!
//! List/response items never carry plaintext secrets — only `CreatePatResponse.token`
//! holds the one-time reveal value.

use serde::{Deserialize, Serialize};

/// Classic PAT string prefix (D-08 locked `oxidean_prefixes`).
pub const CLASSIC_PAT_PREFIX: &str = "oxidean_pat_";

/// Fine-grained PAT string prefix (D-08 locked `oxidean_prefixes`).
pub const FINE_GRAINED_PAT_PREFIX: &str = "oxidean_fg_";

/// Token kind. Serialized as `classic` | `fine_grained`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatKind {
    Classic,
    FineGrained,
}

impl PatKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::FineGrained => "fine_grained",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "classic" => Ok(Self::Classic),
            "fine_grained" => Ok(Self::FineGrained),
            other => Err(format!("invalid pat kind: {other}")),
        }
    }

    pub const fn token_prefix(self) -> &'static str {
        match self {
            Self::Classic => CLASSIC_PAT_PREFIX,
            Self::FineGrained => FINE_GRAINED_PAT_PREFIX,
        }
    }
}

/// Classic scope catalog: `repo` (git) + `package:read` / `package:write` (D-PKG-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ClassicPatScope {
    #[serde(rename = "repo")]
    Repo,
    #[serde(rename = "package:read")]
    PackageRead,
    #[serde(rename = "package:write")]
    PackageWrite,
}

impl ClassicPatScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::PackageRead => "package:read",
            Self::PackageWrite => "package:write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "repo" => Ok(Self::Repo),
            "package:read" => Ok(Self::PackageRead),
            "package:write" => Ok(Self::PackageWrite),
            other => Err(format!("invalid classic scope: {other}")),
        }
    }
}

/// Fine-grained Packages permission (D-PKG-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackagesPerm {
    Read,
    Write,
}

impl PackagesPerm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(format!("invalid packages_perm: {other}")),
        }
    }
}

/// Fine-grained repository access (D-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FgRepoAccess {
    Selected,
    All,
}

impl FgRepoAccess {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::All => "all",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "selected" => Ok(Self::Selected),
            "all" => Ok(Self::All),
            other => Err(format!("invalid repo_access: {other}")),
        }
    }
}

/// Fine-grained Contents permission — `read` = upload-pack; `write` = receive-pack + read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ContentsPerm {
    Read,
    Write,
}

impl ContentsPerm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            other => Err(format!("invalid contents_perm: {other}")),
        }
    }
}

/// `pat.createClassic` input (RPC wired in 08-04+).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateClassicPatRequest {
    /// Required note/name (D-16).
    pub name: String,
    /// Must include `repo` in Phase 8.
    pub scopes: Vec<ClassicPatScope>,
    /// Optional expiry; omit / null = no expiration (D-07).
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// `pat.createFineGrained` input (RPC wired in 08-04+).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateFineGrainedPatRequest {
    /// Required note/name (D-16).
    pub name: String,
    pub repo_access: FgRepoAccess,
    /// Required when `repo_access` is `selected`.
    #[serde(default)]
    pub repository_ids: Vec<String>,
    pub contents: ContentsPerm,
    /// Optional packages permission for FG tokens (D-PKG-04).
    #[serde(default)]
    pub packages: Option<PackagesPerm>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// List / metadata item — **no** plaintext token field (D-15, T-08-01).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatListItem {
    pub id: String,
    pub kind: PatKind,
    pub name: String,
    pub token_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<ClassicPatScope>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<ContentsPerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packages: Option<PackagesPerm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_access: Option<FgRepoAccess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repository_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_ip: Option<String>,
    pub created_at: String,
}

/// One-time create response — plaintext `token` only here (D-15).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreatePatResponse {
    pub token: String,
    pub item: PatListItem,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_oxidean_brand_not_ona() {
        assert_eq!(CLASSIC_PAT_PREFIX, "oxidean_pat_");
        assert_eq!(FINE_GRAINED_PAT_PREFIX, "oxidean_fg_");
        assert_eq!(PatKind::Classic.token_prefix(), "oxidean_pat_");
        assert_eq!(PatKind::FineGrained.token_prefix(), "oxidean_fg_");
        assert!(!CLASSIC_PAT_PREFIX.starts_with("ona_"));
        assert!(!FINE_GRAINED_PAT_PREFIX.starts_with("ona_"));
        assert!(!CLASSIC_PAT_PREFIX.contains("github"));
        assert!(!CLASSIC_PAT_PREFIX.starts_with("gh"));
    }

    #[test]
    fn pat_kind_serde_snake_case() {
        let json = serde_json::to_string(&PatKind::FineGrained).unwrap();
        assert_eq!(json, "\"fine_grained\"");
        let kind: PatKind = serde_json::from_str("\"classic\"").unwrap();
        assert_eq!(kind, PatKind::Classic);
    }

    #[test]
    fn pat_list_item_json_has_no_token_field() {
        let item = PatListItem {
            id: "p1".into(),
            kind: PatKind::Classic,
            name: "laptop".into(),
            token_prefix: CLASSIC_PAT_PREFIX.into(),
            scopes: Some(vec![ClassicPatScope::Repo]),
            contents: None,
            packages: None,
            repo_access: None,
            repository_ids: vec![],
            expires_at: None,
            last_used_at: None,
            last_used_ip: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let v = serde_json::to_value(&item).unwrap();
        assert!(v.get("token").is_none());
        assert_eq!(v["token_prefix"], CLASSIC_PAT_PREFIX);
    }

    #[test]
    fn create_pat_response_includes_token_once() {
        let resp = CreatePatResponse {
            token: format!("{CLASSIC_PAT_PREFIX}deadbeef"),
            item: PatListItem {
                id: "p1".into(),
                kind: PatKind::Classic,
                name: "ci".into(),
                token_prefix: CLASSIC_PAT_PREFIX.into(),
                scopes: Some(vec![ClassicPatScope::Repo]),
                contents: None,
                packages: None,
                repo_access: None,
                repository_ids: vec![],
                expires_at: None,
                last_used_at: None,
                last_used_ip: None,
                created_at: "2026-01-01T00:00:00Z".into(),
            },
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v["token"].as_str().unwrap().starts_with(CLASSIC_PAT_PREFIX));
        assert!(v["item"].get("token").is_none());
    }
}
