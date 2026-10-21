//! `release.*` RPC handlers (#162); `release::asset_files` (served by the asset-download routes) stays in core.

mod dispatch;
mod handlers;

pub use dispatch::dispatch;

/// Procedures this crate owns — aggregated by the shell's `PROCEDURES`.
pub const PROCEDURES: &[&str] = &[
    "release.create",
    "release.delete",
    "release.deleteAsset",
    "release.get",
    "release.list",
    "release.update",
];
