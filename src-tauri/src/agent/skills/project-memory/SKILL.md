---
name: project-memory
description: Read and maintain durable Cubism project stage facts or reusable Live2D experience. Use when prior project context may affect the task, the user asks to remember or recall something, or completed work creates verified knowledge worth reusing.
---

# Project And Memory

1. The current conversation's project is assigned automatically from the active saved CMO3 document; do not ask the user to bind or name it.
2. Read memories only when prior knowledge can materially affect the task; do not call memory tools on every turn.
3. Start with `list_memories` (index + Overview/Summary only). Use `read_memory` with explicit layers when deeper facts are needed; do not assume list results contain Stage/Structure/Decisions or Technique/Caveats.
4. Before saving or replacing knowledge, list or read existing memories, update the matching ID instead of duplicating it, and archive obsolete entries.
5. Store verified current-project facts as project memory and transferable Live2D guidance as global memory. Bodies are fixed-layer Markdown.
6. Project Markdown layers (H2, exact names): `Overview`, `Stage`, `Structure`, `Decisions`. Global layers: `Summary`, `Technique`, `Caveats`. Missing layers are empty; do not invent other H2 names.
7. Prefer whole-document `body` upserts, or patch one layer with `layer` + `content`. Keep Overview/Summary non-empty and current.
8. Do not store temporary plans, conversation summaries, uncertain inferences, tokens, session UIDs, or raw protocol data in any layer.
9. If the conversation is in the inbox, do not claim project memory was stored; global experience remains available.
10. Report memory reads or writes only from successful tool results; never claim a failed operation succeeded.
