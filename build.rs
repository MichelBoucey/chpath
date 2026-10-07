use std::process::Command;

/// Compute the version of chpath with the current git short hash at build
/// time, like `0.1.0 (33c0e12)`.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    let hash = git_short_hash().unwrap_or_else(|| "unknown".to_string());
    let version = format!("{} ({hash})", env!("CARGO_PKG_VERSION"));

    println!("cargo:rustc-env=CHPATH_VERSION={version}");
}

/// Read the short hash of the current commit with `git rev-parse --short HEAD`.
fn git_short_hash() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;

    let hash = String::from_utf8(output.stdout).ok()?;
    let hash = hash.trim();

    if hash.is_empty() || !output.status.success() {
        return None;
    }

    Some(hash.to_string())
}
