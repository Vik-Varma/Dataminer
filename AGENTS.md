# Rules for coding agents

These rules bind every AI agent working in Dataminer. Humans follow them too.

Dataminer exists to serve [Ferrock](https://github.com/Vik-Varma/Ferrock), a clean-room Rust
reimplementation of the Minecraft: Bedrock Edition client. Dataminer's job is to turn **legally clean
sources** into **versioned, diffable specs** — facts about the game (ids, names, enum values, packet
field order, class names, constants) that Ferrock can build against. It is the "dirty" side of a
clean-room process. Its output is facts, never code to copy.

Two things make this work dangerous if done carelessly: licenses and clean-room hygiene. Most rules
below close one of those doors. A PR that breaks one gets reverted, not patched.

## 1. Clean-room, in one paragraph

A **dirty** side (Dataminer, and anyone reading disassembly) produces functional specs: facts only,
no expressive code. A **clean** side (Ferrock) implements from those specs and never sees
disassembly, decompiler output, or another project's source. Keep the two apart. If you have read
Bedrock's decompiled code, or Cinnabar's / LeviLamina's / Endstone's source, you are on the dirty
side for that subject and must not also write the Ferrock implementation of it. Record which side
each person and artifact is on.

## 2. Never commit tainted inputs or outputs

- **Never commit Mojang assets or Mojang's data files.** `Mojang/bedrock-samples` and
  `Mojang/bedrock-protocol-docs` are published under the Minecraft EULA as "all rights reserved".
  Reading them to extract facts is fine; **redistributing the files, or verbatim copies of them, in
  this GPL repository is not.** Dataminer reads them from a local checkout the user supplies and
  emits derived specs. Do not check the Mojang files into this repo, and do not paste large verbatim
  excerpts of them into specs, tests or docs.
- **Never commit the BDS binary**, or any symbol file, PDB, or `bedrock-headers`-style artifact
  derived from it. BDS is Mojang's; symbol/header artifacts that circulate are often NDA-protected.
- **Never import another project's reverse-engineered data as if it were a source.** Endstone's and
  LeviLamina's headers and symbol tables are derived from the binary (and partly from NDA artifacts).
  They are at most *hints to verify independently*, cited as such, never copied.
- **Never commit** account tokens, captures that contain account details, or non-public server
  addresses.
- Generated specs (the JSON this tool emits) are facts and may live in Ferrock or a data repo, but
  the raw Mojang/binary inputs that produced them stay out of version control. See
  [docs/clean-room-and-legal.md](docs/clean-room-and-legal.md).

## 3. Every fact carries its provenance

- Every value Dataminer emits goes through `dataminer-core`'s `SpecDocument`, which records the
  source kind, confidence, game version, artifact description, and the SHA-256 of the exact bytes it
  was read from. No fact ships without this. A reviewer must be able to reproduce any number.
- Use the honest method (matching Ferrock's requirements doc): `extracted` (read verbatim, or
  computed from such reads by a documented deterministic rule like a block network-id hash),
  `measured` (observed by running the game, e.g. recording a strict-movement BDS's corrections —
  preferred for physics), `inferred` (a guess from incomplete evidence), `unknown` (an honest gap).
  An `inferred` value must never, on its own, gate a behaviour decision in Ferrock, and you never
  fill a gap with a guess to raise coverage — mark it `unknown`.
- Output is deterministic (sorted records, byte-identical on re-run) and laid out as Ferrock
  consumes it: `out/<game-version>/<dataset>.json` plus a `manifest.json` (game version, protocol,
  source hashes, Dataminer commit, per-dataset schema version). See
  [docs/attack-plan.md](docs/attack-plan.md).
- When you could not establish something, say so in the provenance `notes`. An honest gap is a good
  outcome; a confident wrong fact is the worst one.

## 4. Verify, don't trust — especially LLM output

- Prefer the most authoritative source and **cross-check across sources**. When Mojang's metadata and
  a binary disagree, report the difference; do not silently pick one.
- Binary analysis facts (`bds-*`) need a second source before Ferrock relies on them: a matching
  string cross-reference, an RTTI name, a matching struct size, a packet capture, or Mojang metadata.
- **LLM-assisted RE is a proposer, never an authority.** A model may suggest a name or type; accept it
  only when a check derivable from the binary confirms it, label it in the provenance, and keep it on
  the dirty side. Decompiled code fed to or produced by a model is dirty-side material.

## 5. Scope and shape

- Do only the task. No unrequested extractors, options or "while I'm here" changes. Ask for a target
  when the goal is vague.
- No new `unsafe` (the workspace denies it). Keep crates engine-free and deterministic: an extractor
  takes bytes and returns a `SpecDocument`, with no global state.
- A new dependency needs a sentence in the PR on why, and must be under a GPL-3.0-compatible license.
- Follow the style of the code around you. Comment only what the code can't say.

## 6. Verify before you claim

- Before saying a change is done, run `scripts/check.sh` (format, clippy with warnings-as-errors,
  tests) and paste the summary into the PR. Never land code that wasn't compiled and tested.
- Say exactly what you ran, and on which OS. "Should work" isn't a result. Report failures as
  failures, with the output.
- Fix a bug with a regression test that failed before the fix.

## 7. Outside this repository

- Never open a PR or issue on Mojang's or anyone else's repositories that a human hasn't read and
  stands behind.
- Keep Dataminer non-commercial in tone and clear that it is unofficial and unaffiliated with Mojang
  or Microsoft.
