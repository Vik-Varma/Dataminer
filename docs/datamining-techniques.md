# Datamining Minecraft: Bedrock Edition — an industry survey

Written 2026-10-22. This surveys how the Bedrock ecosystem extracts game facts, so Dataminer can use
the best legally clean techniques and skip the tainted ones. Every claim has a source; uncertainty is
flagged.

## Version context

Mojang uses year-based marketing names ("26.60") but technical strings stay `1.26.x`. As of this
writing the latest BDS release is **1.26.52** and the latest preview is **1.26.70-preview.21**
([EndstoneMC/bedrock-server-data](https://github.com/EndstoneMC/bedrock-server-data)). The 26.60
release is dated 2026-10-27 ([minecraft.wiki](https://minecraft.wiki/w/Bedrock_Edition_26.60)). The
network protocol for 1.26.50/1.26.51 is **2193** ([gophertunnel](https://github.com/Sandertv/gophertunnel);
[Mojang/bedrock-protocol-docs releases](https://github.com/Mojang/bedrock-protocol-docs/releases)).

## 1. Mojang's own public data — the cleanest source

### `Mojang/bedrock-samples`
`metadata/` holds machine-readable descriptions of the vanilla game. Exact files Dataminer reads
today (`main` branch, `version.json` latest `1.26.50.4`):

- `vanilladata_modules/`: `mojang-blocks.json` (157 `block_properties`, 1,463 `data_items`),
  `mojang-items.json` (1,623), `mojang-entities.json` (129), `mojang-biomes.json` (89),
  `mojang-effects.json` (37), `mojang-enchantments.json` (42), plus camera presets, dimensions,
  features, potion effects/types, cooldown categories.
- `command_modules/mojang-commands.json`: 84 commands, 236 command enums. A command carries `name`,
  `description`, `aliases` (as `{name}` objects), `permission_level`, `requires_cheats`, and
  `overloads[].params[]` of `{name, is_optional, type:{name}}`.
- `molang_modules/mojang-molang-queries.json`, `engine_modules/`, `doc_modules/` (21 files),
  `script_modules/@minecraft/*.json`, and `json_schemas/{client,server,client_server}/…` (≈1,073
  files).

Counts above are confirmed against a local checkout. **License:** `(c) Mojang AB. All rights
reserved`, under the Minecraft EULA ([LICENSE.md](https://github.com/Mojang/bedrock-samples/blob/main/LICENSE.md)).
Extracting facts is standard practice; redistributing the files is not licensed.

### `Mojang/bedrock-protocol-docs`
`json/` holds one **JSON Schema (draft-07)** per packet, payload, type and enum (≈240 `*Packet.json`).
Each property carries `x-underlying-type` (`uint8`/`float`/`uint64`), `x-serialization-options`
(e.g. `Compression` = varint, `Enum-as-Value`) and `x-ordinal-index` (wire order), plus a `required`
list. Registry-relevant schemas include `StartGamePacket`, `ItemRegistryPacket`,
`BiomeDefinitionListPacket`, `CreativeContentPacket`, `CraftingDataPacket`, `AvailableCommandsPacket`,
`AvailableActorIdentifiersPacket`. Each release is tagged with its protocol number
([releases](https://github.com/Mojang/bedrock-protocol-docs/releases)). Same license posture as
above. **This is the authoritative clean source for packet layouts** and is Dataminer's next
ingester.

### `@minecraft/vanilla-data` (npm) — MIT
TypeScript enums generated from the same vanilladata modules, e.g.
`MinecraftBlockTypes.AcaciaButton = "minecraft:acacia_button"`. The one **permissively licensed**
Mojang-derived data source ([npm](https://www.npmjs.com/package/@minecraft/vanilla-data)); useful as
a cross-check and a license-clean fallback.

## 2. Symbols: what survives in the binary

- **Windows BDS PDBs are gone.** Removal began in preview 1.21.10.22 (June 2024) and was complete by
  release 1.21.20 (Aug 2024), stated reason "security"
  ([minecraft.wiki BDS history](https://minecraft.wiki/w/Bedrock_Dedicated_Server);
  [Apex Hosting](https://apexminecrafthosting.com/blog/did-mojang-just-remove-modding-for-minecraft-bedrock/)).
- **Linux BDS is stripped of function names but keeps RTTI.** The `_ZTV`/`_ZTI`/`_ZTS` vtable,
  typeinfo and type-name records survive, so polymorphic class names and vtables are recoverable.
  Windows builds are compiled `/GR-`, so they have **no** RTTI. (Endstone maintainer notes,
  `bump-bds` skill.) Dataminer's `binary rtti` is built directly on this: it reads `_ZTI*` symbols
  when present and `_ZTS*` type strings from rodata when stripped — verified on a stripped test ELF.
- **NDA red flag.** Private `bedrock_server.pdb` and DWARF-bearing Android `libminecraftpe.so`
  (`dwarf2cpp`-generated headers) circulate under NDA. Dataminer treats all of these, and anything
  derived from them, as off-limits.

How community loaders get addresses today (all symbol-dependent or RE-derived — hints only, never
copied):

| Project | Method | License |
| --- | --- | --- |
| Endstone | `dump_symbols.py`: LIEF byte-signature scan of `.text` using per-platform `.toml` patterns → name→offset header. Re-cut in IDA each update; [PR #534](https://github.com/EndstoneMC/endstone/pull/534) reports many entries unresolved by pattern-only scanning until re-cut | Apache-2.0 |
| LeviLamina | Runtime mangled-name resolution via PreLoader/SymbolProvider; name→address from a prebuilt per-version `bedrock-runtime-data` package (generation not public) | LGPL/GPL parts |
| pmmp | Linux BDS + LD_PRELOAD modloader; `export-symbols.py`, Frida `tracer.py`, objdump packet-id dumper — depends on a symbol-bearing (internal) build | LGPL-3.0 |

## 3. Community data pipelines and their techniques

| Project | Data | Technique | License |
| --- | --- | --- | --- |
| [pmmp/BedrockData](https://github.com/pmmp/BedrockData) | `canonical_block_states.nbt`, `block_properties_table.json`, `biome_definitions`, `creative/*`, `recipes/*`, `protocol_info.json`, enums | BDS mod ([bds-mod-mapping-generator](https://github.com/pmmp/bds-mod-mapping)); vanilla packet traces; binary-analysis script; protocol-spec scrape | CC0 |
| [CloudburstMC/Data](https://github.com/CloudburstMC/Data) | `block_palette.nbt`, `runtime_item_states.json`, `creative_items.json`, `recipes.json`, `biome_definitions.json`, `voxel_shapes.json` | Proxy dumps (via [ProxyPass](https://github.com/CloudburstMC/ProxyPass), **AGPL-3.0**) | Apache-2.0 |
| GeyserMC core + [mappings-generator](https://github.com/GeyserMC/mappings-generator) | `block_palette.26_50.nbt`, `runtime_item_states`, creative items | Captures + Fabric datagen; downloads CloudburstMC Data and bedrock-samples | MIT |
| [PrismarineJS minecraft-data](https://github.com/PrismarineJS/minecraft-data) + [bedrock-protocol](https://github.com/PrismarineJS/bedrock-protocol) | ProtoDef protocol defs, sample packet dumps | `genPacketDumps.js`: start a vanilla BDS, connect its own client, serialize received packets | MIT |
| gophertunnel / [Dragonfly](https://github.com/df-mc/dragonfly) | Protocol + block registry code | Hand-maintained from captures | MIT |

The three repeatable, clean techniques here are: **(a) read Mojang's published data**, **(b) run a
behaviour pack / BDS mod on your own server and read the Script API**, **(c) capture your own session
and read the registries the server sends**. Dataminer does (a) today and targets (b) and (c).

## 4. Script API enumeration (on your own BDS)

`@minecraft/server` (stable `2.10.0`) exposes enumeration that a behaviour pack can dump to stdout:
`BlockTypes.getAll`, `BlockStates.getAll` (`id` + `validValues`), `BlockPermutation.resolve` /
`getAllStates`, `ItemTypes.getAll`, `EntityTypes.getAll`, `EffectTypes.getAll`,
`EnchantmentTypes.getAll` (`id` + `maxLevel`), `BiomeTypes.getAll` (`id` + tags), `DimensionTypes`,
`Potions.*` ([MS Learn](https://learn.microsoft.com/en-us/minecraft/creator/scriptapi/)). There is
**no** command-enumeration API — use `mojang-commands.json` or a captured `AvailableCommandsPacket`.
`@minecraft/server-net` (BDS-only, pre-release `1.0.0-beta`) gives `http` and packet `beforeEvents`,
but the packet events expose only `packetId`/`packetSize`/`sender`, not payloads, and the module must
be listed in `permissions.json`. Beta modules need the world's Beta APIs experiment on; doing that
for a headless world is unverified.

**Limit:** the Script API gives names, states and tags — not network runtime ids, NBT encodings,
hardness, or recipe wire formats.

## 5. Network-level facts

- **Block network ids.** In palette mode the runtime id is an index into a client-computed palette;
  Dragonfly stable-sorts states by FNV-1 64-bit of the block name
  ([block_registry.go](https://github.com/df-mc/dragonfly/blob/master/server/world/block_registry.go)).
  With `StartGame.UseBlockNetworkIDHashes = true`, the id is **FNV-1a 32-bit** over a little-endian
  NBT compound (root unnamed; `name` string tag; `states` compound with keys sorted; no `version`),
  `minecraft:unknown` → `0xFFFFFFFE`
  ([network_block_hash.go](https://github.com/df-mc/dragonfly/blob/master/server/world/network_block_hash.go)).
  This is a prime example of a `derived` fact for Dataminer: computable by a documented rule from
  observed state NBT.
- **The vanilla block palette is not sent at join** — only custom block properties are. A full state
  list must come from BDS (Script API cartesian product, or `mojang-blocks.json`), then be verified
  against BDS for which combinations are valid.
- **Registries sent at join** (per schemas + gophertunnel): `StartGame`, `ItemRegistry` (renamed from
  ItemComponent in 1.21.60), `BiomeDefinitionList`, `AvailableActorIdentifiers`, `CreativeContent`,
  `CraftingData`, `AvailableCommands`, and possibly `DimensionData`/`TrimData`/`CameraPresets`/
  `VoxelShapes`/`FeatureRegistry` (capture to confirm which vanilla actually sends).
- **Transport.** NetherNet (WebRTC data channels, HTTP signaling, DTLS so no MC-layer encryption)
  became the BDS default in 26.50; RakNet is removed in 26.60 (2026-10-27)
  ([Mojang NetherNet guide](https://github.com/Mojang/bedrock-protocol-docs/blob/main/additional_docs/NetherNetOnboardingGuide.md);
  [GeyserMC](https://geysermc.org/blog/preparing-for-nethernet/)). Proxies: ProxyPass (AGPL) added
  NetherNet 2026-10-03; gophertunnel and bedrock-protocol support it.

## 6. Version diffing

No single published tool; the inputs are all public and versioned per preview: git diffs of
`bedrock-samples` (`preview` branch), diffs of protocol-docs release JSON (protocol numbers move each
preview), diffs of `@minecraft/*` beta `.d.ts` on npm, diffs of BDS preview zips, and `.lang` key
diffs. Dataminer's own `diff` command operates on normalized specs, so a block/item/command/registry
change shows up as added/removed/modified entries keyed by name, with reordering ignored.
