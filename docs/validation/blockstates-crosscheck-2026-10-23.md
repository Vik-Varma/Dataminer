# Block-state hash cross-check vs Dragonfly

Date: 2026-10-23. This records an independent cross-check of Dataminer's `blockstates` dataset (the
block-state network hashes), as the trust rules in [AGENTS.md](../../AGENTS.md) and the attack plan
require. Facts only; no third-party data is redistributed here.

## What was checked

- **Our data:** `blockstates.json` for Mojang `bedrock-samples` **1.26.50.4** — 1,463 blocks,
  **22,189 states**, each hashed with `dataminer-core::block_hash` (FNV-1a-32 over the state's
  little-endian NBT).
- **Independent source:** Dragonfly's embedded `server/world/block_states.nbt` (MIT), the
  authoritative list of valid vanilla block states. It was decoded (network little-endian NBT:
  varint string lengths, zig-zag varint ints), and each state hashed **from its own stored NBT tags**
  using an independent replica of Dragonfly's `networkBlockHash`. That file reports **22,091 states**.

Two independent paths therefore meet at the hash: our metadata-driven enumeration + our Rust hasher,
vs. Dragonfly's state file + a separate Python hasher reading the file's actual tags.

## Result

- **Hashes reproduced exactly: 22,055 of Dragonfly's 22,091 (99.84%).**
- The authoritative NBT tags confirm Dataminer's value typing: `open_bit`, `upside_down_bit`,
  `button_pressed_bit` are **Byte**; `facing_direction`, `direction`, `weirdo_direction` are **Int**.
  This is exactly what Dataminer emits (metadata `bool` → NBT Byte, `int` → NBT Int), so the hash
  bytes match.

### The 170-state difference, fully attributed

- **36 states only in Dragonfly** are blocks absent from Mojang's public vanilladata:
  `chalkboard`, `deprecated_anvil`, `deprecated_purpur_block_1`/`_2`, `info_update`/`info_update2`,
  `netherreactor`, `end_gateway`, `stonecutter`, `invisible_bedrock`,
  `client_request_placeholder_block`, `minecraft:unknown`. These are deprecated / education /
  internal blocks, legitimately outside our source.
- **134 states only in Dataminer** are **cartesian over-generations** for a few blocks whose metadata
  property domains are wider than the block's real valid set: `cocoa` (52), `shelf_mushroom` (24),
  `frosted_ice` (12), `nether_wart` (12), `chorus_flower` (10), `activator_rail`/`detector_rail`/
  `golden_rail` (8 each). For these blocks, Bedrock prunes some property combinations that the
  cartesian product includes.

Dragonfly's file also carries a different internal `version`, so a small amount of the delta may be
version drift between it and 1.26.50.4.

## Conclusion

- The **hash algorithm is validated**: 22,055 exact matches against an independent implementation and
  an independent state-encoding path. Confidence in the per-state hash is high.
- The **cartesian enumeration is ~99.4% correct** and its one weakness is now quantified: it
  over-generates states for a known short list of blocks. This is why `blockstates` provenance flags
  the set as cartesian and points here.
- **Final oracle:** a live capture of the target version's block palette from the real BDS (M3) will
  give the exact valid set — pruning the over-generated states and including the correct
  edition-appropriate blocks. That capture is the next step for this dataset.

## Reproduce

Reproduce with the built-in command (a Rust network-LE NBT reader in
`dataminer-core::nbt` plus a hash/set diff):

```sh
dataminer metadata --samples <bedrock-samples> --out out
dataminer crosscheck blockstates \
    --spec out/1.26.50.4/blockstates.json \
    --against <df-mc/dragonfly>/server/world/block_states.nbt \
    --out crosscheck.json
```

It prints the summary above (`ours=22189 theirs=22091 matched=22055`) and writes the per-block
difference report. The numbers here were first obtained with a throwaway script and then reproduced
exactly by this command.
