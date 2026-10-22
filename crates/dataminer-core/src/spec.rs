//! The spec document: a typed payload plus its provenance, ready to serialize.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::provenance::Provenance;

/// Errors from building or (de)serializing spec documents.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// The payload could not be turned into / read from JSON.
    #[error("spec JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Metadata every spec document carries, independent of its payload type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecMeta {
    /// The spec schema name, e.g. `blocks`, `commands`, `bds-symbols`. A reader
    /// keys off this to pick a deserializer.
    pub kind: String,
    /// The schema version of *this document format*, bumped when the shape of
    /// the payload changes. Separate from the game version in `provenance`.
    pub schema_version: u32,
    /// Where the facts came from.
    pub provenance: Provenance,
}

/// A spec document: [`SpecMeta`] plus a typed `payload`.
///
/// Serializes to a stable JSON shape:
/// ```json
/// { "meta": { "kind": "...", "schema_version": 1, "provenance": { ... } },
///   "payload": { ... } }
/// ```
/// Stability matters because these documents are checked in and diffed across
/// game versions; a reordered or reshaped envelope would make every diff noisy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecDocument<T> {
    /// Format and provenance metadata.
    pub meta: SpecMeta,
    /// The typed facts.
    pub payload: T,
}

impl<T> SpecDocument<T> {
    /// Build a spec document.
    pub fn new(
        kind: impl Into<String>,
        schema_version: u32,
        provenance: Provenance,
        payload: T,
    ) -> Self {
        Self {
            meta: SpecMeta {
                kind: kind.into(),
                schema_version,
                provenance,
            },
            payload,
        }
    }
}

impl<T: Serialize> SpecDocument<T> {
    /// Serialize to pretty JSON with a trailing newline, the on-disk form.
    pub fn to_pretty_json(&self) -> Result<String, SpecError> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }
}

impl<T: DeserializeOwned> SpecDocument<T> {
    /// Read a spec document back from JSON.
    pub fn from_json(s: &str) -> Result<Self, SpecError> {
        Ok(serde_json::from_str(s)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest::Sha256Digest;
    use crate::provenance::Provenance;

    #[test]
    fn round_trips() {
        let prov = Provenance::mojang_metadata(
            "1.26.50.4",
            "mojang-blocks.json",
            Sha256Digest::of_bytes(b"x"),
            "dataminer 0.0.1",
        );
        let doc = SpecDocument::new("demo", 1, prov, vec![1u32, 2, 3]);
        let json = doc.to_pretty_json().unwrap();
        assert!(json.ends_with('\n'));
        let back: SpecDocument<Vec<u32>> = SpecDocument::from_json(&json).unwrap();
        assert_eq!(doc, back);
        assert_eq!(back.meta.kind, "demo");
    }
}
