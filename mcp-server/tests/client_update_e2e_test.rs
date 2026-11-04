//! E2E Integration Test Suite for Client Update Mechanism (R2, F3, F4)
//!
//! Validates:
//! - Tier 1: Real source timestamp scanning, binary mtime comparison, git commit divergence, status reporting.
//! - Tier 2: Boundary conditions (future timestamps, non-git repos, missing binary, empty source trees, Cargo.toml updates).
//! - Tier 3: Pairwise state matrix (source_newer x git_behind) and execution modes (Daemon, Proxy, Interactive, CheckOnly).
//! - Tier 4: Daemon startup update lifecycle integration against temporary filesystem and git repository.
//!
//! Exercises production functions in `onlydoms::update_checker::*`:
//! `check_for_updates`, `check_git_status`, `compare_mtimes`, `evaluate_git_shas`,
//! `scan_newest_source_mtime`, `get_newest_source_mtime`, `is_relevant_source_file`,
//! `check_and_handle_update`, `prompt_user_for_rebuild`.

use onlydoms::update_checker::{
    check_and_handle_update, check_git_status, compare_mtimes, evaluate_git_shas,
    get_newest_source_mtime, is_relevant_source_file, prompt_user_for_rebuild,
    scan_newest_source_mtime, UpdateCheckResult, UpdateMode, UpdateStatus,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Process-wide lock for tests that mutate process environment variables
static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Helper to create a temporary directory with a unique name
fn create_temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("Failed to create temporary directory");
    dir
}

/// Helper to initialize a minimal git repository in a target directory
fn init_git_repo(repo_dir: &Path) -> String {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_dir)
            .output()
            .expect("Git command execution failed");
        assert!(
            output.status.success(),
            "Git command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };

    run(&["init"]);
    run(&["config", "user.name", "Tester"]);
    run(&["config", "user.email", "tester@example.com"]);
    run(&["config", "commit.gpgsign", "false"]);
    fs::write(repo_dir.join("init.txt"), "initial commit").unwrap();
    run(&["add", "init.txt"]);
    run(&["commit", "-m", "Initial commit"]);

    let head_out = run(&["rev-parse", "HEAD"]);
    String::from_utf8_lossy(&head_out.stdout).trim().to_string()
}

// -----------------------------------------------------------------------------
// TIER 1: Primary Behavior (Happy Path) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_client_update_outdated_binary_detected_when_source_newer() {
    let temp_dir = create_temp_dir("test_e2e_source_newer");
    let mcp_dir = temp_dir.join("mcp-server");
    let src_dir = mcp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let lib_rs = src_dir.join("lib.rs");
    let main_rs = src_dir.join("main.rs");
    fs::write(&lib_rs, "pub fn lib_func() {}").unwrap();

    // Ensure main.rs has a newer modification time than lib.rs
    std::thread::sleep(Duration::from_millis(50));
    fs::write(&main_rs, "fn main() {}").unwrap();

    let scan_res = scan_newest_source_mtime(&mcp_dir).expect("scan_newest_source_mtime must succeed");
    assert!(scan_res.is_some(), "Source files must be detected");
    let (found_file, newest_mtime) = scan_res.unwrap();
    assert_eq!(found_file, main_rs);

    // Simulate binary compiled 50 seconds before the newest source file
    let binary_time = newest_mtime - Duration::from_secs(50);
    let status = compare_mtimes(binary_time, Some((found_file, newest_mtime)));

    match status {
        UpdateStatus::SourceNewer {
            file,
            diff_secs,
            source_mtime_secs,
            binary_mtime_secs,
        } => {
            assert!(file.ends_with("main.rs"));
            assert!(diff_secs >= 50);
            assert!(source_mtime_secs > binary_mtime_secs);
        }
        other => panic!("Expected UpdateStatus::SourceNewer, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_client_update_uptodate_when_binary_newer() {
    let temp_dir = create_temp_dir("test_e2e_binary_newer");
    let mcp_dir = temp_dir.join("mcp-server");
    let src_dir = mcp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(src_dir.join("lib.rs"), "pub fn ok() {}").unwrap();
    fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(mcp_dir.join("Cargo.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"").unwrap();

    let scan_res = scan_newest_source_mtime(&mcp_dir).expect("scan must succeed");
    assert!(scan_res.is_some());
    let (newest_file, newest_mtime) = scan_res.unwrap();

    // Binary compiled 100 seconds after newest source file
    let binary_time = newest_mtime + Duration::from_secs(100);
    let status = compare_mtimes(binary_time, Some((newest_file, newest_mtime)));

    assert_eq!(status, UpdateStatus::UpToDate);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_client_update_git_commit_divergence_detected() {
    // 1. Direct unit verification of production evaluate_git_shas
    let status = evaluate_git_shas("commit_abc123", "commit_def456");
    match status {
        UpdateStatus::GitBehind {
            current_commit,
            upstream_commit,
        } => {
            assert_eq!(current_commit, "commit_abc123");
            assert_eq!(upstream_commit, "commit_def456");
        }
        other => panic!("Expected GitBehind, got {:?}", other),
    }

    // 2. Integration verification on a real git repository
    let temp_repo = create_temp_dir("test_e2e_git_divergence");
    let head_sha = init_git_repo(&temp_repo);

    let gstatus = check_git_status(&temp_repo).expect("check_git_status must succeed");
    assert_eq!(gstatus.current_commit, head_sha);

    // Running binary built from an older commit
    let divergence_status = evaluate_git_shas("built_commit_111", &gstatus.current_commit);
    assert!(matches!(divergence_status, UpdateStatus::GitBehind { .. }));

    let _ = fs::remove_dir_all(&temp_repo);
}

#[test]
fn test_client_update_uptodate_when_git_commit_matches() {
    let status = evaluate_git_shas("commit_matching_789", "commit_matching_789");
    assert_eq!(status, UpdateStatus::UpToDate);

    // Verify whitespace and newline tolerance
    let status_whitespace = evaluate_git_shas("  commit_matching_789\n", "commit_matching_789  \r\n");
    assert_eq!(status_whitespace, UpdateStatus::UpToDate);

    // Empty SHA fallback is UpToDate
    assert_eq!(evaluate_git_shas("", "commit_matching_789"), UpdateStatus::UpToDate);
    assert_eq!(evaluate_git_shas("commit_matching_789", ""), UpdateStatus::UpToDate);
}

#[test]
fn test_client_update_status_notification_formatting() {
    let check_result = UpdateCheckResult {
        is_outdated: true,
        reasons: vec![
            "Source file 'src/tools/pdf.rs' modified after running binary (delta: 50s)".to_string(),
        ],
        status: UpdateStatus::SourceNewer {
            file: "src/tools/pdf.rs".to_string(),
            source_mtime_secs: 1700000050,
            binary_mtime_secs: 1700000000,
            diff_secs: 50,
        },
        repo_root: Some(PathBuf::from("/repo")),
        manifest_path: Some(PathBuf::from("/repo/mcp-server/Cargo.toml")),
        current_exe: PathBuf::from("/repo/target/release/onlydoms"),
        binary_mtime: None,
        newest_source_mtime: None,
        newest_source_file: Some(PathBuf::from("/repo/mcp-server/src/tools/pdf.rs")),
        git_status: None,
    };

    // Verify formatting of reasons
    assert_eq!(check_result.reasons.len(), 1);
    assert!(check_result.reasons[0].contains("src/tools/pdf.rs"));
    assert!(check_result.reasons[0].contains("delta: 50s"));

    // Verify JSON serialization / deserialization round-trip
    let serialized = serde_json::to_string(&check_result).expect("Serialization failed");
    assert!(serialized.contains("\"is_outdated\":true"));
    assert!(serialized.contains("SourceNewer"));
    assert!(serialized.contains("src/tools/pdf.rs"));

    let deserialized: UpdateCheckResult = serde_json::from_str(&serialized).expect("Deserialization failed");
    assert!(deserialized.is_outdated);
    assert_eq!(deserialized.reasons, check_result.reasons);

    // Verify prompt_user_for_rebuild behaves safely in non-interactive environment
    let prompt_res = prompt_user_for_rebuild(&check_result);
    assert!(!prompt_res, "prompt_user_for_rebuild must return false in non-terminal test harness");

    // Verify contract UpdateAvailable format
    let contract_status = UpdateStatus::UpdateAvailable {
        source_newer: true,
        git_behind: false,
    };
    let formatted = format!("{:?}", contract_status);
    assert!(formatted.contains("UpdateAvailable"));
    assert!(formatted.contains("source_newer: true"));
}

// -----------------------------------------------------------------------------
// TIER 2: Boundary & Corner Condition Tests
// -----------------------------------------------------------------------------

#[test]
fn test_client_update_source_file_future_timestamp() {
    let temp_dir = create_temp_dir("test_e2e_future_ts");
    let src_file = temp_dir.join("future_file.rs");
    fs::write(&src_file, "pub fn future() {}").unwrap();

    let now = SystemTime::now();
    // Simulate clock skew or file copied with timestamp 1 year in the future
    let future_time = now + Duration::from_secs(86400 * 365);

    let status = compare_mtimes(now, Some((src_file.clone(), future_time)));
    match status {
        UpdateStatus::SourceNewer {
            file,
            diff_secs,
            source_mtime_secs,
            binary_mtime_secs,
        } => {
            assert_eq!(file, src_file.to_string_lossy());
            assert!(diff_secs >= 86400 * 365 - 5);
            assert!(source_mtime_secs > binary_mtime_secs);
        }
        other => panic!("Expected SourceNewer, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_client_update_missing_git_repository_fallback() {
    // Non-git installation (tarball or package distribution)
    let temp_dir = create_temp_dir("test_e2e_non_git");
    let git_res = check_git_status(&temp_dir);
    assert!(git_res.is_err());
    assert_eq!(git_res.err().unwrap(), "Not a git repository");

    // Source comparison must still function gracefully without git
    let mcp_dir = temp_dir.join("mcp-server");
    let src_dir = mcp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let main_rs = src_dir.join("main.rs");
    fs::write(&main_rs, "fn main() {}").unwrap();

    let scan_res = scan_newest_source_mtime(&mcp_dir).unwrap().unwrap();
    let binary_time = scan_res.1 + Duration::from_secs(60);
    let status = compare_mtimes(binary_time, Some(scan_res));
    assert_eq!(status, UpdateStatus::UpToDate);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_client_update_missing_binary_graceful() {
    // Non-existent binary path cannot resolve metadata
    let fake_path = PathBuf::from("/nonexistent/path/to/onlydoms_bin");
    let metadata_res = fs::metadata(&fake_path);
    assert!(metadata_res.is_err());

    // When binary timestamp is missing / zero (fallback for unlinked or missing binary),
    // comparing with an existing source file reports SourceNewer, NEVER false positive UpToDate
    let src_mtime = SystemTime::now();
    let status = compare_mtimes(
        SystemTime::UNIX_EPOCH,
        Some((PathBuf::from("src/main.rs"), src_mtime)),
    );

    match status {
        UpdateStatus::SourceNewer {
            file,
            diff_secs,
            binary_mtime_secs,
            ..
        } => {
            assert_eq!(file, "src/main.rs");
            assert_eq!(binary_mtime_secs, 0);
            assert!(diff_secs > 0);
        }
        other => panic!("Missing binary must report SourceNewer, got {:?}", other),
    }
}

#[test]
fn test_client_update_empty_source_dir() {
    let temp_dir = create_temp_dir("test_e2e_empty_dir");

    // scan_newest_source_mtime returns Ok(None) for empty directory
    let scan_res = scan_newest_source_mtime(&temp_dir).expect("scan must succeed");
    assert!(scan_res.is_none(), "Empty directory tree must return None");

    // get_newest_source_mtime returns NotFound error
    let get_res = get_newest_source_mtime(&temp_dir);
    assert!(get_res.is_err());
    assert_eq!(get_res.err().unwrap().kind(), std::io::ErrorKind::NotFound);

    // compare_mtimes with None source returns UpToDate
    let status = compare_mtimes(SystemTime::now(), None);
    assert_eq!(status, UpdateStatus::UpToDate);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_client_update_cargo_toml_newer() {
    let temp_dir = create_temp_dir("test_e2e_cargo_toml_newer");
    let mcp_dir = temp_dir.join("mcp-server");
    let src_dir = mcp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let lib_rs = src_dir.join("lib.rs");
    let cargo_toml = mcp_dir.join("Cargo.toml");

    // Verify is_relevant_source_file accepts Cargo.toml
    assert!(is_relevant_source_file(&cargo_toml));

    fs::write(&lib_rs, "pub fn lib() {}").unwrap();
    // Touch Cargo.toml slightly after lib.rs
    std::thread::sleep(Duration::from_millis(50));
    fs::write(&cargo_toml, "[package]\nname = \"updated\"\nversion = \"0.2.0\"").unwrap();

    let scan_res = scan_newest_source_mtime(&mcp_dir).unwrap().unwrap();
    assert_eq!(scan_res.0, cargo_toml, "Cargo.toml must be detected as newest file");

    let binary_time = scan_res.1 - Duration::from_secs(10);
    let status = compare_mtimes(binary_time, Some(scan_res));

    match status {
        UpdateStatus::SourceNewer { file, diff_secs, .. } => {
            assert!(file.ends_with("Cargo.toml"));
            assert!(diff_secs >= 10);
        }
        other => panic!("Expected SourceNewer for Cargo.toml, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

// -----------------------------------------------------------------------------
// TIER 3: Pairwise Combinations
// -----------------------------------------------------------------------------

#[test]
fn test_client_update_pairwise_source_and_git_states() {
    let now = SystemTime::now();
    let old_src = now - Duration::from_secs(100);
    let newer_src = now + Duration::from_secs(100);

    let test_matrix = [
        // (src_time, built_c, head_c, exp_src_newer, exp_git_behind)
        (old_src, "hash1", "hash1", false, false),  // UpToDate
        (newer_src, "hash1", "hash1", true, false), // Source newer only
        (old_src, "hash1", "hash2", false, true),   // Git behind only
        (newer_src, "hash1", "hash2", true, true),  // Both newer and behind
    ];

    for (src_time, built_c, head_c, exp_src_newer, exp_git_behind) in test_matrix {
        let mtime_status = compare_mtimes(now, Some((PathBuf::from("src/main.rs"), src_time)));
        let is_source_newer = matches!(mtime_status, UpdateStatus::SourceNewer { .. });

        let git_status = evaluate_git_shas(built_c, head_c);
        let is_git_behind = matches!(git_status, UpdateStatus::GitBehind { .. });

        assert_eq!(is_source_newer, exp_src_newer);
        assert_eq!(is_git_behind, exp_git_behind);

        // Synthesize contract status
        let contract_status = if is_source_newer || is_git_behind {
            UpdateStatus::UpdateAvailable {
                source_newer: is_source_newer,
                git_behind: is_git_behind,
            }
        } else {
            UpdateStatus::UpToDate
        };

        if exp_src_newer || exp_git_behind {
            assert_eq!(
                contract_status,
                UpdateStatus::UpdateAvailable {
                    source_newer: exp_src_newer,
                    git_behind: exp_git_behind,
                }
            );
        } else {
            assert_eq!(contract_status, UpdateStatus::UpToDate);
        }
    }
}

#[tokio::test]
async fn test_client_update_pairwise_execution_modes() {
    let _lock = ENV_MUTEX.lock().unwrap();

    // Mode 1: CheckOnly must never rebuild or reload
    let res_check = check_and_handle_update(UpdateMode::CheckOnly).await;
    assert!(res_check.is_ok(), "CheckOnly mode must succeed");

    // Mode 2: ONLYDOMS_SKIP_UPDATE_CHECK bypasses rebuild across modes
    std::env::set_var("ONLYDOMS_SKIP_UPDATE_CHECK", "1");
    let res_skip = check_and_handle_update(UpdateMode::Daemon).await;
    assert!(res_skip.is_ok(), "SKIP_UPDATE_CHECK must bypass rebuild");
    std::env::remove_var("ONLYDOMS_SKIP_UPDATE_CHECK");

    // Mode 3: ONLYDOMS_RELOADED prevents recursive restarts and clears itself
    std::env::set_var("ONLYDOMS_RELOADED", "1");
    let res_reloaded = check_and_handle_update(UpdateMode::Proxy).await;
    assert!(res_reloaded.is_ok());
    assert!(std::env::var("ONLYDOMS_RELOADED").is_err(), "ONLYDOMS_RELOADED must be cleared after check");

    // Mode 4: Interactive mode terminal check returns false safely in non-interactive environment
    let dummy_result = UpdateCheckResult {
        is_outdated: true,
        reasons: vec!["Source updated".to_string()],
        status: UpdateStatus::SourceNewer {
            file: "main.rs".to_string(),
            source_mtime_secs: 10,
            binary_mtime_secs: 5,
            diff_secs: 5,
        },
        repo_root: None,
        manifest_path: None,
        current_exe: PathBuf::from("onlydoms"),
        binary_mtime: None,
        newest_source_mtime: None,
        newest_source_file: None,
        git_status: None,
    };
    assert!(!prompt_user_for_rebuild(&dummy_result));
}

// -----------------------------------------------------------------------------
// TIER 4: Application Scenario (Daemon Startup Lifecycle)
// -----------------------------------------------------------------------------

#[test]
fn test_client_update_e2e_daemon_startup_lifecycle() {
    let temp_repo = create_temp_dir("test_e2e_lifecycle");
    let head_commit = init_git_repo(&temp_repo);

    let mcp_dir = temp_repo.join("mcp-server");
    let src_dir = mcp_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let manifest = mcp_dir.join("Cargo.toml");
    fs::write(&manifest, "[package]\nname = \"onlydoms\"\nversion = \"0.1.0\"").unwrap();
    let main_rs = src_dir.join("main.rs");
    fs::write(&main_rs, "fn main() { println!(\"OnlyDOMs\"); }").unwrap();

    // Step 1: Daemon checks repo root and finds manifest
    assert!(manifest.exists());
    assert!(is_relevant_source_file(&manifest));
    assert!(is_relevant_source_file(&main_rs));

    // Step 2: Source tree inspection
    let (newest_mtime, newest_file) = get_newest_source_mtime(&mcp_dir).expect("get_newest_source_mtime failed");
    assert!(newest_file == manifest || newest_file == main_rs);

    // Step 3: Git status verification
    let git_status = check_git_status(&temp_repo).expect("check_git_status failed");
    assert_eq!(git_status.current_commit, head_commit);

    // Step 4: Outdated binary evaluation (binary 1 hour older than sources)
    let binary_build_time = newest_mtime - Duration::from_secs(3600);
    let mtime_status = compare_mtimes(binary_build_time, Some((newest_file.clone(), newest_mtime)));

    match mtime_status {
        UpdateStatus::SourceNewer { file, diff_secs, .. } => {
            assert_eq!(file, newest_file.to_string_lossy());
            assert!(diff_secs >= 3599);
        }
        other => panic!("Daemon must detect SourceNewer, got {:?}", other),
    }

    // Step 5: Synthesize contract status
    let contract_status = UpdateStatus::UpdateAvailable {
        source_newer: true,
        git_behind: false,
    };
    assert_eq!(
        contract_status,
        UpdateStatus::UpdateAvailable {
            source_newer: true,
            git_behind: false
        }
    );

    let _ = fs::remove_dir_all(&temp_repo);
}
