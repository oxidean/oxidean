//! Own test binary so `set_current_dir` stays process-local: mutating the
//! process cwd inside the shared `cli.rs` unit-test binary races sibling
//! tests' spawned `git` processes under plain `cargo test` (they inherit the
//! moved/deleted cwd and fail with `getcwd` / "must be run in a work tree").

use oxidean_git::{CliGitBackend, GitBackend};
use std::path::PathBuf;

#[tokio::test]
async fn seed_commit_works_with_relative_bare_path() {
    let root = tempfile::tempdir().unwrap();
    let prev = std::env::current_dir().unwrap();
    std::env::set_current_dir(root.path()).unwrap();
    let bare = PathBuf::from("relative-bare.git");
    let git = CliGitBackend::new();
    let result = async {
        git.init_bare(&bare, "main").await?;
        git.seed_commit(
            &bare,
            "main",
            "seed",
            &[("README.md".into(), b"# hi\n".to_vec())],
        )
        .await
    }
    .await;
    let _ = std::env::set_current_dir(&prev);
    result.expect("relative bare seed");
    assert!(root.path().join("relative-bare.git").join("HEAD").exists());
}
