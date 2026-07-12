# mini-world

A sandbox, semi-AFK simulation platform where **every character is a small on-device AI model**.

- **SOUL model** (~1-5M params): a tiny policy network with a "digital body" — it tool-calls `move / attack / interact / speak / trade / ...` against a validated action manifest, every simulation tick, for hundreds of characters at once, on a phone.
- **TEXT model** (~0.6B, shared): renders dialogue only when someone is actually watching ("latent dialogue"). Off-screen, conversations resolve to outcomes; the words are generated on demand.
- Characters **diverge over their lifetime**: memory, habits, experience biases, and slow trait drift — shared frozen weights, per-character plastic state (~10-100KB each).
- One deterministic kernel, many **scenario packs**: village social sims, advanced NPCs, AFK autobattlers, MOBA management, football-manager-style stat games.

Status: **design phase**. Read the full research-grounded architecture in [DESIGN.md](DESIGN.md).
