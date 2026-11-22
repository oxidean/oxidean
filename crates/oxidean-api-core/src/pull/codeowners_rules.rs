use std::ops::RangeInclusive;

pub const MAX_FILE_BYTES: usize = 64 * 1024;
pub const MAX_LINES: usize = 3_000;
pub const MAX_PATTERN_CHARS: usize = 255;
pub const MAX_OWNERS_PER_LINE: usize = 20;
pub const MAX_CHANGED_PATHS: usize = 1_000;
const MAX_PATTERN_MATCH_WORK: usize = 50_000_000;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    User(String),
    Team { organization: String, slug: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationError {
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone)]
struct Rule {
    pattern: Pattern,
    owners: Vec<Owner>,
}

#[derive(Debug, Clone, Default)]
pub struct Codeowners {
    rules: Vec<Rule>,
    pub errors: Vec<EvaluationError>,
}

#[derive(Debug, Clone)]
struct Pattern(Vec<Token>);

#[derive(Debug, Clone)]
enum Token {
    Literal(char),
    Any,
    Star,
    DoubleStar,
    DoubleStarDirs,
    Class {
        negated: bool,
        ranges: Vec<RangeInclusive<char>>,
    },
}

impl Codeowners {
    pub fn parse(bytes: &[u8]) -> Self {
        if bytes.len() > MAX_FILE_BYTES {
            return Self {
                rules: Vec::new(),
                errors: vec![EvaluationError {
                    line: None,
                    message: format!("file exceeds {MAX_FILE_BYTES} bytes"),
                }],
            };
        }

        let Ok(contents) = std::str::from_utf8(bytes) else {
            return Self {
                rules: Vec::new(),
                errors: vec![EvaluationError {
                    line: None,
                    message: "file is not valid UTF-8".into(),
                }],
            };
        };
        let lines: Vec<_> = contents.lines().collect();
        if lines.len() > MAX_LINES {
            return Self {
                rules: Vec::new(),
                errors: vec![EvaluationError {
                    line: None,
                    message: format!("file exceeds {MAX_LINES} lines"),
                }],
            };
        }

        let mut parsed = Self::default();
        for (index, raw) in lines.into_iter().enumerate() {
            let line_number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut fields = line.split_whitespace();
            let Some(raw_pattern) = fields.next() else {
                continue;
            };
            let raw_owners: Vec<_> = fields.collect();
            if raw_owners.is_empty() {
                parsed.errors.push(EvaluationError {
                    line: Some(line_number),
                    message: "rule has no owners".into(),
                });
                continue;
            }
            if raw_owners.len() > MAX_OWNERS_PER_LINE {
                parsed.errors.push(EvaluationError {
                    line: Some(line_number),
                    message: format!("rule exceeds {MAX_OWNERS_PER_LINE} owners"),
                });
                continue;
            }

            let pattern = match Pattern::parse(raw_pattern) {
                Ok(pattern) => pattern,
                Err(message) => {
                    parsed.errors.push(EvaluationError {
                        line: Some(line_number),
                        message,
                    });
                    continue;
                }
            };
            let owners = raw_owners
                .iter()
                .map(|owner| parse_owner(owner))
                .collect::<Result<Vec<_>, _>>();
            match owners {
                Ok(owners) => parsed.rules.push(Rule { pattern, owners }),
                Err(message) => parsed.errors.push(EvaluationError {
                    line: Some(line_number),
                    message,
                }),
            }
        }
        parsed
    }

    pub fn owners_for_path(&self, path: &str) -> Vec<Owner> {
        if !valid_repo_path(path) {
            return Vec::new();
        }
        self.rules
            .iter()
            .rfind(|rule| rule.pattern.matches(path))
            .map(|rule| rule.owners.clone())
            .unwrap_or_default()
    }

    pub fn owners_for_paths(&self, paths: &[String]) -> Result<Vec<Vec<Owner>>, String> {
        if paths.iter().any(|path| !valid_repo_path(path)) {
            return Err("changed path is not a valid repository-relative path".into());
        }
        let pattern_tokens = self
            .rules
            .iter()
            .map(|rule| rule.pattern.0.len())
            .fold(0_usize, usize::saturating_add);
        let path_chars = paths
            .iter()
            .map(|path| path.chars().count())
            .fold(0_usize, usize::saturating_add);
        if pattern_tokens.saturating_mul(path_chars) > MAX_PATTERN_MATCH_WORK {
            return Err(format!(
                "CODEOWNERS matching exceeds {MAX_PATTERN_MATCH_WORK} work units"
            ));
        }
        Ok(paths
            .iter()
            .map(|path| self.owners_for_path(path))
            .collect())
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

fn valid_repo_path(path: &str) -> bool {
    path.len() <= 4_096
        && !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn parse_owner(raw: &str) -> Result<Owner, String> {
    let Some(owner) = raw.strip_prefix('@') else {
        return Err(format!("invalid owner token: {raw}"));
    };
    let mut parts = owner.split('/');
    let Some(first) = parts.next() else {
        return Err(format!("invalid owner token: {raw}"));
    };
    match (parts.next(), parts.next()) {
        (None, None) if valid_slug(first) => Ok(Owner::User(first.to_owned())),
        (Some(slug), None) if valid_slug(first) && valid_slug(slug) => Ok(Owner::Team {
            organization: first.to_owned(),
            slug: slug.to_owned(),
        }),
        _ => Err(format!("invalid owner token: {raw}")),
    }
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

impl Pattern {
    fn parse(raw: &str) -> Result<Self, String> {
        if raw.is_empty() || raw.chars().count() > MAX_PATTERN_CHARS {
            return Err(format!("pattern must be 1–{MAX_PATTERN_CHARS} characters"));
        }
        if raw.starts_with('!') || raw.contains('\\') {
            return Err("negation and escaping are unsupported".into());
        }
        let raw = raw.strip_prefix('/').unwrap_or(raw);
        if raw.is_empty() || raw.starts_with('/') {
            return Err("pattern must be repository-root-relative".into());
        }
        let normalized;
        let raw = if let Some(prefix) = raw.strip_suffix('/') {
            normalized = format!("{prefix}/**");
            normalized.as_str()
        } else {
            raw
        };

        let chars: Vec<char> = raw.chars().collect();
        let mut tokens = Vec::with_capacity(chars.len());
        let mut index = 0;
        while index < chars.len() {
            match chars[index] {
                '*' => {
                    let start = index;
                    while chars.get(index) == Some(&'*') {
                        index += 1;
                    }
                    let is_double = index - start > 1;
                    if is_double && chars.get(index) == Some(&'/') {
                        tokens.push(Token::DoubleStarDirs);
                        index += 1;
                    } else {
                        tokens.push(if is_double {
                            Token::DoubleStar
                        } else {
                            Token::Star
                        });
                    }
                }
                '?' => {
                    tokens.push(Token::Any);
                    index += 1;
                }
                '[' => {
                    let (class, next) = parse_class(&chars, index)?;
                    tokens.push(class);
                    index = next;
                }
                ']' => return Err("unmatched ] in pattern".into()),
                '/' => {
                    if index == 0 || index + 1 == chars.len() || chars[index - 1] == '/' {
                        return Err("pattern contains an empty path segment".into());
                    }
                    tokens.push(Token::Literal('/'));
                    index += 1;
                }
                '.' if chars.get(index + 1) == Some(&'.') => {
                    let before = index == 0 || chars[index - 1] == '/';
                    let after = index + 2 == chars.len() || chars[index + 2] == '/';
                    if before && after {
                        return Err("pattern may not contain a parent path segment".into());
                    }
                    tokens.push(Token::Literal('.'));
                    index += 1;
                }
                literal => {
                    tokens.push(Token::Literal(literal));
                    index += 1;
                }
            }
        }
        Ok(Self(tokens))
    }

    fn matches(&self, path: &str) -> bool {
        let chars: Vec<char> = path.chars().collect();
        let mut previous = vec![false; chars.len() + 1];
        let mut next = vec![false; chars.len() + 1];
        previous[0] = true;
        for token in &self.0 {
            next.fill(false);
            match token {
                Token::Literal(wanted) => {
                    for index in 0..chars.len() {
                        if previous[index] && chars[index] == *wanted {
                            next[index + 1] = true;
                        }
                    }
                }
                Token::Any => {
                    for index in 0..chars.len() {
                        if previous[index] && chars[index] != '/' {
                            next[index + 1] = true;
                        }
                    }
                }
                Token::Class { negated, ranges } => {
                    for index in 0..chars.len() {
                        if previous[index] && chars[index] != '/' {
                            let contains = ranges.iter().any(|range| range.contains(&chars[index]));
                            if contains != *negated {
                                next[index + 1] = true;
                            }
                        }
                    }
                }
                Token::Star => {
                    next.clone_from_slice(&previous);
                    for index in 0..chars.len() {
                        if next[index] && chars[index] != '/' {
                            next[index + 1] = true;
                        }
                    }
                }
                Token::DoubleStar => {
                    next.clone_from_slice(&previous);
                    for index in 0..chars.len() {
                        if next[index] {
                            next[index + 1] = true;
                        }
                    }
                }
                Token::DoubleStarDirs => {
                    let mut active = false;
                    for index in 0..=chars.len() {
                        if previous[index] {
                            active = true;
                            next[index] = true;
                        }
                        if active && index > 0 && chars[index - 1] == '/' {
                            next[index] = true;
                        }
                    }
                }
            }
            std::mem::swap(&mut previous, &mut next);
        }
        previous[chars.len()]
    }
}

fn parse_class(chars: &[char], start: usize) -> Result<(Token, usize), String> {
    let mut index = start + 1;
    let negated = matches!(chars.get(index), Some('!' | '^'));
    if negated {
        index += 1;
    }
    let mut ranges = Vec::new();
    while index < chars.len() && chars[index] != ']' {
        let first = chars[index];
        if first == '/' {
            return Err("character classes may not include /".into());
        }
        if chars.get(index + 1) == Some(&'-') {
            let Some(&last) = chars.get(index + 2) else {
                return Err("unterminated character range".into());
            };
            if last == ']' || last == '/' || first > last {
                return Err("invalid character range".into());
            }
            ranges.push(first..=last);
            index += 3;
        } else {
            ranges.push(first..=first);
            index += 1;
        }
    }
    if ranges.is_empty() || index == chars.len() {
        return Err("unterminated or empty character class".into());
    }
    Ok((Token::Class { negated, ranges }, index + 1))
}

#[cfg(test)]
mod tests {
    use super::{Codeowners, Owner};

    #[test]
    fn parses_comments_blank_lines_and_users_and_teams() {
        let parsed = Codeowners::parse(b"# owners\n\n/src/** @alice @oxidean/platform\n");
        assert!(parsed.errors.is_empty());
        assert_eq!(
            parsed.owners_for_path("src/api/main.rs"),
            vec![
                Owner::User("alice".into()),
                Owner::Team {
                    organization: "oxidean".into(),
                    slug: "platform".into(),
                },
            ]
        );
    }

    #[test]
    fn matches_supported_globs_and_root_anchor() {
        let parsed =
            Codeowners::parse(b"*.rs @root\nsrc/**/*.rs @rust\nsrc/?est/[a-c].rs @narrow\n");
        assert_eq!(
            parsed.owners_for_path("lib.rs"),
            vec![Owner::User("root".into())]
        );
        assert_eq!(
            parsed.owners_for_path("src/lib.rs"),
            vec![Owner::User("rust".into())]
        );
        assert_eq!(
            parsed.owners_for_path("src/test/b.rs"),
            vec![Owner::User("narrow".into())]
        );
        assert!(parsed.owners_for_path("nested/lib.rs").is_empty());

        let anchored = Codeowners::parse(b"/src/lib.rs @owner\n");
        assert_eq!(
            anchored.owners_for_path("src/lib.rs"),
            vec![Owner::User("owner".into())]
        );
        assert!(anchored.owners_for_path("nested/src/lib.rs").is_empty());

        let directory = Codeowners::parse(b"docs/ @docs-team\n");
        assert_eq!(
            directory.owners_for_path("docs/guide.md"),
            vec![Owner::User("docs-team".into())]
        );
    }

    #[test]
    fn rejects_invalid_changed_paths_instead_of_leaving_them_uncovered() {
        let parsed = Codeowners::parse(b"src/** @owner\n");
        let paths = vec!["src\\main.rs".into()];
        assert!(parsed.owners_for_paths(&paths).is_err());
        assert!(parsed
            .owners_for_paths(&["src/new\nline.rs".into()])
            .is_err());
    }

    #[test]
    fn later_matching_rule_replaces_earlier_owners() {
        let parsed = Codeowners::parse(b"src/** @team-a\nsrc/private/** @team-b\n");
        assert_eq!(
            parsed.owners_for_path("src/private/key.rs"),
            vec![Owner::User("team-b".into())]
        );
    }

    #[test]
    fn malformed_patterns_and_owners_are_reported_and_not_used() {
        let parsed = Codeowners::parse(
            b"!secret @alice\nsrc/../secret @alice\nsrc/[abc @alice\nsrc/private @bad/name/extra\nsrc/valid @alice\n",
        );
        assert_eq!(parsed.errors.len(), 4);
        assert!(parsed.owners_for_path("src/private").is_empty());
        assert_eq!(
            parsed.owners_for_path("src/valid"),
            vec![Owner::User("alice".into())]
        );
    }

    #[test]
    fn unsupported_escaping_and_invalid_paths_do_not_match() {
        let parsed = Codeowners::parse(b"src/*.rs @alice\nescaped\\*.rs @bob\n");
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.owners_for_path("../src/main.rs").is_empty());
        assert!(parsed.owners_for_path("/src/main.rs").is_empty());
        assert!(parsed.owners_for_path("src//main.rs").is_empty());
    }

    #[test]
    fn rejects_path_sets_that_exceed_the_matching_work_budget() {
        let source = std::iter::repeat_n("* @alice", 2_000)
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = Codeowners::parse(source.as_bytes());
        assert!(parsed.errors.is_empty());
        let paths = vec!["a".repeat(1_000); 1_000];
        assert!(parsed.owners_for_paths(&paths).is_err());
    }

    #[test]
    fn rejects_oversized_files_and_owner_lists() {
        let oversized = vec![b'a'; super::MAX_FILE_BYTES + 1];
        assert_eq!(Codeowners::parse(&oversized).errors.len(), 1);

        let owners = std::iter::repeat_n("@alice", super::MAX_OWNERS_PER_LINE + 1)
            .collect::<Vec<_>>()
            .join(" ");
        let parsed = Codeowners::parse(format!("* {owners}\n").as_bytes());
        assert_eq!(parsed.errors.len(), 1);
    }
}
