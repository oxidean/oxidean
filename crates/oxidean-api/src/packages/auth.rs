//! Registry auth — PAT Basic/Bearer only; session Cookie ignored (D-PKG-04 / Phase 8 D-12).

use axum::http::HeaderMap;
use oxidean_core::{
    ClassicPatScope, PackagesPerm, CLASSIC_PAT_PREFIX, FINE_GRAINED_PAT_PREFIX,
};
use oxidean_db::{Database, PatRow};
use sha2::{Digest, Sha256};

use crate::packages::acl::{self, PackageAction};

#[derive(Debug, Clone)]
pub struct RegistryIdentity {
    pub user_id: String,
    pub pat: PatRow,
    pub classic_scopes: Option<Vec<ClassicPatScope>>,
    pub fg_packages: Option<PackagesPerm>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(out.len() * 2);
    for &b in out.as_slice() {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

mod base64_lite {
    pub fn decode(input: &str) -> Option<Vec<u8>> {
        const TABLE: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let bytes: Vec<u8> = input
            .bytes()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        let mut buf = 0u32;
        let mut bits = 0u32;
        for &b in &bytes {
            if b == b'=' {
                break;
            }
            let val = TABLE.iter().position(|&c| c == b)? as u32;
            buf = (buf << 6) | val;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((buf >> bits) & 0xff) as u8);
            }
        }
        Some(out)
    }
}

pub fn decode_basic(headers: &HeaderMap) -> Option<(String, String)> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let b64 = raw.strip_prefix("Basic ")?;
    let bytes = base64_lite::decode(b64.trim())?;
    let pair = String::from_utf8(bytes).ok()?;
    let (user, pass) = pair.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

pub fn decode_bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    raw.strip_prefix("Bearer ").map(|s| s.trim().to_string())
}

pub async fn authenticate_registry(
    db: &Database,
    headers: &HeaderMap,
) -> Result<Option<RegistryIdentity>, String> {
    let token = if let Some((_user, pass)) = decode_basic(headers) {
        pass
    } else if let Some(bearer) = decode_bearer(headers) {
        bearer
    } else {
        return Ok(None);
    };

    if !(token.starts_with(CLASSIC_PAT_PREFIX) || token.starts_with(FINE_GRAINED_PAT_PREFIX)) {
        return Ok(None);
    }
    let hash = sha256_hex(token.as_bytes());
    let Some(pat) = db.find_pat_by_token_hash(&hash).await? else {
        return Ok(None);
    };
    if pat.revoked_at.is_some() {
        return Ok(None);
    }

    // Soft-ban: keep PAT rows; reject at resolve so unban restores access.
    match db.find_user_by_id(&pat.user_id).await? {
        Some(u) if u.banned_at.is_some() => return Ok(None),
        None => return Ok(None),
        Some(_) => {}
    }

    let scopes_parsed = match pat.scopes_json.as_deref() {
        Some(raw) => {
            let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
            let mut out = Vec::new();
            for n in names {
                if let Ok(s) = ClassicPatScope::parse(&n) {
                    out.push(s);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        None => None,
    };

    let fg_packages = if pat.kind == "fine_grained" {
        scopes_parsed.as_ref().and_then(|scopes| {
            if scopes
                .iter()
                .any(|s| matches!(s, ClassicPatScope::PackageWrite))
            {
                Some(PackagesPerm::Write)
            } else if scopes
                .iter()
                .any(|s| matches!(s, ClassicPatScope::PackageRead))
            {
                Some(PackagesPerm::Read)
            } else {
                None
            }
        })
    } else {
        None
    };

    let classic_for_auth = if pat.kind == "classic" {
        scopes_parsed
    } else {
        None
    };

    Ok(Some(RegistryIdentity {
        user_id: pat.user_id.clone(),
        pat,
        classic_scopes: classic_for_auth,
        fg_packages,
    }))
}

impl RegistryIdentity {
    pub fn allows(&self, action: PackageAction) -> bool {
        acl::pat_allows_packages(self.classic_scopes.as_deref(), self.fg_packages, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn cookie_header_ignored_without_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("oxidean_session=abc"),
        );
        assert!(decode_basic(&headers).is_none());
        assert!(decode_bearer(&headers).is_none());
    }
}
