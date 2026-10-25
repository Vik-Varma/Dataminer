# Break speed and timing, measured on BDS 1.26.52.3

Written 2026-10-25. The evidence behind `out/1.26.52/break-speed.json`. It uses the same
server, setup and headless client as [bamboo-2026-10-24.md](bamboo-2026-10-24.md), on
difficulty normal. This note gives facts and method only.

## Method

**Setup.**
- The player stood at feet (60.5, 200, 0.5) facing +z, pitch 36.75, on stone. The target
  block was at (60, 200, 2), hit on its north face.
- Each case rebuilt the area, set the block, gave the held item to hotbar slot 0, and applied
  the case's enchantment or effect with console commands.
- **In water:** a glass box filled with still water around the player, so the eyes were under
  water.
- **Off the ground:** the player was pinned under a ceiling by Levitation.

**What was sent.** The client sent the block actions of the case in `PlayerAuthInput`, with
input flag 35. It recorded what came back:
- LevelEvent 3600 (start cracking) and 3602 (update cracking) at the target;
- LevelEvent 2001 (destroy);
- the UpdateBlock packets the server sent for the target after each destroy prediction.

**Checking.** `dataminer measure break-speed` checks every case against the model in the
spec (`payload.definition`):
- **The crack data:** the 3600 value, and every 3602 value before the break, must equal
  floor(65535 · r) exactly.
- **When the block breaks:** the break must come on input N − 1 counted from the start-break,
  where N = ceil(1/r). The reply may arrive on that input or the next.
- **Every destroy prediction:** it is accepted from input N − 1 and rejected before it.
  - A rejected prediction is answered by re-sending the block as it was.
  - An accepted one is answered with the block's new value, or not at all. Both happened:
    stone and cobblestone got the new value, dirt and leaves no re-send.
- **Abort and creative:** an abort cancels the break, and a later start starts again from
  zero. In creative, a start-break breaks the block on the same input, except while holding a
  sword.

**Hardness.** The model's hardness for each block is checked against the interval all its cases
allow (`hardness_bracket`). The intervals are narrow; for example, obsidian (34.95, 35.02].

**Second run.** The 94 cases were run twice, on 2026-10-25. Both runs gave the same crack value,
and the same answer to whether the block broke, in every case.

## What the 94 cases cover

| Group | Cases |
| --- | --- |
| Blocks and tools | 16 blocks: dirt, sand, gravel, oak leaves, glass, netherrack, stone, cobblestone, oak log, oak planks, deepslate, iron ore, end stone, cobweb, obsidian, bedrock. Each by hand and with the suitable tools: all six pickaxe tiers on stone, shovels on dirt, axes on wood, shears and a sword on cobweb, shears and a hoe on leaves. Also tools that suit the block but are too low a tier (iron ore by a wooden pickaxe, obsidian by an iron one) and tools of the wrong kind (stone by a shovel, dirt by a pickaxe) |
| Enchantment and effects | Efficiency 1–5; Haste 1–5; Mining Fatigue 1–4; Conduit Power, on stone with a wooden pickaxe |
| Water and air | dirt by hand and stone by a wooden pickaxe: in water, off the ground, and both; Conduit Power in water |
| Timing | a destroy prediction at inputs N − 3 to N for dirt, stone and cobblestone, and N − 3 to N − 1 for leaves; aborts at 1, 5 and 13; an abort then a new start; a stop; crack and continue actions every tick; a prediction with no start |
| Creative | start-break on dirt, obsidian and bedrock; start, predict and stop together; predict only; start while holding a sword |

## Two side effects, recorded rather than checked

- **The server keeps breaking a cell after its block breaks** while the client sends neither stop
  nor abort.
  - When water flowed into the broken cell, crack updates for the water followed: 4 for dirt by
    hand, 6 with Conduit Power. Water at hardness 100 with D = 30 would give exactly these:
    floor(65535 / (100 · 30 · 5)) = 4, and × 1.44 → 6. That explanation is inferred.
  - Continuing to destroy after the break started cracking the empty cell, with data 0.
- **A prediction exactly on input N − 1 is answered either way.** In some cases it got the
  block's new value, in others no answer at all.

## Not covered

- Aqua Affinity.
- Efficiency on a tool that doesn't suit the block, and Efficiency combined with Haste.
- Tool kinds on blocks other than those in the observations.
- Breaking while looking or moving away from the block.
- What the vanilla client sends: there is no vanilla client in this setup.
