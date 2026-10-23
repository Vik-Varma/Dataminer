//! The `dataminer fetch` subcommand: download and pin the inputs for a version.

use std::path::PathBuf;

use anyhow::Result;
use dataminer_fetch::sources::{Match, Sources};

/// Arguments for `dataminer fetch`.
#[derive(clap::Args)]
pub struct FetchArgs {
    /// The target game version, e.g. `1.26.51`.
    #[arg(long)]
    game_version: String,
    /// Base working directory. Inputs land in `<out>/<game-version>/`; keep it
    /// git-ignored (`work/` is).
    #[arg(long)]
    out: PathBuf,
    /// Accept the Minecraft EULA (https://www.minecraft.net/eula), which covers
    /// every source fetched. Required to fetch.
    #[arg(long)]
    accept_eula: bool,
    /// Don't fetch the Bedrock Dedicated Server.
    #[arg(long)]
    skip_bds: bool,
    /// Use a BDS zip you downloaded yourself instead of downloading it. It must
    /// match the SHA-256 the index lists for the version. The index and
    /// Mojang's download-links API must still be reachable.
    #[arg(long, conflicts_with = "skip_bds")]
    bds_zip: Option<PathBuf>,
    /// Check out this `bedrock-samples` tag instead of resolving one.
    #[arg(long)]
    samples_ref: Option<String>,
    /// Check out this `bedrock-protocol-docs` tag instead of resolving one.
    #[arg(long)]
    docs_ref: Option<String>,
    /// Re-check the hashes of an existing fetch; download nothing.
    #[arg(long, conflicts_with_all = ["accept_eula", "skip_bds", "bds_zip", "samples_ref", "docs_ref"])]
    verify: bool,
}

/// Run the fetch, or the verification of an earlier one.
pub fn run(args: FetchArgs) -> Result<()> {
    if args.verify {
        let dir = args.out.join(&args.game_version);
        let sources = dataminer_fetch::verify(&dir)?;
        summarize(&sources);
        eprintln!(
            "verified {}: every source matches sources.json",
            dir.display()
        );
        return Ok(());
    }

    let report = dataminer_fetch::fetch(&dataminer_fetch::Options {
        game_version: args.game_version,
        out: args.out,
        eula_accepted: args.accept_eula,
        skip_bds: args.skip_bds,
        bds_zip: args.bds_zip,
        samples_ref: args.samples_ref,
        docs_ref: args.docs_ref,
        fetched_by: format!(
            "dataminer {} ({})",
            env!("CARGO_PKG_VERSION"),
            env!("DATAMINER_COMMIT")
        ),
        remotes: dataminer_fetch::Remotes::default(),
    })?;
    summarize(&report.sources);
    if report.fetched.is_empty() {
        eprintln!("{} is up to date; nothing fetched", report.dir.display());
    } else {
        eprintln!("wrote {}", report.dir.join("sources.json").display());
    }
    Ok(())
}

fn summarize(sources: &Sources) {
    let protocol = sources.protocol.map_or("unknown".into(), |p| p.to_string());
    eprintln!("target {} (protocol {protocol})", sources.game_version);
    for s in &sources.sources {
        let what = s.git_ref.as_deref().or(s.file.as_deref()).unwrap_or("?");
        let how = match s.r#match {
            Match::Exact => "exact",
            Match::SameProtocol => "same protocol, earlier version",
            Match::Explicit => "explicit ref, unchecked",
        };
        eprintln!(
            "  {:<22} {what:<30} game {:<12} {how}",
            s.kind.dir(),
            s.game_version
        );
        for note in &s.notes {
            eprintln!("    note: {note}");
        }
    }
}
