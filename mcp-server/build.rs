fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Check candidate .git directories (current crate directory or parent workspace root)
    let candidate_git_dirs = [
        std::path::Path::new(".git"),
        std::path::Path::new("../.git"),
    ];

    for git_dir in candidate_git_dirs {
        if git_dir.exists() {
            let head = git_dir.join("HEAD");
            if head.exists() {
                println!("cargo:rerun-if-changed={}", head.display());
            }
            let refs = git_dir.join("refs");
            if refs.exists() {
                println!("cargo:rerun-if-changed={}", refs.display());
            }
            let refs_heads = refs.join("heads");
            if refs_heads.exists() {
                println!("cargo:rerun-if-changed={}", refs_heads.display());
            }
            break;
        }
    }

    // Attempt to capture current git commit hash
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
    {
        if output.status.success() {
            let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !commit.is_empty() {
                println!("cargo:rustc-env=ONLYDOMS_GIT_COMMIT={}", commit);
            }
        }
    }
}
