//! In-process background jobs: orphan reconcile + scheduled git gc (D-36 / D-37) + LFS GC.

mod lfs_gc;
mod reconcile;
mod schedule;

pub use lfs_gc::{run_lfs_gc, wipe_lfs_dir_contents, LfsGcStats};
pub use reconcile::{
    delete_under_repos_dir, orphan_reconcile, orphan_reconcile_with_retention, run_gc_all,
    run_gc_one, soft_delete_retention_days,
};
pub use schedule::{spawn_background_jobs, JobConfig};
