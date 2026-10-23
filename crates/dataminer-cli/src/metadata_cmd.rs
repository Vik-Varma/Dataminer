//! The `dataminer metadata` subcommand: ingest a `bedrock-samples` checkout.
//!
//! Output follows Ferrock's requirements (`docs/dataminer-requirements.md`):
//! one directory per game version, `out/<game-version>/<dataset>.json`, plus a
//! `manifest.json` recording the game version, protocol (when known), source
//! file hashes, the Dataminer commit, and each dataset's schema version.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dataminer_core::Sha256Digest;
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
    /// The wire protocol number for this version, recorded in the manifest.
    /// Metadata alone does not state it; pass it when known (e.g. 2193).
    #[arg(long)]
    protocol: Option<u32>,
    /// Base output directory. Specs are written under `<out>/<game-version>/`.
    #[arg(long)]
    out: PathBuf,
}

/// One dataset written, for the manifest.
struct Written {
    dataset: &'static str,
    schema_version: u32,
    source: &'static str,
    source_sha256: Sha256Digest,
    records: usize,
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

    let out_dir = args.out.join(&game_version);
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let mut written = Vec::new();

    // Blocks.
    {
        let (bytes, hash) = read(&meta_dir, "vanilladata_modules/mojang-blocks.json")?;
        let doc = dataminer_metadata::blocks::parse(&bytes, &game_version, &repo_ref)?;
        write_doc(&out_dir, "blocks.json", &doc.to_pretty_json()?)?;
        tracing::info!("blocks: {} types", doc.payload.blocks.len());
        written.push(Written {
            dataset: "blocks",
            schema_version: dataminer_metadata::blocks::SCHEMA_VERSION,
            source: "vanilladata_modules/mojang-blocks.json",
            source_sha256: hash.clone(),
            records: doc.payload.blocks.len(),
        });

        // Block states + network hashes, derived from the same blocks source.
        let bs = dataminer_metadata::blockstates::enumerate(&doc);
        write_doc(&out_dir, "blockstates.json", &bs.to_pretty_json()?)?;
        tracing::info!(
            "blockstates: {} blocks, {} states ({} unresolved, {} collisions)",
            bs.payload.block_count,
            bs.payload.state_count,
            bs.payload.unresolved.len(),
            bs.payload.collisions.len()
        );
        written.push(Written {
            dataset: "blockstates",
            schema_version: dataminer_metadata::blockstates::SCHEMA_VERSION,
            source: "vanilladata_modules/mojang-blocks.json",
            source_sha256: hash,
            records: bs.payload.state_count,
        });
    }
    // Items.
    {
        let (bytes, hash) = read(&meta_dir, "vanilladata_modules/mojang-items.json")?;
        let doc = dataminer_metadata::registries::parse_items(&bytes, &game_version, &repo_ref)?;
        write_doc(&out_dir, "items.json", &doc.to_pretty_json()?)?;
        tracing::info!("items: {} types", doc.payload.items.len());
        written.push(Written {
            dataset: "items",
            schema_version: dataminer_metadata::registries::SCHEMA_VERSION,
            source: "vanilladata_modules/mojang-items.json",
            source_sha256: hash,
            records: doc.payload.items.len(),
        });
    }
    // Commands.
    {
        let (bytes, hash) = read(&meta_dir, "command_modules/mojang-commands.json")?;
        let doc = dataminer_metadata::commands::parse(&bytes, &game_version, &repo_ref)?;
        write_doc(&out_dir, "commands.json", &doc.to_pretty_json()?)?;
        tracing::info!("commands: {} commands", doc.payload.commands.len());
        written.push(Written {
            dataset: "commands",
            schema_version: dataminer_metadata::commands::SCHEMA_VERSION,
            source: "command_modules/mojang-commands.json",
            source_sha256: hash,
            records: doc.payload.commands.len(),
        });
    }
    // Molang queries and math functions.
    {
        let (bytes, hash) = read(&meta_dir, "molang_modules/mojang-molang-queries.json")?;
        let doc = dataminer_metadata::molang::parse(&bytes, &game_version, &repo_ref)?;
        write_doc(&out_dir, "molang.json", &doc.to_pretty_json()?)?;
        tracing::info!(
            "molang: {} queries, {} math functions",
            doc.payload.queries.len(),
            doc.payload.math_functions.len()
        );
        written.push(Written {
            dataset: "molang",
            schema_version: dataminer_metadata::molang::SCHEMA_VERSION,
            source: "molang_modules/mojang-molang-queries.json",
            source_sha256: hash,
            records: doc.payload.queries.len() + doc.payload.math_functions.len(),
        });
    }
    // Simple registries.
    for (registry, file, dataset) in [
        (SimpleRegistry::Entities, "entities.json", "entities"),
        (SimpleRegistry::Biomes, "biomes.json", "biomes"),
        (SimpleRegistry::Effects, "effects.json", "effects"),
        (
            SimpleRegistry::Enchantments,
            "enchantments.json",
            "enchantments",
        ),
    ] {
        let (bytes, hash) = read(&meta_dir, registry.rel_path())?;
        let doc = dataminer_metadata::registries::parse_simple(
            registry,
            &bytes,
            &game_version,
            &repo_ref,
        )?;
        write_doc(&out_dir, file, &doc.to_pretty_json()?)?;
        tracing::info!("{}: {} entries", registry.kind(), doc.payload.entries.len());
        written.push(Written {
            dataset,
            schema_version: dataminer_metadata::registries::SCHEMA_VERSION,
            source: registry.rel_path(),
            source_sha256: hash,
            records: doc.payload.entries.len(),
        });
    }

    write_manifest(&out_dir, &game_version, &repo_ref, args.protocol, &written)?;

    eprintln!(
        "wrote {} datasets + manifest for {game_version} to {}",
        written.len(),
        out_dir.display()
    );
    Ok(())
}

/// Read a source file and hash its exact bytes.
fn read(meta_dir: &Path, rel: &str) -> Result<(Vec<u8>, Sha256Digest)> {
    let path = meta_dir.join(rel);
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let hash = Sha256Digest::of_bytes(&bytes);
    Ok((bytes, hash))
}

fn write_doc(out_dir: &Path, name: &str, json: &str) -> Result<()> {
    let path = out_dir.join(name);
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn write_manifest(
    out_dir: &Path,
    game_version: &str,
    repo_ref: &str,
    protocol: Option<u32>,
    written: &[Written],
) -> Result<()> {
    use serde_json::json;
    let datasets: Vec<_> = written
        .iter()
        .map(|w| {
            json!({
                "dataset": w.dataset,
                "file": format!("{}.json", w.dataset),
                "schema_version": w.schema_version,
                "records": w.records,
                "source": format!("Mojang/bedrock-samples@{repo_ref} metadata/{}", w.source),
                "source_sha256": w.source_sha256.to_string(),
            })
        })
        .collect();
    let manifest = json!({
        "game_version": game_version,
        "wire_protocol": protocol,
        "dataminer_commit": env!("DATAMINER_COMMIT"),
        "tool": concat!("dataminer ", env!("CARGO_PKG_VERSION")),
        "datasets": datasets,
    });
    let path = out_dir.join("manifest.json");
    let body = serde_json::to_string_pretty(&manifest)? + "\n";
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
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
