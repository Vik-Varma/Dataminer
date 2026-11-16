# Measuring a server with `dataminer-probe`

Written 2026-11-16. How to record the run log that `dataminer measure traces` turns into a
`movement-traces` spec. Method only; no facts about the game are stated here.

## What the probe is

A headless client for our own Bedrock Dedicated Server. It joins with an offline login through
[Acacia](https://github.com/Vik-Varma/acacia) (MIT), sends one `PlayerAuthInput` every 50 ms, and
logs what the server's `CorrectPlayerMovePrediction` packets say. It has no physics of its own:
it never predicts a position, it only claims one and records the server's answer.

For each case it teleports the player to the start, waits until the server stops correcting an
idle player, then replays the case's inputs. Ticks it does not know yet are claimed 0.05 blocks
wrong on purpose, so the server corrects them and thereby reports its own position, velocity and
on-ground flag for that tick. When every tick is known it replays the case twice more claiming
every tick exactly, and writes which ticks were still corrected: none, for a trace to count as
measured.

What it sends, so a spec's provenance can say so: input mode mouse, play mode normal, interaction
model crosshair; the move vector exactly as the case gives it, also as the analogue and raw move
vectors; camera orientation from yaw and pitch; a zero delta; only the input flags the case lists,
plus "handled teleport" once after each teleport.

## The server

- Our own BDS, never someone else's. 1.26.52.3 is the newest that accepts RakNet with an offline
  login; the probe speaks protocol 2193.
- `server.properties`: `online-mode=false`, `allow-list=false`, `transport=raknet`,
  `server-authoritative-movement-strict=true`, `player-position-acceptance-threshold=0.0001`,
  `player-rewind-min-correction-delay-ticks=0`, `allow-cheats=true`,
  `default-player-permission-level=operator`, `player-idle-timeout=0`, and a flat world.
  With the rewind delay at 0 the server corrects every wrong tick, so one replay usually learns a
  whole case.
- Setup commands go to the server's console. Start it reading from a FIFO
  (`tail -f in.fifo | ./bedrock_server`) and pass that FIFO as `--console`.
- A host without IPv6 needs a preloaded shim that hands the server IPv4 sockets; it exits
  otherwise.
- The server takes its sine, cosine and similar functions from the host's C library. Record the
  host's libc version with the run: results can differ in the last bit between versions.

## Cases

A JSON array. Each case has a `name`, console commands in `setup` (once) and `pass_setup` (before
every replay; `{name}` is the player's name), a feet `start`, a `yaw`, a number of `ticks`, and
`spans` of held input: `from`, `to`, `forward`, `strafe` (left positive), `jump`, `sneak`, `yaw`,
`pitch`, and extra input-flag `bits`. Every case must build what it needs and start from rest.

## From log to spec

```sh
dataminer measure traces --runs traces.jsonl --verify verify.jsonl \
    --bds path/to/bedrock_server --bds-version 1.26.52.3 \
    --game-version 1.26.52 --protocol 2193 --out out
```
