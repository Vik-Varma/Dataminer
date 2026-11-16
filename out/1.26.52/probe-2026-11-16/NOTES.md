# Probe run of 2026-11-16

177 movement traces recorded by `dataminer-probe` on our own Linux BDS 1.26.52.3 (the same
`bedrock_server` as `out/1.26.52/`), derived with `dataminer measure traces`. They are kept beside
the 75 traces in `out/1.26.52/movement-traces.json` rather than merged into them, because that
file's run logs no longer exist and the command writes a whole file from its logs.

- Host C library: glibc 2.39. The server takes `sinf` and `cosf` from it, and those results are in
  the traces' last bits.
- The host has no IPv6; the server ran behind a preloaded shim that gives it IPv4 sockets.
- `player-rewind-min-correction-delay-ticks=0`, so the server corrected every wrong tick and each
  case needed one replay. Every trace then verified twice with no correction.
- Left out: ten cases at yaws of 34,000,000 degrees and beyond, in either direction. The server
  sent no correction for them at all, so there is nothing to record.
- Server settings and what the probe sends: `docs/measuring.md`.
