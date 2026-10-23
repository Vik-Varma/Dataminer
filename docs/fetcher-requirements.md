# What Dataminer needs from a fetcher tool

This is a note for whoever builds the **fetcher** — a tool that automatically downloads and pins the
inputs Dataminer reads. It says what to fetch, in what shape, and under which rules, so its output
plugs straight into Dataminer's commands. Written 2026-10-22 against Dataminer `main`.

## Why

Dataminer turns legally clean sources about Minecraft: Bedrock into versioned specs for
[Ferrock](https://github.com/Vik-Varma/Ferrock). Today a human has to clone/checkout those sources
by hand and point Dataminer at them. The fetcher automates that: given a target version, it fetches
every input at the matching ref, verifies it, and lays it out so Dataminer's subcommands run against
it unchanged. It must do this **without breaking the clean-room and legal rules** in
[AGENTS.md](../AGENTS.md) and [docs/clean-room-and-legal.md](clean-room-and-legal.md).

## The inputs to fetch

Ferrock and Acacia target **game 1.26.51, wire protocol 2193**. Resolve each source to the ref that
matches the target, and record it. The three sources below are all that Dataminer consumes today; a
fourth and fifth are cross-check sources it will consume soon.

### 1. `Mojang/bedrock-samples` — needed now (metadata datasets)
- Repo: `https://github.com/Mojang/bedrock-samples`. Git tags per version, `v<version>` (e.g.
  `v1.26.50.4`). Branch `main` is release, `preview` is preview.
- Dataminer needs the `metadata/` directory and `version.json`. A sparse, blobless checkout of just
  those paths is enough and much smaller:
  `git clone --filter=blob:none --sparse … && git sparse-checkout set metadata` then checkout the tag.
- **License:** `(c) Mojang AB. All rights reserved`, under the Minecraft EULA. Read for facts, never
  redistribute — so the checkout lives in the git-ignored working dir, never committed.
- Version note: the newest samples tag can lag the protocol-docs tag (e.g. samples `1.26.50.4` vs
  protocol-docs `v1.26.51`); both are wire protocol 2193. Record the actual samples version; pass it
  to Dataminer as `--game-version`.
- Consumed by: `dataminer metadata --samples <dir> [--protocol 2193] --out <out>`.

### 2. `Mojang/bedrock-protocol-docs` — needed now (packet layouts)
- Repo: `https://github.com/Mojang/bedrock-protocol-docs`. Git tags per version, `v<version>` (e.g.
  `v1.26.51`, which is protocol 2193). Each tag's packet JSON carries `x-protocol-version`.
- Dataminer needs the `json/` directory. Checkout the tag that matches the target protocol.
- **License:** same EULA posture as bedrock-samples. Working dir only, never committed.
- Consumed by: `dataminer protocol --docs <dir> --game-version 1.26.51 --out <out>`.

### 3. Bedrock Dedicated Server (BDS) binary — **verified working** (RTTI, symbols, strings)
- Mojang's free download, on Mojang's CDN under `https://www.minecraft.net/bedrockdedicatedserver/`.
- **Resolve the URL from an index.** `Bedrock-OSS/BDS-Versions` is the best one: `versions.json` gives
  `cdn_root` and the `stable`/`preview` versions per platform, and `linux/<version>.json` gives the
  exact `download_url`, a **`sha1`** and `size_in_bytes`. (`EndstoneMC/bedrock-server-data` is a
  similar fallback.) Download **from Mojang's own `download_url`**, not a mirror. Example for the
  Ferrock target: `linux/1.26.51.1.json` →
  `https://www.minecraft.net/bedrockdedicatedserver/bin-linux/bedrock-server-1.26.51.1.zip`,
  sha1 `0610779dc0b8461736fc3aff74d72e53fe8c37e2`, 94,926,669 bytes.
- **Download recipe that works behind this environment's egress proxy** (verified 2026-10-23):
  Mojang's CDN rejects the default client here, so send a browser `User-Agent` and force HTTP/1.1,
  and follow redirects: `curl -sSL --http1.1 -A "Mozilla/5.0 (X11; Linux x86_64)" -o bds.zip <url>`.
  Without the UA you get an empty reply / HTTP-2 stream error, not a policy denial. Then **verify the
  sha1 against the index** before using the file, and fail on mismatch.
- **Accept the EULA explicitly.** The fetcher must require an `--accept-eula` flag (or equivalent)
  and record that the user accepted it. Do not download BDS without it.
- Pick the right build: **Linux `bedrock_server` ELF** is the primary RE target — confirmed on
  1.26.51.1 to be a stripped 256 MB x86-64 PIE ELF with **no static symbol table (391 dynamic
  symbols) but RTTI intact**: Dataminer's `binary rtti` recovered **51,590 class names** from its
  `_ZTS*` rodata type strings. The Windows PE has no PDB (removed ~1.21.20) and is built `/GR-`
  (no RTTI), so prefer Linux.
- Extract the zip into the working dir. **Never commit the binary or anything derived from it**
  (the RTTI/strings specs are ~27 MB / ~80 MB — they go to Ferrock as a pinned release, not into git).
- Consumed by: `dataminer inspect <bin>`, `dataminer binary rtti|symbols|strings <bin> --game-version <v>`.

### 4. Cross-check sources — soon (trust/verification), all permissive licenses
Fetch these so Dataminer can cross-check its facts and report disagreements (see the trust section of
[attack-plan.md](attack-plan.md)). None are EULA-restricted; they may be cached locally.
- `@minecraft/vanilla-data` (npm, **MIT**) — block/item/entity enums.
- PrismarineJS `minecraft-data` (bedrock) — protocol and data, MIT.
- `gophertunnel` / `df-mc/dragonfly` (MIT) — protocol and block registry, incl. the block
  network-hash algorithm.
- Acacia's `blocks.bin` and `acacia-physics` (MIT) — the bedsim physics port, for the movement work.

### Explicitly out of scope — never fetch
- The DRM-protected Windows **client** (UWP/GDK), or any Marketplace content. No decryption, ever.
- NDA artifacts: private `bedrock_server.pdb`, `bedrock-headers`, the Android `libminecraftpe.so`
  DWARF, or any symbol/header dump derived from them.
- `cinnabar-mc/cinnabar` (PolyForm Noncommercial) — never, including its registry blobs.
- Mojang assets (textures, sounds, models).

## Output shape (what Dataminer points at)

Lay everything out under one git-ignored working directory, one subtree per source, plus a
`sources.json` recording exactly what was fetched. Suggested layout:

```
work/<game-version>/
  bedrock-samples/            # checkout at tag (metadata/ + version.json)
  bedrock-protocol-docs/      # checkout at tag (json/)
  bedrock-server/             # extracted BDS (bedrock_server ELF, definitions/, etc.)
  sources.json                # provenance of the fetch (see below)
```

`sources.json` records, per source: the kind, the exact URL, the git ref or file name, the
**SHA-256** of what was downloaded, the resolved game version / protocol, and the fetch timestamp.
Dataminer already hashes the specific files it reads into each spec's provenance and manifest; this
file documents the fetch itself so a run is reproducible and auditable.

Then the pipeline is just:

```sh
fetcher fetch --game-version 1.26.51 --accept-eula --out work
dataminer metadata --samples work/1.26.51/bedrock-samples  --protocol 2193 --out out
dataminer protocol --docs    work/1.26.51/bedrock-protocol-docs --game-version 1.26.51 --out out
dataminer binary rtti work/1.26.51/bedrock-server/bedrock_server --game-version 1.26.51 --out out/1.26.51/bds-rtti.json
```

## Rules the fetcher must follow

- **Nothing fetched is ever committed.** Everything lands in the git-ignored working dir (Dataminer's
  `.gitignore` already excludes `bedrock-samples/`, `bedrock_server*`, `*.pdb`, `*.debug`, `captures/`,
  etc.; keep the working dir under one of those or add it). The fetcher distributes nothing — it only
  downloads for the user who accepted the EULA.
- **Pin and verify.** Resolve to an exact git tag/commit or a named download, record its SHA-256, and
  re-verify on re-fetch. Fail loudly on a hash mismatch rather than proceeding.
- **Deterministic and idempotent.** Re-running for the same version is a no-op if the cache is intact
  and hashes match; it exits non-zero on any failure, with a clear message (so it can run in CI on a
  machine that is allowed to download).
- **EULA-gated for Mojang downloads.** Require explicit acceptance for BDS (and note that the samples
  and protocol-docs repos are EULA too, though public on GitHub).
- **Report blocked hosts; never route around a policy denial.** If an egress proxy refuses a host,
  say so and stop.
- **Honest about versions.** If no source has an exact match for the target, report the closest
  available ref per source and the protocol each maps to; do not silently substitute.

## Suggested implementation

Keeping it one tool is cleanest: add a **`dataminer fetch`** subcommand backed by a new
`dataminer-fetch` crate in this workspace (Rust 1.99, GPL-3.0, no `unsafe`, same `scripts/check.sh`
gate). It can shell out to `git` for the two repos and use a small HTTP client for the BDS zip; or it
can be a standalone tool that writes the same `work/<version>/` layout — Dataminer only cares about
the layout, not how it was produced. Either way:
- Take `--game-version` and resolve refs per source; `--accept-eula`; `--out <working-dir>`;
  optional `--preview` and `--skip-bds`.
- Write `sources.json` as described.
- Add a `verify` mode that re-checks hashes without downloading.

## Acceptance

- `fetcher fetch --game-version 1.26.51 --accept-eula --out work` produces a `work/1.26.51/` tree with
  the two repo checkouts (and BDS when not skipped) and a complete `sources.json`.
- The three `dataminer` commands above run against it unchanged and produce the expected specs
  (metadata: 7 datasets; protocol: protocol 2193, 231 packets, 160 enums; binary: RTTI/strings).
- Re-running the fetch is idempotent and byte-stable; a corrupted cache is detected by hash.
- Nothing downloaded appears in `git status`.
