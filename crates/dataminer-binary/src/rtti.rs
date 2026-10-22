//! Recover C++ class names from RTTI.
//!
//! The Itanium C++ ABI (clang on Linux) emits, for each polymorphic type, a
//! `typeinfo` object (`_ZTI...`), a `typeinfo name` string (`_ZTS...`) and a
//! vtable (`_ZTV...`). Two independent recovery paths fall out of this, and this
//! crate runs both:
//!
//! 1. **Typeinfo symbols.** When the build keeps `_ZTI*` symbols, demangling
//!    them yields class names directly.
//! 2. **Typeinfo-name strings in rodata.** The `_ZTS*` *strings* — the mangled
//!    type name, e.g. `11LoginPacket` — survive in read-only data even when the
//!    symbol table is stripped. This is the situation current Bedrock Dedicated
//!    Server Linux builds are in: stripped of function names, but with RTTI
//!    intact. We scan rodata for Itanium type-name strings and demangle them.
//!
//! Both paths read names that are literally present in the binary Mojang ships,
//! so the names are `Observed`. What this crate does **not** do is resolve the
//! full vtable → typeinfo → base-class chain to reconstruct inheritance and slot
//! layout; that structural work belongs in an external engine (e.g. Ghidra's
//! `RecoverClassesFromRTTIScript`) and is recorded here as a gap, not guessed.
//!
//! Note: MSVC Windows BDS builds are compiled `/GR-` (no RTTI), so neither path
//! applies there; see the provenance note emitted for PE inputs.

use std::collections::BTreeSet;

use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::{Method as CoreMethod, Provenance, SourceKind};
use dataminer_core::spec::SpecDocument;
use object::{Object, ObjectSection, ObjectSymbol, SectionKind};
use serde::{Deserialize, Serialize};

use crate::{BinaryError, Mangling};

/// The spec `kind` for RTTI documents.
pub const KIND: &str = "bds-rtti";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// How a class name was recovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// Demangled from a `_ZTI*` typeinfo symbol in the symbol table.
    TypeinfoSymbol,
    /// Demangled from a `_ZTS*` type-name string found in read-only data.
    RodataTypeString,
}

/// A class name recovered from RTTI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RttiClass {
    /// The demangled class name, e.g. `LoginPacket`.
    pub name: String,
    /// How it was recovered.
    pub method: Method,
    /// The raw evidence: the typeinfo symbol name, or the rodata type string.
    pub evidence: String,
}

/// The RTTI spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RttiPayload {
    /// The recovered classes, sorted by name.
    pub classes: Vec<RttiClass>,
}

/// Recover class names from an Itanium binary's RTTI.
///
/// Returns an empty class list (not an error) for non-Itanium formats, with a
/// provenance note explaining why.
pub fn extract(
    data: &[u8],
    game_version: &str,
    artifact: &str,
) -> Result<SpecDocument<RttiPayload>, BinaryError> {
    let file = object::File::parse(data)?;
    let mangling = Mangling::for_format(file.format())
        .ok_or_else(|| BinaryError::UnsupportedFormat(file.format()))?;

    let mut notes = Vec::new();
    let mut by_name: std::collections::BTreeMap<String, RttiClass> = Default::default();

    if mangling != Mangling::Itanium {
        notes.push(
            "Binary is not Itanium-ABI. Windows BDS is built /GR- (no RTTI); MSVC \
             CompleteObjectLocator recovery is out of scope for this crate."
                .to_string(),
        );
        return Ok(finish(data, game_version, artifact, notes, by_name));
    }

    // Path 1: `_ZTI*` typeinfo symbols.
    for sym in file.symbols() {
        let Ok(name) = sym.name() else { continue };
        if name.starts_with("_ZTI")
            && let Some(class) = demangle_typeinfo(name)
        {
            by_name.entry(class.clone()).or_insert(RttiClass {
                name: class,
                method: Method::TypeinfoSymbol,
                evidence: name.to_string(),
            });
        }
    }
    let from_symbols = by_name.len();

    // Path 2: `_ZTS*` type-name strings in read-only data. These survive a strip.
    let mut seen_strings = BTreeSet::new();
    for section in file.sections() {
        if !matches!(
            section.kind(),
            SectionKind::ReadOnlyData | SectionKind::ReadOnlyString | SectionKind::Data
        ) {
            continue;
        }
        let Ok(bytes) = section.data() else { continue };
        for run in bytes.split(|&b| b == 0) {
            if run.len() < 2 || run.len() > 2048 {
                continue;
            }
            let Ok(s) = std::str::from_utf8(run) else {
                continue;
            };
            if !looks_like_type_name(s) || !seen_strings.insert(s.to_string()) {
                continue;
            }
            if let Some(class) = demangle_typeinfo(&format!("_ZTS{s}")) {
                by_name.entry(class.clone()).or_insert(RttiClass {
                    name: class,
                    method: Method::RodataTypeString,
                    evidence: s.to_string(),
                });
            }
        }
    }
    let from_strings = by_name.len() - from_symbols;

    if by_name.is_empty() {
        notes.push(
            "No RTTI found: no `_ZTI*` symbols and no `_ZTS*` type-name strings in read-only \
             data. The binary may be fully stripped of RTTI or not C++."
                .to_string(),
        );
    } else {
        notes.push(format!(
            "Recovered {from_symbols} class name(s) from `_ZTI*` typeinfo symbols and \
             {from_strings} more from `_ZTS*` type-name strings in read-only data. Names are \
             read verbatim; inheritance and vtable layout are not resolved here."
        ));
    }

    Ok(finish(data, game_version, artifact, notes, by_name))
}

fn finish(
    data: &[u8],
    game_version: &str,
    artifact: &str,
    notes: Vec<String>,
    by_name: std::collections::BTreeMap<String, RttiClass>,
) -> SpecDocument<RttiPayload> {
    let classes: Vec<RttiClass> = by_name.into_values().collect();
    let provenance = Provenance {
        source: SourceKind::BdsRtti,
        method: CoreMethod::Extracted,
        game_version: game_version.to_string(),
        artifact: artifact.to_string(),
        artifact_sha256: Some(Sha256Digest::of_bytes(data)),
        tool: concat!("dataminer-binary ", env!("CARGO_PKG_VERSION")).to_string(),
        notes,
    };
    SpecDocument::new(KIND, SCHEMA_VERSION, provenance, RttiPayload { classes })
}

/// Demangle a `_ZTI`/`_ZTS` name and strip the `typeinfo [name] for ` prefix,
/// leaving the class name. Returns `None` if it does not demangle to one.
fn demangle_typeinfo(mangled: &str) -> Option<String> {
    let demangled = crate::demangle(mangled, Mangling::Itanium)?;
    let class = demangled
        .strip_prefix("typeinfo for ")
        .or_else(|| demangled.strip_prefix("typeinfo name for "))?;
    (!class.is_empty()).then(|| class.to_string())
}

/// A cheap filter for rodata runs that could be an Itanium `_ZTS` type-name
/// body: they start with `N` (nested) or a length digit, and use only the
/// characters that appear in mangled names. This only gates which strings we
/// *try* to demangle; `demangle_typeinfo` is the real check.
fn looks_like_type_name(s: &str) -> bool {
    let first = s.as_bytes()[0];
    if first != b'N' && !first.is_ascii_digit() {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demangles_typeinfo_and_name() {
        assert_eq!(
            demangle_typeinfo("_ZTI11LoginPacket").as_deref(),
            Some("LoginPacket")
        );
        assert_eq!(
            demangle_typeinfo("_ZTS11LoginPacket").as_deref(),
            Some("LoginPacket")
        );
    }

    #[test]
    fn type_name_filter() {
        assert!(looks_like_type_name("11LoginPacket"));
        assert!(looks_like_type_name("N3foo3barE"));
        assert!(!looks_like_type_name("minecraft:packet")); // starts with a letter, has ':'
        assert!(!looks_like_type_name("hello world"));
    }
}
