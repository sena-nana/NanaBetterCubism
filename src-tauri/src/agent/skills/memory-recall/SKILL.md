---
name: memory-recall
description: Recall durable Cubism project stage facts and reusable Live2D experience relevant to the current task. Use when prior project context may affect the answer or action, or the user asks what was previously learned or completed.
---

# Memory Recall

1. Recall only when prior knowledge can materially affect the current task; do not recall on every turn.
2. Call `recall_memory` with a concise task-focused query. Use `focused` by default, `index` for a summary-only scan, and `full` only when the complete matched memories are necessary.
3. Search both scopes by default. Restrict to `project` or `global` only when the user or task clearly requires it.
4. Treat returned layers as the complete successful result for that call. If the result is truncated, narrow the query or scope before increasing the limit.
5. Do not infer remembered facts from an empty result or claim that an unsuccessful recall found anything.
