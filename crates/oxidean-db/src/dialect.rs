//! Dialect resolution from `DATABASE_URL` / `OXIDEAN_DB_DIALECT` (D-01, D-02).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgres,
    MySql,
    Sqlite,
}

impl Dialect {
    pub fn as_str(self) -> &'static str {
        match self {
            Dialect::Postgres => "postgres",
            Dialect::MySql => "mysql",
            Dialect::Sqlite => "sqlite",
        }
    }
}

/// Infer dialect from URL scheme prefix only (SQLite URLs are not always RFC-3986).
pub fn from_url(url: &str) -> Result<Dialect, String> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        Ok(Dialect::Postgres)
    } else if url.starts_with("mysql://") {
        Ok(Dialect::MySql)
    } else if url.starts_with("sqlite:") {
        Ok(Dialect::Sqlite)
    } else {
        Err(format!(
            "unrecognized DATABASE_URL scheme: {}",
            redact_url(url)
        ))
    }
}

pub fn parse_declared(value: &str) -> Result<Dialect, String> {
    match value {
        "postgres" => Ok(Dialect::Postgres),
        "mysql" => Ok(Dialect::MySql),
        "sqlite" => Ok(Dialect::Sqlite),
        _ => Err(format!(
            "unknown OXIDEAN_DB_DIALECT: {value} (expected postgres|mysql|sqlite)"
        )),
    }
}

/// Prefer URL scheme; when `declared` is set it must agree (D-01, D-02).
pub fn resolve_dialect(url: &str, declared: Option<&str>) -> Result<Dialect, String> {
    let detected = from_url(url)?;
    match declared {
        None => Ok(detected),
        Some(raw) => {
            let declared_dialect = parse_declared(raw)?;
            if declared_dialect != detected {
                return Err(format!(
                    "OXIDEAN_DB_DIALECT={} does not match DATABASE_URL scheme (detected {})",
                    raw,
                    detected.as_str()
                ));
            }
            Ok(detected)
        }
    }
}

pub fn resolve_dialect_from_env(url: &str) -> Result<Dialect, String> {
    let declared = std::env::var("OXIDEAN_DB_DIALECT").ok();
    let declared_ref = declared.as_deref().filter(|s| !s.is_empty());
    resolve_dialect(url, declared_ref)
}

/// `IN (...)` placeholder list for positional bind params (batch lookups).
/// `start` is the 1-based bind index of the first id (Postgres `$n` / SQLite `?n`).
pub fn in_placeholders(dialect: Dialect, start: usize, count: usize) -> String {
    match dialect {
        Dialect::Postgres => (start..start + count)
            .map(|i| format!("${i}"))
            .collect::<Vec<_>>()
            .join(", "),
        Dialect::MySql => std::iter::repeat_n("?", count)
            .collect::<Vec<_>>()
            .join(", "),
        Dialect::Sqlite => (start..start + count)
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Strip password between `://user:` and `@` (T-02-01).
pub fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after_scheme = scheme_end + 3;
    let rest = &url[after_scheme..];
    let Some(at) = rest.find('@') else {
        return url.to_string();
    };
    let credentials = &rest[..at];
    let Some(colon) = credentials.find(':') else {
        return url.to_string();
    };
    let user = &credentials[..colon];
    format!("{}{}:***@{}", &url[..after_scheme], user, &rest[at + 1..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_dialect_schemes() {
        let cases = [
            ("postgres://u:p@h/db", Dialect::Postgres),
            ("postgresql://u:p@h/db", Dialect::Postgres),
            ("mysql://u:p@h:3306/db", Dialect::MySql),
            ("sqlite:./var/oxidean.db", Dialect::Sqlite),
            ("sqlite://./var/oxidean.db", Dialect::Sqlite),
            ("sqlite::memory:", Dialect::Sqlite),
        ];
        for (url, expected) in cases {
            assert_eq!(resolve_dialect(url, None).unwrap(), expected, "{url}");
            assert_eq!(from_url(url).unwrap(), expected, "{url}");
        }
    }

    #[test]
    fn resolve_dialect_unknown_scheme() {
        assert!(from_url("mssql://x").is_err());
        assert!(from_url("mariadb://x").is_err());
    }

    #[test]
    fn resolve_dialect_mismatch() {
        let err = resolve_dialect("postgres://u:p@h/db", Some("mysql")).unwrap_err();
        assert!(
            err.contains("does not match DATABASE_URL scheme"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn resolve_dialect_agreeing_declared() {
        assert_eq!(
            resolve_dialect("postgres://u:p@h/db", Some("postgres")).unwrap(),
            Dialect::Postgres
        );
    }

    #[test]
    fn redact_url_hides_password() {
        let redacted = redact_url("postgres://oxidean:hunter2@db:5432/oxidean");
        assert!(!redacted.contains("hunter2"), "password leaked: {redacted}");
        assert!(redacted.contains("oxidean:***@db:5432/oxidean"));
    }
}
