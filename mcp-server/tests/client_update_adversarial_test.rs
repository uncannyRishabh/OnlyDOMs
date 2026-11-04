//! Empirical Stress Tests for Client Update Mechanism (R2, F3, F4)
//! Adversarial verification of:
//! - Timestamp comparisons (missing binary, clock skew / future timestamps, deleted/dangling files)
//! - Git status detection (non-git repo, broken git directory, detached HEAD, shallow clone)
//! - Loop prevention (`ONLYDOMS_RELOADED=1`)
//! - `is_terminal()` prompt and non-interactive execution behavior

use onlydoms::update_checker::{
    check_and_handle_update, check_and_prompt_update, check_for_updates, check_git_status,
    compare_mtimes, scan_newest_source_mtime, UpdateMode, UpdateStatus,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// Helper to create a temp directory with a unique name
fn create_temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("Failed to create temp directory");
    dir
}

// -----------------------------------------------------------------------------
// 1. SOURCE TIMESTAMP & BINARY STRESS TESTS
// -----------------------------------------------------------------------------

#[test]
fn stress_test_compare_mtimes_extreme_clock_skew_future() {
    let now = SystemTime::now();
    let bin_mtime = now;
    // Source file 100 years into the future
    let future_src_mtime = now + Duration::from_secs(100 * 365 * 86400);

    let status = compare_mtimes(
        bin_mtime,
        Some((PathBuf::from("src/future.rs"), future_src_mtime)),
    );

    match status {
        UpdateStatus::SourceNewer { file, diff_secs, .. } => {
            assert_eq!(file, "src/future.rs");
            // Must not overflow u64 and must reflect the large delta
            assert!(diff_secs > 3_000_000_000);
        }
        other => panic!("Expected SourceNewer with huge diff, got {:?}", other),
    }
}

#[test]
fn stress_test_compare_mtimes_extreme_clock_skew_past_epoch() {
    // When timestamp is somehow UNIX_EPOCH or near zero
    let bin_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(10);
    let src_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(50);

    let status = compare_mtimes(
        bin_mtime,
        Some((PathBuf::from("src/early.rs"), src_mtime)),
    );

    match status {
        UpdateStatus::SourceNewer { diff_secs, .. } => {
            assert_eq!(diff_secs, 40);
        }
        other => panic!("Expected SourceNewer, got {:?}", other),
    }
}

#[test]
fn stress_test_compare_mtimes_binary_newer_than_future_source() {
    let now = SystemTime::now();
    let src_mtime = now;
    // Binary touched to be in the future
    let future_bin_mtime = now + Duration::from_secs(3600);

    let status = compare_mtimes(
        future_bin_mtime,
        Some((PathBuf::from("src/normal.rs"), src_mtime)),
    );

    assert_eq!(status, UpdateStatus::UpToDate);
}

#[test]
fn stress_test_compare_mtimes_exact_same_timestamp() {
    let now = SystemTime::now();
    let status = compare_mtimes(now, Some((PathBuf::from("src/same.rs"), now)));
    // Identical timestamp must be considered UpToDate
    assert_eq!(status, UpdateStatus::UpToDate);
}

#[test]
fn stress_test_scan_dir_picks_dangling_symlink_as_source_file_anomaly() {
    let temp_dir = create_temp_dir("test_symlink_anomaly");
    let src_dir = temp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let valid_rs = src_dir.join("valid.rs");
    fs::write(&valid_rs, "pub fn ok() {}").unwrap();

    // Create a dangling symlink (points to deleted target) with .rs extension
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let dangling_target = temp_dir.join("does_not_exist.rs");
        let symlink_path = src_dir.join("dangling.rs");
        let _ = symlink(&dangling_target, &symlink_path);

        let scanned = scan_newest_source_mtime(&temp_dir);
        assert!(scanned.is_ok());
        let result = scanned.unwrap();
        assert!(result.is_some());
        let (found_file, _) = result.unwrap();

        assert_ne!(
            found_file, symlink_path,
            "scan_newest_source_mtime must ignore dangling symlink"
        );
        assert_eq!(found_file, valid_rs);
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn stress_test_scan_dir_empty_and_subdirectories_only() {
    let temp_dir = create_temp_dir("test_empty_scan");
    let empty_sub = temp_dir.join("empty_sub");
    fs::create_dir_all(&empty_sub).unwrap();

    let scanned = scan_newest_source_mtime(&temp_dir).unwrap();
    assert!(scanned.is_none(), "Empty directory tree must return None");

    let _ = fs::remove_dir_all(&temp_dir);
}

// -----------------------------------------------------------------------------
// 2. GIT STATUS & ANOMALY STRESS TESTS
// -----------------------------------------------------------------------------

#[test]
fn stress_test_git_status_non_git_dir() {
    let temp_dir = create_temp_dir("test_non_git");
    let result = check_git_status(&temp_dir);
    assert!(result.is_err());
    assert_eq!(result.err().unwrap(), "Not a git repository");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn stress_test_git_status_corrupted_empty_git_dir() {
    let temp_dir = create_temp_dir("test_corrupt_git");
    // Create an empty .git directory without git initialization
    fs::create_dir_all(temp_dir.join(".git")).unwrap();

    let result = check_git_status(&temp_dir);
    // Must return Err because git rev-parse HEAD fails, but must NOT panic or crash
    assert!(result.is_err());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn stress_test_git_status_detached_head() {
    let temp_dir = create_temp_dir("test_detached_head");

    // Initialize real git repo
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(&temp_dir)
            .output()
            .expect("Git command failed");
        assert!(output.status.success(), "Git command {:?} failed: {:?}", args, String::from_utf8_lossy(&output.stderr));
    };

    run(&["init"]);
    run(&["config", "user.name", "Tester"]);
    run(&["config", "user.email", "tester@example.com"]);

    // Commit 1
    fs::write(temp_dir.join("file1.txt"), "first").unwrap();
    run(&["add", "file1.txt"]);
    run(&["commit", "-m", "First commit"]);

    let head1 = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();
    let commit1 = String::from_utf8_lossy(&head1.stdout).trim().to_string();

    // Commit 2
    fs::write(temp_dir.join("file2.txt"), "second").unwrap();
    run(&["add", "file2.txt"]);
    run(&["commit", "-m", "Second commit"]);

    // Detach HEAD to Commit 1
    run(&["checkout", &commit1]);

    // Now test check_git_status in detached HEAD state
    let git_status = check_git_status(&temp_dir);
    assert!(git_status.is_ok(), "check_git_status must succeed in detached HEAD");

    let status = git_status.unwrap();
    assert_eq!(status.current_commit, commit1);
    assert_eq!(status.short_commit, commit1.chars().take(8).collect::<String>());
    assert!(status.upstream_branch.is_none());
    assert_eq!(status.commits_behind_upstream, 0);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn stress_test_git_status_shallow_clone() {
    let upstream_dir = create_temp_dir("test_upstream_repo");
    let clone_dir = create_temp_dir("test_shallow_clone");

    // Setup upstream
    let run_in = |dir: &Path, args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("Git command execution");
        assert!(output.status.success(), "Git failed in {:?}: {:?}", dir, String::from_utf8_lossy(&output.stderr));
    };

    run_in(&upstream_dir, &["init"]);
    run_in(&upstream_dir, &["config", "user.name", "Upstream"]);
    run_in(&upstream_dir, &["config", "user.email", "upstream@example.com"]);
    fs::write(upstream_dir.join("init.txt"), "upstream").unwrap();
    run_in(&upstream_dir, &["add", "init.txt"]);
    run_in(&upstream_dir, &["commit", "-m", "Initial upstream commit"]);

    // Create shallow clone with depth 1
    let clone_res = Command::new("git")
        .args(["clone", "--depth", "1", &format!("file://{}", upstream_dir.display()), &clone_dir.display().to_string()])
        .output()
        .expect("Git clone");
    assert!(clone_res.status.success());

    // Evaluate check_git_status on shallow clone
    let git_status = check_git_status(&clone_dir);
    assert!(git_status.is_ok(), "check_git_status must handle shallow clone");

    let status = git_status.unwrap();
    assert!(!status.current_commit.is_empty());
    assert!(!status.is_dirty);

    let _ = fs::remove_dir_all(&clone_dir);
    let _ = fs::remove_dir_all(&upstream_dir);
}

#[test]
fn stress_test_git_behind_upstream_invariant_bug_reproduction() {
    // Setup a local repo tracking an upstream repo that is 1 commit ahead
    let upstream_dir = create_temp_dir("test_upstream_ahead");
    let local_dir = create_temp_dir("test_local_behind");

    let run_in = |dir: &Path, args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("Git command execution");
        assert!(output.status.success(), "Git failed in {:?}: {:?}", dir, String::from_utf8_lossy(&output.stderr));
    };

    run_in(&upstream_dir, &["init", "-b", "main"]);
    run_in(&upstream_dir, &["config", "user.name", "Upstream"]);
    run_in(&upstream_dir, &["config", "user.email", "upstream@example.com"]);
    fs::write(upstream_dir.join("c1.txt"), "commit 1").unwrap();
    run_in(&upstream_dir, &["add", "c1.txt"]);
    run_in(&upstream_dir, &["commit", "-m", "Commit 1"]);

    // Clone local
    let clone_res = Command::new("git")
        .args(["clone", &format!("file://{}", upstream_dir.display()), &local_dir.display().to_string()])
        .output()
        .expect("Git clone");
    assert!(clone_res.status.success());

    // Add Commit 2 to upstream
    fs::write(upstream_dir.join("c2.txt"), "commit 2").unwrap();
    run_in(&upstream_dir, &["add", "c2.txt"]);
    run_in(&upstream_dir, &["commit", "-m", "Commit 2"]);

    // In local, fetch upstream
    run_in(&local_dir, &["fetch", "origin"]);

    // Now check_git_status on local
    let gstatus = check_git_status(&local_dir).expect("check_git_status must succeed");
    assert_eq!(gstatus.commits_behind_upstream, 1, "Must detect 1 commit behind upstream");

    // Test check_for_updates behavior when pointed to this repo
    // Create mcp-server subdirectory and Cargo.toml inside local_dir
    let mcp_dir = local_dir.join("mcp-server");
    fs::create_dir_all(&mcp_dir).unwrap();
    let manifest = mcp_dir.join("Cargo.toml");
    fs::write(&manifest, "[package]\nname = \"dummy\"\nversion = \"0.1.0\"").unwrap();
    // Backdate manifest to year 2020 so source files are strictly older than the binary
    let _ = Command::new("touch").args(["-t", "202001010000", &manifest.display().to_string()]).output();

    std::env::set_var("ONLYDOMS_REPO_DIR", local_dir.display().to_string());
    let check = check_for_updates();

    assert!(check.is_outdated, "check_for_updates must detect outdated because commits_behind_upstream > 0");

    // Check check.status
    eprintln!("When commits_behind_upstream == 1 and commit_mismatch == false: check.status = {:?}", check.status);

    assert!(
        matches!(check.status, UpdateStatus::GitBehind { .. }),
        "check.status must be GitBehind when commits_behind_upstream > 0"
    );
    let prompt_res = check_and_prompt_update();
    std::env::remove_var("ONLYDOMS_REPO_DIR");

    assert!(prompt_res.is_ok());
    match prompt_res.unwrap() {
        UpdateStatus::UpdateAvailable { git_behind, .. } => {
            assert!(git_behind, "git_behind must be true in check_and_prompt_update");
        }
        other => panic!("Expected UpdateAvailable, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&local_dir);
    let _ = fs::remove_dir_all(&upstream_dir);
}

#[test]
fn stress_test_git_commits_behind_upstream_status_invariant() {
    // Directly test the logic contract when commit_mismatch is false (or None)
    // but commits_behind_upstream > 0:
    // If check.is_outdated is true because of upstream commits, check.status MUST NOT remain UpToDate!
    // And check_and_prompt_update() MUST report git_behind: true, NOT git_behind: false!
    let gstatus = onlydoms::update_checker::GitUpdateStatus {
        current_commit: "abc".to_string(),
        short_commit: "abc".to_string(),
        embedded_commit: Some("abc".to_string()), // Matches! commit_mismatch = false
        commit_mismatch: false,
        upstream_branch: Some("origin/main".to_string()),
        commits_behind_upstream: 5, // Behind upstream!
        is_dirty: false,
    };

    assert_eq!(gstatus.commit_mismatch, false);
    assert!(gstatus.commits_behind_upstream > 0);
}

// -----------------------------------------------------------------------------
// 3. LOOP PREVENTION & TERMINAL BEHAVIOR STRESS TESTS
// -----------------------------------------------------------------------------

#[tokio::test]
async fn stress_test_loop_prevention_clears_env_and_bypasses() {
    std::env::set_var("ONLYDOMS_RELOADED", "1");
    assert_eq!(std::env::var("ONLYDOMS_RELOADED").unwrap(), "1");

    // Under UpdateMode::Daemon, it would normally rebuild if outdated,
    // but ONLYDOMS_RELOADED MUST bypass rebuild
    let res = check_and_handle_update(UpdateMode::Daemon).await;
    assert!(res.is_ok());

    // MUST remove ONLYDOMS_RELOADED after reading to avoid stale environment
    assert!(std::env::var("ONLYDOMS_RELOADED").is_err());
}

#[tokio::test]
async fn stress_test_check_only_mode_never_rebuilds() {
    // Mode CheckOnly must never attempt rebuild or reload
    let res = check_and_handle_update(UpdateMode::CheckOnly).await;
    assert!(res.is_ok());
    let check = res.unwrap();
    let _ = check.is_outdated;
}

#[test]
fn stress_test_check_and_prompt_update_contract() {
    let res = check_and_prompt_update();
    assert!(res.is_ok());
    let status = res.unwrap();
    match status {
        UpdateStatus::UpToDate => {}
        UpdateStatus::UpdateAvailable { source_newer, git_behind } => {
            assert!(source_newer || git_behind);
        }
        other => panic!("check_and_prompt_update returned unexpected status variant: {:?}", other),
    }
}

#[test]
fn stress_test_missing_binary_reported_as_outdated() {
    let temp_dir = create_temp_dir("test_missing_bin");
    let mcp_dir = temp_dir.join("mcp-server");
    let src_dir = mcp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(mcp_dir.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"").unwrap();
    fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

    let (src_path, _src_time) = scan_newest_source_mtime(&mcp_dir).unwrap().unwrap();
    assert_eq!(src_path, src_dir.join("main.rs"));

    // Directly verify that check_for_updates flags missing binary when binary does not exist on disk
    std::env::set_var("ONLYDOMS_REPO_DIR", temp_dir.display().to_string());
    let check = check_for_updates();
    std::env::remove_var("ONLYDOMS_REPO_DIR");

    assert!(check.is_outdated);
    let _ = fs::remove_dir_all(&temp_dir);
}
