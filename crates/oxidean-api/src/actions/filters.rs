//! `on.<event>` filter evaluation (Phase 6) — branches/tags/paths/types.
//!
//! Pattern matching is GitHub's minimatch-flavored glob: `*` within a path
//! segment, `**` across segments, `?` one char, `!` prefix negates with
//! last-match-wins ordering (a `!` match undoes earlier positive matches).
//!
//! Filter semantics per GitHub docs:
//! - Positive list (`branches`/`tags`/`paths`/`types`): at least one pattern
//!   must match — empty list means unconstrained.
//! - `-ignore` twin (`branches-ignore`, `paths-ignore`): fire only when the
//!   subject escapes the ignore list — for paths, when at least one changed
//!   file matches none of the ignore patterns.
//! - Both on the same event is a GitHub validation error (flagged at parse).

use super::parse::EventFilters;

/// Glob-match `name` against a single GitHub pattern.
///
/// - `*` — any run of non-`/` chars (within one segment).
/// - `**` — any run including `/` (across segments); `**/` additionally
///   matches zero directories (`docs/**/x` matches `docs/x`).
/// - `?` — exactly one non-`/` char.
fn glob_one(pattern: &str, name: &str) -> bool {
    glob_match(pattern.as_bytes(), name.as_bytes())
}

fn glob_match(p: &[u8], s: &[u8]) -> bool {
    let Some((&pc, prest)) = p.split_first() else {
        return s.is_empty();
    };
    match pc {
        b'*' if prest.first() == Some(&b'*') => {
            let rest = &prest[1..];
            // `**/` collapses to zero-or-more directories — try skipping it.
            if rest.first() == Some(&b'/') && glob_match(&rest[1..], s) {
                return true;
            }
            // `**` consumes any prefix (including `/` boundaries).
            (0..=s.len()).any(|i| glob_match(rest, &s[i..]))
        }
        b'*' => {
            // `*` consumes a non-`/` prefix of any length.
            let max = s.iter().position(|&b| b == b'/').unwrap_or(s.len());
            (0..=max).any(|i| glob_match(prest, &s[i..]))
        }
        b'?' => match s.split_first() {
            Some((&sc, srest)) => sc != b'/' && glob_match(prest, srest),
            None => false,
        },
        // `[abc]`/`[a-z]`/`[!abc]` character classes (minimatch parity);
        // never matches `/`.
        b'[' => {
            if let Some((matched, rest)) = match_class(prest, s) {
                return matched && glob_match(rest, &s[1..]);
            }
            // Unclosed `[` is a literal.
            match s.split_first() {
                Some((&sc, srest)) => sc == b'[' && glob_match(prest, srest),
                None => false,
            }
        }
        c => match s.split_first() {
            Some((&sc, srest)) => sc == c && glob_match(prest, srest),
            None => false,
        },
    }
}

/// Match a `[...]` class against the next byte of `s`. Returns
/// `(matched, pattern-after-']')` — `None` when the class is unterminated.
fn match_class<'a>(body: &'a [u8], s: &[u8]) -> Option<(bool, &'a [u8])> {
    let (negate, body) = match body.split_first() {
        Some((b'!', rest)) | Some((b'^', rest)) => (true, rest),
        _ => (false, body),
    };
    let (&sc, _) = s.split_first()?;
    if sc == b'/' {
        return Some((false, &[])); // classes never match `/` — rest unused
    }
    let mut i = 0usize;
    let mut hit = false;
    let mut closed = false;
    while i < body.len() {
        if body[i] == b']' && i > 0 {
            closed = true;
            break;
        }
        // `a-z` range — `]` is a member only in first position.
        if i + 2 < body.len() && body[i + 1] == b'-' && body[i + 2] != b']' {
            if body[i] <= sc && sc <= body[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if body[i] == sc {
                hit = true;
            }
            i += 1;
        }
    }
    if !closed {
        return None;
    }
    Some((hit != negate, &body[i + 1..]))
}

/// Ordered pattern-list match — `!` negations apply last-match-wins
/// (a later `!` clears a match; a later positive re-sets it).
pub fn pattern_list_matches(patterns: &[String], name: &str) -> bool {
    let mut matched = false;
    for p in patterns {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(neg) = p.strip_prefix('!') {
            if glob_one(neg.trim(), name) {
                matched = false;
            }
        } else if glob_one(p, name) {
            matched = true;
        }
    }
    matched
}

/// Does the event fire for this ref? `ref_name` is the full git ref
/// (`refs/heads/x`, `refs/tags/v1`); `branch`/`tag` are the short names;
/// `changed` is the changed-file list for `paths` (`None` = paths filters
/// unconstrained — e.g. branch creation, where GitHub skips path checks);
/// `action_type` is the PR activity type (`None` on push).
pub fn event_matches(
    filters: &EventFilters,
    branch: Option<&str>,
    tag: Option<&str>,
    changed: Option<&[String]>,
    action_type: Option<&str>,
) -> bool {
    // `types` — activity-type constraint (pull_request family only).
    if let Some(t) = action_type {
        if !filters.types.is_empty() && !filters.types.iter().any(|x| x.trim() == t) {
            return false;
        }
    }
    // Ref-kind scoping: `branches`/`branches-ignore` constrain branch pushes
    // (and PR base branches), `tags`/`tags-ignore` constrain tag pushes —
    // each list judges only its own kind. When only one kind's list is
    // defined, pushes of the other kind never run (GitHub's "undefined ref
    // kind" rule); when both are defined, each kind checks its own list.
    let has_branch_filt = !filters.branches.is_empty() || !filters.branches_ignore.is_empty();
    let has_tag_filt = !filters.tags.is_empty() || !filters.tags_ignore.is_empty();
    match (branch, tag) {
        // Branch filters — GitHub matches the *base* branch for PR events.
        (Some(b), None) => {
            if has_tag_filt && !has_branch_filt {
                return false; // tags-only workflow → branch events never fire
            }
            if !filters.branches.is_empty() && !pattern_list_matches(&filters.branches, b) {
                return false;
            }
            if !filters.branches_ignore.is_empty()
                && pattern_list_matches(&filters.branches_ignore, b)
            {
                return false;
            }
        }
        (None, Some(t)) => {
            if has_branch_filt && !has_tag_filt {
                return false; // branches-only workflow → tag pushes never fire
            }
            if !filters.tags.is_empty() && !pattern_list_matches(&filters.tags, t) {
                return false;
            }
            if !filters.tags_ignore.is_empty() && pattern_list_matches(&filters.tags_ignore, t) {
                return false;
            }
        }
        _ => {}
    }
    // Path filters — `None` (creation/unknown) counts as matching, and
    // GitHub never evaluates `paths`/`paths-ignore` for tag pushes.
    match changed {
        _ if tag.is_some() => {}
        Some(files) => {
            if !filters.paths.is_empty()
                && !files
                    .iter()
                    .any(|f| pattern_list_matches(&filters.paths, f))
            {
                return false;
            }
            if !filters.paths_ignore.is_empty()
                && files
                    .iter()
                    .all(|f| pattern_list_matches(&filters.paths_ignore, f))
            {
                return false;
            }
        }
        None => {
            // GitHub skips path checks on ref creation (no diff range) — the
            // constraint is vacuously satisfied.
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(patterns: &[&str]) -> Vec<String> {
        patterns.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn glob_basics() {
        assert!(glob_one("main", "main"));
        assert!(!glob_one("main", "develop"));
        assert!(glob_one("release/*", "release/1.2"));
        assert!(!glob_one("release/*", "release/1.2/x"));
        assert!(glob_one("release/**", "release/1.2/x"));
        assert!(glob_one("*.md", "README.md"));
        assert!(!glob_one("*.md", "docs/a.md"));
        assert!(glob_one("docs/**", "docs/a/b.md"));
        assert!(glob_one("docs/**/x", "docs/x"));
        assert!(glob_one("mona/**/*.md", "mona/a/b.md"));
        assert!(glob_one("v?", "v1"));
        assert!(!glob_one("v?", "v12"));
    }

    #[test]
    fn negation_last_match_wins() {
        let pats = f(&["release/**", "!release/beta"]);
        assert!(pattern_list_matches(&pats, "release/1.0"));
        assert!(!pattern_list_matches(&pats, "release/beta"));
        let pats = f(&["!main", "main"]);
        assert!(pattern_list_matches(&pats, "main"));
    }

    #[test]
    fn branch_filters() {
        let filt = EventFilters {
            branches: f(&["main", "release/*"]),
            ..Default::default()
        };
        assert!(event_matches(&filt, Some("main"), None, None, None));
        assert!(event_matches(&filt, Some("release/1"), None, None, None));
        assert!(!event_matches(&filt, Some("dev"), None, None, None));
    }

    #[test]
    fn branch_ignore() {
        let filt = EventFilters {
            branches_ignore: f(&["docs-*"]),
            ..Default::default()
        };
        assert!(event_matches(&filt, Some("main"), None, None, None));
        assert!(!event_matches(&filt, Some("docs-a"), None, None, None));
    }

    #[test]
    fn tags_constrain_only_tag_pushes() {
        let filt = EventFilters {
            tags: f(&["v*"]),
            ..Default::default()
        };
        assert!(event_matches(&filt, None, Some("v1"), None, None));
        assert!(!event_matches(&filt, None, Some("x"), None, None));
        assert!(!event_matches(&filt, Some("main"), None, None, None));
    }

    #[test]
    fn paths_positive_and_ignore() {
        let files = vec!["src/a.rs".to_string(), "docs/b.md".to_string()];
        let pos = EventFilters {
            paths: f(&["src/**"]),
            ..Default::default()
        };
        assert!(event_matches(&pos, None, None, Some(&files), None));
        let pos = EventFilters {
            paths: f(&["web/**"]),
            ..Default::default()
        };
        assert!(!event_matches(&pos, None, None, Some(&files), None));
        let ign = EventFilters {
            paths_ignore: f(&["docs/**"]),
            ..Default::default()
        };
        assert!(event_matches(&ign, None, None, Some(&files), None));
        let only_docs = vec!["docs/b.md".to_string()];
        assert!(!event_matches(&ign, None, None, Some(&only_docs), None));
    }

    #[test]
    fn paths_never_gate_tag_pushes() {
        // GitHub does not evaluate paths/paths-ignore for tag pushes —
        // a non-matching paths list still fires on `refs/tags/*`.
        let files = vec!["src/a.rs".to_string()];
        let filt = EventFilters {
            paths: f(&["docs/**"]),
            ..Default::default()
        };
        assert!(!event_matches(
            &filt,
            Some("main"),
            None,
            Some(&files),
            None
        ));
        assert!(event_matches(&filt, None, Some("v1"), Some(&files), None));
        let ign = EventFilters {
            paths_ignore: f(&["src/**"]),
            ..Default::default()
        };
        assert!(!event_matches(&ign, Some("main"), None, Some(&files), None));
        assert!(event_matches(&ign, None, Some("v1"), Some(&files), None));
    }

    #[test]
    fn glob_char_classes() {
        assert!(glob_one("[ab]c.rs", "ac.rs"));
        assert!(glob_one("[a-c]d.rs", "bd.rs"));
        assert!(!glob_one("[ab]c.rs", "xc.rs"));
        assert!(glob_one("[!ab]c.rs", "xc.rs"));
        assert!(!glob_one("[!ab]c.rs", "ac.rs"));
        assert!(glob_one("[]a].rs", "].rs"));
        assert!(!glob_one("[ab]c.rs", "a/c.rs")); // classes never match `/`
        assert!(glob_one("[ab.rs", "[ab.rs")); // unclosed `[` is literal
    }

    #[test]
    fn branch_and_path_filters_are_conjunctive() {
        // `branches` AND `paths` both gate: a matching path on a
        // non-matching branch does not fire, and vice versa.
        let files = vec!["src/a.rs".to_string()];
        let filt = EventFilters {
            branches: f(&["main"]),
            paths: f(&["src/**"]),
            ..Default::default()
        };
        assert!(event_matches(&filt, Some("main"), None, Some(&files), None));
        assert!(!event_matches(&filt, Some("dev"), None, Some(&files), None));
        assert!(!event_matches(
            &filt,
            Some("main"),
            None,
            Some(&["docs/x".into()]),
            None
        ));
    }

    #[test]
    fn types_filter() {
        let filt = EventFilters {
            types: f(&["opened", "synchronize"]),
            ..Default::default()
        };
        assert!(event_matches(&filt, None, None, None, Some("opened")));
        assert!(!event_matches(&filt, None, None, None, Some("closed")));
    }
}
