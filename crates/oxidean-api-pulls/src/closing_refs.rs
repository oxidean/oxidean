//! Shared closing-keyword extraction for PR rail links and merge-time issue
//! closure. The grammar mirrors GitHub's closing keywords and keeps
//! `owner/repo#N` refs repo-qualified.

/// Parsed `(owner, repo)` pair from an `owner/repo#N` ref.
pub(crate) type RepoSlug = (String, String);

/// `parse_ref_tail` result: repo qualifier, issue number, end offset.
type RefTail = (Option<RepoSlug>, i64, usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClosingRef {
    pub(crate) relation: &'static str,
    pub(crate) repo: Option<(String, String)>,
    pub(crate) number: i64,
}

const KEYWORDS: &[(&str, &str)] = &[
    ("closes", "closes"),
    ("closed", "closed"),
    ("close", "close"),
    ("fixes", "fixes"),
    ("fixed", "fixed"),
    ("fix", "fix"),
    ("resolves", "resolves"),
    ("resolved", "resolved"),
    ("resolve", "resolve"),
];

fn word_byte(text: &str, idx: usize) -> bool {
    text.as_bytes()[idx].is_ascii_alphanumeric() || text.as_bytes()[idx] == b'_'
}

fn boundary_ok(text: &str, start: usize, end: usize) -> bool {
    let before = start == 0 || !word_byte(text, start - 1);
    let after = end == text.len() || !word_byte(text, end);
    before && after
}

fn skip_ws(text: &str, mut idx: usize) -> usize {
    while let Some(ch) = text[idx..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        idx += ch.len_utf8();
    }
    idx
}

fn valid_repo_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= 100
        && component
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !component.starts_with('-')
        && !component.ends_with('-')
        && !component.starts_with('.')
        && !component.ends_with('.')
}

fn parse_ref_tail(text: &str, idx: usize) -> Option<RefTail> {
    let idx = skip_ws(text, idx);
    if text[idx..].starts_with('#') {
        let digits_start = idx + 1;
        let mut end = digits_start;
        while text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
            end += 1;
        }
        let number = text[digits_start..end].parse::<i64>().ok()?;
        let trailing_ok = end == text.len() || !word_byte(text, end);
        return (number > 0 && trailing_ok).then_some((None, number, end));
    }

    let rest = &text[idx..];
    let slash = rest.find('/')?;
    let owner = &rest[..slash];
    let after_slash = &rest[slash + 1..];
    let hash = after_slash.find('#')?;
    let repo = &after_slash[..hash];
    if !valid_repo_component(owner) || !valid_repo_component(repo) {
        return None;
    }
    let digits_start = idx + slash + 1 + hash + 1;
    let mut end = digits_start;
    while text[end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
    {
        end += 1;
    }
    let number = text[digits_start..end].parse::<i64>().ok()?;
    let trailing_ok = end == text.len() || !word_byte(text, end);
    (number > 0 && trailing_ok).then_some((
        Some((owner.to_string(), repo.to_string())),
        number,
        end,
    ))
}

fn matches_keyword(text: &str, idx: usize) -> Option<(&'static str, usize)> {
    for (prefix, relation) in KEYWORDS {
        let Some(candidate) = text.get(idx..idx + prefix.len()) else {
            continue;
        };
        if candidate.eq_ignore_ascii_case(prefix) && boundary_ok(text, idx, idx + prefix.len()) {
            return Some((relation, prefix.len()));
        }
    }
    None
}

pub(crate) fn closing_refs(text: &str) -> Vec<ClosingRef> {
    let mut hits: Vec<(usize, ClosingRef)> = Vec::new();
    let mut idx = 0;
    while idx < text.len() {
        let Some((relation, len)) = matches_keyword(text, idx) else {
            idx += text[idx..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let after_keyword = idx + len;
        let has_space = text[after_keyword..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace);
        if has_space {
            if let Some((repo, number, end)) = parse_ref_tail(text, after_keyword) {
                hits.push((
                    idx,
                    ClosingRef {
                        relation,
                        repo,
                        number,
                    },
                ));
                idx = end;
                continue;
            }
        }
        idx += len;
    }
    hits.sort_by_key(|(pos, _)| *pos);
    let mut out: Vec<ClosingRef> = Vec::new();
    for (_, hit) in hits {
        if !out.iter().any(|seen| {
            seen.number == hit.number
                && seen
                    .repo
                    .as_ref()
                    .map(|(o, r)| (o.to_lowercase(), r.to_lowercase()))
                    == hit
                        .repo
                        .as_ref()
                        .map(|(o, r)| (o.to_lowercase(), r.to_lowercase()))
        }) {
            out.push(hit);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{closing_refs, RepoSlug};

    fn refs(text: &str) -> Vec<(Option<RepoSlug>, i64, &'static str)> {
        closing_refs(text)
            .into_iter()
            .map(|r| (r.repo, r.number, r.relation))
            .collect()
    }

    #[test]
    fn parses_closing_keywords() {
        assert_eq!(
            refs("closes #12 and fixes #7"),
            vec![(None, 12, "closes"), (None, 7, "fixes")]
        );
        assert_eq!(refs("close #3"), vec![(None, 3, "close")]);
        assert_eq!(refs("closed #4"), vec![(None, 4, "closed")]);
        assert_eq!(refs("fixed #5"), vec![(None, 5, "fixed")]);
        assert_eq!(refs("resolved #6"), vec![(None, 6, "resolved")]);
    }

    #[test]
    fn parses_qualified_refs() {
        assert_eq!(
            refs("closes owner/repo#9 fixes acme/widgets#10"),
            vec![
                (Some(("owner".into(), "repo".into())), 9, "closes"),
                (Some(("acme".into(), "widgets".into())), 10, "fixes"),
            ]
        );
    }

    #[test]
    fn dedupes_qualified_and_unqualified() {
        assert_eq!(
            refs("closes #3 resolves #3 closes Owner/Repo#4 owner/repo#4"),
            vec![
                (None, 3, "closes"),
                (Some(("Owner".into(), "Repo".into())), 4, "closes"),
            ]
        );
    }

    #[test]
    fn rejects_invalid_refs() {
        for text in [
            "closes #",
            "closes #0",
            "closes owner/repo#",
            "closes -bad/repo#1",
            "closes owner/repo.#1",
            "closes owner/repo#1x",
            "encloses #9",
            "close#9",
            "closes#9",
        ] {
            assert_eq!(refs(text), vec![], "{text}");
        }
    }

    #[test]
    fn never_panics_on_trailing_or_multibyte() {
        for text in [
            "closes #",
            "fixes #é",
            "resolves #\u{1f600}",
            "closes",
            "closes #abc",
            "text closes #12 trailing closes #",
            "closes owner/repo#é",
        ] {
            let _ = closing_refs(text);
        }
        assert_eq!(refs("closes #"), Vec::new());
        assert_eq!(refs("fixes #é"), Vec::new());
        assert_eq!(refs("closes owner/repo#é"), Vec::new());
        assert_eq!(
            refs("closes #12 trailing closes #"),
            vec![(None, 12, "closes")]
        );
    }
}
