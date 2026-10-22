//! The two Mojang repositories, fetched by shelling out to `git`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::Value;

/// The release and preview tags of a remote, without fetching anything else.
pub fn ls_remote_tags(url: &str) -> Result<Vec<String>> {
    let out = run(git(None).args(["ls-remote", "--tags", "--refs", url]))?;
    Ok(out
        .lines()
        .filter_map(|l| l.split_once("refs/tags/").map(|(_, t)| t.to_string()))
        .collect())
}

/// A shallow, blobless clone of `tag` into `dest`, checking out only the
/// top-level files and `subdir`.
pub fn clone_sparse(url: &str, tag: &str, subdir: &str, dest: &Path) -> Result<()> {
    run(git(None)
        .args([
            "clone",
            "--quiet",
            "--depth",
            "1",
            "--filter=blob:none",
            "--sparse",
        ])
        .args(["--branch", tag, url])
        .arg(dest))?;
    run(git(Some(dest)).args(["sparse-checkout", "set", subdir]))?;
    Ok(())
}

/// The commit checked out in `dir`.
pub fn head(dir: &Path) -> Result<String> {
    Ok(run(git(Some(dir)).args(["rev-parse", "HEAD"]))?
        .trim()
        .to_string())
}

/// The wire protocol stated by the packet JSON in a checked-out `json/` dir.
pub fn protocol_in_dir(json_dir: &Path) -> Result<u32> {
    let mut names: Vec<_> = std::fs::read_dir(json_dir)
        .with_context(|| format!("reading {}", json_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    names.sort();
    for path in names {
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        if let Some(p) = protocol_of(&bytes) {
            return Ok(p);
        }
    }
    anyhow::bail!("no x-protocol-version in {}", json_dir.display())
}

/// The wire protocol of another tag of the protocol-docs clone at `docs`.
/// Fetches that one tag (shallow, blobless) and reads one packet file from it.
pub fn protocol_at_tag(docs: &Path, tag: &str) -> Result<u32> {
    run(git(Some(docs))
        .args([
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "--filter=blob:none",
            "origin",
        ])
        .arg(format!("+refs/tags/{tag}:refs/tags/{tag}")))?;
    let listing = run(git(Some(docs)).args(["ls-tree", "--name-only", tag, "json/"]))?;
    let mut names: Vec<&str> = listing.lines().filter(|n| n.ends_with(".json")).collect();
    names.sort_unstable();
    for name in names {
        let out = git(Some(docs))
            .args(["show", &format!("{tag}:{name}")])
            .output()?;
        if let Some(p) = out
            .status
            .success()
            .then(|| protocol_of(&out.stdout))
            .flatten()
        {
            return Ok(p);
        }
    }
    anyhow::bail!("no x-protocol-version in bedrock-protocol-docs {tag}")
}

fn protocol_of(bytes: &[u8]) -> Option<u32> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    let p = v.get("x-protocol-version")?.as_u64()?;
    u32::try_from(p).ok()
}

fn git(dir: Option<&Path>) -> Command {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "advice.detachedHead=false"]);
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    cmd
}

/// Run a command and return its stdout, or fail with its stderr.
pub(crate) fn run(cmd: &mut Command) -> Result<String> {
    let shown = format!("{cmd:?}");
    let out = cmd
        .output()
        .with_context(|| format!("could not start {shown} (is it installed?)"))?;
    anyhow::ensure!(
        out.status.success(),
        "{shown} failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
