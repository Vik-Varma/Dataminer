//! Stamp the current git commit into `DATAMINER_COMMIT` so the manifest records
//! exactly which Dataminer built a spec. Falls back to `unknown` when git isn't
//! available (e.g. building from a packaged tarball).

use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DATAMINER_COMMIT={commit}");
    // Rebuild when HEAD moves so the stamp stays current.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
