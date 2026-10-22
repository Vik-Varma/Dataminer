//! Where a fact came from, and how much to trust it.

use serde::{Deserialize, Serialize};

use crate::digest::Sha256Digest;

/// The kind of source a fact was extracted from.
///
/// This is the first thing a reviewer looks at: a block id read straight out of
/// Mojang's own published data is a different kind of claim from one inferred
/// from a disassembly, and the clean-room rules treat them differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// Mojang's public `bedrock-samples` repository: `metadata/` JSON, documentation
    /// and JSON schemas, published by Mojang under the Minecraft EULA. Highest
    /// trust, no reverse engineering involved.
    MojangMetadata,
    /// A symbol or type read from a Bedrock Dedicated Server build that still
    /// carries that information (an exported symbol table, a shipped PDB, DWARF).
    BdsSymbol,
    /// A C++ type recovered from a BDS binary's RTTI (Itanium `type_info` or the
    /// MSVC equivalent) or vtable layout.
    BdsRtti,
    /// A string literal recovered from a BDS binary's read-only data.
    BdsString,
    /// A fact derived by analysing BDS machine code or static initializers.
    /// Lowest trust; always needs a second source before Ferrock relies on it.
    BdsAnalysis,
    /// A fact observed on the wire in a capture of our own session (e.g. the
    /// registries a server sends at join). Clean, because it is our own traffic.
    PacketCapture,
    /// A fact produced by running a behaviour pack on our own BDS and reading
    /// back the Script API (`@minecraft/server`) enumerations.
    ScriptApiDump,
}

impl SourceKind {
    /// Whether this source involves reverse engineering a binary. The clean-room
    /// policy requires these to be corroborated and attributed with care.
    pub fn is_reverse_engineering(self) -> bool {
        matches!(
            self,
            SourceKind::BdsSymbol
                | SourceKind::BdsRtti
                | SourceKind::BdsString
                | SourceKind::BdsAnalysis
        )
    }
}

/// How a fact was obtained. These are the four methods Ferrock's requirements
/// doc (`docs/dataminer-requirements.md`) asks every record to carry, so the
/// output plugs straight in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// Read verbatim from a source, or computed from such reads by a documented
    /// deterministic rule (e.g. a JSON field, a symbol name, a captured packet
    /// field, or a block network id derived by hashing its state NBT).
    /// Reproducible by anyone with the same artifact.
    Extracted,
    /// Observed by running the game and measuring (e.g. driving a headless
    /// client on a strict-movement BDS and recording its corrections). The
    /// preferred method for physics constants.
    Measured,
    /// A best guess from incomplete evidence. Must never gate a behaviour change
    /// in Ferrock on its own; recorded so the gap is visible, not hidden.
    Inferred,
    /// Not established. Used to mark a gap honestly rather than fill it with a
    /// guess; counts against coverage.
    Unknown,
}

/// The full provenance attached to a spec document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The kind of source.
    pub source: SourceKind,
    /// How the facts were obtained.
    pub method: Method,
    /// The Minecraft: Bedrock version this is about, e.g. `1.26.50.4`.
    pub game_version: String,
    /// A human-readable description of the exact artifact, e.g.
    /// `Mojang/bedrock-samples@v1.26.50.4 metadata/vanilladata_modules/mojang-blocks.json`.
    pub artifact: String,
    /// The digest of the exact bytes the facts were read from, when there is a
    /// single artifact. `None` for facts assembled from many inputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<Sha256Digest>,
    /// The tool and version that produced the spec, e.g. `dataminer 0.0.1`.
    pub tool: String,
    /// Free-form notes: assumptions made, limitations, what was *not* covered.
    /// Honesty about gaps lives here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Provenance {
    /// A provenance for a single Mojang metadata file, read verbatim.
    pub fn mojang_metadata(
        game_version: impl Into<String>,
        artifact: impl Into<String>,
        artifact_sha256: Sha256Digest,
        tool: impl Into<String>,
    ) -> Self {
        Self {
            source: SourceKind::MojangMetadata,
            method: Method::Extracted,
            game_version: game_version.into(),
            artifact: artifact.into(),
            artifact_sha256: Some(artifact_sha256),
            tool: tool.into(),
            notes: Vec::new(),
        }
    }

    /// Add a note and return self, for ergonomic construction.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_engineering_classification() {
        assert!(SourceKind::BdsRtti.is_reverse_engineering());
        assert!(SourceKind::BdsAnalysis.is_reverse_engineering());
        assert!(!SourceKind::MojangMetadata.is_reverse_engineering());
        assert!(!SourceKind::PacketCapture.is_reverse_engineering());
    }

    #[test]
    fn metadata_provenance_is_observed() {
        let p = Provenance::mojang_metadata(
            "1.26.50.4",
            "mojang-blocks.json",
            Sha256Digest::of_bytes(b"x"),
            "dataminer 0.0.1",
        )
        .with_note("read verbatim");
        assert_eq!(p.method, Method::Extracted);
        assert_eq!(p.notes, vec!["read verbatim".to_string()]);
    }
}
