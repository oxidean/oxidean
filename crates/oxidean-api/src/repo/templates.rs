//! Vendored stack / license / gitignore catalogs + initial-commit file assembly (D-02–D-04).
//!
//! Assets live under `crates/oxidean-api/assets/` and are embedded at compile time.
//! Pack IDs are resolved via allowlist maps only — never raw filesystem joins (T-07-09).

use std::collections::BTreeMap;

use include_dir::{include_dir, Dir};
use oxidean_core::{AppError, RepoTemplateOption, TemplateProvenance};
use serde::Deserialize;

static ASSETS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");

#[derive(Debug, Deserialize)]
struct StackCatalog {
    packs: Vec<CatalogEntry>,
}

#[derive(Debug, Deserialize)]
struct GitignoreCatalog {
    templates: Vec<CatalogEntry>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // refresh metadata is for catalog.json / tooling; parsed for validation
struct CatalogEntry {
    id: String,
    label: String,
    group: String,
    #[serde(default)]
    description: String,
    /// Stack packs only — recommended `.gitignore` catalog id.
    #[serde(default)]
    default_gitignore: Option<String>,
    /// Built-in refresh metadata (issue #18) — ignored at runtime beyond parse.
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    source_ref: Option<String>,
    #[serde(default)]
    last_synced: Option<String>,
    #[serde(default)]
    verify_commands: Option<Vec<String>>,
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' || c == '.')
}

fn is_safe_spdx_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

fn none_like(v: &Option<String>) -> bool {
    match v {
        None => true,
        Some(s) => {
            let t = s.trim();
            t.is_empty() || t.eq_ignore_ascii_case("none")
        }
    }
}

/// List stack packs for `/new` picker modal (grouped by `group`).
pub fn list_stacks() -> Result<Vec<RepoTemplateOption>, AppError> {
    let file = ASSETS
        .get_file("stack-presets/catalog.json")
        .ok_or_else(|| AppError::new("repo.template_catalog", "stack catalog missing"))?;
    let catalog: StackCatalog = serde_json::from_slice(file.contents()).map_err(|e| {
        AppError::new(
            "repo.template_catalog",
            format!("invalid stack catalog: {e}"),
        )
    })?;
    Ok(catalog
        .packs
        .into_iter()
        .map(|p| RepoTemplateOption {
            id: p.id,
            label: p.label,
            group: p.group,
            description: p.description,
            default_gitignore: p.default_gitignore,
            provenance: TemplateProvenance::Builtin,
            source_label: None,
        })
        .collect())
}

/// List gitignore templates for `/new` picker modal.
pub fn list_gitignores() -> Result<Vec<RepoTemplateOption>, AppError> {
    let file = ASSETS
        .get_file("gitignore/catalog.json")
        .ok_or_else(|| AppError::new("repo.template_catalog", "gitignore catalog missing"))?;
    let catalog: GitignoreCatalog = serde_json::from_slice(file.contents()).map_err(|e| {
        AppError::new(
            "repo.template_catalog",
            format!("invalid gitignore catalog: {e}"),
        )
    })?;
    Ok(catalog
        .templates
        .into_iter()
        .map(|p| RepoTemplateOption {
            id: p.id,
            label: p.label,
            group: p.group,
            description: p.description,
            default_gitignore: None,
            provenance: TemplateProvenance::Builtin,
            source_label: None,
        })
        .collect())
}

fn load_stack_files(stack_id: &str) -> Result<BTreeMap<String, Vec<u8>>, AppError> {
    if !is_safe_id(stack_id) {
        return Err(AppError::new(
            "repo.invalid_template",
            format!("invalid stack_id: {stack_id}"),
        ));
    }
    let packs = list_stacks()?;
    if !packs.iter().any(|p| p.id == stack_id) {
        return Err(AppError::new(
            "repo.invalid_template",
            format!("unknown stack_id: {stack_id}"),
        ));
    }
    let dir = ASSETS
        .get_dir(format!("stack-presets/{stack_id}"))
        .ok_or_else(|| {
            AppError::new(
                "repo.invalid_template",
                format!("stack pack missing on disk: {stack_id}"),
            )
        })?;
    let mut out = BTreeMap::new();
    collect_dir_files(dir, "", &mut out);
    if out.is_empty() {
        return Err(AppError::new(
            "repo.invalid_template",
            format!("stack pack has no files: {stack_id}"),
        ));
    }
    Ok(out)
}

fn collect_dir_files(dir: &Dir<'_>, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for file in dir.files() {
        let name = file
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name.is_empty() || name == "catalog.json" {
            continue;
        }
        let rel = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        out.insert(rel, file.contents().to_vec());
    }
    for child in dir.dirs() {
        let name = child
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let next = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        collect_dir_files(child, &next, out);
    }
}

fn load_gitignore(gitignore_id: &str) -> Result<Vec<u8>, AppError> {
    if !is_safe_id(gitignore_id) {
        return Err(AppError::new(
            "repo.invalid_template",
            format!("invalid gitignore_id: {gitignore_id}"),
        ));
    }
    let known = list_gitignores()?;
    if !known.iter().any(|p| p.id == gitignore_id) {
        return Err(AppError::new(
            "repo.invalid_template",
            format!("unknown gitignore_id: {gitignore_id}"),
        ));
    }
    let path = format!("gitignore/{gitignore_id}.gitignore");
    let file = ASSETS.get_file(&path).ok_or_else(|| {
        AppError::new(
            "repo.invalid_template",
            format!("gitignore template missing: {gitignore_id}"),
        )
    })?;
    Ok(file.contents().to_vec())
}

fn load_license(license_id: &str) -> Result<Vec<u8>, AppError> {
    if !is_safe_spdx_id(license_id) {
        return Err(AppError::new(
            "repo.invalid_template",
            format!("invalid license_id: {license_id}"),
        ));
    }
    let path = format!("licenses/{license_id}.txt");
    if let Some(file) = ASSETS.get_file(&path) {
        return Ok(file.contents().to_vec());
    }
    // Full SPDX picker may select IDs without a vendored body — seed a stub (D-04).
    let stub = format!(
        "SPDX-License-Identifier: {license_id}\n\nSee https://spdx.org/licenses/{license_id}.html for the license text.\n"
    );
    Ok(stub.into_bytes())
}

/// Look up a stack pack's recommended gitignore id (if any).
fn stack_default_gitignore(stack_id: &str) -> Result<Option<String>, AppError> {
    let packs = list_stacks()?;
    Ok(packs
        .into_iter()
        .find(|p| p.id == stack_id)
        .and_then(|p| p.default_gitignore))
}

/// Resolve which gitignore to seed.
/// - Explicit id → that template
/// - Explicit `"none"` / empty → none (even when the stack has a default)
/// - Omitted (`None`) + stack with `default_gitignore` → stack default
fn resolve_gitignore_id(
    stack_id: &Option<String>,
    gitignore_id: &Option<String>,
) -> Result<Option<String>, AppError> {
    match gitignore_id {
        Some(raw) => {
            let t = raw.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("none") {
                Ok(None)
            } else {
                Ok(Some(t.to_string()))
            }
        }
        None => {
            if none_like(stack_id) {
                Ok(None)
            } else {
                stack_default_gitignore(stack_id.as_ref().unwrap().trim())
            }
        }
    }
}

/// Assemble files for the initial commit. Empty when all template pickers are none (ASSUME Q2).
pub fn assemble_seed_files(
    stack_id: &Option<String>,
    license_id: &Option<String>,
    gitignore_id: &Option<String>,
) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    let mut map = BTreeMap::new();

    if !none_like(stack_id) {
        let id = stack_id.as_ref().unwrap().trim();
        for (path, bytes) in load_stack_files(id)? {
            map.insert(path, bytes);
        }
    }

    if let Some(id) = resolve_gitignore_id(stack_id, gitignore_id)? {
        map.insert(".gitignore".into(), load_gitignore(&id)?);
    }

    if !none_like(license_id) {
        let id = license_id.as_ref().unwrap().trim();
        map.insert("LICENSE".into(), load_license(id)?);
    }

    Ok(map.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_catalog_nonempty() {
        let stacks = list_stacks().expect("stacks");
        assert!(stacks.iter().any(|s| s.id == "rust"));
        assert!(stacks.iter().any(|s| s.group == "Frontend"));
    }

    #[test]
    fn rejects_path_traversal_stack_id() {
        let err = load_stack_files("../etc").unwrap_err();
        assert_eq!(err.code, "repo.invalid_template");
    }

    #[test]
    fn all_none_yields_empty_seed() {
        let files = assemble_seed_files(&None, &None, &Some("none".into())).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn rust_plus_mit_seeds_files() {
        let files = assemble_seed_files(
            &Some("rust".into()),
            &Some("MIT".into()),
            &Some("Rust".into()),
        )
        .unwrap();
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"Cargo.toml"));
        assert!(paths.contains(&"LICENSE"));
        assert!(paths.contains(&".gitignore"));
    }

    #[test]
    fn stack_alone_seeds_default_gitignore() {
        let files = assemble_seed_files(&Some("rust".into()), &None, &None).unwrap();
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&".gitignore"));
        assert!(paths.contains(&"Cargo.toml"));
    }

    #[test]
    fn explicit_none_skips_stack_default_gitignore() {
        // Explicit "none" skips the catalog default_gitignore overlay. Official-style
        // packs may still include their own .gitignore from the upstream template.
        let with_default = assemble_seed_files(&Some("go".into()), &None, &None).unwrap();
        let with_none =
            assemble_seed_files(&Some("go".into()), &None, &Some("none".into())).unwrap();
        let paths_default: Vec<_> = with_default.iter().map(|(p, _)| p.as_str()).collect();
        let paths_none: Vec<_> = with_none.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths_default.contains(&"go.mod") || paths_default.contains(&"main.go"));
        assert!(paths_none.contains(&"go.mod") || paths_none.contains(&"main.go"));
        // Catalog overlay path: when the pack has no vendored .gitignore, none clears it.
        let pack_has_gitignore = ASSETS.get_file("stack-presets/go/.gitignore").is_some();
        if !pack_has_gitignore {
            assert!(paths_default.contains(&".gitignore"));
            assert!(!paths_none.contains(&".gitignore"));
        }
    }

    #[test]
    fn stacks_with_defaults_point_at_known_gitignores() {
        let known: std::collections::HashSet<_> = list_gitignores()
            .unwrap()
            .into_iter()
            .map(|g| g.id)
            .collect();
        for stack in list_stacks().unwrap() {
            if let Some(gi) = stack.default_gitignore {
                assert!(
                    known.contains(&gi),
                    "stack {} default_gitignore {} missing from catalog",
                    stack.id,
                    gi
                );
            }
        }
    }

    #[test]
    fn catalog_entries_have_refresh_metadata() {
        let file = ASSETS
            .get_file("stack-presets/catalog.json")
            .expect("catalog");
        let catalog: StackCatalog = serde_json::from_slice(file.contents()).unwrap();
        for p in &catalog.packs {
            assert!(
                p.source.as_ref().map(|s| !s.is_empty()).unwrap_or(false),
                "pack {} missing source",
                p.id
            );
            assert!(
                p.source_ref
                    .as_ref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
                "pack {} missing source_ref",
                p.id
            );
            assert!(
                p.last_synced
                    .as_ref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
                "pack {} missing last_synced",
                p.id
            );
        }
    }

    #[test]
    fn vite_vanilla_ts_seeds_multi_file_tree() {
        let files =
            assemble_seed_files(&Some("vite-vanilla-ts".into()), &None, &Some("none".into()))
                .unwrap();
        let paths: Vec<_> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"package.json"));
        assert!(paths.contains(&"index.html"));
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with("main.ts") || *p == "src/main.ts"),
            "expected main.ts in seed: {paths:?}"
        );
        assert!(
            files.len() >= 5,
            "expected multi-file pack, got {}",
            files.len()
        );
    }
}
