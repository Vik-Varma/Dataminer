//! The `dataminer protocol` subcommand: ingest a `bedrock-protocol-docs` checkout.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Arguments for `dataminer protocol`.
#[derive(clap::Args)]
pub struct ProtocolArgs {
    /// Path to a `Mojang/bedrock-protocol-docs` checkout (the directory
    /// containing `json/`), checked out at the tag for the target version.
    #[arg(long)]
    docs: PathBuf,
    /// The game version string for provenance, e.g. `1.26.51`.
    #[arg(long)]
    game_version: String,
    /// The repository ref for provenance, e.g. `v1.26.51`. Defaults to the game
    /// version prefixed with `v`.
    #[arg(long)]
    repo_ref: Option<String>,
    /// Base output directory. The spec is written to `<out>/<game-version>/packets.json`.
    #[arg(long)]
    out: PathBuf,
}

/// Run the protocol-docs ingest.
pub fn run(args: ProtocolArgs) -> Result<()> {
    let json_dir = args.docs.join("json");
    anyhow::ensure!(
        json_dir.is_dir(),
        "{} does not look like a bedrock-protocol-docs checkout (no json/ dir)",
        args.docs.display()
    );
    let repo_ref = args
        .repo_ref
        .unwrap_or_else(|| format!("v{}", args.game_version));

    let files = read_json_dir(&json_dir)?;
    anyhow::ensure!(
        !files.is_empty(),
        "no .json files in {}",
        json_dir.display()
    );

    let doc = dataminer_protocol::packets::parse(&files, &args.game_version, &repo_ref)?;

    let out_dir = args.out.join(&args.game_version);
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let path = out_dir.join("packets.json");
    std::fs::write(&path, doc.to_pretty_json()?)
        .with_context(|| format!("writing {}", path.display()))?;

    let p = &doc.payload;
    tracing::info!(
        "protocol {}: {} packets, {} enums, {} unresolved payloads",
        p.protocol_version,
        p.packets.len(),
        p.enums.len(),
        p.unresolved.len()
    );
    eprintln!(
        "wrote packets.json (protocol {}, {} packets, {} enums; {} unresolved) to {}",
        p.protocol_version,
        p.packets.len(),
        p.enums.len(),
        p.unresolved.len(),
        out_dir.display()
    );
    if !p.unresolved.is_empty() {
        eprintln!("unresolved payloads: {}", p.unresolved.join(", "));
    }
    Ok(())
}

/// Read every `*.json` directly inside `json_dir` into a basename→bytes map.
fn read_json_dir(json_dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for entry in
        std::fs::read_dir(json_dir).with_context(|| format!("reading {}", json_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        files.insert(name.to_string(), bytes);
    }
    Ok(files)
}
