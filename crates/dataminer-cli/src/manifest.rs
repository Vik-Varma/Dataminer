//! `out/<game-version>/manifest.json`: one file listing every dataset written
//! for a game version. Every subcommand merges its datasets into it, so one
//! command never drops another's entries.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

/// Add or replace `entries` (each with a `dataset` key) in `<out_dir>/manifest.json`.
///
/// Each entry is stamped with the Dataminer commit that produced it; the
/// top-level `dataminer_commit` and `tool` name the last writer. The manifest's
/// `game_version` must match, and a known `wire_protocol` must agree with
/// `protocol` when both are given.
pub fn merge(
    out_dir: &Path,
    game_version: &str,
    protocol: Option<u32>,
    entries: Vec<Value>,
) -> Result<()> {
    let path = out_dir.join("manifest.json");
    let mut manifest: Value = match std::fs::read(&path) {
        Ok(b) => {
            serde_json::from_slice(&b).with_context(|| format!("parsing {}", path.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({
            "game_version": game_version,
            "wire_protocol": null,
            "datasets": [],
        }),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if manifest["game_version"] != json!(game_version) {
        bail!(
            "{} is for game version {}, not {game_version}",
            path.display(),
            manifest["game_version"]
        );
    }
    if let Some(p) = protocol {
        match manifest["wire_protocol"].as_u64() {
            Some(old) if old != u64::from(p) => {
                bail!("{} says wire protocol {old}, not {p}", path.display())
            }
            _ => manifest["wire_protocol"] = json!(p),
        }
    }
    manifest["dataminer_commit"] = json!(env!("DATAMINER_COMMIT"));
    manifest["tool"] = json!(concat!("dataminer ", env!("CARGO_PKG_VERSION")));
    let datasets = manifest["datasets"]
        .as_array_mut()
        .context("manifest.json has no datasets array")?;
    for mut entry in entries {
        entry["dataminer_commit"] = json!(env!("DATAMINER_COMMIT"));
        datasets.retain(|d| d["dataset"] != entry["dataset"]);
        datasets.push(entry);
    }
    datasets.sort_by(|a, b| a["dataset"].as_str().cmp(&b["dataset"].as_str()));
    let body = serde_json::to_string_pretty(&manifest)? + "\n";
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("dataminer-manifest-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(d: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(d.join("manifest.json")).unwrap()).unwrap()
    }

    #[test]
    fn keeps_other_datasets_and_stamps_each_entry() {
        let d = dir("keep");
        merge(&d, "1.0", Some(7), vec![json!({"dataset": "a"})]).unwrap();
        merge(&d, "1.0", None, vec![json!({"dataset": "b"})]).unwrap();
        let m = read(&d);
        let names: Vec<_> = m["datasets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["dataset"].clone())
            .collect();
        assert_eq!(names, vec![json!("a"), json!("b")]);
        assert_eq!(m["wire_protocol"], json!(7));
        assert!(m["datasets"][0]["dataminer_commit"].is_string());
    }

    #[test]
    fn sets_a_missing_protocol_and_rejects_a_conflict() {
        let d = dir("proto");
        merge(&d, "1.0", None, vec![json!({"dataset": "a"})]).unwrap();
        merge(&d, "1.0", Some(7), vec![json!({"dataset": "b"})]).unwrap();
        assert_eq!(read(&d)["wire_protocol"], json!(7));
        assert!(merge(&d, "1.0", Some(8), vec![json!({"dataset": "c"})]).is_err());
    }

    #[test]
    fn rejects_another_game_version() {
        let d = dir("ver");
        merge(&d, "1.0", None, vec![json!({"dataset": "a"})]).unwrap();
        assert!(merge(&d, "2.0", None, vec![json!({"dataset": "b"})]).is_err());
    }
}
