//! Release-asset file removal shared by the RPC handlers (`release.delete`,
//! `release.deleteAsset`). Lives in core because the routes-layer edge module
//! owns upload/download, while deletion is driven from the domain handlers.

use std::path::Path;

use crate::rpc::RpcCtx;

/// Opaque on-disk path: `{release_assets_dir}/{asset_id}` (D-REL-04).
pub fn asset_fs_path(release_assets_dir: &Path, asset_id: &str) -> std::path::PathBuf {
    release_assets_dir.join(asset_id)
}

/// Remove the on-disk file for an asset id; missing files are fine.
pub async fn remove_asset_file(release_assets_dir: &Path, asset_id: &str) {
    let path = asset_fs_path(release_assets_dir, asset_id);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(error = %e, path = %path.display(), "remove release asset file"),
    }
}

/// Helper for RPC deleteAsset / release.delete — remove DB row + file.
pub async fn delete_asset_with_file(
    ctx: &RpcCtx,
    release_assets_dir: &Path,
    asset_id: &str,
) -> Result<(), oxidean_core::AppError> {
    ctx.db.delete_release_asset(asset_id).await.map_err(|e| {
        tracing::error!(error = %e, "delete_release_asset");
        oxidean_core::AppError::new("release.internal", "could not delete asset")
    })?;
    remove_asset_file(release_assets_dir, asset_id).await;
    Ok(())
}

pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("asset")
        .trim();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        "asset".into()
    } else {
        cleaned
    }
}
