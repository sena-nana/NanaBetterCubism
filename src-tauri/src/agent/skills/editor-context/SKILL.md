---
name: editor-context
description: Inspect Cubism Editor documents, modes, notifications, logs, physics information, or the visible Editor window.
---

# Editor Context

1. Call `get_editor_snapshot` before Editor work and respect the reported capability flags.
2. Use document references returned by `list_editor_documents`; never invent document or model UIDs.
3. Event subscription tools only configure notifications. Read received events with `list_editor_notifications`.
4. Before window capture, call `list_cubism_windows`, choose the intended current window, then pass its exact `windowId` to `capture_cubism_editor_window`. Never guess a title or window ID.
5. Use window capture only when visual confirmation helps, and describe only content visible in the returned image.
6. Treat physics information and Editor logging as inspection or communication features, not model editing.
