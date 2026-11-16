# Clean-room method and legal guardrails

Written 2026-10-22. A factual summary to keep Dataminer's output usable by a GPL-3.0 clean-room
project. **This is not legal advice.** Someone with authority on the project should make the final
calls, especially on redistributing Mojang data.

## The clean-room method

Reverse-engineering for interoperability is defensible when done as a two-team ("Chinese wall")
process:

- The **dirty team** examines the original (disassembly, decompiler output, other projects' source)
  and writes **functional specifications**: facts only — ids, names, enum values, field order, sizes,
  constants, class names. No expressive code, no copied comments, no pseudocode.
- The **clean team** implements from those specs and has **never seen** the original's code. It
  produces the shipping implementation (Ferrock).
- Both sides keep records of who saw what.

Dataminer is a dirty-side tool. Its specs are the wall. The rules in [AGENTS.md](../AGENTS.md)
enforce the separation: facts carry provenance, tainted inputs/outputs are never committed, and
anyone who has read the original's code for a subject must not also write Ferrock's implementation of
it.

Courts have blessed intermediate copying done for this purpose:

- *Sega v. Accolade*, 977 F.2d 1510 (9th Cir. 1992): disassembly is fair use when there is a
  legitimate reason and no other way to access the interoperability information.
- *Sony v. Connectix*, 203 F.3d 596 (9th Cir. 2000): intermediate copies made during reverse
  engineering are fair use, and the resulting product was transformative.

## Interoperability law (factual)

- **EU Directive 2009/24/EC.** Art. 5(3) lets a lawful user observe, study and test a running program.
  Art. 6 permits decompilation without permission when it is *indispensable* for interoperability of
  an independently created program, done by a licensee, where the information is not readily
  available, and limited to the necessary parts — and the information may not be used to build a
  program *substantially similar in expression*. Art. 8 voids contract terms that override these.
  *SAS v WPL* (CJEU C-406/10, 2012): functionality, programming languages and data file formats are
  not themselves protected by copyright.
- **US DMCA §1201(f)** permits circumventing access controls to identify interoperability elements
  and sharing that information solely to enable interoperability
  ([17 U.S.C. 1201](https://www.law.cornell.edu/uscode/text/17/1201)).

Dataminer itself never circumvents a technical protection measure: it reads Mojang's *freely
distributed* Bedrock Dedicated Server, Mojang's *published* data, and files the owner supplies.

**The Windows client (changed 2026-11-16).** Until this date Dataminer did "not touch the
DRM-protected Windows client", which kept it well inside "study of a lawfully obtained program".
The owner lifted that line: Ferrock's goal is now physics and packets bit-for-bit those of the
vanilla client, and some of that can't be learned from BDS or from captures alone. From this date:

- A dirty-side session may statically study a copy of `Minecraft.Windows.exe` that the owner
  supplies, for interoperability facts only, limited to the parts those facts need.
- Dataminer still performs no decryption, unpacking or other circumvention itself, and no tool
  that does is added to it. The copy arrives readable.
- The owner is responsible for holding a licence to the game and for how the copy was made
  readable. **Not yet recorded:** how this copy was obtained. The owner should state it here; it
  bears on whether §1201(f) or Art. 6 is being relied on, which the BDS-only scope never needed.
- Every spec from it records the executable's SHA-256 and game version, and counts as binary
  analysis under AGENTS.md §4: a second source before Ferrock relies on it.
- Captures of the running client and measurements on BDS stay the preferred sources. The
  executable is for what they can't explain.

**Who saw what (2026-11-16).** The owner allowed the dirty-side session that studied the Windows
client to open the Ferrock repository, to look for the measuring client. It read only
`crates/ferrock-headless/src/main.rs` (the command line) and `docs/bds-testing.md` at commit
`c2c02a0`, and built and ran `ferrock-headless` unmodified against its own strict BDS 1.26.52.3. It
wrote nothing to Ferrock. That session has read the client's disassembly and decompiler output for
player movement, so it stays barred from writing Ferrock's movement code.

## What is and isn't OK to commit

| Thing | Read it? | Commit it to this GPL repo? |
| --- | --- | --- |
| `bedrock-samples` / `bedrock-protocol-docs` files (EULA, all rights reserved) | Yes — for facts | **No.** Supply a local checkout; emit derived specs |
| Large verbatim excerpts of the above | — | **No** |
| Derived specs (ids/names/field order as facts) | — | Yes — in Ferrock or a data repo (facts, with provenance) |
| `@minecraft/vanilla-data` (MIT) | Yes | The data is MIT; still prefer emitting facts over bundling files |
| BDS binary, PDBs, `bedrock_server.debug`, Android `libminecraftpe.so`, generated headers | BDS: static study only | **No** — never commit the binary or symbol/header artifacts |
| `Minecraft.Windows.exe`, supplied by the owner (from 2026-11-16) | Static study only, on the dirty side | **No** — never commit it, or a database, listing or export derived from it |
| NDA-protected symbols/headers (private PDBs, `dwarf2cpp` headers) | **No** | **No** |
| Other projects' RE'd headers/tables (Endstone, LeviLamina) | As hints to verify, cited | **No** — do not copy |
| Mojang assets (textures, sounds, models) | — | **No** |
| Account tokens, captures with account details, non-public addresses | — | **No** |

## Facts vs. expression

The line Dataminer walks: **facts and interfaces are freely usable; expression is not.** A block's
id, a packet's field order, an enum's values, a class's name — these are facts about an interface,
the kind of thing *SAS v WPL* and §1201(f) protect the right to extract and reuse. The original's
*code* — how a function is written, its structure and comments — is expression and must not be copied
or paraphrased into Ferrock. When in doubt, emit the fact and its provenance, and let the clean team
write the implementation from scratch.
