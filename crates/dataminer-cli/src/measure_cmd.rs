//! The `dataminer measure` subcommands: turn a headless client's run log from
//! our own strict-movement BDS into specs.

use std::path::PathBuf;

use anyhow::{Context, Result};
use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::{Method, Provenance, SourceKind};
use serde_json::json;

/// Arguments shared by the `dataminer measure` subcommands.
#[derive(clap::Args)]
pub struct MeasureArgs {
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
    /// Base output directory. Writes `<out>/<game-version>/<dataset>.json` and
    /// adds it to that directory's `manifest.json`.
    #[arg(long)]
    out: PathBuf,
}

/// Read the run log and hash the measured server binary.
fn inputs(args: &MeasureArgs) -> Result<(String, Sha256Digest, Sha256Digest)> {
    let log =
        std::fs::read(&args.runs).with_context(|| format!("reading {}", args.runs.display()))?;
    let log_hash = Sha256Digest::of_bytes(&log);
    let log = String::from_utf8(log).context("run log is not UTF-8")?;
    let bds_hash = Sha256Digest::of_reader(
        std::fs::File::open(&args.bds)
            .with_context(|| format!("opening {}", args.bds.display()))?,
    )?;
    Ok((log, log_hash, bds_hash))
}

fn provenance(args: &MeasureArgs, bds_hash: Sha256Digest, notes: Vec<String>) -> Provenance {
    Provenance {
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
        notes,
    }
}

/// Run `dataminer measure bamboo`.
pub fn run_bamboo(args: MeasureArgs) -> Result<()> {
    let (log, log_hash, bds_hash) = inputs(&args)?;
    let runs = dataminer_measure::parse_runs(&log)?;
    let provenance = provenance(
        &args,
        bds_hash,
        vec![
            format!("run log {log_hash}, {} runs", runs.len()),
            "collision boxes and the offset values are measured; the offset model's form \
             (inputs, generator, per-axis range and steps) was found by static and dynamic \
             analysis of the same BDS on the dirty side and is accepted only because it \
             predicts every measured stop within validation.tolerance_blocks"
                .into(),
            "outline use is inferred (static analysis only); rendering use is unknown".into(),
        ],
    );
    let doc = dataminer_measure::bamboo::derive(&runs, provenance)?;

    let path = write(&args, &doc, doc.payload.states.len())?;
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

/// Arguments for `dataminer measure traces`.
#[derive(clap::Args)]
pub struct TracesArgs {
    #[command(flatten)]
    common: MeasureArgs,
    /// The client's verification log: replays of each finished trace that
    /// claimed every tick. A trace is `measured` only if these drew no correction.
    #[arg(long)]
    verify: Option<PathBuf>,
}

/// Run `dataminer measure traces`.
pub fn run_traces(args: TracesArgs) -> Result<()> {
    let verify = args
        .verify
        .as_ref()
        .map(|p| std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display())))
        .transpose()?;
    let args = args.common;
    let (log, log_hash, bds_hash) = inputs(&args)?;
    let provenance = provenance(
        &args,
        bds_hash,
        vec![
            format!("trace log {log_hash}"),
            match &verify {
                Some(v) => format!("verification log {}", Sha256Digest::of_bytes(v.as_bytes())),
                None => "no verification log: every trace is unknown".into(),
            },
            "tick i is the server's state after it processed input i; each trace was replayed \
             from the same start with the same inputs until every tick had been corrected"
                .into(),
            "the client sent PlayerAuthInput at 20 Hz with input mode mouse, play mode normal, \
             interaction model crosshair, the move vector also as the analogue and raw move \
             vectors, camera orientation from yaw and pitch, and a zero delta"
                .into(),
        ],
    );
    let doc = dataminer_measure::traces::derive(&log, verify.as_deref(), provenance)?;
    let path = write(&args, &doc, doc.payload.traces.len())?;
    let unknown = doc
        .payload
        .traces
        .iter()
        .filter(|t| t.method == Method::Unknown)
        .count();
    eprintln!(
        "wrote {} ({} traces, {unknown} with unrevealed ticks)",
        path.display(),
        doc.payload.traces.len()
    );
    Ok(())
}

/// Write a spec to `<out>/<game-version>/<kind>.json` and list it in the manifest.
fn write<T: serde::Serialize>(
    args: &MeasureArgs,
    doc: &dataminer_core::SpecDocument<T>,
    records: usize,
) -> Result<PathBuf> {
    let out_dir = args.out.join(&args.game_version);
    std::fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let file = format!("{}.json", doc.meta.kind);
    let path = out_dir.join(&file);
    std::fs::write(&path, doc.to_pretty_json()?)
        .with_context(|| format!("writing {}", path.display()))?;
    crate::manifest::merge(
        &out_dir,
        &args.game_version,
        args.protocol,
        vec![json!({
            "dataset": doc.meta.kind,
            "file": file,
            "schema_version": doc.meta.schema_version,
            "records": records,
            "source": format!("measured on BDS {}", args.bds_version),
            "source_sha256": doc.meta.provenance.artifact_sha256.as_ref().map(ToString::to_string),
        })],
    )?;
    Ok(path)
}
