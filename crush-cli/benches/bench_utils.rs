use std::process::Command;

/// Helper to get the crush binary path.
///
/// Builds the binary in release mode if not already built.
pub fn crush_binary() -> std::path::PathBuf {
    let output = Command::new("cargo")
        .args(["build", "--release", "--bin", "crush"])
        .output()
        .expect("Failed to build crush binary");

    if !output.status.success() {
        panic!(
            "Failed to build crush: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Find the target directory (could be in workspace root or current dir)
    let mut path = std::env::current_dir().expect("Failed to get current dir");

    // If we're in crush-cli, go up one level to workspace root
    if path.ends_with("crush-cli") {
        path.pop();
    }

    path.push("target");
    path.push("release");
    path.push("crush");

    #[cfg(windows)]
    path.set_extension("exe");

    if !path.exists() {
        panic!("Crush binary not found at: {}", path.display());
    }

    path
}
