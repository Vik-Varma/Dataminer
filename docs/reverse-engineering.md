# Reverse-engineering a large C++ server binary — an industry survey

Written 2026-10-22. The target is Bedrock Dedicated Server: a ~100+ MB C++ binary, a Linux ELF built
with clang and a Windows PE built with MSVC. Dataminer reads it statically to produce *facts* (names,
ids, constants, class names, struct sizes, enum values, packet layouts) for a clean-room Rust
reimplementation. Output is specs, never decompiled code. Sources are given; uncertainty is flagged.

## 1. Disassemblers and decompilers

| Tool | Latest (2026) | Headless | License |
| --- | --- | --- | --- |
| **Ghidra** | 12.x ([releases](https://github.com/NationalSecurityAgency/ghidra/releases)) | `analyzeHeadless`; PyGhidra 3 (CPython via JPype, default since 11.3) | Apache-2.0, free |
| **IDA Pro** | 9.3 (9.4 beta) | `idalib` since 9.0 | Paid |
| **Binary Ninja** | 5.3 "Jotunheim" | `import binaryninja` (Commercial+) | Paid; first-party Rust API |
| radare2 / rizin / Cutter | r2 6.x | r2pipe | LGPL |
| angr | — | Python lib | BSD |

For a free, GPL-friendly pipeline the default is **Ghidra headless driven by PyGhidra**, scripted to
emit **JSON facts only**. It is slow on a 100 MB binary (hours of auto-analysis, a 16–32 GB heap in
practice) and benefits from caching the analysed project per BDS version. IDA (via `idalib`) is
faster and more robust at this size if a license is available. Ghidra 12.x adds decompiler bitfield
recovery, a struct/union/enum merger and BSim server tooling. angr and r2 are not practical for
whole-program analysis at this scale.

## 2. C++-specific recovery

- **Itanium RTTI** (clang/Linux): `__class_type_info`, `__si_class_type_info` (single base),
  `__vmi_class_type_info` (flags + base array) per the
  [Itanium C++ ABI §2.9.4](https://itanium-cxx-abi.github.io/cxx-abi/abi.html). Each vtable prefix is
  `[vbase offsets][offset-to-top][RTTI pointer]`. **Linux BDS keeps these even when stripped**, so
  class names, vtable slot counts and the hierarchy are the most stable anchors across versions.
  Dataminer already recovers class *names* from `_ZTI*` symbols and `_ZTS*` rodata strings; resolving
  the vtable→typeinfo→base chain for full hierarchy/slot layout is the job of an engine script.
- **MSVC RTTI**: CompleteObjectLocator → TypeDescriptor and ClassHierarchyDescriptor → BaseClassArray;
  on x64 the COL's self-RVA makes it scannable. **But Windows BDS is `/GR-`, so there is no RTTI
  there** — Linux is the RTTI target.
- **Tools**: Ghidra's built-in `RecoverClassesFromRTTIScript` (Windows recovery more complete than
  GCC/ELF, which is "early stages" for virtual inheritance); CMU SEI's **OOAnalyzer** (now in CERT
  Kaiju; MSVC x86 PE only); IDA's built-in `rtti` plugin / PyClassInformer. Ghidra-Cpp-Class-Analyzer
  is archived (2023) — don't rely on it.
- **`std::string` SSO layouts** (x86-64), for spotting strings embedded in structs: libc++ 24 bytes /
  22-char SSO; libstdc++ 32 bytes / 15-char; MSVC 32 bytes / 15-char. `NSt3__1` in mangled names marks
  libc++ ([Raymond Chen](https://devblogs.microsoft.com/oldnewthing/20240510-00?p=109742)).

## 3. Cross-version diffing and symbol porting

- **BinDiff** (open-sourced, [google/bindiff](https://github.com/google/bindiff)) via BinExport, with
  IDA/Binary Ninja/Ghidra; CLI diffing works without the GUI.
- **Ghidra Version Tracking** (carries names/types/comments via correlators) and **BSim**
  (decompiler-based similarity search, CLI workflow).
- **WARP** (Binary Ninja 5.1+, [Vector35/warp](https://github.com/Vector35/warp)): function GUIDs from
  normalized basic-block hashes, disambiguated by caller/callee constraints.
- **Diaphora** (IDA), **QBinDiff** (Quarkslab, experimental).
- **FLIRT**/**Lumina** match library functions, not game-version deltas.

**Porting in practice:** anchor on facts that survive versions (RTTI/typeinfo names, unique string
xrefs, import thunks); run structural matching on the rest; propagate names along the call graph;
accept only high-confidence matches or two methods agreeing. **Re-derive per version** rather than
trusting a port, and record a confidence and method for every ported name.

## 4. Rust libraries (crates.io, 2026-10-22; all GPL-3.0-compatible)

| Crate | Version | Notes |
| --- | --- | --- |
| `object` | 0.40 | ELF/PE/COFF/Mach-O; Dataminer's loader |
| `gimli` | 0.34 | DWARF; `.eh_frame` for function boundaries |
| `goblin` | 0.10 | alternative object parser |
| `cpp_demangle` | 0.5 | Itanium demangler; Dataminer uses it |
| `msvc-demangler` | 0.11 | MSVC demangler; Dataminer uses it |
| `undname` | 2.1 | LLVM MSVC demangler port |
| `pdb2` | 0.11 | maintained fork of `pdb` (whose last release was 2022) |
| `symbolic` | 13.x | Sentry; wraps demangling and debug files |
| `iced-x86` | 1.21 | x86 decoder (no release since 2024-01, repo not archived) |
| `yaxpeax-x86` | 2.2 | x86 decoder, 0BSD |
| `capstone` | 0.14 | multi-arch decoder |
| `lief` | 1.0 | bindings to the C++ LIEF library |

Dataminer stays pure-Rust and `unsafe`-free; it pins `object`, `gimli`, `cpp_demangle`,
`msvc-demangler` today and reserves `pdb2` for Windows PDB work if a symbolicated PE ever comes into
clean scope.

## 5. Dynamic analysis (own server, Linux)

For *validating* statically recovered facts against live behaviour — never as the primary source:
**Frida** (Interceptor/Stalker; very active), **eBPF uprobes/bpftrace** (cheap, accept demangled C++
names), **rr** (deterministic record/replay for "where did this field come from"), **DynamoRIO** and
**Intel PIN** (Ghidra 12.2 can import PIN traces). QBDI is a poor fit for a multithreaded server.
BDS modders hook with `funchook`/Detours (Endstone) or a custom preloader/trampoline (LeviLamina) —
useful as technique references, not as code to copy.

## 6. LLM-assisted RE — a gated proposer only

Research is sobering at this scale: LLM4Decompile reaches ~45% re-executability on *small* C
functions; GenNm name recovery is only ~22.8% for names unseen in training; 2026 agent benchmarks
(CrackMeBench, SRE-Bench) report agents still struggle. Tooling exists (GhidraMCP, ida-pro-mcp,
Gepetto, aiDAPal). **Practice for Dataminer:** use an LLM only to *propose* a name or type; accept it
only when a binary-derivable check confirms it (string xref, RTTI name, matching struct size/offset,
or a dynamic trace); label it in provenance; and keep any decompiled-code input or output on the
**dirty** side. Never let a model write an unchecked fact into a spec.

## 7. Generic binary data-mining

- **String tables**: scan `.rodata`/`.rdata`, cluster by xref. Packet names, component ids and
  registry keys appear as literals passed to registration calls. (Dataminer's `binary strings` does
  the extraction; xref clustering is an engine step.)
- **Static initializers**, where registries are populated: ELF `.init_array` (clang default since
  2019), MSVC `.CRT$XCU` between `__xc_a`/`__xc_z`. Walking these recovers registration order and ids.
- **Enums/constants**: magic_enum-style tables, switch jump tables, string tables indexed by enum.
- **Protocol/format inference from code** (the read/write call sequence in each packet's serialize
  method) is far more accurate than inferring from captures; use captures (Netzob, NEMESYS,
  BinaryInferno) only to validate.

## Recommended pipeline (and where Dataminer sits)

1. **Native Rust ingest** — `object`+`gimli`: sections, function starts from `.eh_frame`/`.pdata`,
   strings with xrefs, `.init_array`/`.CRT$XC*` entries. Deterministic, fast, no external tool.
   *(Dataminer: strings and symbols today; init-array/xref next.)*
2. **RTTI and vtables in Rust** — Itanium typeinfo walk and MSVC COL scan, demangled.
   *(Dataminer: class names today; vtable/hierarchy walk next.)*
3. **Heavy analysis in a pinned, scripted engine** — Ghidra 12 headless + PyGhidra emitting JSON
   facts (sizes, offsets, constants, serializer call sequences); swap in IDA/idalib if licensed.
4. **Version porting** — anchor on RTTI/strings, BinExport→BinDiff/BSim for the rest, record
   confidence+method, re-derive per version. *(Dataminer: `diff` over specs today.)*
5. **Cross-check against Mojang's published data**; report differences rather than silently choosing.
6. **Dynamic validation on your own Linux BDS** (Frida/uprobes on serializers; rr for hard cases).
7. **LLM as an optional gated proposer**, labelled and dirty-side.
8. **Output** versioned, diffable spec files with full provenance; no pseudocode ever crosses to the
   clean side.
