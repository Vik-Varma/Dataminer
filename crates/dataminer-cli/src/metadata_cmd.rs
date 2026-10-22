//! The `dataminer metadata` subcommand: ingest a `bedrock-samples` checkout.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dataminer_metadata::registries::SimpleRegistry;

/// Arguments for `dataminer metadata`.
#[derive(clap::Args)]
pub struct MetadataArgs {
    /// Path to a `Mojang/bedrock-samples` checkout (the directory containing
    /// `metadata/` and `version.json`).
    #[arg(long)]
    samples: PathBuf,
    /// The game version string for provenance. If omitted, read from
    /// `version.json`'s `latest.version`.
    #[arg(long)]
    game_version: Option<String>,
    /// The repository ref for provenance, e.g. `v1.26.50.4`. Defaults to the
    /// game version prefixed with `v`.
    #[arg(long)]
    repo_ref: Option<String>,
    /// Directory to write spec documents into. Created if missing.
    #[arg(long)]
    out: PathBuf,
}

/// Run the metadata ingest.
pub fn run(args: MetadataArgs) -> Result<()> {
    let meta_dir = args.samples.join("metadata");
    anyhow::ensure!(
        meta_dir.is_dir(),
        "{} does not look like a bedrock-samples checkout (no metadata/ dir)",
        args.samples.display()
    );

    let game_version = match args.game_version {
        Some(v) => v,
        None => read_latest_version(&args.samples)
            .context("could not read version from version.json; pass --game-version")?,
    };
    let repo_ref = args.repo_ref.unwrap_or_else(|| format!("v{game_version}"));

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;

    let mut written = 0usize;

    // Blocks.
    {
        let bytes = read(&meta_dir, "vanilladata_modules/mojang-blocks.json")?;
        let doc = dataminer_metadata::blocks::parse(&bytes, &game_version, &repo_ref)?;
        write_doc(&args.out, "blocks.json", &doc.to_pretty_json()?)?;
        tracing::info!("blocks: {} types", doc.payload.blocks.len());
        written += 1;
    }
    // Items.
    {
        let bytes = read(&meta_dir, "vanilladata_modules/mojang-items.json")?;
        let doc = dataminer_metadata::registries::parse_items(&bytes, &game_version, &repo_ref)?;
        write_doc(&args.out, "items.json", &doc.to_pretty_json()?)?;
        tracing::info!("items: {} types", doc.payload.items.len());
        written += 1;
    }
    // Commands.
    {
        let bytes = read(&meta_dir, "command_modules/mojang-commands.json")?;
        let doc = dataminer_metadata::commands::parse(&bytes, &game_version, &repo_ref)?;
        write_doc(&args.out, "commands.json", &doc.to_pretty_json()?)?;
        tracing::info!("commands: {} commands", doc.payload.commands.len());
        written += 1;
    }
    // Simple registries.
    for (registry, file) in [
        (SimpleRegistry::Entities, "entities.json"),
        (SimpleRegistry::Biomes, "biomes.json"),
        (SimpleRegistry::Effects, "effects.json"),
        (SimpleRegistry::Enchantments, "enchantments.json"),
    ] {
        let bytes = read(&meta_dir, registry.rel_path())?;
        let doc = dataminer_metadata::registries::parse_simple(
            registry,
            &bytes,
            &game_version,
            &repo_ref,
        )?;
        write_doc(&args.out, file, &doc.to_pretty_json()?)?;
        tracing::info!("{}: {} entries", registry.kind(), doc.payload.entries.len());
        written += 1;
    }

    eprintln!(
        "wrote {written} spec documents for {game_version} to {}",
        args.out.display()
    );
    Ok(())
}

fn read(meta_dir: &Path, rel: &str) -> Result<Vec<u8>> {
    let path = meta_dir.join(rel);
    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
}

fn write_doc(out_dir: &Path, name: &str, json: &str) -> Result<()> {
    let path = out_dir.join(name);
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn read_latest_version(samples: &Path) -> Result<String> {
    let bytes = std::fs::read(samples.join("version.json")).context("reading version.json")?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    v.get("latest")
        .and_then(|l| l.get("version"))
        .and_then(|s| s.as_str())
        .map(ToString::to_string)
        .context("version.json has no latest.version")
}
