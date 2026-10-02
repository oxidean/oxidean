//! `repo_disk_usage` — apparent-size walk with hardlink dedup (GIT-25).

use oxidean_git::repo_disk_usage;

#[tokio::test]
async fn repo_disk_usage_sums_files_and_dedups_hardlinks() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("x.git");
    std::fs::create_dir_all(repo.join("objects/pack")).unwrap();
    std::fs::write(repo.join("objects/pack/a.pack"), vec![0u8; 1000]).unwrap();
    std::fs::write(repo.join("HEAD"), b"ref: refs/heads/main\n").unwrap(); // 21 B

    let used = repo_disk_usage(&repo).await.unwrap();
    assert_eq!(used, 1021, "files only; dirs/symlinks not counted");

    #[cfg(unix)]
    {
        // Hardlinked inode counted once even though the dirent is separate.
        std::fs::hard_link(
            repo.join("objects/pack/a.pack"),
            repo.join("objects/pack/a2.pack"),
        )
        .unwrap();
        let used2 = repo_disk_usage(&repo).await.unwrap();
        assert_eq!(used2, 1021, "hardlink dedup by (dev, ino)");
    }
}

#[tokio::test]
async fn repo_disk_usage_missing_dir_errors() {
    let dir = tempfile::tempdir().unwrap();
    let err = repo_disk_usage(&dir.path().join("nope.git"))
        .await
        .expect_err("missing dir must error");
    let _ = err; // GitError::Io — message not asserted (platform-dependent)
}

#[tokio::test]
async fn repo_disk_usage_does_not_traverse_symlinked_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("x.git");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("big.bin"), vec![0u8; 4096]).unwrap();
    std::fs::write(repo.join("HEAD"), b"ref: refs/heads/main\n").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, repo.join("outside-link")).unwrap();
        let used = repo_disk_usage(&repo).await.unwrap();
        // HEAD (21) + the symlink's own dirent size — never the 4 KiB target.
        assert!(used < 1021, "symlinked dir must not be traversed: {used}");
    }
}
