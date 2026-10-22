//! The Bedrock Dedicated Server zip: find it in the index, download it from
//! Mojang, check its hash, and unpack it.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use dataminer_core::Sha256Digest;
use serde_json::Value;

/// The Linux build's download, as one index `metadata.json` lists it.
#[derive(Debug, PartialEq)]
pub struct IndexEntry {
    pub url: String,
    pub sha256: Sha256Digest,
}

/// Read the Linux entry from an EndstoneMC `bedrock-server-data` metadata file:
/// `{"version": …, "binary": {"linux": {"url": …, "sha256": …}, …}}`.
pub fn parse_index(bytes: &[u8]) -> Result<IndexEntry> {
    let v: Value = serde_json::from_slice(bytes).context("parsing BDS index entry")?;
    let linux = v
        .pointer("/binary/linux")
        .context("BDS index entry has no binary.linux")?;
    let url = linux
        .get("url")
        .and_then(Value::as_str)
        .context("BDS index entry has no binary.linux.url")?;
    let sha256 = serde_json::from_value(linux.get("sha256").cloned().unwrap_or(Value::Null))
        .context("BDS index entry has no valid binary.linux.sha256")?;
    Ok(IndexEntry {
        url: url.to_string(),
        sha256,
    })
}

/// Download `url` with `curl`, to `dest` or (when `None`) into memory.
///
/// No redirects are followed, so the bytes come from the host that was named.
/// `protocols` is curl's `--proto` list, `=https` outside tests.
pub fn download(url: &str, dest: Option<&Path>, protocols: &str) -> Result<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(["--fail", "--silent", "--show-error", "--max-redirs", "0"])
        .args(["--proto", protocols, "--user-agent", USER_AGENT]);
    if let Some(dest) = dest {
        cmd.arg("--output").arg(dest);
    }
    let out = crate::git::run(cmd.arg(url))?;
    Ok(out.into_bytes())
}

const USER_AGENT: &str = concat!(
    "dataminer/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Vik-Varma/Dataminer)"
);

/// Unpack `zip` into `dest`, refusing entries that would land outside it.
pub fn extract(zip: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip).with_context(|| format!("opening {}", zip.display()))?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
        .with_context(|| format!("reading {} as a zip", zip.display()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.to_string();
        let rel = entry
            .enclosed_name()
            .with_context(|| format!("zip entry {name:?} escapes the target dir"))?;
        anyhow::ensure!(
            !entry.is_symlink(),
            "zip entry {name:?} is a symlink; refusing it"
        );
        let path = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out =
            std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
        std::io::copy(&mut entry, &mut out).with_context(|| format!("unpacking {name}"))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o755))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_linux_entry() {
        let json = br#"{"version":"1.26.51","binary":{
            "windows":{"url":"https://www.minecraft.net/w.zip","sha256":"00"},
            "linux":{"url":"https://www.minecraft.net/bedrockdedicatedserver/bin-linux/bedrock-server-1.26.51.1.zip",
                     "sha256":"ad91d3b824e51ea50b5bb601c295cbd8f543a29b14315c2ad89ff27311e2d860"}}}"#;
        let e = parse_index(json).unwrap();
        assert!(e.url.ends_with("bedrock-server-1.26.51.1.zip"));
        assert_eq!(
            e.sha256.to_hex(),
            "ad91d3b824e51ea50b5bb601c295cbd8f543a29b14315c2ad89ff27311e2d860"
        );
        assert!(parse_index(br#"{"binary":{"linux":{"url":"x","sha256":"zz"}}}"#).is_err());
    }

    #[test]
    fn refuses_entries_outside_the_target() {
        let dir = crate::test_dir("zip-slip");
        let zip = dir.join("evil.zip");
        crate::write_zip(&zip, &[("../escaped.txt", b"x".as_slice())]);
        let dest = dir.join("out");
        let err = extract(&zip, &dest).unwrap_err();
        assert!(err.to_string().contains("escapes"), "{err:#}");
        assert!(!dir.join("escaped.txt").exists());
    }
}
