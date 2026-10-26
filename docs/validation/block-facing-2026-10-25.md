# Block facing, measured on BDS 1.26.52.3

Written 2026-10-25. The evidence behind `out/1.26.52/block-facing.json`. It uses the same
server, setup and headless client as [bamboo-2026-10-24.md](bamboo-2026-10-24.md). This note
gives facts and method only.

## Method

1. A survival player was teleported to a chosen position, given the block as an item in hotbar
   slot 0, and turned to a chosen yaw and pitch.
2. The client sent one InventoryTransaction:
   - UseItem, action ClickBlock, trigger type PlayerInput, prediction success;
   - the clicked block and face, and the click position;
   - the clicked block's runtime id (stone's);
   - the held item as the server had last sent it.
3. A behaviour pack on our server then read the placed cell with the Script API:
   `Block.permutation.getAllStates()`, logged to the server console. The client joined that
   log to its own case list by a tag unique to the run.
4. The player's feet are the server's own position, the eye minus 1.62, taken after the player
   had settled on what it stood on: stone, snow layers, a carpet, a trapdoor, or a ceiling it
   was pinned under by Levitation.

`dataminer measure block-facing` turns the records into the spec:
- It requires every placement to have placed the block.
- It requires two floor placements at the same yaw to agree.
- It derives the up/down limits as brackets from the cases themselves. Cases that would make a
  bracket empty are refused.

## What the 204 placements cover

| Group | Cases |
| --- | --- |
| By yaw | furnace, carved pumpkin, chest, loom, stonecutter, observer, dispenser, piston and oak stairs, each placed on the floor from two blocks away at yaw 180, −90, 0 and 90 |
| The same nine blocks, other ways | looking down at 60° and 75° from closer; under a ceiling (cell above the player); against a block's north face, clicked at height 0.75 and at 0.25 |
| Observer, dispenser, piston: height | the player's feet above the cell by 0, 0.0625 (carpet), 0.125–0.5 (snow), 0.1825 (trapdoor) and 1 (a hole); the cell above the feet by 1 (head height) and by 1.5 to 2 (ceilings, snow, carpet, trapdoor, pinned by Levitation) |
| Observer, dispenser, piston: distance | the feet 0.81 to 2.3 from the cell's centre along z, and four diagonal positions |

## Results

- **Horizontal facing by yaw:** in `payload.horizontal`, and the same in both runs made on
  2026-10-24 and 2026-10-25. Every block faced as by yaw unless the vertical rule applied.
  Pitch made no difference.
- **Observer, dispenser and piston face vertically** only when the player is close to the cell
  along both x and z: max(|dx|, |dz|) ≤ L, with L in [1.50, 1.53).
  - A straight-line limit can't explain the cases: (−1.4, −1.4), 1.98 away, faced vertically,
    while (0, −1.53) did not.
  - Then they face by `feet_above` when feet − Y > a, with a in [0, 0.0625).
  - They face by `cell_above` when feet − Y < b, with b in (−1.8175, −1.8].
- **Stairs:** clicking a block's side at height 0.75, or a ceiling's bottom face, gave
  `upside_down_bit` true. At height 0.25, or on a floor, it gave false.

## Not covered

- The click height at which stairs switch.
- The yaw boundaries between the four directions; only the four axis yaws were placed.
- The horizontal-only blocks placed with the player above the cell.
