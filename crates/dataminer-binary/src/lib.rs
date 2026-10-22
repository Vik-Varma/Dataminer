//! Extract facts from a Bedrock Dedicated Server binary.
//!
//! The target is BDS, which Mojang distributes for free: a Linux ELF built with
//! clang and a Windows PE built with MSVC. Dataminer reads it statically — it
//! does not run it or patch it — and emits *facts*: which symbols the build
//! still exports, their demangled C++ names, class names recovered from RTTI,
//! and string literals. These feed a clean-room reimplementation as a map of
//! names and layouts, never as copied code.
//!
//! What this crate deliberately does **not** do: it does not decompile, and it
//! does not emit function bodies. Deeper structural work (vtable layout, class
//! hierarchy, type sizes) is the job of an external engine like Ghidra driven
//! headlessly; this crate covers what can be read directly and reproducibly from
//! the object file, and records clearly where that stops.

pub mod rtti;
pub mod strings;
pub mod symbols;

use object::{Architecture, BinaryFormat, Object};

/// Errors from binary analysis.
#[derive(Debug, thiserror::Error)]
pub enum BinaryError {
    /// The file could not be parsed as an object file.
    #[error("parsing object: {0}")]
    Object(#[from] object::Error),
    /// The file format is not one Dataminer handles (we target ELF and PE).
    #[error("unsupported object format: {0:?}")]
    UnsupportedFormat(BinaryFormat),
}

/// A high-level description of a loaded binary, independent of any one extractor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinaryInfo {
    /// The object format (ELF, PE, Mach-O, ...).
    pub format: BinaryFormat,
    /// The CPU architecture.
    pub architecture: Architecture,
    /// Whether the binary carries a static symbol table (`.symtab`). When false,
    /// the build is stripped and only dynamic/exported symbols are available —
    /// the situation modern BDS Linux builds are in.
    pub has_symbol_table: bool,
    /// The number of entries in the static symbol table.
    pub symbol_count: usize,
    /// The number of dynamic (exported/imported) symbols.
    pub dynamic_symbol_count: usize,
}

/// The kind of C++ name mangling a binary uses, inferred from its format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mangling {
    /// Itanium C++ ABI (clang/gcc on Linux/macOS): names start with `_Z`.
    Itanium,
    /// MSVC mangling (Windows): names start with `?`.
    Msvc,
}

impl Mangling {
    fn for_format(format: BinaryFormat) -> Option<Self> {
        match format {
            BinaryFormat::Elf | BinaryFormat::MachO => Some(Mangling::Itanium),
            BinaryFormat::Pe => Some(Mangling::Msvc),
            _ => None,
        }
    }
}

/// Parse a binary and summarize it.
pub fn inspect(data: &[u8]) -> Result<BinaryInfo, BinaryError> {
    let file = object::File::parse(data)?;
    let format = file.format();
    if Mangling::for_format(format).is_none() {
        return Err(BinaryError::UnsupportedFormat(format));
    }
    let symbol_count = file.symbols().count();
    Ok(BinaryInfo {
        format,
        architecture: file.architecture(),
        has_symbol_table: symbol_count > 0,
        symbol_count,
        dynamic_symbol_count: file.dynamic_symbols().count(),
    })
}

/// Demangle a C++ symbol name under the given ABI. Returns `None` if the name is
/// not a mangled name of that ABI (e.g. a plain C symbol), so callers can keep
/// the original.
pub fn demangle(name: &str, mangling: Mangling) -> Option<String> {
    match mangling {
        Mangling::Itanium => cpp_demangle::Symbol::new(name)
            .ok()
            .and_then(|s| s.demangle().ok()),
        Mangling::Msvc => {
            let flags = msvc_demangler::DemangleFlags::llvm();
            msvc_demangler::demangle(name, flags).ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demangles_itanium() {
        // _ZN3fooC1Ev -> foo::foo()
        let out = demangle("_ZN3fooC1Ev", Mangling::Itanium).unwrap();
        assert!(out.contains("foo"));
    }

    #[test]
    fn leaves_plain_c_names_alone() {
        assert_eq!(demangle("main", Mangling::Itanium), None);
    }

    #[test]
    fn mangling_follows_format() {
        assert_eq!(
            Mangling::for_format(BinaryFormat::Elf),
            Some(Mangling::Itanium)
        );
        assert_eq!(Mangling::for_format(BinaryFormat::Pe), Some(Mangling::Msvc));
        assert_eq!(Mangling::for_format(BinaryFormat::Coff), None);
    }
}
