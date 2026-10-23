//! Ingest Mojang's public `bedrock-samples` metadata into normalized specs.
//!
//! `Mojang/bedrock-samples` is published by Mojang under the Minecraft EULA and
//! carries, under `metadata/`, machine-readable descriptions of the vanilla game:
//! the block and item tables, the command tree, Molang queries, the Script API
//! surface, and JSON schemas. It is the cleanest possible source — no reverse
//! engineering, no binary, just data Mojang chose to publish — so it is the first
//! thing Dataminer mines.
//!
//! Each module here defines the *raw* shape exactly as Mojang writes it (so we
//! fail loudly if the format shifts between versions) and a *normalized* payload
//! that Ferrock consumes. We keep Mojang's own ids and serialization names
//! verbatim; we never invent values.

pub mod blocks;
pub mod commands;
pub mod molang;
pub mod registries;

use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::Provenance;

/// Errors from reading or parsing metadata.
#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    /// The JSON did not match the shape we expect for this file.
    #[error("parsing {file}: {source}")]
    Parse {
        /// The logical file name, e.g. `mojang-blocks.json`.
        file: String,
        /// The underlying serde error.
        source: serde_json::Error,
    },
    /// A sanity check on the parsed data failed (e.g. zero items where there
    /// should be many), which usually means the format changed.
    #[error("{file}: {message}")]
    Sanity {
        /// The logical file name.
        file: String,
        /// What was wrong.
        message: String,
    },
}

/// The tool identifier stamped into every spec's provenance.
pub const TOOL: &str = concat!("dataminer-metadata ", env!("CARGO_PKG_VERSION"));

/// Build a Mojang-metadata provenance for one source file.
pub(crate) fn provenance_for(
    game_version: &str,
    repo_ref: &str,
    rel_path: &str,
    bytes: &[u8],
) -> Provenance {
    Provenance::mojang_metadata(
        game_version,
        format!("Mojang/bedrock-samples@{repo_ref} metadata/{rel_path}"),
        Sha256Digest::of_bytes(bytes),
        TOOL,
    )
}
