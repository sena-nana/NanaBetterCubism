---
name: project-memory
description: Read and maintain durable Cubism project stage facts or reusable Live2D experience. Use when prior project context may affect the task, the user asks to remember or recall something, or completed work creates verified knowledge worth reusing.
---

# Project And Memory

1. The current conversation's project is assigned automatically from the active saved CMO3 document; do not ask the user to bind or name it.
2. Read memories only when prior knowledge can materially affect the task; do not call memory tools on every turn.
3. Before saving or replacing knowledge, read existing memories, update the matching ID instead of duplicating it, and archive obsolete entries.
4. Store verified current-project facts as project memory and transferable Live2D guidance as global memory.
5. Do not store temporary plans, conversation summaries, uncertain inferences, tokens, session UIDs, or raw protocol data.
6. If the conversation is in the inbox, do not claim project memory was stored; global experience remains available.
7. Report memory reads or writes only from successful tool results; never claim a failed operation succeeded.
