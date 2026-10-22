//! Repo search pathspec machinery shared with the top-level `search` module
//! (issue #162). `repo.search` RPC handlers live in `oxidean-api-repo`.

use oxidean_core::languages::LanguageSpec;

/// Build `git grep` pathspecs intersecting `path:` prefix with `language:`
/// matchers from the shared table. `:(icase)` keeps search consistent with
/// `language_for_path` (lowercased basename); `:(glob)` `**/` pins filename and
/// extension matches under the prefix at any depth. No qualifiers → no
/// pathspecs (full-tree search).
pub fn code_search_pathspecs(path: Option<&str>, langs: &[&'static LanguageSpec]) -> Vec<String> {
    let prefix = path
        .map(|p| p.trim().trim_matches('/'))
        .filter(|p| !p.is_empty());
    if langs.is_empty() {
        return match (path, prefix) {
            (Some(raw), Some(_)) => vec![raw.trim().to_string()],
            (Some(raw), None) => vec![raw.trim().to_string()],
            (None, _) => Vec::new(),
        };
    }
    let mut specs = Vec::new();
    for lang in langs {
        for ext in lang.extensions {
            match prefix {
                Some(p) => specs.push(format!(":(icase,glob){p}/**/*.{ext}")),
                None => specs.push(format!(":(icase)*.{ext}")),
            }
        }
        for name in lang.filenames {
            match prefix {
                Some(p) => specs.push(format!(":(icase,glob){p}/**/{name}")),
                None => specs.push(format!(":(icase,glob)**/{name}")),
            }
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::code_search_pathspecs;
    use oxidean_core::languages::languages_named;

    #[test]
    fn pathspecs_for_language_cover_extensions_and_filenames() {
        let ts = languages_named("typescript");
        // TypeScript + TSX rows → extension globs; no filenames.
        let specs = code_search_pathspecs(None, &ts);
        assert!(specs.contains(&":(icase)*.ts".to_string()));
        assert!(specs.contains(&":(icase)*.tsx".to_string()));

        let docker = languages_named("dockerfile");
        let specs = code_search_pathspecs(None, &docker);
        assert!(specs.contains(&":(icase)*.dockerfile".to_string()));
        assert!(specs.contains(&":(icase,glob)**/dockerfile".to_string()));
        assert!(specs.contains(&":(icase,glob)**/containerfile".to_string()));
    }

    #[test]
    fn pathspecs_intersect_path_prefix_with_language() {
        let rust = languages_named("Rust");
        let specs = code_search_pathspecs(Some("src/"), &rust);
        assert_eq!(specs, vec![":(icase,glob)src/**/*.rs".to_string()]);

        // path: alone keeps the legacy raw prefix pathspec.
        let specs = code_search_pathspecs(Some("src"), &[]);
        assert_eq!(specs, vec!["src".to_string()]);

        // No qualifiers → no pathspecs (whole tree).
        assert!(code_search_pathspecs(None, &[]).is_empty());
    }

    #[test]
    fn language_name_resolution_is_case_insensitive_and_grouped() {
        assert_eq!(languages_named("RUBY").len(), 1);
        assert!(languages_named("unknown-lang").is_empty());
        // Group name (TypeScript) pulls in the TSX row too.
        assert!(languages_named("typescript").len() > 1);
    }
}
