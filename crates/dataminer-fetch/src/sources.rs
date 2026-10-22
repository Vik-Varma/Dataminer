//! `sources.json`: what a fetch downloaded, from where, and its hashes.

use std::path::Path;

use anyhow::{Context, Result};
use dataminer_core::Sha256Digest;
use serde::{Deserialize, Serialize};

/// Bumped when the layout of `sources.json` changes.
pub const SCHEMA_VERSION: u32 = 1;

/// The file name inside `<out>/<game-version>/`.
pub const FILE_NAME: &str = "sources.json";

/// The whole provenance record of one fetch directory.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sources {
    pub schema_version: u32,
    /// The version the fetch was asked for.
    pub game_version: String,
    /// The wire protocol of the target, from `bedrock-protocol-docs`.
    pub protocol: Option<u32>,
    /// When the user accepted the Minecraft EULA for this fetch (RFC 3339 UTC).
    pub eula_accepted_at: String,
    /// The Dataminer build that fetched, e.g. `dataminer 0.0.1 (c222f2f)`.
    pub fetched_by: String,
    /// One entry per source, in a fixed order.
    pub sources: Vec<Source>,
}

/// One fetched source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub kind: Kind,
    /// Directory relative to `<out>/<game-version>/`.
    pub path: String,
    /// Where it came from: a git remote or a download URL.
    pub url: String,
    /// The git tag checked out.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// The commit that tag resolved to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The downloaded file's name and SHA-256 (for downloads, not git).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_sha256: Option<Sha256Digest>,
    /// The index entry the download URL and expected hash came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_sha256: Option<Sha256Digest>,
    /// The game version this source actually is.
    pub game_version: String,
    /// Its wire protocol, when established.
    pub protocol: Option<u32>,
    pub r#match: Match,
    /// [`crate::tree::digest`] of the fetched directory, and its file count.
    pub tree_sha256: Sha256Digest,
    pub files: u64,
    pub fetched_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A source. The order is the order of `sources.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    BedrockProtocolDocs,
    BedrockSamples,
    BedrockServer,
}

impl Kind {
    /// The directory name, which is also the serialized kind.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::BedrockProtocolDocs => "bedrock-protocol-docs",
            Kind::BedrockSamples => "bedrock-samples",
            Kind::BedrockServer => "bedrock-server",
        }
    }
}

/// How the source's version relates to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Match {
    /// Same game version as the target.
    Exact,
    /// An earlier version whose protocol was checked to equal the target's.
    SameProtocol,
    /// Chosen by the user with an explicit ref; nothing was checked.
    Explicit,
}

impl Sources {
    pub fn read(dir: &Path) -> Result<Option<Self>> {
        let path = dir.join(FILE_NAME);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let s: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?;
        anyhow::ensure!(
            s.schema_version == SCHEMA_VERSION,
            "{} has schema version {}, this Dataminer writes {SCHEMA_VERSION}",
            path.display(),
            s.schema_version
        );
        Ok(Some(s))
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = dir.join(FILE_NAME);
        let json = serde_json::to_string_pretty(self)? + "\n";
        std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
    }

    pub fn get(&self, kind: Kind) -> Option<&Source> {
        self.sources.iter().find(|s| s.kind == kind)
    }
}

/// The current time as RFC 3339 UTC, to the second.
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    rfc3339(secs)
}

fn rfc3339(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339(1_792_670_400), "2026-10-22T12:00:00Z");
    }

    #[test]
    fn round_trips() {
        let s = Sources {
            schema_version: SCHEMA_VERSION,
            game_version: "1.26.51".into(),
            protocol: Some(2193),
            eula_accepted_at: "2026-10-22T12:00:00Z".into(),
            fetched_by: "dataminer test".into(),
            sources: vec![Source {
                kind: Kind::BedrockSamples,
                path: Kind::BedrockSamples.dir().into(),
                url: "https://example.invalid/samples".into(),
                git_ref: Some("v1.26.50.4".into()),
                commit: Some("46ba6ea985fb5a92d79a9419198f10dda14c199d".into()),
                file: None,
                file_sha256: None,
                index_url: None,
                index_sha256: None,
                game_version: "1.26.50.4".into(),
                protocol: Some(2193),
                r#match: Match::SameProtocol,
                tree_sha256: Sha256Digest::of_bytes(b""),
                files: 3,
                fetched_at: "2026-10-22T12:00:00Z".into(),
                notes: vec![],
            }],
        };
        let dir = crate::test_dir("sources");
        s.write(&dir).unwrap();
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("\"kind\": \"bedrock-samples\""));
        assert!(text.contains("\"match\": \"same-protocol\""));
        assert!(!text.contains("file_sha256"));
        assert_eq!(Sources::read(&dir).unwrap().unwrap(), s);
    }
}
