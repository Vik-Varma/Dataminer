//! `dataminer`: the command-line front end.
//!
//! It wires the extractor crates to the filesystem. Every subcommand writes spec
//! documents (pretty JSON with provenance) that a clean-room reimplementation can
//! check in and diff. Nothing here runs the game server; the binary commands read
//! an object file statically.

mod metadata_cmd;

use std::path::PathBuf;

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
    /// Ingest Mojang's public `bedrock-samples` metadata into specs.
    Metadata(metadata_cmd::MetadataArgs),
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

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Metadata(args) => metadata_cmd::run(args),
        Command::Inspect { file } => inspect(&file),
        Command::Binary { what } => run_binary(what),
        Command::Diff { old, new, out } => run_diff(&old, &new, out.as_deref()),
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
