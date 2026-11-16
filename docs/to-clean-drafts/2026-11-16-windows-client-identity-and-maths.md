> Copy of a message for clean-room-tool `to-clean/`, kept here at the owner's request because the session could not at first send it. Sent later as `to-clean/2026-11-16-1047-windows-client-identity-and-maths.md`; that file is the message of record.

# Windows client: identity and movement maths

- Direction: dirty -> clean. Facts only; no code, listings or code addresses.
- Answers: `to-dirty/2026-11-16-0646-windows-client-first-questions.md`.
  Checked against its bullets; what could not be answered is marked `unknown`.
- Artifact for every fact below unless stated: `Minecraft.Windows.exe`, owner-supplied,
  SHA-256 `616bb9baf78a66aa98704c0b9f5481265d396ea67da4f0ac8f7d9663cef26bb3`, 323,825,008 bytes.
  Static study only; never run.
- Methods: `strings`, `llvm-readobj`, `llvm-objdump` (LLVM, linear disassembly) and byte scans
  written for the purpose. **REA and Ghidra were not used** (see "Analysis effort").
- Everything from the executable is binary analysis (Dataminer AGENTS.md §4): a second source is
  needed before Ferrock relies on it. Second sources found are named per fact.

## Part 1: which executable

| Fact | Value | Method | How / second source |
| --- | --- | --- | --- |
| SHA-256 | `616bb9ba…6bb3` (full value above) | extracted | `sha256sum` of the downloaded file |
| Size | 323,825,008 bytes | extracted | file size |
| Format | PE32+ x86-64, GUI subsystem, 8 sections (`.text .rdata .data .pdata .tls .xbld .rsrc .reloc`) | extracted | PE headers |
| Game version | 1.26.63.1 | extracted | version resource, FileVersion and ProductVersion agree. No second source |
| Build stamp | branch `r/26_u6`, build id `53341386`, git `2fcf3da2f65401b298466e1f5953182d548b2de7`, commit time 2026-11-05T22:35:03Z | extracted | the executable's five exported `stamp` data symbols |
| Link time | 2026-11-06T00:02:14Z | extracted | PE header timestamp (same value in the debug directory) |
| Protocol version | **unknown** from the executable | unknown | not looked for beyond strings; a bare integer cannot be found that way |
| Protocol version, likely | 2225 | inferred | Mojang `bedrock-protocol-docs` tag `v1.26.62` declares 2225; there is no `v1.26.63` tag on 2026-11-16. Same minor series, not the same build |
| Ordinary readable executable | yes | inferred | `.text` byte entropy 6.40 bits/byte, even across 32 MB windows (encrypted or packed data sits near 8); four 4 MB samples disassemble with zero undecodable instructions; normal import table (1,012 imports); 991,818 exception-table function entries covering 239 MB of the 250 MB `.text`. Nothing was decrypted or unpacked |
| Signature | certificate table present | extracted | header only; the signature was not verified |
| Symbols | none: no COFF symbol table, no PDB shipped (the debug directory only names one) | extracted | PE headers |
| Exports | 8: five build-stamp data symbols, two GPU-selection hints, `initMallocRedirect` | extracted | export table |
| RTTI | mostly absent for game classes | extracted + inferred | 16,049 type-name strings exist, but 14,279 are lambdas and 6,628 are `std` types; no type name for `Player`, `Mob` or `Actor`, none ending in `Packet`. Do not plan on RTTI for class layout |
| Other names in the file | about 944 function-signature strings; strings such as packet field validation messages | extracted | `strings` |
| Toolchain | clang-cl with lld-link | inferred | PDB id field reads "LLD PDB."; lambda names are in clang's spelling |
| Instruction set | SSE/SSE2 baseline; no AVX-encoded instructions in an 8 MB sample | extracted (sample) | linear disassembly |

## Part 2: sine, cosine and square root in the movement tick

Which functions are "the player's movement tick" is **inferred**: with no symbols they were picked by
the physics constants they use (0.42, 0.2, 0.6, 0.1 for the jump; a 1e-4 threshold with yaw sine and
cosine for move-relative). The arithmetic inside them is **extracted**.

### Two different sines are in use

1. **A lookup table** (extracted).
   - 65,536 `f32` entries, built once at run time. It is not stored in the file.
   - Entry `i` is the C runtime's `sinf` of (`i` as f32 divided by the f32 constant 10430.378).
     So the table's exact bits depend on the Windows C runtime (`api-ms-win-crt-math`, i.e.
     ucrtbase) that the game runs with. **Unknown:** which `sinf` results that runtime gives;
     the runtime is not part of this executable.
   - Lookup of an angle in radians (f32): multiply by f32 10430.378, truncate toward zero to a
     32-bit integer, keep the low 16 bits, read the entry. Cosine uses the same table: add f32
     16384.0 to the scaled value before truncating.
   - Degrees are first multiplied by the f32 constant 0.017453292.
   - 185 functions reference the table (226 references).
2. **The C runtime's `sinf` / `cosf` called directly** (extracted): 175 and 171 references in the
   whole executable, in 138 functions. 13 functions use both the table and a direct call.

### Which one the movement code uses

| Fact | Method | Second source |
| --- | --- | --- |
| Move-relative (input vector rotated by yaw and added to velocity) calls `sinf` and `cosf` directly on yaw × f32 0.017453292. No table | extracted (function identity inferred) | **Agrees** with Dataminer `out/1.26.52/movement-traces.json`, trace "sprint at yaw 10, from rest": velocity x/z ratio on every one of the first six ticks is 0.1763270, the f32 `sinf/cosf` ratio; the table would give 0.1762830 |
| Several copies of this move-relative arithmetic exist (at least four functions); which copy the local player runs is **unknown** | extracted / unknown | — |
| In move-relative: squared length of the 3-component input is compared with f32 1e-4 (skip if below); length by an inline single-precision square-root instruction; the speed is divided by max(1.0, length); all f32 | extracted | — |
| The sprint-jump boost uses the **table**: velocity x decreases by table-sine(yaw) × f32 0.2, velocity z increases by table-cosine(yaw) × f32 0.2 | extracted (function identity inferred) | **Agrees** with trace "sprint-jump held at yaw 10, from rest", tick 0: the velocity splits into a direct-`sinf` part of magnitude 0.069560 (the ground tick's own 0.069560) plus a table part of 0.109201 = 0.2 × 0.54600 |
| Same function also uses f32 0.42, 0.6, 0.1 and 1.0 | extracted | their roles (jump power, a 0.6 variant, 0.1 per jump-boost level) are inferred |

The second source is Linux BDS 1.26.52.3 (measured), a different build and platform from this
1.26.63.1 Windows client. It supports the split between the two sines; it cannot confirm Windows
bit patterns.

### Precision and fused multiply-add

| Fact | Method | Notes |
| --- | --- | --- |
| The identified movement arithmetic is f32 throughout; no conversion to f64 in it | extracted | for the functions read (jump, two move-relative copies, table build) |
| Double-precision `sin`, `cos`, `sqrt` exist in the executable but are rare: 5, 3 and 20 call sites. None is in the functions identified | extracted | whether any is on the movement path elsewhere is unknown |
| Square root: 783 inline single-precision square-root instructions in the executable, plus 134 references to the runtime's `sqrtf` in 79 functions. Move-relative uses the inline instruction | extracted | which of the two other movement code uses is unknown |
| `floorf`, `ceilf`, `roundf` are calls into the C runtime (4,595 / 3,000 / 1,446 call sites), not inline rounding instructions | extracted | consistent with an SSE2 baseline |
| **No fused multiply-add.** In the functions read, each multiply and each add or subtract is a separate f32 instruction | extracted | — |
| `sinf`, `cosf`, `sqrtf`, `floorf` and the other maths functions are imported from the system C runtime, DLL name `api-ms-win-crt-math-l1-1-0.dll`; they are not linked statically | extracted | import table (70 maths imports from that DLL); the call sites counted above go to those import slots |
| Power, floor and absolute value inside the player's movement and collision for one tick | unknown | collision was not looked at. The jump function calls the runtime's `floorf` twice (extracted); nothing else was established |
| No fused multiply-add anywhere | inferred | a linear disassembly of all of `.text` shows one decode with an FMA mnemonic, in a masked AVX-512 form among bytes that do not read as compiled code; taken as a misdecode. The import table has no `fma` / `fmaf` |

## Still unknown

- How long REA's analysis takes, with any engine: it was never run.
- Power, floor and absolute value in movement and collision; collision as a whole.
- The protocol number as stored in this executable.
- Which copy of move-relative the local player uses, and the full call order of a movement tick
  (friction, gravity, collision). Only the trigonometry and square root were looked at.
- The bit results of the Windows runtime's `sinf` / `cosf`, which fix both the table and the direct
  calls. Candidate source: measure them on Windows, or capture a real client's
  PlayerAuthInput positions at many yaws.
- Whether anything on the movement path uses f64 outside the functions read.
- How the owner's copy was obtained (still not recorded in Dataminer's legal doc).

## Analysis effort

| Step | Time | Result |
| --- | --- | --- |
| Download (MediaFire) | 15 s | 323.8 MB |
| Hash, PE headers, imports, exports | under 10 s | — |
| `strings`, ASCII and UTF-16 | 6 s | 1.46 M strings |
| Linear disassembly of all of `.text` with a filter | 5.5 min, 4 cores, well under 16 GB | fine for instruction counts; no cross-references or function names |
| Byte scans for references to chosen imports and constants, mapped to functions by the exception table | 7–30 s each | this is what located the functions |
| REA 6.9.0 (npm, Node 22.23.4) | installed in 17 s; **not run on the executable** | the session's permission layer refused the run. So "how long REA's analysis takes, and with which engine" is **unknown** |
| Ghidra 12.1.4 | **not obtained** | the session may fetch GitHub repositories by git only; release downloads are refused, and looking for mirrors was refused |
| Full auto-analysis | **not attempted** | unknown how long it takes. For scale: 250 MB of code, 991,818 functions; Dataminer's docs expect hours and a 16–32 GB heap for a 100 MB binary, so 16 GB is unlikely to be enough |

For a fuller part 2 (the whole tick, in order) the dirty side needs: Ghidra supplied to the session
and permission to run REA, and realistically more than 16 GB of memory or an analysis limited to
the functions reached from the ones found here.
