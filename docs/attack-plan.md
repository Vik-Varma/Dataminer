# Dataminer plan: what we mine, in what order, and why

Written 2026-10-22. Dataminer feeds [Ferrock](https://github.com/Vik-Varma/Ferrock). Ferrock needs an
exact, trustworthy, versioned map of Minecraft: Bedrock Edition. This plan sequences the work by
value and cleanliness: cheap, unambiguous, permissively sourced facts first; expensive binary
structure last.

Ferrock's own consumption contract is written up in its repo at `docs/dataminer-requirements.md`.
This plan tracks it. Ferrock targets **game 1.26.51, wire protocol 2193**. Its sharpest need is
**player movement constants** (gravity, drag, speeds, jump, step height, and the swim/glide/fly/
crawl/ladder/liquid coefficients) — the competitor's movement values are provisional or unmeasured,
so exact values there are a real advantage. Movement is `measured`, not `extracted`: the method is to
drive a headless client (Acacia) on a strict-movement BDS and record its corrections (it corrects
anything more than 0.0001 off), with golden tests on known values (stone hardness 1.5, gravity 0.08).

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

Ferrock priority tiers: **P0** (Phase 1, walking a world), **P1** (Phase 2, survival), **P2**
(Phase 3, PvP).

| Tier | Layer | Source | Method | Status |
| --- | --- | --- | --- | --- |
| P0 | Blocks: names, state properties, raw ids | `bedrock-samples` vanilladata | extracted | **done** |
| P0 | Blocks: collision/outline boxes, friction, light, render layer, solidity | BDS binary / Script API | extracted | planned |
| P0 | Block network runtime id / hash | derived: FNV-1a over sorted state NBT | extracted | planned |
| P0 | Player movement constants (gravity, drag, speeds, jump, step, swim/glide/fly/crawl/ladder/liquid) | **measured**: headless Acacia on strict-movement BDS | measured | planned |
| P0 | Items: ids, stack size, durability, tags, block↔item map | `bedrock-samples` + BDS | extracted | **partial** (ids/names done) |
| P0 | Packet schemas (id, fields in wire order) + enums + version diff | `bedrock-protocol-docs` JSON schemas | extracted | **done** |
| P1 | Commands + enums | `bedrock-samples` command module | extracted | **done** |
| P1 | Entities (hitboxes, attributes, rideability) | `bedrock-samples` + BDS | extracted | **partial** (ids done) |
| P1 | Biomes / effects / enchantments (names) | `bedrock-samples` vanilladata | extracted | **done** |
| P1 | Hardness + break-time formula; recipes; biome tints; sound/particle maps | BDS + packs | extracted | planned |
| P2 | Combat timing, knockback, item-use durations, reach | BDS / measured | measured | planned |
| — | BDS symbols (demangled) | BDS binary symbol table | extracted | **done** |
| — | RTTI class names | BDS `_ZTI*` symbols + `_ZTS*` rodata strings | extracted | **done** |
| — | Binary strings | BDS rodata/data sections | extracted | **done** |
| — | Struct sizes/offsets, vtable layout, hierarchy, serializer field order | Ghidra/IDA headless → JSON facts | extracted | planned (engine) |
| — | Spec diffing across versions | Dataminer `diff` | — | **done** |

## Output contract (what Ferrock pins)

- `out/<game-version>/<dataset>.json` plus `manifest.json` (game version, wire protocol, source file
  hashes, Dataminer commit, per-dataset schema version). **Done** for the metadata datasets.
- Deterministic: sorted records, byte-identical on re-run. **Done** for metadata and symbols.
- A JSON Schema per dataset, and a small serde types crate under a permissive (MIT/Apache) license so
  non-GPL consumers can use it too. *Planned.* (The extractor crates already expose serde types,
  under GPL-3.0.)
- Provenance on every record (source + method `extracted`/`measured`/`inferred`/`unknown`); gaps are
  marked `unknown`, never guessed. **Done** at the document level; per-record provenance arrives with
  the binary/physics datasets.
- A non-interactive CLI that exits non-zero on failure and prints a coverage summary (e.g. "1,193 of
  1,201 blocks have collision boxes; 8 unknown, listed"). *Planned.*
- A `diff` command across versions. **Done.**
- Tagged releases; Ferrock pins a tag and checks the manifest hashes. *Planned.*

## Trust (how we avoid shipping a wrong number)

- Cross-check each dataset against an independent source and report disagreements: PrismarineJS
  minecraft-data, gophertunnel/Dragonfly (MIT), Acacia's `blocks.bin` and `acacia-physics` (a port of
  Oomph's bedsim). Never weaken a cross-check to make it pass.
- Prefer measurement over inference for physics (strict-movement BDS corrections).
- Golden tests for well-known values (stone hardness 1.5, gravity 0.08 blocks/tick²).

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
