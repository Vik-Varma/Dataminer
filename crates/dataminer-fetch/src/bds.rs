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
pub fn download(url: &str, dest: Option<&Path>, protocols: &str) -> Result<Vec<u8>> {
    crate::git::run(curl(dest, protocols).arg("--").arg(url))
}

/// The `curl` command for [`download`], before the URL.
///
/// A redirect is an error, so the bytes come from the host that was named.
/// The user's `~/.curlrc` is ignored (`-q`, which must come first), and a
/// stalled transfer fails. `protocols` is curl's `--proto` list, `=https`
/// outside tests.
fn curl(dest: Option<&Path>, protocols: &str) -> Command {
    let mut cmd = Command::new("curl");
    cmd.args(["-q", "--fail", "--silent", "--show-error", "--globoff"])
        .args(["--location", "--max-redirs", "0", "--http1.1"])
        .args([
            "--connect-timeout",
            "30",
            "--speed-limit",
            "1024",
            "--speed-time",
            "60",
        ])
        .args(["--proto", protocols, "--user-agent", USER_AGENT]);
    if let Some(dest) = dest {
        cmd.arg("--output").arg(dest);
    }
    cmd
}

/// Mojang's CDN resets requests from non-browser clients (an empty reply on
/// HTTP/1.1, a stream error on HTTP/2), so downloads send a browser
/// User-Agent over HTTP/1.1. The project owner chose this; see
/// `docs/fetcher-requirements.md`.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64)";

/// The current Linux release URL from Mojang's download-links API:
/// `{"result": {"links": [{"downloadType": "serverBedrockLinux", "downloadUrl": …}, …]}}`.
pub fn current_linux_url(links: &Value) -> Option<String> {
    links
        .pointer("/result/links")?
        .as_array()?
        .iter()
        .find(|l| l.get("downloadType").and_then(Value::as_str) == Some("serverBedrockLinux"))?
        .get("downloadUrl")?
        .as_str()
        .map(str::to_string)
}

/// The build version in a BDS URL: `…/bedrock-server-1.26.52.3.zip` → `1.26.52.3`.
pub fn version_of(url: &str) -> Option<String> {
    let file = url.rsplit('/').next()?;
    let v = file.strip_prefix("bedrock-server-")?.strip_suffix(".zip")?;
    Some(v.to_string())
}

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
    fn reads_mojangs_current_release() {
        let json = br#"{"result":{"links":[
            {"downloadType":"serverBedrockWindows","downloadUrl":"https://www.minecraft.net/bedrockdedicatedserver/bin-win/bedrock-server-1.26.52.3.zip"},
            {"downloadType":"serverBedrockLinux","downloadUrl":"https://www.minecraft.net/bedrockdedicatedserver/bin-linux/bedrock-server-1.26.52.3.zip"}]}}"#;
        let url = current_linux_url(&serde_json::from_slice(json).unwrap()).unwrap();
        assert!(url.contains("/bin-linux/"));
        assert_eq!(version_of(&url).as_deref(), Some("1.26.52.3"));
        let empty = serde_json::json!({"result": {"links": []}});
        assert_eq!(current_linux_url(&empty), None);
    }

    #[test]
    fn downloads_like_a_browser_over_http1() {
        let cmd = format!("{:?}", curl(Some(Path::new("out.zip")), "=https"));
        assert!(cmd.contains("\"--http1.1\""), "{cmd}");
        assert!(cmd.contains("\"Mozilla/5.0 (X11; Linux x86_64)\""), "{cmd}");
        assert!(cmd.starts_with("\"curl\" \"-q\""), "{cmd}");
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
