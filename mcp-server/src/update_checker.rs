use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

/// Operating mode dictating whether to prompt interactively or automate rebuild
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// Background daemon: automated rebuild with stderr logging
    Daemon,
    /// Interactive terminal (TTY): user prompt [Y/n]
    Interactive,
    /// MCP stdio proxy: automated rebuild or stderr notification without corrupting stdout
    Proxy,
    /// Diagnostic mode: inspect and report status without performing rebuild
    CheckOnly,
}

/// Status variants indicating update availability
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateStatus {
    UpToDate,
    SourceNewer {
        file: String,
        source_mtime_secs: u64,
        binary_mtime_secs: u64,
        diff_secs: u64,
    },
    GitBehind {
        current_commit: String,
        upstream_commit: String,
    },
    UpdateAvailable {
        source_newer: bool,
        git_behind: bool,
    },
}

/// Comprehensive diagnostics regarding git repository state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitUpdateStatus {
    pub current_commit: String,
    pub short_commit: String,
    pub embedded_commit: Option<String>,
    pub commit_mismatch: bool,
    pub upstream_branch: Option<String>,
    pub commits_behind_upstream: usize,
    pub is_dirty: bool,
}

/// Result of evaluating local source timestamps, git status, and binary age
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub is_outdated: bool,
    pub reasons: Vec<String>,
    pub status: UpdateStatus,
    pub repo_root: Option<PathBuf>,
    pub manifest_path: Option<PathBuf>,
    pub current_exe: PathBuf,
    pub binary_mtime: Option<SystemTime>,
    pub newest_source_mtime: Option<SystemTime>,
    pub newest_source_file: Option<PathBuf>,
    pub git_status: Option<GitUpdateStatus>,
}

/// Error conditions encountered during update checking, building, or reloading
#[derive(Debug)]
pub enum UpdateError {
    Io(std::io::Error),
    BuildFailed { code: Option<i32>, stderr: String },
    GitError(String),
    CargoNotFound,
    RepoNotFound,
    RestartFailed(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Io(e) => write!(f, "Update check I/O error: {}", e),
            UpdateError::BuildFailed { code, stderr } => {
                write!(f, "Cargo build failed (code {:?}): {}", code, stderr.trim())
            }
            UpdateError::GitError(msg) => write!(f, "Git check error: {}", msg),
            UpdateError::CargoNotFound => write!(f, "Cargo executable not found in PATH"),
            UpdateError::RepoNotFound => write!(f, "Repository root could not be located"),
            UpdateError::RestartFailed(msg) => write!(f, "Failed to reload process: {}", msg),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<std::io::Error> for UpdateError {
    fn from(e: std::io::Error) -> Self {
        UpdateError::Io(e)
    }
}

/// Resolve repository root containing mcp-server/Cargo.toml
pub fn find_repo_root() -> Option<PathBuf> {
    // 1. Explicit environment override
    if let Ok(dir) = std::env::var("ONLYDOMS_REPO_DIR") {
        let p = PathBuf::from(dir);
        if p.join("mcp-server").join("Cargo.toml").exists() {
            return Some(p);
        }
    }

    // 2. Inspect current running binary and traverse up
    if let Ok(exe) = std::env::current_exe() {
        let canonical_exe = exe.canonicalize().unwrap_or(exe);
        let mut curr = canonical_exe.parent();
        for _ in 0..6 {
            if let Some(dir) = curr {
                if dir.join("mcp-server").join("Cargo.toml").exists() {
                    return Some(dir.to_path_buf());
                }
                if dir.join(".git").exists() && dir.join("mcp-server").exists() {
                    return Some(dir.to_path_buf());
                }
                curr = dir.parent();
            } else {
                break;
            }
        }
    }

    // 3. Compile-time manifest dir fallback
    let compile_time_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(parent) = compile_time_manifest.parent() {
        if parent.join("mcp-server").join("Cargo.toml").exists() {
            return Some(parent.to_path_buf());
        }
    }

    // 4. Current working directory fallback
    if let Ok(cwd) = std::env::current_dir() {
        let mut curr = Some(cwd.as_path());
        for _ in 0..6 {
            if let Some(dir) = curr {
                if dir.join("mcp-server").join("Cargo.toml").exists() {
                    return Some(dir.to_path_buf());
                }
                if dir.join(".git").exists() && dir.join("mcp-server").exists() {
                    return Some(dir.to_path_buf());
                }
                curr = dir.parent();
            } else {
                break;
            }
        }
    }

    None
}

/// Determine whether a given path is an active source or manifest file
pub fn is_relevant_source_file(path: &Path) -> bool {
    // Inspect path components: ignore target, .git, node_modules, and hidden directories
    for comp in path.components() {
        if let std::path::Component::Normal(os_str) = comp {
            if let Some(s) = os_str.to_str() {
                if s == "target" || s == "node_modules" || s.starts_with('.') {
                    return false;
                }
            }
        }
    }

    if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
        if file_name == "Cargo.toml" || file_name == "Cargo.lock" || file_name == "build.rs" {
            return true;
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            return ext == "rs";
        }
    }
    false
}

/// Recursively scan a directory for the newest source file modification time
pub fn scan_newest_source_mtime(dir: &Path) -> Result<Option<(PathBuf, SystemTime)>, std::io::Error> {
    let mut newest: Option<(PathBuf, SystemTime)> = None;
    scan_dir_recursive(dir, &mut newest)?;
    Ok(newest)
}

fn scan_dir_recursive(dir: &Path, newest: &mut Option<(PathBuf, SystemTime)>) -> Result<(), std::io::Error> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() && !file_type.is_symlink() {
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if dir_name == "target" || dir_name == ".git" || dir_name.starts_with('.') || dir_name == "node_modules" {
                continue;
            }
            scan_dir_recursive(&path, newest)?;
        } else if is_relevant_source_file(&path) {
            // Use std::fs::metadata to follow symlinks to their target.
            // Broken/dangling symlinks will fail resolution (NotFound) and be safely skipped.
            if let Ok(metadata) = std::fs::metadata(&path) {
                if metadata.is_file() {
                    if let Ok(mtime) = metadata.modified() {
                        match newest {
                            Some((_, current_newest_mtime)) => {
                                if mtime > *current_newest_mtime {
                                    *newest = Some((path, mtime));
                                }
                            }
                            None => {
                                *newest = Some((path, mtime));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Get newest source mtime for mcp-server package
pub fn get_newest_source_mtime(mcp_server_dir: &Path) -> Result<(SystemTime, PathBuf), std::io::Error> {
    match scan_newest_source_mtime(mcp_server_dir)? {
        Some((path, mtime)) => Ok((mtime, path)),
        None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No source files found")),
    }
}

/// Compare binary modification time with newest source file modification time
pub fn compare_mtimes(binary_mtime: SystemTime, newest_source: Option<(PathBuf, SystemTime)>) -> UpdateStatus {
    if let Some((path, src_mtime)) = newest_source {
        if src_mtime > binary_mtime {
            let src_secs = src_mtime.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
            let bin_secs = binary_mtime.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
            let diff = src_secs.saturating_sub(bin_secs);
            return UpdateStatus::SourceNewer {
                file: path.to_string_lossy().to_string(),
                source_mtime_secs: src_secs,
                binary_mtime_secs: bin_secs,
                diff_secs: diff,
            };
        }
    }
    UpdateStatus::UpToDate
}

/// Compare current commit against upstream commit
pub fn evaluate_git_shas(current_commit: &str, upstream_commit: &str) -> UpdateStatus {
    let cur = current_commit.trim();
    let up = upstream_commit.trim();

    if cur.is_empty() || up.is_empty() {
        return UpdateStatus::UpToDate;
    }

    if cur == up {
        UpdateStatus::UpToDate
    } else {
        UpdateStatus::GitBehind {
            current_commit: cur.to_string(),
            upstream_commit: up.to_string(),
        }
    }
}

/// Inspect git repository status (HEAD, compile-time commit, tracking upstream branch)
pub fn check_git_status(repo_dir: &Path) -> Result<GitUpdateStatus, String> {
    if !repo_dir.join(".git").exists() {
        return Err("Not a git repository".into());
    }

    let head_output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| format!("Failed to execute git rev-parse HEAD: {}", e))?;

    if !head_output.status.success() {
        return Err(String::from_utf8_lossy(&head_output.stderr).to_string());
    }

    let current_commit = String::from_utf8_lossy(&head_output.stdout).trim().to_string();
    let short_commit = current_commit.chars().take(8).collect::<String>();

    let embedded_commit = option_env!("ONLYDOMS_GIT_COMMIT").map(|s| s.to_string());
    let commit_mismatch = match &embedded_commit {
        Some(emb) => emb != &current_commit,
        None => false,
    };

    let mut upstream_branch = None;
    let mut commits_behind_upstream = 0;

    if let Ok(upstream_output) = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(repo_dir)
        .output()
    {
        if upstream_output.status.success() {
            let branch = String::from_utf8_lossy(&upstream_output.stdout).trim().to_string();
            if !branch.is_empty() {
                upstream_branch = Some(branch.clone());
                if let Ok(rev_list) = std::process::Command::new("git")
                    .args(["rev-list", "--count", &format!("HEAD..{}", branch)])
                    .current_dir(repo_dir)
                    .output()
                {
                    if rev_list.status.success() {
                        let count_str = String::from_utf8_lossy(&rev_list.stdout).trim().to_string();
                        commits_behind_upstream = count_str.parse().unwrap_or(0);
                    }
                }
            }
        }
    }

    let is_dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_dir)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);

    Ok(GitUpdateStatus {
        current_commit,
        short_commit,
        embedded_commit,
        commit_mismatch,
        upstream_branch,
        commits_behind_upstream,
        is_dirty,
    })
}

/// Comprehensive update check inspecting executable, source mtimes, and git state
pub fn check_for_updates() -> UpdateCheckResult {
    let current_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("onlydoms"));
    let canonical_exe = current_exe.canonicalize().unwrap_or_else(|_| current_exe.clone());
    let binary_mtime = std::fs::metadata(&canonical_exe).and_then(|m| m.modified()).ok();

    let repo_root = find_repo_root();
    let mut manifest_path = None;
    let mut newest_source_mtime = None;
    let mut newest_source_file = None;
    let mut git_status = None;
    let mut reasons = Vec::new();
    let mut is_outdated = false;
    let mut status = UpdateStatus::UpToDate;

    if let Some(ref root) = repo_root {
        let mcp_dir = root.join("mcp-server");
        let manifest = mcp_dir.join("Cargo.toml");
        if manifest.exists() {
            manifest_path = Some(manifest);
        }

        if let Ok(Some((src_file, src_mtime))) = scan_newest_source_mtime(&mcp_dir) {
            newest_source_mtime = Some(src_mtime);
            newest_source_file = Some(src_file.clone());

            if let Some(bin_time) = binary_mtime {
                let mtime_status = compare_mtimes(bin_time, Some((src_file.clone(), src_mtime)));
                if let UpdateStatus::SourceNewer { ref file, diff_secs, .. } = mtime_status {
                    is_outdated = true;
                    reasons.push(format!("Source file '{}' modified after running binary (delta: {}s)", file, diff_secs));
                    status = mtime_status;
                }
            } else {
                is_outdated = true;
                reasons.push("Running executable binary timestamp could not be determined on disk (binary missing or unreadable)".to_string());
                let src_secs = src_mtime.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
                status = UpdateStatus::SourceNewer {
                    file: src_file.to_string_lossy().to_string(),
                    source_mtime_secs: src_secs,
                    binary_mtime_secs: 0,
                    diff_secs: src_secs,
                };
            }
        }

        if let Ok(gstatus) = check_git_status(root) {
            if gstatus.commit_mismatch {
                is_outdated = true;
                reasons.push(format!(
                    "Running binary commit ({:?}) differs from repo HEAD ({})",
                    gstatus.embedded_commit, gstatus.current_commit
                ));
                if status == UpdateStatus::UpToDate {
                    status = UpdateStatus::GitBehind {
                        current_commit: gstatus.embedded_commit.clone().unwrap_or_default(),
                        upstream_commit: gstatus.current_commit.clone(),
                    };
                }
            }
            if gstatus.commits_behind_upstream > 0 {
                is_outdated = true;
                reasons.push(format!(
                    "Local repository is {} commit(s) behind upstream '{}'",
                    gstatus.commits_behind_upstream,
                    gstatus.upstream_branch.as_deref().unwrap_or("upstream")
                ));
                if status == UpdateStatus::UpToDate {
                    status = UpdateStatus::GitBehind {
                        current_commit: gstatus.embedded_commit.clone().unwrap_or_else(|| gstatus.current_commit.clone()),
                        upstream_commit: gstatus.upstream_branch.clone().unwrap_or_else(|| "upstream".to_string()),
                    };
                }
            }
            git_status = Some(gstatus);
        }
    }

    UpdateCheckResult {
        is_outdated,
        reasons,
        status,
        repo_root,
        manifest_path,
        current_exe: canonical_exe,
        binary_mtime,
        newest_source_mtime,
        newest_source_file,
        git_status,
    }
}

/// Prompt interactive user in terminal whether to rebuild and reload
pub fn prompt_user_for_rebuild(result: &UpdateCheckResult) -> bool {
    use std::io::IsTerminal;
    if std::env::var("ONLYDOMS_NON_INTERACTIVE").is_ok()
        || std::env::var("CI").is_ok()
        || std::env::current_exe().map_or(false, |p| p.components().any(|c| c.as_os_str() == "deps"))
        || !std::io::stdin().is_terminal()
        || !std::io::stderr().is_terminal()
    {
        return false;
    }

    eprintln!("\n┌──────────────────────────────────────────────────────────────┐");
    eprintln!("│  🔄 OnlyDOMs Update Available                                │");
    eprintln!("└──────────────────────────────────────────────────────────────┘");
    eprintln!("Reasons:");
    for r in &result.reasons {
        eprintln!("  • {}", r);
    }
    eprint!("\nRebuild and reload OnlyDOMs now? [Y/n]: ");
    use std::io::Write;
    let _ = std::io::stderr().flush();

    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_ok() {
        let trimmed = input.trim().to_lowercase();
        trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
    } else {
        false
    }
}

/// Execute `cargo build --release --manifest-path <path>` to rebuild OnlyDOMs binary
pub fn run_cargo_build(manifest_path: &Path) -> Result<(), UpdateError> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    
    let working_dir = manifest_path.parent().unwrap_or(manifest_path);
    tracing::info!("Compiling updated OnlyDOMs release binary via cargo build --release...");

    let output = std::process::Command::new(cargo)
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(manifest_path)
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(UpdateError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(UpdateError::BuildFailed {
            code: output.status.code(),
            stderr,
        });
    }

    tracing::info!("OnlyDOMs binary rebuilt successfully.");
    Ok(())
}

/// Atomically replace process image using exec() on Unix or respawn on Windows
pub fn safe_reload_process() -> Result<(), UpdateError> {
    let exe = std::env::current_exe().map_err(UpdateError::Io)?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    std::env::set_var("ONLYDOMS_RELOADED", "1");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(exe)
            .args(&args)
            .env("ONLYDOMS_RELOADED", "1")
            .exec();
        Err(UpdateError::RestartFailed(err.to_string()))
    }

    #[cfg(not(unix))]
    {
        std::process::Command::new(exe)
            .args(&args)
            .env("ONLYDOMS_RELOADED", "1")
            .spawn()
            .map_err(UpdateError::Io)?;
        std::process::exit(0);
    }
}

/// High-level startup check orchestrator
pub async fn check_and_handle_update(mode: UpdateMode) -> Result<UpdateCheckResult, UpdateError> {
    if std::env::var("ONLYDOMS_SKIP_UPDATE_CHECK").is_ok() {
        return Ok(check_for_updates());
    }

    if std::env::var("ONLYDOMS_RELOADED").is_ok() {
        std::env::remove_var("ONLYDOMS_RELOADED");
        return Ok(check_for_updates());
    }

    let check = check_for_updates();
    if !check.is_outdated {
        return Ok(check);
    }

    let should_rebuild = match mode {
        UpdateMode::Interactive => prompt_user_for_rebuild(&check),
        UpdateMode::Daemon => true,
        UpdateMode::Proxy => true,
        UpdateMode::CheckOnly => false,
    };

    if should_rebuild {
        if let Some(ref manifest) = check.manifest_path {
            run_cargo_build(manifest)?;
            safe_reload_process()?;
        } else {
            tracing::warn!("Update available but Cargo.toml manifest could not be located; skipping rebuild.");
        }
    }

    Ok(check)
}

/// Convenience entry point adhering to PROJECT.md interface contract
pub fn check_and_prompt_update() -> Result<UpdateStatus, UpdateError> {
    let check = check_for_updates();
    if !check.is_outdated {
        return Ok(UpdateStatus::UpToDate);
    }

    let source_newer = matches!(check.status, UpdateStatus::SourceNewer { .. });
    let git_behind = matches!(check.status, UpdateStatus::GitBehind { .. })
        || check.git_status.as_ref().map_or(false, |g| g.commits_behind_upstream > 0 || g.commit_mismatch);

    Ok(UpdateStatus::UpdateAvailable {
        source_newer,
        git_behind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_find_repo_root_detects_mcp_server() {
        let root = find_repo_root();
        assert!(root.is_some(), "Repo root must be detected in development workspace");
        let manifest = root.unwrap().join("mcp-server").join("Cargo.toml");
        assert!(manifest.exists(), "Manifest path must exist");
    }

    #[test]
    fn test_is_relevant_source_file() {
        assert!(is_relevant_source_file(Path::new("src/main.rs")));
        assert!(is_relevant_source_file(Path::new("src/tools/visual.rs")));
        assert!(is_relevant_source_file(Path::new("Cargo.toml")));
        assert!(is_relevant_source_file(Path::new("Cargo.lock")));
        assert!(is_relevant_source_file(Path::new("build.rs")));
        assert!(is_relevant_source_file(Path::new("src/target_utils/helper.rs")));

        assert!(!is_relevant_source_file(Path::new("target/debug/build.rs")));
        assert!(!is_relevant_source_file(Path::new("target/debug/onlydoms")));
        assert!(!is_relevant_source_file(Path::new("target/debug/deps/lib.rlib")));
        assert!(!is_relevant_source_file(Path::new(".git/HEAD")));
        assert!(!is_relevant_source_file(Path::new(".git/hooks/build.rs")));
        assert!(!is_relevant_source_file(Path::new(".DS_Store")));
        assert!(!is_relevant_source_file(Path::new(".hidden_dir/main.rs")));
        assert!(!is_relevant_source_file(Path::new("README.md")));
        assert!(!is_relevant_source_file(Path::new("tests/sample.png")));
        assert!(!is_relevant_source_file(Path::new("node_modules/package.json")));
    }

    #[test]
    fn test_compare_mtimes_uptodate() {
        let now = SystemTime::now();
        let older = now - Duration::from_secs(100);

        let status = compare_mtimes(now, Some((PathBuf::from("src/main.rs"), older)));
        assert_eq!(status, UpdateStatus::UpToDate);

        let status_none = compare_mtimes(now, None);
        assert_eq!(status_none, UpdateStatus::UpToDate);
    }

    #[test]
    fn test_compare_mtimes_source_newer() {
        let base = SystemTime::now();
        let bin_mtime = base - Duration::from_secs(200);
        let src_mtime = base;

        let status = compare_mtimes(bin_mtime, Some((PathBuf::from("src/tools/visual.rs"), src_mtime)));
        match status {
            UpdateStatus::SourceNewer { file, diff_secs, .. } => {
                assert_eq!(file, "src/tools/visual.rs");
                assert!(diff_secs >= 199 && diff_secs <= 201);
            }
            other => panic!("Expected SourceNewer, got {:?}", other),
        }
    }

    #[test]
    fn test_evaluate_git_shas() {
        assert_eq!(
            evaluate_git_shas("e8d1a45", "e8d1a45"),
            UpdateStatus::UpToDate
        );

        let status = evaluate_git_shas("1111111", "2222222");
        match status {
            UpdateStatus::GitBehind { current_commit, upstream_commit } => {
                assert_eq!(current_commit, "1111111");
                assert_eq!(upstream_commit, "2222222");
            }
            other => panic!("Expected GitBehind, got {:?}", other),
        }

        assert_eq!(
            evaluate_git_shas("abc123\n", "  abc123 "),
            UpdateStatus::UpToDate
        );

        assert_eq!(
            evaluate_git_shas("", "abc123"),
            UpdateStatus::UpToDate
        );
    }

    #[test]
    fn test_scan_dir_with_temp_files() {
        let temp_dir = std::env::temp_dir().join(format!("test_update_scan_{}", uuid::Uuid::new_v4()));
        let src_dir = temp_dir.join("src");
        let target_dir = temp_dir.join("target");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();

        let file_a = src_dir.join("lib.rs");
        let file_target = target_dir.join("ignored.rs");
        let file_toml = temp_dir.join("Cargo.toml");

        std::fs::write(&file_a, "pub fn test() {}").unwrap();
        std::fs::write(&file_target, "pub fn ignored() {}").unwrap();
        std::fs::write(&file_toml, "[package]\nname = \"test\"").unwrap();

        let scanned = scan_newest_source_mtime(&temp_dir).unwrap();
        assert!(scanned.is_some());
        let (found_path, _) = scanned.unwrap();
        assert!(!found_path.to_string_lossy().contains("target"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_timestamp_comparison_detects_newer_source() {
        let temp_dir = std::env::temp_dir().join(format!("test_update_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let bin_path = temp_dir.join("dummy_bin");
        std::fs::write(&bin_path, b"binary_data").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));
        let src_path = temp_dir.join("dummy_source.rs");
        std::fs::write(&src_path, b"fn main() {}").unwrap();

        let bin_mtime = std::fs::metadata(&bin_path).unwrap().modified().unwrap();
        let src_mtime = std::fs::metadata(&src_path).unwrap().modified().unwrap();

        assert!(src_mtime > bin_mtime, "Source mtime must be newer than binary");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_loop_prevention_env_var() {
        std::env::set_var("ONLYDOMS_RELOADED", "1");
        // When ONLYDOMS_RELOADED is set, check_and_handle_update must bypass rebuild
        let rt = tokio::runtime::Runtime::new().unwrap();
        let res = rt.block_on(check_and_handle_update(UpdateMode::Daemon));
        assert!(res.is_ok());
        // Environmental variable is cleared to prevent repeated skipping
        assert!(std::env::var("ONLYDOMS_RELOADED").is_err());
    }
}
