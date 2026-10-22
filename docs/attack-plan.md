# Dataminer plan: what we mine, in what order, and why

Written 2026-10-22. Dataminer feeds [Ferrock](https://github.com/Vik-Varma/Ferrock). Ferrock needs an
exact, trustworthy, versioned map of Minecraft: Bedrock Edition. This plan sequences the work by
value and cleanliness: cheap, unambiguous, permissively sourced facts first; expensive binary
structure last.

## Principles

1. **Cleanest source wins.** Prefer Mojang's own published data, then our own captures and Script API
   dumps, then static analysis of the public BDS binary. See
   [clean-room-and-legal.md](clean-room-and-legal.md).
2. **Every fact carries provenance** (source, confidence, game version, artifact hash). No fact ships
   without it; `inferred` facts never gate behaviour alone.
3. **Cross-check and diff.** When sources disagree, report it. Make every output diffable across
   versions so an update is a reviewable delta, not a re-derivation.
4. **Facts, not code.** Output specs. Never emit or copy expression.

## Layers, by priority

| # | Layer | Source | Confidence | Status |
| --- | --- | --- | --- | --- |
| 1 | Blocks + state properties | `bedrock-samples` vanilladata | observed | **done** |
| 1 | Items | `bedrock-samples` vanilladata | observed | **done** |
| 1 | Commands + enums | `bedrock-samples` command module | observed | **done** |
| 1 | Entities / biomes / effects / enchantments | `bedrock-samples` vanilladata | observed | **done** |
| 2 | Packet layouts (field order, types) | `bedrock-protocol-docs` JSON schemas | observed | next |
| 2 | Molang queries / math functions | `bedrock-samples` molang module | observed | next |
| 3 | BDS symbols (demangled) | BDS binary symbol table | observed | **done** |
| 3 | RTTI class names | BDS `_ZTI*` symbols + `_ZTS*` rodata strings | observed | **done** |
| 3 | Binary strings | BDS rodata/data sections | observed | **done** |
| 4 | Block network ids | derived: FNV-1a over state NBT | derived | planned |
| 4 | Registries sent at join | capture of our own session | observed | planned |
| 4 | Script API enumerations | behaviour pack on our own BDS | observed | planned |
| 5 | Struct sizes / offsets, vtable layout, class hierarchy | Ghidra/IDA headless → JSON facts | derived | planned (engine) |
| 5 | Serializer field order from code | engine analysis of serialize methods | derived | planned (engine) |
| — | Spec diffing across versions | Dataminer `diff` | — | **done** |

## Roadmap

- **M1 — Mojang data (done).** Ingest vanilladata, commands; emit normalized specs with provenance;
  diff across versions. *Exit:* `dataminer metadata` produces all layer-1 specs from a real checkout;
  `diff` reports clean deltas. (Met.)
- **M2 — Protocol + binary reads.** Add the `bedrock-protocol-docs` packet-schema ingester and the
  Molang module. Symbols/RTTI/strings from BDS (done); add `.init_array`/`.CRT$XC*` enumeration and
  string cross-references. *Exit:* packet field order for the core join packets, and class names from
  a real stripped Linux BDS, both diffable.
- **M3 — Live facts.** A behaviour pack that dumps Script API enumerations to stdout, and a capture
  ingester for the registries a server sends at join (StartGame, ItemRegistry, …). Derive block
  network ids and verify against capture. *Exit:* block runtime ids cross-checked three ways
  (metadata ↔ derived hash ↔ capture).
- **M4 — Structure via an engine.** A pinned Ghidra-headless/PyGhidra job that emits JSON facts
  (struct sizes, vtable slot counts, hierarchy, serializer call sequences), cached per BDS version,
  with cross-version porting anchored on RTTI and strings. *Exit:* a reviewed set of struct/vtable
  facts for the networking and world layers, each with confidence and method.

## Non-goals

- No decompiled code, pseudocode, or copied expression in any output. Ever.
- No touching the DRM-protected Windows client or any NDA artifact.
- No bundling Mojang's files or assets into version control.
- No "while we're here" extractors: each layer is added because Ferrock needs it.
