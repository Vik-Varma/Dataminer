//! Symbol-table extraction with demangling.

use dataminer_core::digest::Sha256Digest;
use dataminer_core::provenance::{Method, Provenance, SourceKind};
use dataminer_core::spec::SpecDocument;
use object::{Object, ObjectSymbol, SymbolKind};
use serde::{Deserialize, Serialize};

use crate::{BinaryError, Mangling};

/// The spec `kind` for symbol documents.
pub const KIND: &str = "bds-symbols";
/// The document schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// One extracted symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// The raw (possibly mangled) symbol name.
    pub name: String,
    /// The demangled name, when the raw name was a mangled C++ name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demangled: Option<String>,
    /// The symbol's address/value. `0` for undefined or absolute symbols.
    pub address: u64,
    /// The symbol size in bytes, when the format records one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// A coarse classification: `text` (code), `data`, `section`, `file`,
    /// `tls`, `label`, or `unknown`.
    pub kind: String,
    /// Whether this is a definition in this object (vs. an imported/undefined
    /// reference).
    pub defined: bool,
    /// Whether the symbol is globally visible.
    pub global: bool,
}

/// The symbol spec payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolsPayload {
    /// The mangling ABI used, as a string (`itanium` or `msvc`).
    pub mangling: String,
    /// Whether these came from the static symbol table (`false` means the build
    /// was stripped and only dynamic symbols were available).
    pub from_symbol_table: bool,
    /// The extracted symbols.
    pub symbols: Vec<Symbol>,
}

fn kind_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Text => "text",
        SymbolKind::Data => "data",
        SymbolKind::Section => "section",
        SymbolKind::File => "file",
        SymbolKind::Tls => "tls",
        SymbolKind::Label => "label",
        _ => "unknown",
    }
}

/// Extract symbols from a binary and build a spec document.
///
/// Prefers the static symbol table; if the binary is stripped, falls back to the
/// dynamic symbol table and records that in the payload and provenance notes.
pub fn extract(
    data: &[u8],
    game_version: &str,
    artifact: &str,
) -> Result<SpecDocument<SymbolsPayload>, BinaryError> {
    let file = object::File::parse(data)?;
    let mangling = Mangling::for_format(file.format())
        .ok_or_else(|| BinaryError::UnsupportedFormat(file.format()))?;

    let has_symtab = file.symbols().next().is_some();
    let from_symbol_table = has_symtab;

    let collect = |iter: &mut dyn Iterator<Item = object::Symbol<'_, '_>>| -> Vec<Symbol> {
        let mut out = Vec::new();
        for sym in iter {
            let Ok(name) = sym.name() else { continue };
            if name.is_empty() {
                continue;
            }
            let demangled = crate::demangle(name, mangling);
            let size = (sym.size() != 0).then_some(sym.size());
            out.push(Symbol {
                name: name.to_string(),
                demangled,
                address: sym.address(),
                size,
                kind: kind_str(sym.kind()).to_string(),
                defined: sym.is_definition(),
                global: sym.is_global(),
            });
        }
        out
    };

    let mut symbols = if has_symtab {
        collect(&mut file.symbols())
    } else {
        collect(&mut file.dynamic_symbols())
    };
    // Deterministic order for stable diffs: by address, then raw name.
    symbols.sort_by(|a, b| a.address.cmp(&b.address).then_with(|| a.name.cmp(&b.name)));

    let mangling_str = match mangling {
        Mangling::Itanium => "itanium",
        Mangling::Msvc => "msvc",
    };

    let mut notes = vec![
        "Symbol names and addresses read directly from the object file's symbol table.".to_string(),
    ];
    if !from_symbol_table {
        notes.push(
            "Binary is stripped: only dynamic (exported) symbols were available. \
             Interior names require signature scanning or an external engine."
                .to_string(),
        );
    }

    let provenance = Provenance {
        source: SourceKind::BdsSymbol,
        method: Method::Extracted,
        game_version: game_version.to_string(),
        artifact: artifact.to_string(),
        artifact_sha256: Some(Sha256Digest::of_bytes(data)),
        tool: concat!("dataminer-binary ", env!("CARGO_PKG_VERSION")).to_string(),
        notes,
    };

    Ok(SpecDocument::new(
        KIND,
        SCHEMA_VERSION,
        provenance,
        SymbolsPayload {
            mangling: mangling_str.to_string(),
            from_symbol_table,
            symbols,
        },
    ))
}
