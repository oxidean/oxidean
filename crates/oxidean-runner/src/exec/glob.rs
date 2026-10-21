//! `hashFiles(patterns…)` support — minimatch-lite globbing over the
//! workspace, then a deterministic SHA-256 rollup.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// `hashFiles(patterns…)` over the workspace — glob match, then SHA-256 of
/// the concatenated per-file SHA-256 digests (sorted for determinism).
/// Runner-only: server-side contexts leave the callback unset ("" result).
pub(crate) fn hash_files_fn(
    workspace: PathBuf,
) -> impl Fn(&[String]) -> Result<String, String> + Send + Sync {
    move |patterns| {
        let mut include: Vec<PathBuf> = Vec::new();
        for raw in patterns {
            let (neg, pat) = match raw.strip_prefix('!') {
                Some(p) => (true, p),
                None => (false, raw.as_str()),
            };
            let mut hits = glob_walk(&workspace, pat);
            hits.sort();
            hits.dedup();
            if neg {
                let hits: std::collections::HashSet<PathBuf> = hits.into_iter().collect();
                include.retain(|f| !hits.contains(f));
            } else {
                include.extend(hits);
            }
        }
        include.sort();
        include.dedup();
        let mut outer = Sha256::new();
        for f in &include {
            let bytes = std::fs::read(f).map_err(|e| e.to_string())?;
            outer.update(Sha256::digest(&bytes));
        }
        let digest = outer.finalize();
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// Files under `root` whose relative path matches `pattern` (minimatch-lite:
/// `**` crosses segments, `*`/`?`/`[]` within a segment, `{a,b}` alternation,
/// leading `/` = workspace-rooted). `.git` is never walked.
pub(crate) fn glob_walk(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let pat = pattern.trim().trim_start_matches('/');
    let expanded = expand_braces(pat);
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                if entry.file_name() == ".git" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            for p in &expanded {
                if glob_match(p, &rel) {
                    out.push(path.clone());
                    break;
                }
            }
        }
    }
    out
}

/// Expand `{a,b}` alternation into separate patterns (nested braces OK).
pub(crate) fn expand_braces(pat: &str) -> Vec<String> {
    let Some(open) = pat.find('{') else {
        return vec![pat.to_string()];
    };
    let mut depth = 0;
    let mut close = None;
    for (i, c) in pat.char_indices().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pat.to_string()];
    };
    let (head, rest) = pat.split_at(open);
    let (body, tail) = (&rest[1..close - open], &rest[close - open + 1..]);
    body.split(',')
        .flat_map(|alt| expand_braces(&format!("{head}{alt}{tail}")))
        .collect()
}

/// Segment-wise glob match: `**` consumes zero or more path segments.
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    let ps: Vec<&str> = pattern.split('/').collect();
    let fs: Vec<&str> = path.split('/').collect();
    fn go(ps: &[&str], fs: &[&str]) -> bool {
        if ps.is_empty() {
            return fs.is_empty();
        }
        if ps[0] == "**" {
            for skip in 0..=fs.len() {
                if go(&ps[1..], &fs[skip..]) {
                    return true;
                }
            }
            return false;
        }
        match fs.split_first() {
            Some((seg, rest)) if seg_match(ps[0], seg) => go(&ps[1..], rest),
            _ => false,
        }
    }
    go(&ps, &fs)
}

/// Single-segment wildcard match (`*`, `?`, `[class]`).
fn seg_match(pat: &str, seg: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (pat.chars().collect(), seg.chars().collect());
    // Iterative wildcard match with backtracking on `*`.
    let (mut pi, mut si) = (0usize, 0usize);
    let (mut star_p, mut star_s) = (usize::MAX, 0usize);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '[' {
            // Character class.
            let mut j = pi + 1;
            let neg = p.get(j) == Some(&'!') || p.get(j) == Some(&'^');
            if neg {
                j += 1;
            }
            let mut matched = false;
            let mut k = j;
            while k < p.len() && p[k] != ']' {
                if k + 2 < p.len() && p[k + 1] == '-' && p[k + 2] != ']' {
                    if p[k] <= s[si] && s[si] <= p[k + 2] {
                        matched = true;
                    }
                    k += 3;
                } else {
                    if p[k] == s[si] {
                        matched = true;
                    }
                    k += 1;
                }
            }
            if k >= p.len() || matched == neg {
                if star_p != usize::MAX {
                    pi = star_p;
                    star_s += 1;
                    si = star_s;
                    continue;
                }
                return false;
            }
            pi = k + 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_p = pi;
            star_s = si;
            pi += 1;
        } else if star_p != usize::MAX {
            pi = star_p + 1;
            star_s += 1;
            si = star_s;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}
