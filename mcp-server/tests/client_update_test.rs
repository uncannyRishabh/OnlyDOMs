use onlydoms::update_checker::{
    compare_mtimes, evaluate_git_shas, is_relevant_source_file, scan_newest_source_mtime,
    UpdateCheckResult, UpdateStatus,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[test]
fn test_update_status_enum_invariants() {
    let uptodate = UpdateStatus::UpToDate;
    assert_eq!(uptodate, UpdateStatus::UpToDate);

    let src_newer = UpdateStatus::SourceNewer {
        file: "mcp-server/src/main.rs".to_string(),
        source_mtime_secs: 1000,
        binary_mtime_secs: 800,
        diff_secs: 200,
    };
    if let UpdateStatus::SourceNewer { file, diff_secs, .. } = src_newer {
        assert_eq!(file, "mcp-server/src/main.rs");
        assert_eq!(diff_secs, 200);
    } else {
        panic!("Pattern match failed for SourceNewer");
    }

    let git_behind = UpdateStatus::GitBehind {
        current_commit: "aaa111".to_string(),
        upstream_commit: "bbb222".to_string(),
    };
    if let UpdateStatus::GitBehind { current_commit, upstream_commit } = git_behind {
        assert_eq!(current_commit, "aaa111");
        assert_eq!(upstream_commit, "bbb222");
    } else {
        panic!("Pattern match failed for GitBehind");
    }

    let update_avail = UpdateStatus::UpdateAvailable {
        source_newer: true,
        git_behind: false,
    };
    if let UpdateStatus::UpdateAvailable { source_newer, git_behind } = update_avail {
        assert!(source_newer);
        assert!(!git_behind);
    } else {
        panic!("Pattern match failed for UpdateAvailable");
    }
}

#[test]
fn test_synthetic_filesystem_mtime_detection() {
    let temp_root = std::env::temp_dir().join(format!("test_mtime_detect_{}", uuid::Uuid::new_v4()));
    let src_dir = temp_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let main_rs = src_dir.join("main.rs");
    std::fs::write(&main_rs, "fn main() {}").unwrap();

    // Source modification time is now
    let (found_file, newest_mtime) = scan_newest_source_mtime(&temp_root).unwrap().unwrap();
    assert_eq!(found_file, main_rs);

    // If running binary timestamp is older than newest source file:
    let old_binary_mtime = newest_mtime - Duration::from_secs(60);
    let status = compare_mtimes(old_binary_mtime, Some((found_file.clone(), newest_mtime)));
    match status {
        UpdateStatus::SourceNewer { diff_secs, .. } => {
            assert!(diff_secs >= 59 && diff_secs <= 61);
        }
        _ => panic!("Expected SourceNewer status"),
    }

    // If running binary timestamp is newer than newest source file:
    let new_binary_mtime = newest_mtime + Duration::from_secs(60);
    let status_fresh = compare_mtimes(new_binary_mtime, Some((found_file, newest_mtime)));
    assert_eq!(status_fresh, UpdateStatus::UpToDate);

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn test_evaluate_git_shas_divergence() {
    // Exact match
    assert_eq!(
        evaluate_git_shas("commit_abc_123", "commit_abc_123"),
        UpdateStatus::UpToDate
    );

    // Whitespace and newline tolerance
    assert_eq!(
        evaluate_git_shas("  commit_abc_123\n", "commit_abc_123  \r\n"),
        UpdateStatus::UpToDate
    );

    // Divergent commits
    let status = evaluate_git_shas("local_commit_1", "remote_commit_2");
    match status {
        UpdateStatus::GitBehind { current_commit, upstream_commit } => {
            assert_eq!(current_commit, "local_commit_1");
            assert_eq!(upstream_commit, "remote_commit_2");
        }
        _ => panic!("Expected GitBehind status"),
    }

    // Empty strings
    assert_eq!(evaluate_git_shas("", "remote"), UpdateStatus::UpToDate);
    assert_eq!(evaluate_git_shas("local", ""), UpdateStatus::UpToDate);
}

#[test]
fn test_update_check_result_serialization() {
    let result = UpdateCheckResult {
        is_outdated: true,
        reasons: vec!["Source newer than binary".to_string()],
        status: UpdateStatus::SourceNewer {
            file: "src/main.rs".to_string(),
            source_mtime_secs: 1700000000,
            binary_mtime_secs: 1699999000,
            diff_secs: 1000,
        },
        repo_root: Some(PathBuf::from("/repo")),
        manifest_path: Some(PathBuf::from("/repo/mcp-server/Cargo.toml")),
        current_exe: PathBuf::from("/repo/mcp-server/target/release/onlydoms"),
        binary_mtime: None,
        newest_source_mtime: None,
        newest_source_file: Some(PathBuf::from("/repo/mcp-server/src/main.rs")),
        git_status: None,
    };

    let serialized = serde_json::to_string(&result).unwrap();
    assert!(serialized.contains("is_outdated"));
    assert!(serialized.contains("Source newer than binary"));

    let deserialized: UpdateCheckResult = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.is_outdated, true);
    assert_eq!(deserialized.reasons.len(), 1);
}

#[test]
fn test_is_relevant_source_file_comprehensive() {
    assert!(is_relevant_source_file(Path::new("src/main.rs")));
    assert!(is_relevant_source_file(Path::new("src/tools/visual.rs")));
    assert!(is_relevant_source_file(Path::new("Cargo.toml")));
    assert!(is_relevant_source_file(Path::new("Cargo.lock")));
    assert!(is_relevant_source_file(Path::new("build.rs")));

    assert!(!is_relevant_source_file(Path::new("target/release/onlydoms")));
    assert!(!is_relevant_source_file(Path::new("target/debug/deps/lib.rlib")));
    assert!(!is_relevant_source_file(Path::new(".git/HEAD")));
    assert!(!is_relevant_source_file(Path::new(".git/config")));
    assert!(!is_relevant_source_file(Path::new(".DS_Store")));
    assert!(!is_relevant_source_file(Path::new("README.md")));
    assert!(!is_relevant_source_file(Path::new("screenshot.png")));
    assert!(!is_relevant_source_file(Path::new("node_modules/package.json")));
}
