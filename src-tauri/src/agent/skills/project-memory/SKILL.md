---
name: project-memory
description: Maintain durable Cubism project stage facts or reusable Live2D experience. Use when the user asks to remember something, verified completed work should be saved, or an existing memory must be updated or archived.
---

# Project Memory Maintenance

1. The current conversation's project is assigned automatically from the active saved CMO3 document; do not ask the user to bind or name it.
2. Before saving, load `memory-recall`, recall the same topic, and update the matching ID instead of creating a duplicate. When updating, pass that result's `revision` as `expectedRevision`.
3. Store verified current-project facts as project memory and transferable Live2D guidance as global memory. Bodies are fixed-layer Markdown.
4. Project layers are `Overview`, `Stage`, `Structure`, `Decisions`; global layers are `Summary`, `Technique`, `Caveats`. Keep Overview/Summary non-empty and do not invent other H2 names.
5. Prefer a complete `body`; use `layer` plus `content` only for a targeted update that must preserve other layers.
6. Do not store temporary plans, conversation summaries, uncertain inferences, tokens, session UIDs, or raw protocol data.
7. If the conversation is in the inbox, do not claim project memory was stored.
8. If an update returns `memory_conflict`, recall the topic again and reconcile the newest content before one retry; never retry blindly.
9. Report writes only from successful tool results.
