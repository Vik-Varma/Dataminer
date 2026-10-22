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

Dataminer never circumvents a technical protection measure: it reads Mojang's *freely distributed*
Bedrock Dedicated Server and Mojang's *published* data. It does not touch the DRM-protected Windows
client. That keeps it well inside "study of a lawfully obtained program", not circumvention.

## What is and isn't OK to commit

| Thing | Read it? | Commit it to this GPL repo? |
| --- | --- | --- |
| `bedrock-samples` / `bedrock-protocol-docs` files (EULA, all rights reserved) | Yes — for facts | **No.** Supply a local checkout; emit derived specs |
| Large verbatim excerpts of the above | — | **No** |
| Derived specs (ids/names/field order as facts) | — | Yes — in Ferrock or a data repo (facts, with provenance) |
| `@minecraft/vanilla-data` (MIT) | Yes | The data is MIT; still prefer emitting facts over bundling files |
| BDS binary, PDBs, `bedrock_server.debug`, Android `libminecraftpe.so`, generated headers | BDS: static study only | **No** — never commit the binary or symbol/header artifacts |
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
