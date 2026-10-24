# Dataminer

A datamining and reverse-engineering toolkit that turns **legally clean sources** about Minecraft:
Bedrock Edition into **versioned, diffable specs** — facts a clean-room Rust reimplementation can
build against. It is the data pipeline behind [Ferrock](https://github.com/Vik-Varma/Ferrock).

> Dataminer is unofficial. It is not approved by or associated with Mojang or Microsoft. Minecraft is
> a trademark of Microsoft Corporation. Dataminer ships no Mojang assets or data.

**Status: v0, pre-alpha.** It ingests Mojang's public metadata into normalized specs, extracts
symbols, RTTI class names and strings from a Bedrock Dedicated Server binary, and diffs specs across
versions. Deeper structural recovery (vtable layout, protocol-doc ingest, packet captures) is
planned; see [docs/attack-plan.md](docs/attack-plan.md).

## Why

Ferrock needs an exact, trustworthy map of the game: block and item ids, state collision boxes,
player movement constants, the command tree, packet layouts, enum values, class names. Guessing or
copying another project's data would taint a clean-room effort and get Ferrock players banned when a
value is wrong. Dataminer produces those facts from sources we are allowed to use, and attaches to
every fact enough provenance that a reviewer can reproduce it: a source, and a method —
`extracted` (read verbatim or computed deterministically), `measured` (observed by running the
game), `inferred` (a flagged guess), or `unknown` (an honest gap, never filled to pad coverage).
Ferrock's consumption contract is in its repo at `docs/dataminer-requirements.md`; this tool tracks
it.

## What counts as a clean source

| Source | What we take | Notes |
| --- | --- | --- |
| `Mojang/bedrock-samples` `metadata/` | blocks, items, commands, biomes, effects, enchantments, Molang, Script API surface | Mojang's own published data (EULA, all rights reserved): read for facts, **never redistributed** |
| `Mojang/bedrock-protocol-docs` | per-packet JSON schemas with wire field order | Same license posture (planned ingester) |
| `@minecraft/vanilla-data` (npm) | block/item/entity enums | MIT — the one permissively licensed Mojang data source |
| Bedrock Dedicated Server (free download) | symbols, RTTI class names, strings, (later) struct/vtable facts via an external engine | Our own static analysis of the public binary |
| Captures of **our own** sessions | registries sent at join (StartGame, ItemRegistry, …) | Our own traffic |

What we **never** use: the DRM-protected Windows client, NDA symbol/header artifacts, or another
project's reverse-engineered headers and tables. See [AGENTS.md](AGENTS.md) and
[docs/clean-room-and-legal.md](docs/clean-room-and-legal.md).

## Build and run

You need Rust 1.99; `rust-toolchain.toml` selects it automatically.

```sh
cargo build --workspace
cargo test  --workspace

# Download and pin the Mojang inputs for a version into the git-ignored work/ dir
# (needs git and curl; you must accept the Minecraft EULA). Records what it got,
# with hashes, in work/<version>/sources.json; re-running is a no-op.
cargo run -p dataminer-cli -- fetch --game-version 1.26.51 --accept-eula --out work
cargo run -p dataminer-cli -- fetch --game-version 1.26.51 --out work --verify

# Ingest a local bedrock-samples checkout into specs (reads it; copies nothing).
# Writes out/<game-version>/<dataset>.json + a manifest.json, deterministically:
cargo run -p dataminer-cli -- metadata --samples /path/to/bedrock-samples --protocol 2193 --out out

# Ingest Mojang's bedrock-protocol-docs (checked out at the target version tag)
# into packet-layout specs: each packet's id and fields in wire order, plus enums:
cargo run -p dataminer-cli -- protocol --docs /path/to/bedrock-protocol-docs --game-version 1.26.51 --out out

# Inspect a Bedrock Dedicated Server binary:
cargo run -p dataminer-cli -- inspect /path/to/bedrock_server

# Extract facts from it (static read; never runs the server):
cargo run -p dataminer-cli -- binary rtti    /path/to/bedrock_server --game-version 1.26.52 --out rtti.json
cargo run -p dataminer-cli -- binary symbols /path/to/bedrock_server --game-version 1.26.52 --out symbols.json
cargo run -p dataminer-cli -- binary strings /path/to/bedrock_server --game-version 1.26.52 --out strings.json

# Summarize a spec's coverage; --strict exits non-zero on gaps (a CI hook):
cargo run -p dataminer-cli -- report out/1.26.51/packets.json --strict

# Cross-check block-state hashes against an authoritative block_states.nbt
# (network little-endian NBT, e.g. Dragonfly's, MIT) and report differences:
cargo run -p dataminer-cli -- crosscheck blockstates \
    --spec out/1.26.50.4/blockstates.json --against block_states.nbt --out crosscheck.json

# Diff two spec documents to see what a game update changed:
cargo run -p dataminer-cli -- diff out/old/blocks.json out/new/blocks.json
```

On a current Linux BDS (stripped of function names but with RTTI intact), `binary rtti` recovers
polymorphic class names from `_ZTI*` symbols when present and, when stripped, from the `_ZTS*`
type-name strings that survive in read-only data.

## Layout

| Path | What it is |
| --- | --- |
| `crates/dataminer-core` | The spec/provenance vocabulary every extractor shares |
| `crates/dataminer-metadata` | Ingest Mojang's public `bedrock-samples` metadata into specs |
| `crates/dataminer-protocol` | Ingest Mojang's public `bedrock-protocol-docs` into packet-layout specs (fields in wire order) |
| `crates/dataminer-binary` | Static ELF/PE analysis: symbols, RTTI, strings (demangling via `cpp_demangle`/`msvc-demangler`) |
| `crates/dataminer-diff` | Structural diffs between specs across game versions |
| `crates/dataminer-fetch` | Download and pin the inputs (`bedrock-samples`, `bedrock-protocol-docs`, BDS) for a version |
| `crates/dataminer-cli` | The `dataminer` command-line tool |
| `docs/` | Strategy, and the datamining and reverse-engineering technique surveys |

## Documentation

- [docs/attack-plan.md](docs/attack-plan.md) — what we mine, in what order, and why
- [docs/datamining-techniques.md](docs/datamining-techniques.md) — industry survey of Bedrock datamining
- [docs/reverse-engineering.md](docs/reverse-engineering.md) — industry survey of RE tools and methods
- [docs/clean-room-and-legal.md](docs/clean-room-and-legal.md) — the clean-room method and legal guardrails
- [docs/clean-room-log.md](docs/clean-room-log.md) — who has been on which side of the wall, per subject
- [docs/fetcher-requirements.md](docs/fetcher-requirements.md) — what `dataminer fetch` downloads, and the rules it follows

## License

[GPL-3.0-or-later](LICENSE). Dependencies keep their own licenses, all GPL-3.0-compatible.
