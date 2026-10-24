# Movement traces, measured on BDS 1.26.52.3

Written 2026-10-24. The evidence behind `out/1.26.52/movement-traces.json`. It uses the same server,
setup and headless client as [bamboo-2026-10-24.md](bamboo-2026-10-24.md), on difficulty normal.
This note gives facts and method only.

## Method

A strict server corrects a wrong position claim at most once per six `PlayerAuthInput` ticks, so
one run reveals only every sixth tick.

1. The client replays the same inputs from the same start. On each replay it claims the ticks
   already known exactly, and the unknown ticks deliberately wrongly. The server's
   `CorrectPlayerMovePrediction` packets reveal the next unknown ticks, until every tick is known.
   This took 6 replays when the start was deterministic.
2. A merged trace is only as good as the replays agreeing. So each finished trace is **replayed
   twice more, claiming every tick**. It counts as `measured` only if neither replay drew a
   correction. `dataminer measure traces --verify` enforces this.
3. **Every replay must start from rest.** The player stands on a block or the bottom of a pool,
   and the motion is started by an input: walking off a block, walking out from under a ceiling,
   swimming from the bottom. Runs that started in mid-air or mid-water did not replay identically.
   The likely reason is when the teleport lands relative to the inputs. 8 such runs failed
   verification and are not in the spec; each was redone from rest.
4. **Every trace must start where it was meant to.** Verification can't catch a player who never
   reached the start: one run killed the player before it began, and a dead player agrees
   perfectly. `dataminer measure traces` rejects a trace whose first tick is more than 1 block
   from its start. Three such runs were redone.

Food, saturation and exhaustion were set through the Script API before each case. Effects were
applied with `/effect` before the case, so they were already active at its start.

## What the 64 traces cover

All of them are `measured`, and every one verified with zero corrections.

| Group | Traces |
| --- | --- |
| Flat stone | walking and sprinting, 40 ticks from rest, with no effect, Speed I–III, Slowness I–III, Slowness VII (walking only), and Speed II with Slowness II |
| Jumping | a standing jump with Jump Boost 0–III, a jump with Slowness II, a sprint-jump |
| Levitation | I and II, from rest under a ceiling |
| Falling | a fall from a block 30 up, with and without Slow Falling I and II |
| Water, 3 deep | idle on the bottom; walking on the bottom; sprint-swimming at pitch 0, +30 and −30; jump held from the bottom, then sneaking; jump held for 20 ticks then released; walking then sprint-swimming |
| Water, 1 deep | walking, sprinting, a jump |
| Lava, 3 deep (Fire Resistance) | idle on the bottom, walking on the bottom, jump held |
| Climbing | a ladder and vines on a stone column (forward held; forward and jump held; climb then release; climb then sneak); a scaffolding column (jump held; jump then sneak; jump then release). The column's top is capped by a roof at y 209, so the climbs stop at feet y 207.2 |
| Slowing and special blocks | walking and sprinting inside cobweb; walking and jumping on honey; walking and sprinting on soul sand; walking on blue ice; jumping on slime |
| Angled movement, for the clean side's small corrections | sprinting and sprint-jumping (jump held) at yaw 10 on stone; sprint-swimming from a pool's bottom with strafe left, strafe right, and at yaw 10 |

## Headline numbers

All are read from the traces. "Velocity" is the packet's `Delta`.

**Speed and Slowness.**
- Steady walking and sprinting speed is multiplied by exactly 1 + 0.2·L for Speed and
  1 − 0.15·L for Slowness. Slowness VII stops walking.
- Speed II with Slowness II gives 0.98: the factors multiply.
- Steady speeds per tick on stone: walking 0.21586, sprinting 0.28062.

**Jumping.**
- Launch velocity is 0.42 + 0.1·L.
- The peak rise is 1.25224, 1.83615, 2.51682 and 3.29053 blocks for L = 0–3.
- Slowness II leaves the jump unchanged.

**Falling.** Velocity follows v ← (v − 0.08)·0.98. A fall from 30 blocks lands 29 ticks after
leaving the block.

**Slow Falling.**
- Velocity follows v ← (v − 0.01)·0.98.
- Levels I and II are identical.
- Speed reached −0.431 per tick by landing, 30 blocks down, still short of the rule's limit of
  −0.49.

**Levitation.**
- Velocity follows v ← 0.784·v + 0.0098·L. It settles at 0.04537 (L = I) and 0.09074 (L = II),
  which is 0.0098·L / 0.216.
- Pinned under a ceiling, the reported velocity is 0.0098·L.

**Water.**
- Idle on the bottom, the player stays on it: y is constant.
- Walking on the bottom: 0.09799 per tick.
- Sprint-swimming at pitch 0: 0.19310 per tick. The swimming flag is set from the first
  sprinting tick.
- Jump held from the bottom: vertical velocity rises to about 0.133.
- Released mid-water: the player sinks with vertical velocity tending to −0.025.

**Lava.**
- Idle on the bottom, the player stays on it.
- Walking on the bottom: 0.0392 per tick.
- Jump held: rising steadily at 0.04 per tick.

**Climbing.**
- On a ladder or vines, climbing with forward held is a flat 0.2 per tick. Holding jump as well
  doesn't make it faster.
- Released mid-climb, the climber slows over about 5 ticks, then descends at 0.2 per tick.
- Sneaking on a ladder holds the player in place.
- The ladder's collision plate is 3/16 thick: the climber stops 0.3 + 0.1875 from the wall.
- Scaffolding: jump held rises 0.15 per tick, and sneak descends 0.15 per tick. Released, the
  player falls onto the scaffolding block below and stands on it.

**Slowing and special blocks.**
- Cobweb: walking moves a constant 0.0245 per tick, from the first tick; sprinting 0.03185.
- Honey: walking settles at 0.05928 per tick, and a jump peaks 0.51402 above the block.
- Soul sand: its top is 0.875 above its base. Walking settles at 0.11743 per tick, sprinting at
  0.15265.
- Slime: a jump peaks at the normal 1.25224, and the player then bounces.

## Not covered

- Swimming in flowing water, and bubble columns.
- **Powder snow and plain ice.** Their runs never replayed identically: the server corrected the same
  ticks on every replay. They are not in the spec. Some state that persists between replays is
  suspected, such as the freezing in powder snow.
- Starting a swim in 1-deep water by any route other than sprinting. Sprinting there did not set
  the swimming flag.
