//! Extract printable string literals from a binary's data sections.
//!
//! String tables are one of the highest-signal, lowest-risk things to mine: a
//! game server's strings include registry keys, packet names, error messages,
//! Molang identifiers and command literals, none of which are protected
//! expression. We read runs of printable bytes from the loadable sections and
//! leave interpretation to the reader.

use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::{Method, Provenance, SourceKind};
use dataminer_core::spec::SpecDocument;
use object::{Object, ObjectSection, SectionKind};
use serde::{Deserialize, Serialize};

use crate::BinaryError;

/// The spec `kind` for string documents.
pub const KIND: &str = "bds-strings";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// The default minimum run length for an extracted string.
pub const DEFAULT_MIN_LEN: usize = 5;

/// One extracted string and where it was found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedString {
    /// The section it came from, e.g. `.rodata`.
    pub section: String,
    /// The offset of the string within that section.
    pub section_offset: u64,
    /// The printable text (ASCII, without the terminating NUL).
    pub text: String,
}

/// The string spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringsPayload {
    /// The minimum run length used.
    pub min_len: usize,
    /// The extracted strings, in section then offset order.
    pub strings: Vec<ExtractedString>,
}

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b) || b == b'\t'
}

/// Extract NUL- or boundary-terminated printable ASCII runs of at least
/// `min_len` bytes from the binary's read-only and data sections.
pub fn extract(
    data: &[u8],
    min_len: usize,
    game_version: &str,
    artifact: &str,
) -> Result<SpecDocument<StringsPayload>, BinaryError> {
    let file = object::File::parse(data)?;
    let mut strings = Vec::new();

    for section in file.sections() {
        // Only scan sections that hold initialized bytes worth reading.
        if !matches!(
            section.kind(),
            SectionKind::ReadOnlyData
                | SectionKind::ReadOnlyString
                | SectionKind::Data
                | SectionKind::Text
        ) {
            continue;
        }
        let Ok(bytes) = section.data() else { continue };
        let name = section.name().unwrap_or("<unknown>").to_string();

        let mut run_start = 0usize;
        let mut in_run = false;
        for (i, &b) in bytes.iter().enumerate() {
            if is_printable(b) {
                if !in_run {
                    in_run = true;
                    run_start = i;
                }
            } else {
                if in_run && i - run_start >= min_len {
                    push_run(&mut strings, &name, &bytes[run_start..i], run_start as u64);
                }
                in_run = false;
            }
        }
        if in_run && bytes.len() - run_start >= min_len {
            push_run(&mut strings, &name, &bytes[run_start..], run_start as u64);
        }
    }

    let provenance = Provenance {
        source: SourceKind::BdsString,
        method: Method::Extracted,
        game_version: game_version.to_string(),
        artifact: artifact.to_string(),
        artifact_sha256: Some(Sha256Digest::of_bytes(data)),
        tool: concat!("dataminer-binary ", env!("CARGO_PKG_VERSION")).to_string(),
        notes: vec![format!(
            "Printable ASCII runs of >= {min_len} bytes from read-only/data/text sections."
        )],
    };

    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        StringsPayload { min_len, strings },
    ))
}

fn push_run(out: &mut Vec<ExtractedString>, section: &str, run: &[u8], offset: u64) {
    if let Ok(text) = std::str::from_utf8(run) {
        out.push(ExtractedString {
            section: section.to_string(),
            section_offset: offset,
            text: text.to_string(),
        });
    }
}
