//! `dataminer`: the command-line front end.
//!
//! It wires the extractor crates to the filesystem. Every subcommand writes spec
//! documents (pretty JSON with provenance) that a clean-room reimplementation can
//! check in and diff. Nothing here runs the game server; the binary commands read
//! an object file statically.

mod crosscheck_cmd;
mod fetch_cmd;
mod manifest;
mod measure_cmd;
mod metadata_cmd;
mod protocol_cmd;
mod report_cmd;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// Datamine Minecraft: Bedrock Edition into versioned, diffable specs.
#[derive(Parser)]
#[command(name = "dataminer", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download and pin the Mojang inputs for a game version into a working dir.
    Fetch(fetch_cmd::FetchArgs),
    /// Ingest Mojang's public `bedrock-samples` metadata into specs.
    Metadata(metadata_cmd::MetadataArgs),
    /// Ingest Mojang's public `bedrock-protocol-docs` into packet-layout specs.
    Protocol(protocol_cmd::ProtocolArgs),
    /// Derive specs from measurements taken on our own strict-movement BDS.
    Measure {
        #[command(subcommand)]
        what: MeasureCommand,
    },
    /// Summarize a binary (format, architecture, whether it is stripped).
    Inspect {
        /// The binary file to inspect.
        file: PathBuf,
    },
    /// Extract facts from a Bedrock Dedicated Server binary.
    Binary {
        #[command(subcommand)]
        what: BinaryCommand,
    },
    /// Summarize a spec's coverage; with --strict, exit non-zero on any gaps.
    Report(report_cmd::ReportArgs),
    /// Cross-check a dataset against an independent source and report differences.
    Crosscheck {
        #[command(subcommand)]
        what: CrosscheckCommand,
    },
    /// Diff two spec documents and report what changed between versions.
    Diff {
        /// The older spec JSON.
        old: PathBuf,
        /// The newer spec JSON.
        new: PathBuf,
        /// Write the full diff as JSON to this path instead of only summarizing.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CrosscheckCommand {
    /// Check blockstates hashes against an authoritative block_states.nbt.
    Blockstates(crosscheck_cmd::BlockStatesArgs),
}

#[derive(Subcommand)]
enum MeasureCommand {
    /// Bamboo collision boxes per state and the per-position offset.
    Bamboo(measure_cmd::MeasureArgs),
    /// Per-tick movement traces (position, velocity, sprint and swim flags).
    Traces(measure_cmd::TracesArgs),
}

#[derive(Subcommand)]
enum BinaryCommand {
    /// Extract the symbol table (demangled), or dynamic symbols if stripped.
    Symbols(BinaryExtractArgs),
    /// Recover C++ class names from Itanium RTTI typeinfo symbols.
    Rtti(BinaryExtractArgs),
    /// Extract printable string literals from data sections.
    Strings {
        #[command(flatten)]
        common: BinaryExtractArgs,
        /// Minimum run length for an extracted string.
        #[arg(long, default_value_t = dataminer_binary::strings::DEFAULT_MIN_LEN)]
        min_len: usize,
    },
}

#[derive(clap::Args)]
struct BinaryExtractArgs {
    /// The binary file to read.
    file: PathBuf,
    /// The Minecraft: Bedrock version string for provenance, e.g. `1.26.50.4`.
    #[arg(long)]
    game_version: String,
    /// Write the spec JSON here. Defaults to stdout.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> Result<ExitCode> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let ok = ExitCode::SUCCESS;
    match cli.command {
        Command::Fetch(args) => fetch_cmd::run(args).map(|()| ok),
        Command::Metadata(args) => metadata_cmd::run(args).map(|()| ok),
        Command::Protocol(args) => protocol_cmd::run(args).map(|()| ok),
        Command::Measure { what } => match what {
            MeasureCommand::Bamboo(a) => measure_cmd::run_bamboo(a).map(|()| ok),
            MeasureCommand::Traces(a) => measure_cmd::run_traces(a).map(|()| ok),
        },
        Command::Report(args) => report_cmd::run(args),
        Command::Crosscheck { what } => match what {
            CrosscheckCommand::Blockstates(a) => crosscheck_cmd::run_blockstates(a).map(|()| ok),
        },
        Command::Inspect { file } => inspect(&file).map(|()| ok),
        Command::Binary { what } => run_binary(what).map(|()| ok),
        Command::Diff { old, new, out } => run_diff(&old, &new, out.as_deref()).map(|()| ok),
    }
}

fn inspect(file: &std::path::Path) -> Result<()> {
    let data = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let info = dataminer_binary::inspect(&data).context("parsing binary")?;
    println!("format:               {:?}", info.format);
    println!("architecture:         {:?}", info.architecture);
    println!("has symbol table:     {}", info.has_symbol_table);
    println!("symbol count:         {}", info.symbol_count);
    println!("dynamic symbol count: {}", info.dynamic_symbol_count);
    if !info.has_symbol_table {
        println!("note: binary is stripped; only dynamic symbols are available.");
    }
    Ok(())
}

fn run_binary(what: BinaryCommand) -> Result<()> {
    match what {
        BinaryCommand::Symbols(a) => {
            let data = read_binary(&a.file)?;
            let doc =
                dataminer_binary::symbols::extract(&data, &a.game_version, &artifact(&a.file))
                    .context("extracting symbols")?;
            emit(&doc.to_pretty_json()?, a.out.as_deref())
        }
        BinaryCommand::Rtti(a) => {
            let data = read_binary(&a.file)?;
            let doc = dataminer_binary::rtti::extract(&data, &a.game_version, &artifact(&a.file))
                .context("recovering RTTI")?;
            emit(&doc.to_pretty_json()?, a.out.as_deref())
        }
        BinaryCommand::Strings { common, min_len } => {
            let data = read_binary(&common.file)?;
            let doc = dataminer_binary::strings::extract(
                &data,
                min_len,
                &common.game_version,
                &artifact(&common.file),
            )
            .context("extracting strings")?;
            emit(&doc.to_pretty_json()?, common.out.as_deref())
        }
    }
}

fn run_diff(
    old: &std::path::Path,
    new: &std::path::Path,
    out: Option<&std::path::Path>,
) -> Result<()> {
    let old_json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(old).with_context(|| format!("reading {}", old.display()))?,
    )
    .context("parsing old spec")?;
    let new_json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(new).with_context(|| format!("reading {}", new.display()))?,
    )
    .context("parsing new spec")?;

    // Diff the payloads if both are spec documents; otherwise diff whole values.
    let (old_p, new_p) = match (old_json.get("payload"), new_json.get("payload")) {
        (Some(a), Some(b)) => (a, b),
        _ => (&old_json, &new_json),
    };

    let d = dataminer_diff::diff(old_p, new_p);
    let (added, removed, modified) = d.summary();
    eprintln!("diff: {added} added, {removed} removed, {modified} modified");

    if let Some(path) = out {
        let json = serde_json::to_string_pretty(&d)? + "\n";
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());
    } else {
        println!("{}", serde_json::to_string_pretty(&d)?);
    }
    Ok(())
}

fn read_binary(file: &std::path::Path) -> Result<Vec<u8>> {
    std::fs::read(file).with_context(|| format!("reading {}", file.display()))
}

fn artifact(file: &std::path::Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.display().to_string())
}

fn emit(json: &str, out: Option<&std::path::Path>) -> Result<()> {
    match out {
        Some(path) => {
            std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => print!("{json}"),
    }
    Ok(())
}
