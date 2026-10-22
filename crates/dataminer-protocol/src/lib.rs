//! Ingest Mojang's public `bedrock-protocol-docs` into packet-layout specs.
//!
//! `Mojang/bedrock-protocol-docs` publishes, under `json/`, a JSON Schema
//! (draft-07) for every packet, payload, type and enum, tagged per game version
//! with its wire protocol number. A packet file carries the packet id (in
//! `$metaProperties["[cereal:packet]"]`) and `$ref`s a payload file whose
//! `properties` describe the fields: each has an `x-ordinal-index` (the wire
//! order), an `x-underlying-type` (the wire primitive), `x-serialization-options`
//! (e.g. `Compression` = varint, `Enum-as-Value`), and a `$ref` to its type.
//!
//! This is the authoritative clean source for packet layouts: Mojang's own
//! published description, no binary required. Dataminer reads it and emits the
//! ordered field list per packet, plus the enums, as facts. Like
//! `bedrock-samples`, these files are EULA "all rights reserved": read for
//! facts, never redistributed (see AGENTS.md).

pub mod packets;

use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::{Method, Provenance, SourceKind};

/// Errors from reading or parsing protocol docs.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// A file did not parse as JSON.
    #[error("parsing {file}: {source}")]
    Parse {
        /// The file name.
        file: String,
        /// The underlying serde error.
        source: serde_json::Error,
    },
    /// A sanity check failed (e.g. no packets found), usually a format change.
    #[error("{0}")]
    Sanity(String),
}

/// The tool identifier stamped into provenance.
pub const TOOL: &str = concat!("dataminer-protocol ", env!("CARGO_PKG_VERSION"));

/// Build a provenance for the protocol-docs source as a whole. `artifact_sha256`
/// is a digest over the concatenated source files, so the exact input is pinned.
pub(crate) fn provenance(
    game_version: &str,
    repo_ref: &str,
    aggregate_digest: Sha256Digest,
) -> Provenance {
    Provenance {
        source: SourceKind::MojangMetadata,
        method: Method::Extracted,
        game_version: game_version.to_string(),
        artifact: format!("Mojang/bedrock-protocol-docs@{repo_ref} json/"),
        artifact_sha256: Some(aggregate_digest),
        tool: TOOL.to_string(),
        notes: vec![
            "Packet layouts read from Mojang's published JSON schemas; field order is \
             x-ordinal-index. Not redistributed (EULA)."
                .to_string(),
        ],
    }
}
