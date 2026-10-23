//! A content digest of a fetched directory, so a later run can tell whether
//! the cache is intact.

use std::path::Path;

use anyhow::{Context, Result};
use dataminer_core::Sha256Digest;

/// Digest every file under `root` except a top-level `.git`.
///
/// Files are visited in sorted path order (`/`-separated) and each contributes
/// its relative path and the SHA-256 of its contents, so the same files give
/// the same result on every OS and every run (git checkouts are made with
/// line-ending conversion off for that reason). Symlinks contribute their
/// target, not what it points to. Returns the digest and the number of files.
pub fn digest(root: &Path) -> Result<(Sha256Digest, u64)> {
    let mut entries = Vec::new();
    walk(root, "", &mut entries)?;
    entries.sort();

    let mut listing = Vec::new();
    for (rel, line) in &entries {
        listing.extend_from_slice(rel.as_bytes());
        listing.push(0);
        listing.extend_from_slice(line.as_bytes());
        listing.push(b'\n');
    }
    Ok((Sha256Digest::of_bytes(&listing), entries.len() as u64))
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, String)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if prefix.is_empty() && name == ".git" {
            continue;
        }
        let rel = format!("{prefix}{name}");
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            walk(&path, &format!("{rel}/"), out)?;
        } else if kind.is_symlink() {
            let target = std::fs::read_link(&path)?;
            out.push((rel, format!("link:{}", target.to_string_lossy())));
        } else {
            let file = std::fs::File::open(&path)
                .with_context(|| format!("opening {}", path.display()))?;
            let d = Sha256Digest::of_reader(std::io::BufReader::new(file))
                .with_context(|| format!("reading {}", path.display()))?;
            out.push((rel, d.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_ignores_git_and_sees_changes() {
        let root = crate::test_dir("tree");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("a/b/c.json"), b"{}").unwrap();
        std::fs::write(root.join("top.txt"), b"x").unwrap();

        let (d1, n) = digest(&root).unwrap();
        assert_eq!(n, 2);
        std::fs::write(root.join(".git/HEAD"), b"ignored").unwrap();
        assert_eq!(digest(&root).unwrap().0, d1);

        std::fs::write(root.join("a/b/c.json"), b"{ }").unwrap();
        assert_ne!(digest(&root).unwrap().0, d1);
    }
}
