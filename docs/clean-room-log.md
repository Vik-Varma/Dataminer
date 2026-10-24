# Clean-room log

Who has been on which side of the wall, for which subject, and what they saw and produced
([AGENTS.md](../AGENTS.md) section 1). Newest entry first. Facts only.

## 2026-10-24: bamboo shape and position offset

- **Who:** a Claude Code agent session working for Vik-Varma, on the **dirty side** for this subject.
  It must not write Ferrock's implementation of bamboo's shape or offset.
- **Brief:** establish bamboo's collision and render boxes per state, its position offset, and which
  other blocks are offset, from BDS 1.26.52.3, cross-checked by measurement on a strict-movement BDS.
- **Input:** Linux BDS 1.26.52.3 (`bedrock-server-1.26.52.3.zip`), fetched with
  `dataminer fetch --game-version 1.26.52 --accept-eula` into a working directory outside the repo.
  `bedrock_server` SHA-256 `0a490c711d4a2ce075debcd979e8862b7eaba008373f9970f0edbb67f98f9207`
  matched the brief.
- **What it saw of the binary:** type-name strings containing `Bamboo`, found by a plain string
  search: `BambooStalkBlock`, `BambooSaplingBlock`, `BambooItem`, `BambooFeature`. No function was
  disassembled or decompiled.
- **Other activity:** it started the server once with strict movement and placed blocks through the
  console. No movement measurement was taken.
- **Output:** none. No bamboo facts were established, and no spec was written. The work stopped
  before any measurement or extraction. Bamboo's shape and offset remain unestablished.
- **Contact with the clean side:** two status messages to the Ferrock implementation session saying
  what would and would not be delivered. They contained no facts and nothing about the binary.
  The session's tooling loaded Ferrock's agent rules file into its context at start; it read no
  other Ferrock file and changed nothing there.
- **Left outside the repo:** the BDS download and the server's test world, in the session's
  container, which is reclaimed when the session ends. Nothing derived from the binary was committed.
