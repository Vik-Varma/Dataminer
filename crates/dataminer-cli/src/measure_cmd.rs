//! The `dataminer measure` subcommands: turn a headless client's run log from
//! our own strict-movement BDS into specs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::{Method, Provenance, SourceKind};
use serde_json::{Value, json};

/// Arguments for `dataminer measure bamboo`.
#[derive(clap::Args)]
pub struct BambooArgs {
    /// The run log (JSON lines) the headless client wrote.
    #[arg(long)]
    runs: PathBuf,
    /// The `bedrock_server` binary that was measured; its SHA-256 goes into the
    /// provenance.
    #[arg(long)]
    bds: PathBuf,
    /// The exact BDS build, e.g. `1.26.52.3`.
    #[arg(long)]
    bds_version: String,
    /// The game version the spec is filed under, e.g. `1.26.52`.
    #[arg(long)]
    game_version: String,
    /// The wire protocol of the measured server, recorded in a new manifest.
    #[arg(long)]
    protocol: Option<u32>,
    /// Base output directory. Writes `<out>/<game-version>/bamboo.json` and adds
    /// it to that directory's `manifest.json`.
    #[arg(long)]
    out: PathBuf,
}

/// Run `dataminer measure bamboo`.
pub fn run_bamboo(args: BambooArgs) -> Result<()> {
    let log =
        std::fs::read(&args.runs).with_context(|| format!("reading {}", args.runs.display()))?;
    let log_hash = Sha256Digest::of_bytes(&log);
    let runs =
        dataminer_measure::parse_runs(std::str::from_utf8(&log).context("run log is not UTF-8")?)?;
    let bds_hash = Sha256Digest::of_reader(
        std::fs::File::open(&args.bds)
            .with_context(|| format!("opening {}", args.bds.display()))?,
    )?;

    let provenance = Provenance {
        source: SourceKind::PacketCapture,
        method: Method::Measured,
        game_version: args.game_version.clone(),
        artifact: format!(
            "Linux Bedrock Dedicated Server {} (bedrock_server, minecraft.net), run on our own \
             machine with server-authoritative-movement-strict=true, \
             player-position-acceptance-threshold=0.0001, online-mode=false; positions read \
             from its CorrectPlayerMovePrediction packets to a headless client",
            args.bds_version
        ),
        artifact_sha256: Some(bds_hash),
        tool: dataminer_measure::TOOL.into(),
        notes: vec![
            format!("run log {log_hash}, {} runs", runs.len()),
            "collision boxes and the offset values are measured; the offset model's form \
             (inputs, generator, per-axis range and steps) was found by static and dynamic \
             analysis of the same BDS on the dirty side and is accepted only because it \
             predicts every measured stop within validation.tolerance_blocks"
                .into(),
            "outline use is inferred (static analysis only); rendering use is unknown".into(),
        ],
    };
    let doc = dataminer_measure::bamboo::derive(&runs, provenance)?;

    let out_dir = args.out.join(&args.game_version);
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let json = doc.to_pretty_json()?;
    let path = out_dir.join("bamboo.json");
    std::fs::write(&path, &json).with_context(|| format!("writing {}", path.display()))?;
    add_to_manifest(
        &out_dir,
        &args.game_version,
        args.protocol,
        json!({
            "dataset": dataminer_measure::bamboo::KIND,
            "file": "bamboo.json",
            "schema_version": dataminer_measure::bamboo::SCHEMA_VERSION,
            "records": doc.payload.states.len(),
            "source": format!("measured on BDS {}", args.bds_version),
            "source_sha256": doc.meta.provenance.artifact_sha256.as_ref().map(ToString::to_string),
        }),
    )?;
    let v = &doc.payload.validation;
    eprintln!(
        "wrote {} ({} states, {} stops, max error {} blocks)",
        path.display(),
        doc.payload.states.len(),
        v.observations.len(),
        v.max_abs_error_blocks
    );
    Ok(())
}

/// Add or replace one dataset entry in `<out_dir>/manifest.json`, keeping the rest.
fn add_to_manifest(
    out_dir: &Path,
    game_version: &str,
    protocol: Option<u32>,
    entry: Value,
) -> Result<()> {
    let path = out_dir.join("manifest.json");
    let mut manifest = match std::fs::read(&path) {
        Ok(b) => {
            serde_json::from_slice(&b).with_context(|| format!("parsing {}", path.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({
            "game_version": game_version,
            "wire_protocol": protocol,
            "datasets": [],
        }),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    manifest["dataminer_commit"] = json!(env!("DATAMINER_COMMIT"));
    manifest["tool"] = json!(concat!("dataminer ", env!("CARGO_PKG_VERSION")));
    let datasets = manifest["datasets"]
        .as_array_mut()
        .context("manifest.json has no datasets array")?;
    datasets.retain(|d| d["dataset"] != entry["dataset"]);
    datasets.push(entry);
    datasets.sort_by(|a, b| a["dataset"].as_str().cmp(&b["dataset"].as_str()));
    let body = serde_json::to_string_pretty(&manifest)? + "\n";
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
