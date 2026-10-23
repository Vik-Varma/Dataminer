//! Download and pin the inputs Dataminer reads, for one target game version.
//!
//! A fetch lays out `<out>/<game-version>/` with one directory per source
//! (`bedrock-protocol-docs/`, `bedrock-samples/`, `bedrock-server/`) and a
//! `sources.json` recording where each came from and the hashes of what landed.
//! Re-running is a no-op when the cache is intact, and fails loudly when it is
//! not. See `docs/fetcher-requirements.md`.
//!
//! Every source is Mojang's and under the Minecraft EULA, so nothing runs
//! without the user's acceptance, and nothing fetched may be committed.

pub mod bds;
pub mod git;
pub mod sources;
pub mod tree;
pub mod version;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dataminer_core::Sha256Digest;

use sources::{Kind, Match, Source, Sources};
use version::{Pick, Version};

/// Where each source comes from.
pub struct Remotes {
    pub samples: String,
    pub docs: String,
    /// Base URL of the EndstoneMC `bedrock-server-data` index.
    pub bds_index: String,
    /// Mojang's download-links API, which names the current BDS release.
    pub bds_links: String,
    /// BDS is only downloaded from URLs that start with this (Mojang's host).
    pub bds_host: String,
    /// curl's `--proto` list for downloads.
    pub curl_protocols: String,
}

impl Default for Remotes {
    fn default() -> Self {
        Self {
            samples: "https://github.com/Mojang/bedrock-samples".into(),
            docs: "https://github.com/Mojang/bedrock-protocol-docs".into(),
            bds_index: "https://raw.githubusercontent.com/EndstoneMC/bedrock-server-data/HEAD"
                .into(),
            bds_links: "https://net-secondary.web.minecraft-services.net/api/v1.0/download/links"
                .into(),
            bds_host: "https://www.minecraft.net/".into(),
            curl_protocols: "=https".into(),
        }
    }
}

/// What to fetch.
pub struct Options {
    /// The target game version, e.g. `1.26.51`.
    pub game_version: String,
    /// The base working directory; the fetch lands in `<out>/<game-version>/`.
    pub out: PathBuf,
    /// The user accepted the Minecraft EULA for this fetch.
    pub eula_accepted: bool,
    pub skip_bds: bool,
    /// A BDS zip the user downloaded themselves, used instead of downloading.
    pub bds_zip: Option<PathBuf>,
    /// Tags to use instead of resolving one from the target.
    pub samples_ref: Option<String>,
    pub docs_ref: Option<String>,
    /// Recorded in `sources.json`, e.g. `dataminer 0.0.1 (c222f2f)`.
    pub fetched_by: String,
    pub remotes: Remotes,
}

/// The result of a fetch.
pub struct Report {
    pub dir: PathBuf,
    pub sources: Sources,
    /// The sources downloaded by this run; empty when the cache was complete.
    pub fetched: Vec<Kind>,
}

/// Fetch every source missing from `<out>/<game-version>/` and verify the rest.
pub fn fetch(opts: &Options) -> Result<Report> {
    anyhow::ensure!(
        opts.eula_accepted,
        "every source is Mojang's and under the Minecraft EULA \
         (https://www.minecraft.net/eula); pass --accept-eula to accept it and fetch"
    );
    // A canonical three-part version, so the directory, index path and git
    // refs all name the same thing.
    let target = Version::parse(&opts.game_version)
        .filter(|v| v.game() == *v && v.to_string() == opts.game_version)
        .with_context(|| {
            format!(
                "{:?} is not a release version like 1.26.51",
                opts.game_version
            )
        })?;
    let dir = opts.out.join(&opts.game_version);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let existing = Sources::read(&dir)?;
    if let Some(e) = &existing {
        anyhow::ensure!(
            e.game_version == opts.game_version,
            "{} records a fetch of {}, not {}",
            dir.join(sources::FILE_NAME).display(),
            e.game_version,
            opts.game_version
        );
    }
    let mut sources = existing.unwrap_or_else(|| Sources {
        schema_version: sources::SCHEMA_VERSION,
        game_version: opts.game_version.clone(),
        protocol: None,
        eula_accepted_at: String::new(),
        fetched_by: String::new(),
        sources: Vec::new(),
    });
    let mut fetched = Vec::new();

    // Docs first: they state the target's protocol, which picking a samples
    // tag can depend on. Each source is recorded as soon as it lands, so a
    // failure later on leaves a fetch that can be resumed.
    for kind in [
        Kind::BedrockProtocolDocs,
        Kind::BedrockSamples,
        Kind::BedrockServer,
    ] {
        let wanted_ref = match kind {
            Kind::BedrockProtocolDocs => opts.docs_ref.as_deref(),
            Kind::BedrockSamples => opts.samples_ref.as_deref(),
            Kind::BedrockServer => None,
        };
        if let Some(rec) = sources.get(kind) {
            reuse(&dir, rec, wanted_ref)?;
            continue;
        }
        let src = match kind {
            Kind::BedrockProtocolDocs => fetch_docs(&dir, &target, opts)?,
            Kind::BedrockSamples => fetch_samples(&dir, &target, sources.protocol, opts)?,
            Kind::BedrockServer if opts.skip_bds => continue,
            Kind::BedrockServer => fetch_bds(&dir, &target, opts)?,
        };
        // An explicit docs ref may be another version, so its protocol is not
        // the target's.
        if kind == Kind::BedrockProtocolDocs && src.r#match != Match::Explicit {
            sources.protocol = src.protocol;
        }
        sources.sources.push(src);
        sources.sources.sort_by_key(|s| s.kind);
        sources.eula_accepted_at = sources::now_rfc3339();
        sources.fetched_by = opts.fetched_by.clone();
        sources.write(&dir)?;
        fetched.push(kind);
    }

    Ok(Report {
        dir,
        sources,
        fetched,
    })
}

/// Re-check every source recorded in `dir/sources.json` without downloading.
/// Docs and samples must be recorded; BDS may have been skipped.
pub fn verify(dir: &Path) -> Result<Sources> {
    let sources = Sources::read(dir)?
        .with_context(|| format!("no {} in {}", sources::FILE_NAME, dir.display()))?;
    let missing = [Kind::BedrockProtocolDocs, Kind::BedrockSamples]
        .into_iter()
        .filter(|k| sources.get(*k).is_none())
        .map(|k| format!("{} is not recorded; the fetch is incomplete", k.dir()));
    let problems: Vec<String> = missing
        .chain(
            sources
                .sources
                .iter()
                .filter_map(|s| check(dir, s).err().map(|e| format!("{e:#}"))),
        )
        .collect();
    anyhow::ensure!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(sources)
}

fn reuse(dir: &Path, rec: &Source, wanted_ref: Option<&str>) -> Result<()> {
    if let Some(wanted) = wanted_ref {
        anyhow::ensure!(
            rec.git_ref.as_deref() == Some(wanted),
            "{} was already fetched at {}, not {wanted}; delete {} to fetch another ref",
            rec.path,
            rec.git_ref.as_deref().unwrap_or("?"),
            dir.join(&rec.path).display()
        );
    }
    check(dir, rec)
}

/// Fail unless the cached source still has the recorded contents and commit.
fn check(dir: &Path, rec: &Source) -> Result<()> {
    anyhow::ensure!(
        rec.path == rec.kind.dir(),
        "{} records {} at {:?}, not {}",
        sources::FILE_NAME,
        rec.kind.dir(),
        rec.path,
        rec.kind.dir()
    );
    let path = dir.join(&rec.path);
    anyhow::ensure!(
        path.is_dir(),
        "{} is recorded in {} but missing",
        path.display(),
        sources::FILE_NAME
    );
    let (digest, files) = tree::digest(&path)?;
    anyhow::ensure!(
        digest == rec.tree_sha256 && files == rec.files,
        "{} has changed since it was fetched (now {files} files, {digest}; recorded {} files, {}); \
         delete it and fetch again",
        path.display(),
        rec.files,
        rec.tree_sha256
    );
    if let Some(commit) = &rec.commit {
        let head = git::head(&path)?;
        anyhow::ensure!(
            &head == commit,
            "{} is at commit {head}, recorded {commit}",
            path.display()
        );
    }
    Ok(())
}

fn fetch_docs(dir: &Path, target: &Version, opts: &Options) -> Result<Source> {
    let url = &opts.remotes.docs;
    let mut notes = Vec::new();
    let (tag, how) = match &opts.docs_ref {
        Some(r) => (r.clone(), Match::Explicit),
        None => match version::pick(&git::ls_remote_tags(url)?, target) {
            Pick::Exact(t) => (t, Match::Exact),
            pick => {
                // Mojang's bot publishes each release on an `automated/<version>`
                // branch, but not every release gets a tag (1.26.52 didn't).
                let branch = format!("automated/{target}");
                if !git::has_branch(url, &branch)? {
                    let near = match pick {
                        Pick::Exact(t) | Pick::SameTrain(t) => vec![t],
                        Pick::None(near) => near,
                    };
                    return no_tag("bedrock-protocol-docs", target, &near, "--docs-ref");
                }
                notes.push(format!(
                    "no release tag for {target}; used Mojang's {branch} branch"
                ));
                (branch, Match::Exact)
            }
        },
    };
    let staging = clone(dir, Kind::BedrockProtocolDocs, url, &tag, "json")?;
    let mut src = record(&staging, Kind::BedrockProtocolDocs, url, &tag, how)?;
    src.protocol = Some(git::protocol_in_dir(&staging.join("json"))?);
    src.notes = notes;
    install(dir, Kind::BedrockProtocolDocs, &staging)?;
    Ok(src)
}

fn fetch_samples(
    dir: &Path,
    target: &Version,
    protocol: Option<u32>,
    opts: &Options,
) -> Result<Source> {
    let url = &opts.remotes.samples;
    let mut notes = Vec::new();
    let (tag, how, tag_protocol) = match &opts.samples_ref {
        Some(r) => (r.clone(), Match::Explicit, None),
        None => match version::pick(&git::ls_remote_tags(url)?, target) {
            Pick::Exact(t) => (t, Match::Exact, protocol),
            Pick::SameTrain(t) => {
                // Accept an earlier hotfix only if protocol-docs at its version
                // states the same protocol as the target.
                let game = Version::parse(&t).map(|v| v.game()).context("tag")?;
                let docs_tag = format!("v{game}");
                let docs = dir.join(Kind::BedrockProtocolDocs.dir());
                let p = git::protocol_at_tag(&docs, &docs_tag).with_context(|| {
                    format!(
                        "bedrock-samples has no tag for {target}; the nearest is {t}, but its \
                         protocol could not be read from bedrock-protocol-docs {docs_tag}. \
                         Pass --samples-ref {t} to use it anyway"
                    )
                })?;
                anyhow::ensure!(
                    Some(p) == protocol,
                    "bedrock-samples has no tag for {target}; the nearest is {t}, which is \
                     protocol {p} (bedrock-protocol-docs {docs_tag}), not the target's {}. \
                     Pass --samples-ref to use another tag anyway",
                    protocol.map_or("unknown".into(), |p| p.to_string())
                );
                notes.push(format!(
                    "no bedrock-samples tag for {target}; {t} is the same protocol ({p}, per \
                     bedrock-protocol-docs {docs_tag})"
                ));
                (t, Match::SameProtocol, Some(p))
            }
            Pick::None(near) => no_tag("bedrock-samples", target, &near, "--samples-ref")?,
        },
    };
    let staging = clone(dir, Kind::BedrockSamples, url, &tag, "metadata")?;
    let mut src = record(&staging, Kind::BedrockSamples, url, &tag, how)?;
    if let Some(v) = samples_version(&staging) {
        src.game_version = v;
    }
    src.protocol = tag_protocol;
    src.notes = notes;
    install(dir, Kind::BedrockSamples, &staging)?;
    Ok(src)
}

fn fetch_bds(dir: &Path, target: &Version, opts: &Options) -> Result<Source> {
    let r = &opts.remotes;
    let index_url = format!("{}/release/{target}/metadata.json", r.bds_index);
    let index = bds::download(&index_url, None, &r.curl_protocols).with_context(|| {
        format!(
            "no BDS index entry for {target} (see {}/versions.json for the versions it lists)",
            r.bds_index
        )
    })?;
    let entry = bds::parse_index(&index)?;
    anyhow::ensure!(
        entry.url.starts_with(&r.bds_host),
        "the index points BDS at {}, which is not Mojang's {}; refusing to download a mirror",
        entry.url,
        r.bds_host
    );
    let game_version = bds::version_of(&entry.url).with_context(|| {
        format!(
            "the index's URL {} is not a BDS download (…/bedrock-server-<version>.zip)",
            entry.url
        )
    })?;
    anyhow::ensure!(
        Version::parse(&game_version).map(|v| v.game()).as_ref() == Some(target),
        "the index entry for {target} is BDS {game_version}, not {target}"
    );
    let file = format!("bedrock-server-{game_version}.zip");

    // Mojang's own links API names only the current release. When that is the
    // target, the index must agree with it.
    let mut notes = Vec::new();
    let links = bds::download(&r.bds_links, None, &r.curl_protocols).with_context(|| {
        format!(
            "reading Mojang's download-links API at {}. If a proxy refused the host, that is \
             a policy denial: don't route around it",
            r.bds_links
        )
    })?;
    let links: serde_json::Value = serde_json::from_slice(&links).with_context(|| {
        format!(
            "Mojang's download-links API at {} answered with something that isn't the JSON it \
             publishes (a proxy's block page?); not skipping the check",
            r.bds_links
        )
    })?;
    match bds::current_linux_url(&links) {
        Some(url) if url == entry.url => {
            notes.push("download URL confirmed by Mojang's download-links API".to_string());
        }
        Some(url)
            if bds::version_of(&url)
                .and_then(|v| Version::parse(&v))
                .map(|v| v.game())
                .as_ref()
                == Some(target) =>
        {
            anyhow::bail!(
                "the index says {}, but Mojang's download-links API says {url} for the same \
                 version; not guessing which is right",
                entry.url
            )
        }
        Some(url) => notes.push(format!(
            "not the current release (Mojang's download-links API lists {url}); URL from the \
             index only"
        )),
        None => notes.push(
            "Mojang's download-links API lists no Linux build; URL from the index only".to_string(),
        ),
    }

    // Before downloading anything, make sure nothing unrecorded is in the way.
    let dest = dir.join(Kind::BedrockServer.dir());
    let staging = fresh_staging(dir, Kind::BedrockServer)?;
    // A fixed local name: nothing from the index becomes a path.
    let download = dir.join("bedrock-server.zip.partial");
    let zip = opts.bds_zip.clone().unwrap_or_else(|| download.clone());
    let unpacked = unpack_bds(&entry, &zip, opts.bds_zip.is_none(), &staging, r, &file);
    if opts.bds_zip.is_none() {
        let removed = std::fs::remove_file(&download);
        // After a failure the zip may not exist; keep the real error.
        if unpacked.is_ok() {
            removed.with_context(|| format!("removing {}", download.display()))?;
        }
    }
    let (file_sha256, (tree_sha256, files)) = match unpacked {
        Ok(digests) => digests,
        Err(e) => {
            // Best effort, keeping the real error: the next run replaces it anyway.
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    std::fs::rename(&staging, &dest)?;

    if let Some(p) = &opts.bds_zip {
        notes.push(format!(
            "zip supplied by the user ({}), not downloaded; its SHA-256 matches the index",
            p.display()
        ));
    }
    Ok(Source {
        kind: Kind::BedrockServer,
        path: Kind::BedrockServer.dir().into(),
        url: entry.url,
        git_ref: None,
        commit: None,
        file: Some(file),
        file_sha256: Some(file_sha256),
        index_url: Some(index_url),
        index_sha256: Some(Sha256Digest::of_bytes(&index)),
        game_version,
        protocol: None,
        r#match: Match::Exact,
        tree_sha256,
        files,
        fetched_at: sources::now_rfc3339(),
        notes,
    })
}

/// Download the BDS zip to `zip` (unless the user supplied it), check it
/// against the index's SHA-256, and unpack it into `staging`. Returns the
/// zip's digest and the unpacked tree's digest and file count.
fn unpack_bds(
    entry: &bds::IndexEntry,
    zip: &Path,
    download: bool,
    staging: &Path,
    r: &Remotes,
    file: &str,
) -> Result<(Sha256Digest, (Sha256Digest, u64))> {
    if download {
        bds::download(&entry.url, Some(zip), &r.curl_protocols).with_context(|| {
            format!(
                "downloading {}. If a proxy refused the host, that is a policy denial: \
                 don't route around it. Mojang's CDN also refuses some clients, cloud \
                 machines among them. You can download the zip yourself from \
                 https://www.minecraft.net/en-us/download/server/bedrock and pass \
                 --bds-zip <file>; it is checked against the index's SHA-256",
                entry.url
            )
        })?;
    }
    let file_sha256 = Sha256Digest::of_reader(std::io::BufReader::new(
        std::fs::File::open(zip).with_context(|| format!("opening {}", zip.display()))?,
    ))?;
    anyhow::ensure!(
        file_sha256 == entry.sha256,
        "{} has SHA-256 {file_sha256}, but the index says {}; not using it",
        zip.display(),
        entry.sha256
    );
    bds::extract(zip, staging)?;
    anyhow::ensure!(
        staging.join("bedrock_server").is_file(),
        "{file} has no bedrock_server at its root; is it the Linux build?"
    );
    Ok((file_sha256, tree::digest(staging)?))
}

fn no_tag<T>(repo: &str, target: &Version, near: &[String], flag: &str) -> Result<T> {
    let near = if near.is_empty() {
        "none".to_string()
    } else {
        near.join(", ")
    };
    anyhow::bail!("{repo} has no tag for {target} (nearest: {near}); pass {flag} <tag> to use one")
}

/// Clone into a staging dir and return it. The caller reads and hashes it
/// there, then [`install`]s it, so a failure or interruption at any point
/// leaves only a staging dir, which the next run replaces.
fn clone(dir: &Path, kind: Kind, url: &str, tag: &str, subdir: &str) -> Result<PathBuf> {
    let staging = fresh_staging(dir, kind)?;
    git::clone_sparse(url, tag, subdir, &staging)?;
    anyhow::ensure!(
        staging.join(subdir).is_dir(),
        "{} {tag} has no {subdir}/",
        kind.dir()
    );
    Ok(staging)
}

/// Move a finished staging dir into place as `kind`'s directory.
fn install(dir: &Path, kind: Kind, staging: &Path) -> Result<()> {
    let dest = dir.join(kind.dir());
    std::fs::rename(staging, &dest)
        .with_context(|| format!("moving {} to {}", staging.display(), dest.display()))
}

/// An empty staging dir for `kind`, after checking nothing unrecorded is in
/// the way.
fn fresh_staging(dir: &Path, kind: Kind) -> Result<PathBuf> {
    let dest = dir.join(kind.dir());
    anyhow::ensure!(
        !dest.exists(),
        "{} exists but is not recorded in {}; delete it to fetch it again",
        dest.display(),
        sources::FILE_NAME
    );
    let staging = dir.join(format!("{}.partial", kind.dir()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    Ok(staging)
}

/// The record for a git source checked out at `path`.
fn record(path: &Path, kind: Kind, url: &str, tag: &str, how: Match) -> Result<Source> {
    let (tree_sha256, files) = tree::digest(path)?;
    Ok(Source {
        kind,
        path: kind.dir().into(),
        url: url.into(),
        git_ref: Some(tag.into()),
        commit: Some(git::head(path)?),
        file: None,
        file_sha256: None,
        index_url: None,
        index_sha256: None,
        game_version: Version::parse(tag.rsplit('/').next().unwrap_or(tag))
            .map_or_else(|| tag.to_string(), |v| v.to_string()),
        protocol: None,
        r#match: how,
        tree_sha256,
        files,
        fetched_at: sources::now_rfc3339(),
        notes: Vec::new(),
    })
}

/// `latest.version` from a bedrock-samples checkout's `version.json`.
fn samples_version(checkout: &Path) -> Option<String> {
    let bytes = std::fs::read(checkout.join("version.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    Some(v.pointer("/latest/version")?.as_str()?.to_string())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dataminer-fetch-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755);
    for (name, data) in entries {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap();
}
